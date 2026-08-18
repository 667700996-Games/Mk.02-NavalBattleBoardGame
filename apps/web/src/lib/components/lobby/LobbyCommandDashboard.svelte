<script lang="ts">
  import { resolve } from '$app/paths';
  import { ArrowRight, History, Radio, ShieldCheck, X } from '@lucide/svelte';
  import type {
    AiDifficulty,
    MatchmakingPool,
    MatchmakingRegion,
    MatchmakingTicket
  } from '$lib/types';
  import { Badge, Button, Surface } from '$lib/ui';

  type SocketStatus = 'idle' | 'connecting' | 'online' | 'reconnecting' | 'offline';

  interface Props {
    matching: boolean;
    elapsed: number;
    matchPool: MatchmakingPool;
    rankedRegion: MatchmakingRegion;
    measuredLatency: number | null;
    matchmakingTicket: MatchmakingTicket | null;
    practicing: boolean;
    socketStatus: SocketStatus;
    toggleMatchmaking: () => void | Promise<void>;
    measureLatency: () => void | Promise<void>;
    startPractice: (difficulty: AiDifficulty) => void | Promise<void>;
  }

  let {
    matching,
    elapsed,
    matchPool = $bindable(),
    rankedRegion = $bindable(),
    measuredLatency,
    matchmakingTicket,
    practicing,
    socketStatus,
    toggleMatchmaking,
    measureLatency,
    startPractice
  }: Props = $props();
</script>

<section class="command-dashboard" aria-label="작전 현황">
  <Surface tone="elevated" padding="lg" class="quick-match">
    <div class="quick-match__radar" class:searching={matching}>
      <div class="quick-match__sweep"></div>
      <Radio size={32} /><span></span>
    </div>
    <div class="quick-match__copy">
      <Badge tone={matching ? 'warning' : 'cyan'} pulse={matching}
        >{matching ? 'SEARCHING SIGNALS' : 'QUICK DEPLOYMENT'}</Badge
      >
      <h2>
        {matching ? '상대 지휘관 탐색 중' : matchPool === 'RANKED' ? '랭크 교전' : '빠른 교전'}
      </h2>
      <p>
        {matching
          ? `${elapsed}초 경과 · ${matchmakingTicket?.searchWindow.phase ?? 'EXACT'} 범위에서 대기 중입니다.`
          : matchPool === 'RANKED'
            ? '레이팅·리전 RTT 기반 1:1 매칭입니다.'
            : '같은 신호를 기다리는 지휘관과 즉시 1:1 비공개 작전을 편성합니다.'}
      </p>
      <div class="matchmaking-profile" aria-label="매칭 조건">
        <div class="matchmaking-pool" role="group" aria-label="매칭 유형">
          <button
            type="button"
            class:active={matchPool === 'CASUAL'}
            aria-pressed={matchPool === 'CASUAL'}
            disabled={matching}
            onclick={() => (matchPool = 'CASUAL')}>일반</button
          >
          <button
            type="button"
            class:active={matchPool === 'RANKED'}
            aria-pressed={matchPool === 'RANKED'}
            disabled={matching}
            onclick={() => (matchPool = 'RANKED')}>랭크</button
          >
        </div>
        {#if matchPool === 'RANKED'}
          <label>
            <span>리전</span>
            <select bind:value={rankedRegion} disabled={matching} aria-label="랭크 매칭 리전">
              <option value="KOREA">한국</option>
              <option value="JAPAN">일본</option>
              <option value="SOUTHEAST_ASIA">동남아시아</option>
              <option value="NORTH_AMERICA_WEST">북미 서부</option>
              <option value="NORTH_AMERICA_EAST">북미 동부</option>
              <option value="EUROPE">유럽</option>
            </select>
          </label>
          <button class="latency-probe" type="button" disabled={matching} onclick={measureLatency}
            >{measuredLatency ? `${measuredLatency}ms 재측정` : 'RTT 측정'}</button
          >
        {/if}
      </div>
      <div class="matching-telemetry">
        <span><i></i> ENCRYPTED LINK</span><span>SOLO PARTY</span><span
          >{matchmakingTicket?.rating
            ? `RATING ${matchmakingTicket.rating}`
            : 'RANDOM INITIATIVE'}</span
        >
      </div>
    </div>
    <Button variant={matching ? 'danger' : 'primary'} size="lg" onclick={toggleMatchmaking}>
      {#if matching}<X size={17} /> 매칭 취소{:else}<Radio size={17} /> 상대 찾기{/if}
    </Button>
  </Surface>

  <div class="dashboard-side">
    <Surface tone="elevated" padding="md" class="practice-card">
      <div class="practice-heading">
        <span><strong aria-hidden="true">AI</strong></span>
        <div>
          <small>AI TACTICAL RANGE</small><strong>AI 연습 교전</strong>
          <p>서버 권위 AI와 난이도별 실전 훈련</p>
        </div>
      </div>
      <div class="practice-options" aria-label="AI 난이도 선택">
        <button disabled={practicing} onclick={() => startPractice('RECRUIT')}
          ><span>신병</span><small>RECRUIT</small></button
        >
        <button disabled={practicing} onclick={() => startPractice('OFFICER')}
          ><span>장교</span><small>OFFICER</small></button
        >
        <button disabled={practicing} onclick={() => startPractice('ADMIRAL')}
          ><span>제독</span><small>ADMIRAL</small></button
        >
      </div>
    </Surface>
    <Surface tone="interactive" padding="md">
      <a class="dashboard-action" href={resolve('/tutorial')}>
        <span><ShieldCheck size={19} /></span>
        <div>
          <small>COMMAND ACADEMY</small><strong>작전 튜토리얼</strong>
          <p>배치·공격·턴·재접속 훈련</p>
        </div>
        <ArrowRight size={16} />
      </a>
    </Surface>
    <Surface tone="interactive" padding="md">
      <a class="dashboard-action" href={resolve('/stats')}>
        <span><History size={19} /></span>
        <div>
          <small>OPERATION ARCHIVE</small><strong>전투 기록</strong>
          <p>완료한 교전과 명중 통계</p>
        </div>
        <ArrowRight size={16} />
      </a>
    </Surface>
    <Surface tone="quiet" padding="md">
      <div class="network-card">
        <ShieldCheck size={19} />
        <div>
          <small>TACTICAL NETWORK</small><strong
            >{socketStatus === 'online' ? '실시간 동기화 중' : '채널 준비 중'}</strong
          >
        </div>
        <Badge tone={socketStatus === 'online' ? 'success' : 'warning'}
          >{socketStatus.toUpperCase()}</Badge
        >
      </div>
    </Surface>
  </div>
</section>
