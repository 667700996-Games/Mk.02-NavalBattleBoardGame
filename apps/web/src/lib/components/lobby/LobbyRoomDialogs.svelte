<script lang="ts">
  import { ArrowRight, KeyRound, LockKeyhole, Radio } from '@lucide/svelte';
  import { formatNumber, t } from '$lib/i18n';
  import type { GameMode, RoomVisibility } from '$lib/types';
  import { Button, Field, Modal } from '$lib/ui';

  interface Props {
    openCreate: boolean;
    openJoin: boolean;
    roomName: string;
    visibility: RoomVisibility;
    gameMode: GameMode;
    turnDurationSeconds: number;
    roomCode: string;
    submitting: boolean;
    createRoom: () => void | Promise<void>;
    joinRoom: (code?: string) => void | Promise<void>;
  }

  let {
    openCreate = $bindable(),
    openJoin = $bindable(),
    roomName = $bindable(),
    visibility = $bindable(),
    gameMode = $bindable(),
    turnDurationSeconds = $bindable(),
    roomCode = $bindable(),
    submitting,
    createRoom,
    joinRoom
  }: Props = $props();
</script>

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
