<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import {
    Activity,
    Award,
    CalendarCheck,
    CalendarRange,
    Crosshair,
    Flag,
    History,
    Medal,
    Play,
    Star,
    Target,
    Timer,
    Trophy
  } from '@lucide/svelte';
  import { api } from '$lib/api';
  import { session } from '$lib/stores';
  import type { HistoryItem, PlayerProgression, RankedLeaderboardResponse } from '$lib/types';

  let games = $state<HistoryItem[]>([]);
  let loading = $state(true);
  let progression = $state<PlayerProgression | null>(null);
  let claimingMission = $state<string | null>(null);
  let progressionError = $state('');
  let hasAccount = $state(false);
  let leaderboard = $state<RankedLeaderboardResponse | null>(null);
  let leaderboardLoading = $state(false);
  let leaderboardLoadingMore = $state(false);
  let leaderboardVisibilitySaving = $state(false);
  let leaderboardError = $state('');
  onMount(async () => {
    try {
      const current = await api.currentSession();
      session.set(current);
      hasAccount = Boolean(current.accountId);
      const [history, profile] = await Promise.all([api.history(), api.profile()]);
      games = history.games;
      progression = profile;
      if (hasAccount) await loadLeaderboard();
    } catch {
      await goto(resolve('/'));
    } finally {
      loading = false;
    }
  });
  const won = (game: HistoryItem) => game.result.winnerId === game.selfPlayerId;
  const duration = (seconds: number) =>
    `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
  const contentDate = (value: string) =>
    new Intl.DateTimeFormat('ko-KR', { month: 'short', day: 'numeric' }).format(new Date(value));
  const contentStatus = (value: 'UPCOMING' | 'ACTIVE' | 'ENDED') =>
    value === 'ACTIVE' ? '진행 중' : value === 'UPCOMING' ? '예정' : '종료';
  const winTypeLabel = (game: HistoryItem) => {
    if (game.result.winType === 'SURRENDER') return 'SURRENDER';
    if (game.result.winType === 'DISCONNECT') return 'DISCONNECT';
    if (game.result.winType === 'TIMEOUT') return 'TIMEOUT';
    return 'NORMAL VICTORY';
  };
  async function claimMission(missionId: string) {
    claimingMission = missionId;
    progressionError = '';
    try {
      progression = await api.claimMission(missionId);
    } catch (caught) {
      progressionError =
        caught instanceof Error ? caught.message : '임무 보상을 지급하지 못했습니다.';
    } finally {
      claimingMission = null;
    }
  }
  async function loadLeaderboard(seasonId?: string, append = false) {
    if (!hasAccount) return;
    leaderboardError = '';
    if (append) leaderboardLoadingMore = true;
    else leaderboardLoading = true;
    try {
      const next = await api.rankedLeaderboard(
        seasonId ?? leaderboard?.seasonId,
        append ? (leaderboard?.nextCursor ?? undefined) : undefined
      );
      leaderboard =
        append && leaderboard
          ? { ...next, entries: [...leaderboard.entries, ...next.entries] }
          : next;
    } catch (caught) {
      leaderboardError =
        caught instanceof Error ? caught.message : '시즌 순위표를 불러오지 못했습니다.';
    } finally {
      leaderboardLoading = false;
      leaderboardLoadingMore = false;
    }
  }
  async function updateLeaderboardVisibility() {
    if (!leaderboard) return;
    leaderboardVisibilitySaving = true;
    leaderboardError = '';
    try {
      const preference = await api.setRankedLeaderboardVisibility(!leaderboard.viewerVisible);
      leaderboard = { ...leaderboard, viewerVisible: preference.visible };
      await loadLeaderboard(leaderboard.seasonId);
    } catch (caught) {
      leaderboardError =
        caught instanceof Error ? caught.message : '순위표 공개 설정을 저장하지 못했습니다.';
    } finally {
      leaderboardVisibilitySaving = false;
    }
  }
  let wins = $derived(games.filter(won).length);
  let averageAccuracy = $derived(
    games.length
      ? Math.round(
          (games.reduce(
            (total, game) =>
              total +
              (game.result.players.find((player) => player.playerId === game.selfPlayerId)
                ?.accuracy ?? 0),
            0
          ) /
            games.length) *
            100
        )
      : 0
  );
  let totalSunk = $derived(
    games.reduce(
      (total, game) =>
        total +
        (game.result.players.find((player) => player.playerId === game.selfPlayerId)?.shipsSunk ??
          0),
      0
    )
  );
</script>

<svelte:head><title>전투 기록 · Mk.01</title></svelte:head>
<div class="stats-page shell">
  <header>
    <div>
      <p class="eyebrow">OPERATION ARCHIVE / AFTER ACTION DATABASE</p>
      <h1 class="page-title">전투 기록</h1>
      <p>완료된 교전 결과와 명중 통계를 확인합니다.</p>
    </div>
    <span><Activity size={14} /> ARCHIVE SYNCHRONIZED</span>
  </header>
  {#if progression}
    {@const ranked = progression.ranked}
    <section class="season-brief panel" aria-label="현재 시즌 및 이벤트">
      <div class="season-emblem"><Flag size={24} /></div>
      <div class="season-copy">
        <small>LIVE CONTENT · REVISION {progression.liveContent.revision}</small>
        <h2>
          {ranked ? `${ranked.tier} ${ranked.rating} RP` : progression.liveContent.season.title}
        </h2>
        <p>{progression.liveContent.season.description}</p>
        <span
          ><CalendarRange size={13} />
          {contentDate(progression.liveContent.season.startsAt)} –
          {contentDate(progression.liveContent.season.endsAt)} · {contentStatus(
            progression.liveContent.season.status
          )}</span
        >
      </div>
      {#if progression.liveContent.events.length}
        <ul class="event-list" aria-label="진행 및 예정 이벤트">
          {#each progression.liveContent.events as event (event.id)}
            <li>
              <span>{contentStatus(event.status)}</span>
              <div><strong>{event.title}</strong><small>{event.description}</small></div>
              <time datetime={event.endsAt}>{contentDate(event.endsAt)}까지</time>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
    {#if hasAccount}
      <section class="leaderboard panel" aria-labelledby="ranked-leaderboard-title">
        <div class="leaderboard-heading">
          <div class="leaderboard-title">
            <span><Trophy size={17} /></span>
            <div>
              <small>SERVER-AUTHORITATIVE RANKING</small>
              <h2 id="ranked-leaderboard-title">시즌 지휘관 순위</h2>
            </div>
          </div>
          {#if leaderboard}
            <div class="leaderboard-controls">
              <label>
                <span class="sr-only">조회할 랭크 시즌</span>
                <select
                  value={leaderboard.seasonId}
                  aria-label="조회할 랭크 시즌"
                  onchange={(event) => loadLeaderboard(event.currentTarget.value)}
                >
                  {#each leaderboard.availableSeasons as season (season.seasonId)}
                    <option value={season.seasonId}
                      >{season.seasonId}{season.archived ? ' · ARCHIVE' : ' · LIVE'}</option
                    >
                  {/each}
                </select>
              </label>
              <button
                class:private={!leaderboard.viewerVisible}
                class="visibility-toggle"
                type="button"
                aria-pressed={!leaderboard.viewerVisible}
                disabled={leaderboardVisibilitySaving}
                onclick={updateLeaderboardVisibility}
              >
                {leaderboard.viewerVisible ? '공개 중' : '비공개'}
              </button>
            </div>
          {/if}
        </div>
        {#if leaderboardLoading}
          <div class="leaderboard-state" role="status"><div class="spinner"></div></div>
        {:else if leaderboard}
          <div class="leaderboard-meta">
            <span class:archive={leaderboard.archived}
              >{leaderboard.archived ? 'FINAL ARCHIVE' : '5 MIN SNAPSHOT'}</span
            >
            <time datetime={leaderboard.generatedAt}
              >{new Date(leaderboard.generatedAt).toLocaleString('ko-KR')} 생성</time
            >
            <p>
              완료된 배치 경기만 반영하며, 제재 중인 계정과 공개를 거부한 지휘관은 즉시 제외됩니다.
            </p>
          </div>
          {#if leaderboard.entries.length}
            <div class="leaderboard-table" role="table" aria-label={`${leaderboard.seasonId} 순위`}>
              <div class="leaderboard-row leaderboard-row--head" role="row">
                <span role="columnheader">순위</span><span role="columnheader">지휘관</span><span
                  role="columnheader">티어</span
                ><span role="columnheader">RP</span><span role="columnheader">전적</span>
              </div>
              {#each leaderboard.entries as entry (entry.rank)}
                <div class:podium={entry.rank <= 3} class="leaderboard-row" role="row">
                  <div role="cell"><strong>#{entry.rank}</strong></div>
                  <span class="leaderboard-handle" role="cell">{entry.handle}</span>
                  <span role="cell">{entry.tier}</span>
                  <div role="cell"><strong>{entry.rating.toLocaleString('ko-KR')}</strong></div>
                  <span role="cell">{entry.wins}승 {entry.losses}패</span>
                </div>
              {/each}
            </div>
            {#if leaderboard.nextCursor}
              <button
                class="leaderboard-more"
                type="button"
                disabled={leaderboardLoadingMore}
                onclick={() => loadLeaderboard(leaderboard?.seasonId, true)}
                >{leaderboardLoadingMore ? '스냅샷 확인 중…' : '다음 순위 불러오기'}</button
              >
            {/if}
          {:else}
            <div class="leaderboard-state">
              <Medal size={26} />
              <strong>공개 가능한 배치 완료 지휘관이 없습니다</strong>
              <span>5회의 배치 경기를 완료하면 시즌 순위에 참여할 수 있습니다.</span>
            </div>
          {/if}
        {/if}
        {#if leaderboardError}<p class="leaderboard-error" role="alert">{leaderboardError}</p>{/if}
      </section>
    {/if}
    <section class="progression panel" aria-label="지휘관 진행도">
      <div class="rank-seal"><Star size={26} /><strong>{progression.level}</strong></div>
      <div class="rank-copy">
        <small>COMMANDER PROGRESSION</small>
        <h2>LV.{progression.level} · {progression.rankTitle}</h2>
        <div
          class="xp-track"
          role="progressbar"
          aria-label="다음 레벨 진행도"
          aria-valuenow={progression.levelXp}
          aria-valuemin="0"
          aria-valuemax="500"
        >
          <span style={`width: ${Math.min(100, (progression.levelXp / 500) * 100)}%`}></span>
        </div>
        <p>
          총 {progression.totalXp.toLocaleString('ko-KR')} XP · {progression.xpToNextLevel > 0
            ? `다음 레벨까지 ${progression.xpToNextLevel} XP`
            : '최고 계급 달성'}
        </p>
      </div>
      <div class="progression-metric">
        <Award size={18} /><strong
          >{progression.achievements.filter((item) => item.unlocked).length}/{progression
            .achievements.length}</strong
        ><span>업적 해제</span>
      </div>
      <div class="progression-metric">
        <CalendarCheck size={18} /><strong
          >{progression.missions.filter((item) => item.completed).length}/{progression.missions
            .length}</strong
        ><span>현재 임무</span>
      </div>
    </section>
    {#if progression.missions.length}
      <section class="mission-grid" aria-label="현재 임무">
        {#each progression.missions as mission (mission.id)}
          <article class:complete={mission.completed} class="panel mission-card">
            <div><small>{mission.cadence}</small><strong>{mission.title}</strong></div>
            <span>{Math.min(mission.progress, mission.target)} / {mission.target}</span>
            <p>{mission.description}</p>
            <div class="mission-track">
              <i style={`width: ${Math.min(100, (mission.progress / mission.target) * 100)}%`}></i>
            </div>
            {#if mission.claimed}
              <em class="reward-claimed">지급 완료 · +{mission.rewardXp} XP</em>
            {:else if mission.claimable}
              <button
                class="claim-button"
                disabled={claimingMission === mission.id}
                onclick={() => claimMission(mission.id)}
                >{claimingMission === mission.id
                  ? '지급 중…'
                  : `+${mission.rewardXp} XP 받기`}</button
              >
            {:else}
              <em>완료 보상 · +{mission.rewardXp} XP</em>
            {/if}
          </article>
        {/each}
      </section>
    {:else}
      <p class="content-paused panel" role="status">
        현재 임무 운영이 일시 중지되었습니다. 완료 기록과 지급된 보상은 그대로 보존됩니다.
      </p>
    {/if}
    {#if progressionError}<p class="progression-error" role="alert">{progressionError}</p>{/if}
  {/if}
  {#if !loading && games.length > 0}
    <section class="archive-overview" aria-label="전투 기록 요약">
      <article class="panel">
        <small>TOTAL OPERATIONS</small><strong>{games.length}</strong><span>누적 작전</span>
      </article>
      <article class="panel">
        <small>MISSION SUCCESS</small><strong>{Math.round((wins / games.length) * 100)}%</strong
        ><span>{wins}회 승리</span>
      </article>
      <article class="panel">
        <small>AVERAGE ACCURACY</small><strong>{averageAccuracy}%</strong><span>평균 명중률</span>
      </article>
      <article class="panel">
        <small>HOSTILES NEUTRALIZED</small><strong>{totalSunk}</strong><span>누적 격침</span>
      </article>
    </section>
  {/if}
  {#if loading}<div class="empty-state">
      <div class="spinner"></div>
    </div>{:else if games.length === 0}<section class="empty-state panel">
      <div>
        <History size={34} class="muted" />
        <h2>아직 완료된 전투가 없습니다</h2>
        <p class="muted">첫 작전을 완료하면 기록이 이곳에 보존됩니다.</p>
        <a class="button button--primary" href={resolve('/lobby')}>작전 로비로 이동</a>
      </div>
    </section>{:else}<div class="history-list">
      {#each games as game (game.roomId)}<article class="history-row panel">
          <span class:loss={!won(game)} class="result-mark"
            >{#if won(game)}<Trophy size={20} />{:else}<Medal size={20} />{/if}</span
          >
          <div class="history-name">
            <small>{new Date(game.result.finishedAt).toLocaleDateString('ko-KR')}</small><strong
              >{game.roomName}</strong
            ><em>{won(game) ? '승리' : '패배'}</em><span class="win-type">{winTypeLabel(game)}</span
            >
          </div>
          <div>
            <Target size={14} /><span>명중률</span><strong
              >{Math.round(
                (game.result.players.find((p) => p.playerId === game.selfPlayerId)?.accuracy ?? 0) *
                  100
              )}%</strong
            >
          </div>
          <div>
            <Crosshair size={14} /><span>총 턴</span><strong>{game.result.totalTurns}</strong>
          </div>
          <div>
            <Timer size={14} /><span>시간</span><strong
              >{duration(game.result.durationSeconds)}</strong
            >
          </div>
          <a
            class="replay-link"
            aria-label={`${game.roomName} 전투 복기`}
            href={resolve('/replay/[roomId]', { roomId: game.roomId })}><Play size={14} /> 복기</a
          >
        </article>{/each}
    </div>{/if}
</div>

<style>
  .stats-page {
    padding: 64px 0 100px;
  }
  .stats-page header {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 20px;
    margin-bottom: 28px;
  }
  .stats-page header h1 {
    margin-bottom: 7px;
  }
  .stats-page header > div > p:last-child {
    color: var(--steel-300);
  }
  .stats-page header > span {
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--green-400);
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.13em;
  }
  .archive-overview {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 10px;
    margin-bottom: 24px;
  }
  .season-brief {
    display: grid;
    grid-template-columns: auto minmax(260px, 1fr) minmax(280px, 0.8fr);
    align-items: center;
    gap: 20px;
    margin-bottom: 12px;
    padding: 20px;
    border-color: rgba(255, 180, 60, 0.26);
    background:
      linear-gradient(110deg, rgba(45, 34, 21, 0.86), rgba(7, 24, 34, 0.96)), var(--surface-raised);
  }
  .season-emblem {
    display: grid;
    width: 54px;
    height: 54px;
    place-items: center;
    border: 1px solid rgba(255, 180, 60, 0.45);
    border-radius: 50%;
    color: var(--amber-400);
    background: rgba(255, 180, 60, 0.07);
  }
  .season-copy small {
    color: var(--amber-400);
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.14em;
  }
  .season-copy h2 {
    margin: 4px 0;
    font-family: var(--font-display);
    font-size: 16px;
  }
  .season-copy p {
    margin: 0;
    color: var(--ink-300);
    font-size: 10px;
  }
  .season-copy > span {
    display: flex;
    align-items: center;
    gap: 5px;
    margin-top: 8px;
    color: var(--ink-500);
    font-size: 8px;
  }
  .event-list {
    display: grid;
    gap: 7px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .event-list li {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 9px;
    padding: 9px;
    border: 1px solid rgba(255, 180, 60, 0.14);
    border-radius: 7px;
    background: rgba(0, 0, 0, 0.14);
  }
  .event-list li > span {
    padding: 3px 5px;
    border-radius: 999px;
    color: var(--amber-300);
    background: rgba(255, 180, 60, 0.09);
    font-size: 7px;
  }
  .event-list div {
    display: grid;
    gap: 2px;
  }
  .event-list strong {
    font-size: 10px;
  }
  .event-list small,
  .event-list time {
    color: var(--ink-500);
    font-size: 7px;
  }
  .leaderboard {
    margin-bottom: 12px;
    padding: 20px;
    overflow: hidden;
  }
  .leaderboard-heading,
  .leaderboard-title,
  .leaderboard-controls,
  .leaderboard-meta {
    display: flex;
    align-items: center;
  }
  .leaderboard-heading {
    justify-content: space-between;
    gap: 18px;
    margin-bottom: 15px;
  }
  .leaderboard-title {
    gap: 10px;
  }
  .leaderboard-title > span {
    display: grid;
    width: 36px;
    height: 36px;
    place-items: center;
    border: 1px solid rgba(40, 223, 232, 0.24);
    border-radius: 50%;
    color: var(--cyan-300);
    background: rgba(40, 223, 232, 0.06);
  }
  .leaderboard-title small {
    color: var(--cyan-400);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.12em;
  }
  .leaderboard-title h2 {
    margin: 2px 0 0;
    font-family: var(--font-display);
    font-size: 15px;
  }
  .leaderboard-controls {
    gap: 8px;
  }
  .leaderboard-controls select,
  .visibility-toggle,
  .leaderboard-more {
    min-height: 36px;
    border: 1px solid var(--line-strong);
    border-radius: 6px;
    color: var(--ink-200);
    background: rgba(4, 17, 24, 0.78);
    font: inherit;
    font-size: 9px;
  }
  .leaderboard-controls select {
    padding: 0 30px 0 10px;
  }
  .visibility-toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 0 11px;
    color: var(--green-400);
    cursor: pointer;
  }
  .visibility-toggle.private {
    color: var(--amber-300);
  }
  .visibility-toggle:disabled,
  .leaderboard-more:disabled {
    cursor: wait;
    opacity: 0.55;
  }
  .leaderboard-meta {
    flex-wrap: wrap;
    gap: 7px 12px;
    margin-bottom: 10px;
    color: var(--ink-500);
    font-size: 8px;
  }
  .leaderboard-meta > span {
    padding: 3px 6px;
    border-radius: 999px;
    color: var(--green-400);
    background: rgba(53, 215, 147, 0.08);
    font-family: var(--font-display);
    letter-spacing: 0.08em;
  }
  .leaderboard-meta > span.archive {
    color: var(--amber-300);
    background: rgba(255, 180, 60, 0.08);
  }
  .leaderboard-meta p {
    flex-basis: 100%;
    margin: 0;
  }
  .leaderboard-table {
    border: 1px solid var(--line);
    border-radius: 7px;
    overflow: hidden;
  }
  .leaderboard-row {
    display: grid;
    grid-template-columns: 62px minmax(150px, 1.5fr) minmax(90px, 0.8fr) 78px minmax(90px, 0.7fr);
    align-items: center;
    min-height: 42px;
    padding: 0 12px;
    border-bottom: 1px solid rgba(110, 169, 190, 0.09);
    color: var(--ink-400);
    font-size: 9px;
  }
  .leaderboard-row:last-child {
    border-bottom: 0;
  }
  .leaderboard-row--head {
    min-height: 31px;
    color: var(--ink-600);
    background: rgba(38, 94, 112, 0.08);
    font-size: 7px;
    letter-spacing: 0.08em;
  }
  .leaderboard-row.podium {
    background: linear-gradient(90deg, rgba(255, 180, 60, 0.06), transparent 44%);
  }
  .leaderboard-row [role='cell'] > strong {
    color: var(--amber-300);
    font-family: var(--font-display);
  }
  .leaderboard-handle {
    color: var(--ink-100);
    font-weight: 700;
  }
  .leaderboard-state {
    display: grid;
    min-height: 108px;
    place-items: center;
    align-content: center;
    gap: 6px;
    color: var(--ink-500);
    text-align: center;
  }
  .leaderboard-state strong {
    color: var(--ink-300);
    font-size: 10px;
  }
  .leaderboard-state span {
    font-size: 8px;
  }
  .leaderboard-more {
    width: 100%;
    margin-top: 9px;
    cursor: pointer;
  }
  .leaderboard-error {
    margin: 10px 0 0;
    color: var(--danger-400);
    font-size: 9px;
  }
  .content-paused {
    margin: 0 0 24px;
    padding: 14px;
    color: var(--amber-300);
    font-size: 9px;
  }
  .progression {
    display: grid;
    grid-template-columns: auto minmax(260px, 1fr) auto auto;
    align-items: center;
    gap: 22px;
    margin-bottom: 12px;
    padding: 20px;
    background: linear-gradient(110deg, rgba(15, 48, 63, 0.96), rgba(7, 24, 34, 0.96));
  }
  .rank-seal {
    display: grid;
    width: 64px;
    height: 64px;
    place-items: center;
    color: var(--cyan-300);
    border: 1px solid var(--cyan-600);
    border-radius: 50%;
    box-shadow: inset 0 0 24px rgba(40, 223, 232, 0.1);
  }
  .rank-seal strong {
    margin-top: -9px;
    font-family: var(--font-display);
    font-size: 13px;
  }
  .rank-copy small,
  .mission-card small {
    color: var(--cyan-500);
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.15em;
  }
  .rank-copy h2 {
    margin: 4px 0 8px;
    font-family: var(--font-display);
    font-size: 16px;
  }
  .rank-copy p {
    margin: 7px 0 0;
    color: var(--ink-400);
    font-size: 9px;
  }
  .xp-track,
  .mission-track {
    height: 4px;
    overflow: hidden;
    border-radius: 99px;
    background: rgba(255, 255, 255, 0.08);
  }
  .xp-track span,
  .mission-track i {
    display: block;
    height: 100%;
    background: linear-gradient(90deg, var(--cyan-600), var(--cyan-300));
    box-shadow: 0 0 8px rgba(40, 223, 232, 0.5);
  }
  .progression-metric {
    display: grid;
    min-width: 86px;
    place-items: center;
    color: var(--cyan-400);
  }
  .progression-metric strong {
    margin-top: 4px;
    color: var(--ink-100);
    font-family: var(--font-display);
  }
  .progression-metric span {
    color: var(--ink-500);
    font-size: 8px;
  }
  .mission-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
    margin-bottom: 24px;
  }
  .mission-card {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 7px 14px;
    padding: 15px;
  }
  .mission-card.complete {
    border-color: rgba(64, 219, 145, 0.35);
  }
  .mission-card strong {
    display: block;
    margin-top: 3px;
    font-size: 12px;
  }
  .mission-card > span {
    color: var(--cyan-300);
    font-family: var(--font-display);
    font-size: 10px;
  }
  .mission-card p,
  .mission-card em {
    grid-column: 1 / -1;
    margin: 0;
    color: var(--ink-400);
    font-size: 9px;
    font-style: normal;
  }
  .mission-card em {
    color: var(--green-400);
  }
  .mission-card .reward-claimed {
    color: var(--cyan-300);
  }
  .claim-button {
    grid-column: 1 / -1;
    width: fit-content;
    padding: 7px 10px;
    border: 1px solid var(--cyan-600);
    border-radius: 5px;
    color: var(--cyan-200);
    background: rgba(40, 223, 232, 0.08);
    font-family: var(--font-display);
    font-size: 9px;
    cursor: pointer;
  }
  .claim-button:hover:not(:disabled),
  .claim-button:focus-visible {
    border-color: var(--cyan-300);
  }
  .claim-button:disabled {
    cursor: wait;
    opacity: 0.6;
  }
  .progression-error {
    margin: -14px 0 20px;
    color: var(--red-400);
    font-size: 10px;
  }
  .mission-track {
    grid-column: 1 / -1;
  }
  .archive-overview article {
    position: relative;
    display: grid;
    gap: 4px;
    min-height: 122px;
    padding: 18px;
    overflow: hidden;
  }
  .archive-overview article::after {
    position: absolute;
    right: -25px;
    bottom: -55px;
    width: 100px;
    height: 100px;
    content: '';
    border: 1px solid rgba(40, 223, 232, 0.11);
    border-radius: 50%;
    box-shadow: 0 0 30px rgba(40, 223, 232, 0.05);
  }
  .archive-overview small {
    color: var(--ink-500);
    font-family: var(--font-display);
    font-size: 8px;
    letter-spacing: 0.14em;
  }
  .archive-overview strong {
    color: var(--cyan-300);
    font-family: var(--font-display);
    font-size: 31px;
  }
  .archive-overview span {
    color: var(--ink-400);
    font-size: 9px;
  }
  .history-list {
    display: grid;
    gap: 10px;
  }
  .history-row {
    display: grid;
    grid-template-columns: auto 1fr repeat(3, 120px) auto;
    align-items: center;
    gap: 18px;
    padding: 18px;
    border-radius: 13px;
    transition: 250ms var(--ease-out);
  }
  .history-row:hover {
    border-color: var(--line-strong);
    transform: translateX(3px);
  }
  .replay-link {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: 5px;
    color: var(--cyan-300);
    font-family: var(--font-display);
    font-size: 9px;
    letter-spacing: 0.08em;
  }
  .replay-link:hover,
  .replay-link:focus-visible {
    border-color: var(--cyan-300);
    background: rgba(40, 223, 232, 0.07);
  }
  .result-mark {
    display: grid;
    width: 44px;
    height: 44px;
    place-items: center;
    border: 1px solid rgba(255, 180, 60, 0.3);
    border-radius: 50%;
    color: var(--amber-500);
    background: rgba(255, 180, 60, 0.07);
  }
  .result-mark.loss {
    border-color: rgba(255, 83, 100, 0.28);
    color: var(--red-500);
    background: rgba(255, 83, 100, 0.06);
  }
  .history-name {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 3px 8px;
  }
  .history-name small {
    color: #688696;
    font-size: 9px;
  }
  .history-name strong {
    grid-column: 1/2;
    font-size: 13px;
  }
  .history-name em {
    grid-row: 2;
    grid-column: 2;
    color: var(--cyan-400);
    font-size: 10px;
    font-style: normal;
  }
  .history-name .win-type {
    grid-column: 1 / -1;
    width: fit-content;
    padding: 3px 6px;
    border: 1px solid rgba(40, 223, 232, 0.14);
    border-radius: 999px;
    color: var(--ink-400);
    background: rgba(40, 223, 232, 0.04);
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.08em;
  }
  .history-row > div:not(.history-name) {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: 2px 5px;
    color: #7190a0;
  }
  .history-row > div:not(.history-name) span {
    font-size: 9px;
  }
  .history-row > div:not(.history-name) strong {
    grid-column: 2;
    color: #d8e8ef;
    font-family: Rajdhani;
  }
  @media (max-width: 760px) {
    .stats-page {
      padding-top: 40px;
    }
    .history-row {
      grid-template-columns: auto 1fr repeat(3, 1fr) auto;
      gap: 11px;
    }
    .archive-overview {
      grid-template-columns: 1fr 1fr;
    }
    .progression {
      grid-template-columns: auto 1fr;
      gap: 14px;
    }
    .season-brief {
      grid-template-columns: auto 1fr;
    }
    .leaderboard-heading {
      align-items: stretch;
      flex-direction: column;
    }
    .leaderboard-controls,
    .leaderboard-controls label,
    .leaderboard-controls select,
    .visibility-toggle {
      flex: 1;
    }
    .leaderboard-controls select,
    .visibility-toggle {
      width: 100%;
      min-height: 44px;
    }
    .leaderboard-row {
      grid-template-columns: 42px minmax(110px, 1fr) 72px 58px;
      padding: 0 8px;
    }
    .leaderboard-row > :last-child {
      display: none;
    }
    .event-list {
      grid-column: 1 / -1;
    }
    .progression-metric {
      min-width: 0;
      padding-top: 10px;
      border-top: 1px solid var(--line);
    }
    .mission-grid {
      grid-template-columns: 1fr;
    }
    .stats-page header > span {
      display: none;
    }
    .history-row > div:not(.history-name) {
      grid-row: 2;
    }
    .result-mark {
      grid-row: 1;
    }
    .history-name {
      grid-column: 2/6;
    }
    .history-row > div:not(.history-name) :global(svg) {
      display: none;
    }
    .history-row > div:not(.history-name) strong {
      grid-column: 1;
    }
    .replay-link {
      grid-row: 1;
      grid-column: 6;
    }
  }
</style>
