<script lang="ts">
  import { Radio, X } from '@lucide/svelte';
  import type { MatchmakingPool, MatchmakingRegion, MatchmakingTicket } from '$lib/types';
  import { Badge, Button, Surface } from '$lib/ui';
  import { formatNumber, matchPhaseMessageKey, regionMessageKey, t } from '$lib/i18n';

  interface Props {
    matching: boolean;
    elapsed: number;
    matchPool: MatchmakingPool;
    rankedRegion: MatchmakingRegion;
    measuredLatency: number | null;
    matchmakingTicket: MatchmakingTicket | null;
    toggleMatchmaking: () => void | Promise<void>;
    measureLatency: () => void | Promise<void>;
  }

  let {
    matching,
    elapsed,
    matchPool = $bindable(),
    rankedRegion = $bindable(),
    measuredLatency,
    matchmakingTicket,
    toggleMatchmaking,
    measureLatency
  }: Props = $props();

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
</section>
