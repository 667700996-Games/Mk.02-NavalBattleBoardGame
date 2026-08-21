<script lang="ts">
  import { Eye } from '@lucide/svelte';
  import { formatNumber, formatRelativeTime, gameModeMessageKey, t } from '$lib/i18n';
  import type { RoomSummary } from '$lib/types';
  import { Badge, Button, Surface } from '$lib/ui';

  interface Props {
    rooms: RoomSummary[];
    delaySeconds: number;
    spectate: (roomId: string) => void | Promise<void>;
  }

  let { rooms, delaySeconds, spectate }: Props = $props();
  let activeRooms = $derived(rooms.filter((room) => room.status === 'PLAYING'));

  const age = (createdAt: string) => {
    const minutes = Math.max(0, Math.floor((Date.now() - new Date(createdAt).getTime()) / 60_000));
    return formatRelativeTime(-minutes, 'minute');
  };
</script>

{#if activeRooms.length > 0}
  <section class="room-section" aria-labelledby="spectatable-room-title">
    <div class="section-heading">
      <div>
        <p class="eyebrow">{$t('spectator.lobbyEyebrow')}</p>
        <h2 id="spectatable-room-title">{$t('spectator.lobbyTitle')}</h2>
        <p>
          {$t('spectator.lobbyDescription', {
            seconds: formatNumber(delaySeconds)
          })}
        </p>
      </div>
    </div>
    <div class="room-grid">
      {#each activeRooms as room (room.id)}
        <Surface tone="interactive" padding="md" class="room-card">
          <article>
            <div class="room-card__top">
              <Badge tone="warning">{$t('spectator.delayedLiveBadge')}</Badge>
              <span>{age(room.createdAt)}</span>
            </div>
            <div class="room-card__title">
              <small>{$t('spectator.operationFeed')}</small>
              <h3>{room.name}</h3>
            </div>
            <div class="room-card__meta">
              <span><Eye size={13} /> {$t('spectator.visibilityFiltered')}</span>
              <span>{$t(gameModeMessageKey(room.rules.mode))}</span>
            </div>
            <Button variant="outline" full onclick={() => spectate(room.id)}>
              <Eye size={15} />
              {$t('spectator.watch')}
            </Button>
          </article>
        </Surface>
      {/each}
    </div>
  </section>
{/if}
