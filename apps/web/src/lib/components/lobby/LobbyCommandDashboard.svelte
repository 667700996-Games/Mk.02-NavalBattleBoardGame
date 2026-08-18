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
  import {
    formatNumber,
    matchPhaseMessageKey,
    regionMessageKey,
    t,
    type MessageKey
  } from '$lib/i18n';

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

  const socketStatusKeys: Record<SocketStatus, MessageKey> = {
    idle: 'dashboard.statusIdle',
    connecting: 'dashboard.statusConnecting',
    online: 'dashboard.statusOnline',
    reconnecting: 'dashboard.statusReconnecting',
    offline: 'dashboard.statusOffline'
  };
  const rankedRegions: ReadonlyArray<Exclude<MatchmakingRegion, 'AUTO'>> = [
    'KOREA',
    'JAPAN',
    'SOUTHEAST_ASIA',
    'NORTH_AMERICA_WEST',
    'NORTH_AMERICA_EAST',
    'EUROPE'
  ];
</script>

<section class="command-dashboard" aria-label={$t('dashboard.aria')}>
  <Surface tone="elevated" padding="lg" class="quick-match">
    <div class="quick-match__radar" class:searching={matching}>
      <div class="quick-match__sweep"></div>
      <Radio size={32} /><span></span>
    </div>
    <div class="quick-match__copy">
      <Badge tone={matching ? 'warning' : 'cyan'} pulse={matching}
        >{matching ? $t('dashboard.searching') : $t('dashboard.quickDeployment')}</Badge
      >
      <h2>
        {matching
          ? $t('dashboard.searchingTitle')
          : matchPool === 'RANKED'
            ? $t('dashboard.rankedTitle')
            : $t('dashboard.quickTitle')}
      </h2>
      <p>
        {matching
          ? $t('dashboard.searchDescription', {
              elapsed: formatNumber(elapsed),
              phase: $t(matchPhaseMessageKey(matchmakingTicket?.searchWindow.phase ?? 'EXACT'))
            })
          : matchPool === 'RANKED'
            ? $t('dashboard.rankedDescription')
            : $t('dashboard.quickDescription')}
      </p>
      <div class="matchmaking-profile" aria-label={$t('dashboard.conditions')}>
        <div class="matchmaking-pool" role="group" aria-label={$t('dashboard.type')}>
          <button
            type="button"
            class:active={matchPool === 'CASUAL'}
            aria-pressed={matchPool === 'CASUAL'}
            disabled={matching}
            onclick={() => (matchPool = 'CASUAL')}>{$t('dashboard.casual')}</button
          >
          <button
            type="button"
            class:active={matchPool === 'RANKED'}
            aria-pressed={matchPool === 'RANKED'}
            disabled={matching}
            onclick={() => (matchPool = 'RANKED')}>{$t('dashboard.ranked')}</button
          >
        </div>
        {#if matchPool === 'RANKED'}
          <label>
            <span>{$t('dashboard.region')}</span>
            <select
              bind:value={rankedRegion}
              disabled={matching}
              aria-label={$t('dashboard.rankedRegion')}
            >
              {#each rankedRegions as region (region)}
                <option value={region}>{$t(regionMessageKey(region))}</option>
              {/each}
            </select>
          </label>
          <button class="latency-probe" type="button" disabled={matching} onclick={measureLatency}
            >{measuredLatency !== null
              ? $t('dashboard.remeasureLatency', { latency: formatNumber(measuredLatency) })
              : $t('dashboard.measureLatency')}</button
          >
        {/if}
      </div>
      <div class="matching-telemetry">
        <span><i></i> {$t('dashboard.encryptedLink')}</span><span>{$t('dashboard.soloParty')}</span
        ><span
          >{matchmakingTicket?.rating
            ? $t('dashboard.rating', { rating: formatNumber(matchmakingTicket.rating) })
            : $t('dashboard.randomInitiative')}</span
        >
      </div>
    </div>
    <Button variant={matching ? 'danger' : 'primary'} size="lg" onclick={toggleMatchmaking}>
      {#if matching}<X size={17} /> {$t('dashboard.cancelMatch')}{:else}<Radio size={17} />
        {$t('dashboard.findOpponent')}{/if}
    </Button>
  </Surface>

  <div class="dashboard-side">
    <Surface tone="elevated" padding="md" class="practice-card">
      <div class="practice-heading">
        <span><strong aria-hidden="true">AI</strong></span>
        <div>
          <small>{$t('dashboard.aiRange')}</small><strong>{$t('dashboard.aiPractice')}</strong>
          <p>{$t('dashboard.aiDescription')}</p>
        </div>
      </div>
      <div class="practice-options" aria-label={$t('dashboard.aiDifficulty')}>
        <button disabled={practicing} onclick={() => startPractice('RECRUIT')}
          ><span>{$t('dashboard.recruit')}</span><small>{$t('dashboard.recruit')}</small></button
        >
        <button disabled={practicing} onclick={() => startPractice('OFFICER')}
          ><span>{$t('dashboard.officer')}</span><small>{$t('dashboard.officer')}</small></button
        >
        <button disabled={practicing} onclick={() => startPractice('ADMIRAL')}
          ><span>{$t('dashboard.admiral')}</span><small>{$t('dashboard.admiral')}</small></button
        >
      </div>
    </Surface>
    <Surface tone="interactive" padding="md">
      <a class="dashboard-action" href={resolve('/tutorial')}>
        <span><ShieldCheck size={19} /></span>
        <div>
          <small>{$t('dashboard.commandAcademy')}</small><strong>{$t('dashboard.tutorial')}</strong>
          <p>{$t('dashboard.tutorialDescription')}</p>
        </div>
        <ArrowRight size={16} />
      </a>
    </Surface>
    <Surface tone="interactive" padding="md">
      <a class="dashboard-action" href={resolve('/stats')}>
        <span><History size={19} /></span>
        <div>
          <small>{$t('dashboard.operationArchive')}</small><strong>{$t('dashboard.history')}</strong
          >
          <p>{$t('dashboard.historyDescription')}</p>
        </div>
        <ArrowRight size={16} />
      </a>
    </Surface>
    <Surface tone="quiet" padding="md">
      <div class="network-card">
        <ShieldCheck size={19} />
        <div>
          <small>{$t('dashboard.tacticalNetwork')}</small><strong
            >{socketStatus === 'online'
              ? $t('dashboard.synchronizing')
              : $t('dashboard.channelPreparing')}</strong
          >
        </div>
        <Badge tone={socketStatus === 'online' ? 'success' : 'warning'}
          >{$t(socketStatusKeys[socketStatus])}</Badge
        >
      </div>
    </Surface>
  </div>
</section>
