<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/state';
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import { ArrowLeft, Clock3, Eye, Radio, ShieldCheck } from '@lucide/svelte';
  import { api } from '$lib/api';
  import GridBoard from '$lib/components/GridBoard.svelte';
  import { formatDateTime, formatNumber, localizeError, t, type MessageKey } from '$lib/i18n';
  import { Badge, Button, Skeleton, Surface } from '$lib/ui';
  import type {
    AttackRecord,
    SpectatorPhase,
    SpectatorSnapshot,
    TacticalSkillKind,
    TargetBoardSnapshot
  } from '$lib/types';

  type SpectatorAttack = AttackRecord & { skill?: TacticalSkillKind };

  let snapshot = $state<SpectatorSnapshot | null>(null);
  let error = $state('');
  let loading = $state(true);

  onMount(() => {
    let active = true;
    let timer: ReturnType<typeof setInterval>;
    const load = async () => {
      try {
        const next = await api.spectate(page.params.roomId!);
        if (!active) return;
        snapshot = next;
        error = '';
      } catch (caught) {
        if (!active) return;
        error = localizeError(caught, 'spectator.loadError');
      } finally {
        if (active) loading = false;
      }
    };
    void load();
    timer = setInterval(load, 2_500);
    return () => {
      active = false;
      clearInterval(timer);
    };
  });

  let attacks = $derived.by(() =>
    (snapshot?.timeline ?? []).flatMap((event): SpectatorAttack[] => {
      if (event.type === 'TURN_EXPIRED') return [];
      if (event.type === 'ATTACK') return [event.payload];
      return event.payload.cells.map((cell, index) => ({
        requestId: `${event.payload.requestId}:${index}`,
        attackerId: event.payload.attackerId,
        targetId: event.payload.targetId,
        coordinate: cell.coordinate,
        outcome: cell.outcome,
        sunkShip: cell.sunkShip,
        turnNumber: event.payload.turnNumber,
        nextPlayerId: event.payload.nextPlayerId,
        winnerId: index === event.payload.cells.length - 1 ? event.payload.winnerId : null,
        shotsRemainingInTurn: event.payload.shotsRemainingInTurn,
        resolvedVersion: event.payload.resolvedVersion,
        createdAt: event.payload.createdAt,
        skill: event.payload.skill
      }));
    })
  );
  let recentAttacks = $derived(attacks.slice(-8).reverse());
  let winner = $derived(
    snapshot?.players.find((player) => player.id === snapshot?.result?.winnerId) ?? null
  );

  function targetBoard(playerId: string): TargetBoardSnapshot {
    return {
      attacks: attacks
        .filter((attack) => attack.targetId === playerId)
        .map((attack) => ({
          coordinate: attack.coordinate,
          outcome: attack.outcome,
          sunkShip: attack.sunkShip
        }))
    };
  }

  function phaseKey(phase: SpectatorPhase): MessageKey {
    return `spectator.phase.${phase}` as MessageKey;
  }
</script>

<svelte:head><title>{$t('spectator.metaTitle')}</title></svelte:head>

<main class="spectator-page">
  <div class="spectator-shell">
    <header class="spectator-header">
      <Button variant="secondary" size="sm" onclick={() => goto(resolve('/lobby'))}>
        <ArrowLeft size={16} />
        {$t('spectator.backToLobby')}
      </Button>
      <div class="spectator-heading">
        <div class="spectator-heading__signal"><Eye size={19} /></div>
        <div>
          <p class="eyebrow">{$t('spectator.eyebrow')}</p>
          <h1>{snapshot?.room.name ?? $t('spectator.title')}</h1>
          <p>{$t('spectator.subtitle')}</p>
        </div>
      </div>
    </header>

    {#if loading}
      <div class="spectator-loading" aria-label={$t('spectator.loading')}>
        <Skeleton height="120px" />
        <Skeleton height="460px" />
      </div>
    {:else if error || !snapshot}
      <Surface tone="elevated" padding="lg" class="spectator-error">
        <Radio size={28} />
        <h2>{$t('spectator.unavailable')}</h2>
        <p role="alert">{error || $t('spectator.loadError')}</p>
        <Button variant="outline" onclick={() => goto(resolve('/lobby'))}>
          {$t('spectator.return')}
        </Button>
      </Surface>
    {:else}
      <section class="delay-banner" aria-live="polite">
        <div>
          <Badge tone={snapshot.phase === 'FINISHED' ? 'neutral' : 'warning'}>
            {$t(phaseKey(snapshot.phase))}
          </Badge>
          <strong>
            {$t('spectator.delayNotice', { seconds: formatNumber(snapshot.delaySeconds) })}
          </strong>
        </div>
        <span
          ><Clock3 size={15} />
          {$t('spectator.visibleThrough', {
            time: formatDateTime(snapshot.visibleThrough, { timeStyle: 'medium' })
          })}</span
        >
      </section>

      <section class="spectator-stage" aria-label={$t('spectator.boardsLabel')}>
        {#each snapshot.players as player (player.id)}
          <Surface tone="elevated" padding="md" class="spectator-board">
            <div class="board-heading">
              <div>
                <small>{$t('spectator.commandGrid')}</small>
                <h2>{player.nickname}</h2>
              </div>
              {#if snapshot.currentPlayerId === player.id}
                <Badge tone="success" pulse>{$t('spectator.activeTurn')}</Badge>
              {:else if snapshot.result?.winnerId === player.id}
                <Badge tone="success">{$t('spectator.winner')}</Badge>
              {/if}
            </div>
            <GridBoard
              balance={snapshot.balance.manifest}
              mode="target"
              label={$t('spectator.playerBoardLabel', { player: player.nickname })}
              targetBoard={targetBoard(player.id)}
            />
          </Surface>
        {/each}
      </section>

      <section class="spectator-telemetry">
        <Surface tone="quiet" padding="md">
          <div class="telemetry-heading">
            <div>
              <small>{$t('spectator.authoritativeFeed')}</small>
              <h2>{$t('spectator.recentActions')}</h2>
            </div>
            <span><ShieldCheck size={15} /> {$t('spectator.filtered')}</span>
          </div>
          {#if snapshot.phase === 'DELAYED'}
            <p class="telemetry-empty">{$t('spectator.buffering')}</p>
          {:else if recentAttacks.length === 0}
            <p class="telemetry-empty">{$t('spectator.noActions')}</p>
          {:else}
            <ol class="attack-feed">
              {#each recentAttacks as attack (attack.requestId)}
                <li>
                  <span>{$t('spectator.turn', { turn: formatNumber(attack.turnNumber) })}</span>
                  <strong>
                    {snapshot.players.find((player) => player.id === attack.attackerId)?.nickname}
                  </strong>
                  <code
                    >{String.fromCharCode(65 + attack.coordinate.row)}{attack.coordinate.col +
                      1}</code
                  >
                  {#if attack.skill}<small>{$t(`tacticalSkill.${attack.skill}`)}</small>{/if}
                  <Badge
                    tone={attack.outcome === 'MISS'
                      ? 'neutral'
                      : attack.outcome === 'HIT'
                        ? 'warning'
                        : 'danger'}
                  >
                    {$t(
                      attack.outcome === 'MISS'
                        ? 'board.miss'
                        : attack.outcome === 'HIT'
                          ? 'board.hit'
                          : 'board.sunk'
                    )}
                  </Badge>
                </li>
              {/each}
            </ol>
          {/if}
        </Surface>

        {#if snapshot.phase === 'FINISHED' && winner}
          <Surface tone="elevated" padding="md" class="result-card">
            <small>{$t('spectator.declassifiedResult')}</small>
            <h2>{$t('spectator.victory', { player: winner.nickname })}</h2>
            <p>{$t('spectator.resultDelaySafe')}</p>
          </Surface>
        {/if}
      </section>
    {/if}
  </div>
</main>

<style>
  .spectator-page {
    min-height: calc(100vh - var(--shell-header-height, 0px));
    padding: clamp(1rem, 3vw, 2.5rem);
    background:
      radial-gradient(circle at 50% 0%, rgba(32, 218, 255, 0.09), transparent 38%),
      var(--color-bg-primary);
  }

  .spectator-shell {
    width: min(1320px, 100%);
    margin: 0 auto;
    display: grid;
    gap: 1.25rem;
  }

  .spectator-header {
    display: grid;
    gap: 1rem;
  }

  .spectator-heading {
    display: flex;
    align-items: center;
    gap: 1rem;
  }

  .spectator-heading__signal {
    width: 3rem;
    height: 3rem;
    display: grid;
    place-items: center;
    border: 1px solid rgba(66, 220, 255, 0.4);
    border-radius: 50%;
    color: var(--color-cyan-300);
    background: rgba(28, 194, 234, 0.1);
  }

  .spectator-heading h1,
  .board-heading h2,
  .telemetry-heading h2,
  :global(.result-card) h2 {
    margin: 0;
  }

  .spectator-heading p:last-child,
  :global(.result-card) p {
    margin: 0.35rem 0 0;
    color: var(--color-text-secondary);
  }

  .delay-banner {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: center;
    padding: 0.85rem 1rem;
    border: 1px solid rgba(245, 182, 68, 0.35);
    background: rgba(245, 182, 68, 0.08);
  }

  .delay-banner > div,
  .delay-banner > span,
  .telemetry-heading > span {
    display: flex;
    align-items: center;
    gap: 0.6rem;
  }

  .delay-banner > span,
  .telemetry-heading > span {
    color: var(--color-text-secondary);
    font-size: 0.78rem;
  }

  .spectator-stage {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 1rem;
  }

  .board-heading,
  .telemetry-heading {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    margin-bottom: 1rem;
  }

  .board-heading small,
  .telemetry-heading small,
  :global(.result-card) small {
    color: var(--color-cyan-300);
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  .spectator-telemetry {
    display: grid;
    grid-template-columns: minmax(0, 2fr) minmax(240px, 1fr);
    gap: 1rem;
  }

  .attack-feed {
    list-style: none;
    display: grid;
    gap: 0.5rem;
    padding: 0;
    margin: 0;
  }

  .attack-feed li {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto auto;
    gap: 0.75rem;
    align-items: center;
    padding: 0.65rem 0.75rem;
    border: 1px solid rgba(120, 160, 180, 0.15);
    background: rgba(5, 18, 28, 0.44);
  }

  .attack-feed span,
  .attack-feed code {
    color: var(--color-text-secondary);
    font-size: 0.78rem;
  }

  .telemetry-empty {
    color: var(--color-text-secondary);
    text-align: center;
    padding: 2rem 1rem;
  }

  .spectator-loading {
    display: grid;
    gap: 1rem;
  }

  :global(.spectator-error) {
    max-width: 560px;
    margin: 10vh auto;
    text-align: center;
  }

  @media (max-width: 900px) {
    .spectator-stage,
    .spectator-telemetry {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 620px) {
    .spectator-page {
      padding: 0.75rem;
    }

    .delay-banner,
    .board-heading,
    .telemetry-heading {
      align-items: flex-start;
      flex-direction: column;
    }

    .attack-feed li {
      grid-template-columns: auto 1fr auto;
    }

    .attack-feed li :global(.ui-badge) {
      grid-column: 2 / -1;
      justify-self: start;
    }
  }
</style>
