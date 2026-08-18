<script lang="ts">
  import { ArrowRight, KeyRound, Plus, Radio, RefreshCw } from '@lucide/svelte';
  import type { GameMode, RoomSummary, RoomVisibility } from '$lib/types';
  import { formatNumber, formatRelativeTime, gameModeMessageKey, t } from '$lib/i18n';
  import { Avatar, Badge, Button, Skeleton, Surface } from '$lib/ui';
  import LobbyRoomDialogs from './LobbyRoomDialogs.svelte';
  import SpectatableRooms from './SpectatableRooms.svelte';

  interface Props {
    rooms: RoomSummary[];
    spectatableRooms: RoomSummary[];
    spectatorDelaySeconds: number;
    loading: boolean;
    submitting: boolean;
    openCreate: boolean;
    openJoin: boolean;
    roomName: string;
    visibility: RoomVisibility;
    gameMode: GameMode;
    turnDurationSeconds: number;
    roomCode: string;
    loadRooms: () => void | Promise<void>;
    createRoom: () => void | Promise<void>;
    joinRoom: (code?: string) => void | Promise<void>;
    spectate: (roomId: string) => void | Promise<void>;
  }

  let {
    rooms,
    spectatableRooms,
    spectatorDelaySeconds,
    loading,
    submitting,
    openCreate = $bindable(),
    openJoin = $bindable(),
    roomName = $bindable(),
    visibility = $bindable(),
    gameMode = $bindable(),
    turnDurationSeconds = $bindable(),
    roomCode = $bindable(),
    loadRooms,
    createRoom,
    joinRoom,
    spectate
  }: Props = $props();

  const age = (createdAt: string) => {
    const minutes = Math.max(0, Math.floor((Date.now() - new Date(createdAt).getTime()) / 60_000));
    return formatRelativeTime(-minutes, 'minute');
  };
</script>

<section class="room-section" aria-labelledby="public-room-title">
  <div class="section-heading">
    <div>
      <p class="eyebrow">{$t('lobbyRooms.openChannels')}</p>
      <h2 id="public-room-title">{$t('lobbyRooms.title')}</h2>
      <p>
        {rooms.length
          ? $t('lobbyRooms.availableCount', { count: formatNumber(rooms.length) })
          : $t('lobbyRooms.scanning')}
      </p>
    </div>
    <Button variant="ghost" size="sm" onclick={loadRooms}
      ><RefreshCw size={15} /> {$t('lobbyRooms.scan')}</Button
    >
  </div>

  <div class="room-grid">
    {#if loading}
      {#each Array.from({ length: 3 }) as _, index (index)}
        <Surface tone="quiet" padding="md"
          ><div class="room-skeleton">
            <Skeleton width="46%" height="12px" /><Skeleton width="72%" height="22px" /><Skeleton
              height="74px"
            /><Skeleton width="100%" height="40px" />
          </div></Surface
        >
      {/each}
    {:else if rooms.length === 0}
      <Surface tone="quiet" padding="lg" class="rooms-empty">
        <div class="empty-radar"><Radio size={27} /></div>
        <h3>{$t('lobbyRooms.none')}</h3>
        <p>{$t('lobbyRooms.noneDescription')}</p>
        <Button variant="outline" onclick={() => (openCreate = true)}
          ><Plus size={15} /> {$t('lobbyRooms.firstChannel')}</Button
        >
      </Surface>
    {:else}
      {#each rooms as room (room.id)}
        <Surface tone="interactive" padding="md" class="room-card">
          <article>
            <div class="room-card__top">
              <Badge tone="success" pulse>{$t('lobbyRooms.open')}</Badge><span
                >{age(room.createdAt)}</span
              >
            </div>
            <div class="room-card__title">
              <small>{$t('lobbyRooms.operationCode', { code: room.code })}</small>
              <h3>{room.name}</h3>
            </div>
            <div class="room-card__crew">
              <Avatar name={$t('waiting.host')} status="online" />
              <div>
                <small>{$t('lobbyRooms.commandCrew')}</small><strong
                  >{$t('lobbyRooms.commanderCount', {
                    players: formatNumber(room.playerCount),
                    capacity: formatNumber(room.capacity)
                  })}</strong
                >
              </div>
              <div class="crew-slots">
                <i class="filled"></i><i class:filled={room.playerCount > 1}></i>
              </div>
            </div>
            <div class="room-card__meta">
              <span><Radio size={13} /> {$t('lobbyRooms.publicChannel')}</span><span
                ><KeyRound size={13} /> {room.code}</span
              ><span>{$t(gameModeMessageKey(room.rules.mode))}</span>
            </div>
            <Button
              variant="secondary"
              full
              onclick={() => joinRoom(room.code)}
              disabled={submitting}>{$t('lobbyRooms.join')} <ArrowRight size={15} /></Button
            >
          </article>
        </Surface>
      {/each}
    {/if}
  </div>
</section>

<SpectatableRooms rooms={spectatableRooms} delaySeconds={spectatorDelaySeconds} {spectate} />

<LobbyRoomDialogs
  bind:openCreate
  bind:openJoin
  bind:roomName
  bind:visibility
  bind:gameMode
  bind:turnDurationSeconds
  bind:roomCode
  {submitting}
  {createRoom}
  {joinRoom}
/>
