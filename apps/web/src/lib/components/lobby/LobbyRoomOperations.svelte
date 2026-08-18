<script lang="ts">
  import { ArrowRight, KeyRound, LockKeyhole, Plus, Radio, RefreshCw } from '@lucide/svelte';
  import type { GameMode, RoomSummary, RoomVisibility } from '$lib/types';
  import { formatNumber, formatRelativeTime, gameModeMessageKey, t } from '$lib/i18n';
  import { Avatar, Badge, Button, Field, Modal, Skeleton, Surface } from '$lib/ui';

  interface Props {
    rooms: RoomSummary[];
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
  }

  let {
    rooms,
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
    joinRoom
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

<Modal
  open={openCreate}
  title={$t('lobbyRooms.createTitle')}
  eyebrow={$t('lobbyRooms.newOperation')}
  description={$t('lobbyRooms.createDescription')}
  onclose={() => (openCreate = false)}
>
  <form
    class="operation-form"
    onsubmit={(event) => {
      event.preventDefault();
      createRoom();
    }}
  >
    <Field
      id="room-name"
      label={$t('lobbyRooms.roomName')}
      bind:value={roomName}
      minlength={2}
      maxlength={32}
      required
    />
    <fieldset>
      <legend>{$t('lobbyRooms.visibility')}</legend>
      <div class="visibility-grid">
        <label class="choice"
          ><input type="radio" bind:group={visibility} value="PUBLIC" /><span
            ><Radio size={18} /><strong>{$t('lobbyRooms.public')}</strong><small
              >{$t('lobbyRooms.publicCode')}</small
            ><em>{$t('lobbyRooms.publicDescription')}</em></span
          ></label
        >
        <label class="choice"
          ><input type="radio" bind:group={visibility} value="PRIVATE" /><span
            ><LockKeyhole size={18} /><strong>{$t('lobbyRooms.private')}</strong><small
              >{$t('lobbyRooms.privateCode')}</small
            ><em>{$t('lobbyRooms.privateDescription')}</em></span
          ></label
        >
      </div>
    </fieldset>
    <fieldset>
      <legend>{$t('lobbyRooms.rules')}</legend>
      <div class="mode-grid">
        <label class="choice"
          ><input type="radio" bind:group={gameMode} value="CLASSIC" /><span
            ><strong>{$t('gameMode.CLASSIC')}</strong><small>{$t('gameMode.CLASSIC')}</small><em
              >{$t('lobbyRooms.classicDescription')}</em
            ></span
          ></label
        >
        <label class="choice"
          ><input type="radio" bind:group={gameMode} value="RAPID" /><span
            ><strong>{$t('gameMode.RAPID')}</strong><small>{$t('gameMode.RAPID')}</small><em
              >{$t('lobbyRooms.rapidDescription')}</em
            ></span
          ></label
        >
        <label class="choice"
          ><input type="radio" bind:group={gameMode} value="SALVO" /><span
            ><strong>{$t('gameMode.SALVO')}</strong><small>{$t('gameMode.SALVO')}</small><em
              >{$t('lobbyRooms.salvoDescription')}</em
            ></span
          ></label
        >
      </div>
      <label class="duration-choice" for="turn-duration">
        <span
          ><strong>{$t('lobbyRooms.turnLimit')}</strong><small
            >{$t('lobbyRooms.turnLimitCode')}</small
          ></span
        >
        <select id="turn-duration" bind:value={turnDurationSeconds} disabled={gameMode === 'RAPID'}>
          <option value={0}>{$t('lobbyRooms.noLimit')}</option>
          {#each [30, 45, 60, 90, 120] as seconds (seconds)}
            <option value={seconds}
              >{$t('lobbyRooms.seconds', { count: formatNumber(seconds) })}</option
            >
          {/each}
        </select>
      </label>
    </fieldset>
    <Button variant="primary" size="lg" type="submit" loading={submitting} full
      >{$t('lobbyRooms.create')} <ArrowRight size={17} /></Button
    >
  </form>
</Modal>

<Modal
  open={openJoin}
  title={$t('lobbyRooms.joinTitle')}
  eyebrow={$t('lobbyRooms.secureChannel')}
  description={$t('lobbyRooms.joinDescription')}
  onclose={() => (openJoin = false)}
>
  <form
    class="operation-form"
    onsubmit={(event) => {
      event.preventDefault();
      joinRoom();
    }}
  >
    <Field
      id="room-code"
      label={$t('lobbyRooms.operationCodeLabel')}
      bind:value={roomCode}
      minlength={6}
      maxlength={6}
      placeholder="ABC123"
      autocomplete="off"
      code
      required
    />
    <Button
      variant="primary"
      size="lg"
      type="submit"
      loading={submitting}
      disabled={roomCode.length !== 6}
      full><KeyRound size={17} /> {$t('lobbyRooms.connect')}</Button
    >
  </form>
</Modal>
