<script lang="ts">
  import { ArrowRight, KeyRound, LockKeyhole, Plus, Radio, RefreshCw } from '@lucide/svelte';
  import type { GameMode, RoomSummary, RoomVisibility } from '$lib/types';
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
    return minutes < 1 ? '방금 전' : `${minutes}분 전`;
  };
</script>

<section class="room-section" aria-labelledby="public-room-title">
  <div class="section-heading">
    <div>
      <p class="eyebrow">OPEN CHANNELS</p>
      <h2 id="public-room-title">공개 작전실</h2>
      <p>
        {rooms.length
          ? `${rooms.length}개 채널이 신규 지휘관을 기다리고 있습니다.`
          : 'SCANNING TACTICAL CHANNELS / 활성 작전 신호 대기 중'}
      </p>
    </div>
    <Button variant="ghost" size="sm" onclick={loadRooms}><RefreshCw size={15} /> 채널 스캔</Button>
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
        <h3>NO ACTIVE OPERATIONS DETECTED</h3>
        <p>전술 채널 스캔이 완료되었습니다. 첫 작전을 편성하거나 빠른 교전을 시작하십시오.</p>
        <Button variant="outline" onclick={() => (openCreate = true)}
          ><Plus size={15} /> 첫 채널 편성</Button
        >
      </Surface>
    {:else}
      {#each rooms as room (room.id)}
        <Surface tone="interactive" padding="md" class="room-card">
          <article>
            <div class="room-card__top">
              <Badge tone="success" pulse>OPEN</Badge><span>{age(room.createdAt)}</span>
            </div>
            <div class="room-card__title">
              <small>OPERATION / {room.code}</small>
              <h3>{room.name}</h3>
            </div>
            <div class="room-card__crew">
              <Avatar name="HOST" status="online" />
              <div>
                <small>COMMAND CREW</small><strong
                  >{room.playerCount} / {room.capacity} 지휘관</strong
                >
              </div>
              <div class="crew-slots">
                <i class="filled"></i><i class:filled={room.playerCount > 1}></i>
              </div>
            </div>
            <div class="room-card__meta">
              <span><Radio size={13} /> PUBLIC CHANNEL</span><span
                ><KeyRound size={13} /> {room.code}</span
              ><span>{room.rules.mode}</span>
            </div>
            <Button
              variant="secondary"
              full
              onclick={() => joinRoom(room.code)}
              disabled={submitting}>채널 참가 <ArrowRight size={15} /></Button
            >
          </article>
        </Surface>
      {/each}
    {/if}
  </div>
</section>

<Modal
  open={openCreate}
  title="새 작전실 편성"
  eyebrow="NEW OPERATION"
  description="작전 이름과 보안 범위를 지정하십시오. 편성 후 초대 코드가 즉시 발급됩니다."
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
      label="작전실 이름"
      bind:value={roomName}
      minlength={2}
      maxlength={32}
      required
    />
    <fieldset>
      <legend>공개 범위</legend>
      <div class="visibility-grid">
        <label class="choice"
          ><input type="radio" bind:group={visibility} value="PUBLIC" /><span
            ><Radio size={18} /><strong>공개</strong><small>OPEN CHANNEL</small><em
              >로비에서 누구나 참가</em
            ></span
          ></label
        >
        <label class="choice"
          ><input type="radio" bind:group={visibility} value="PRIVATE" /><span
            ><LockKeyhole size={18} /><strong>비공개</strong><small>SECURE CHANNEL</small><em
              >초대 링크와 코드로만 참가</em
            ></span
          ></label
        >
      </div>
    </fieldset>
    <fieldset>
      <legend>교전 규칙</legend>
      <div class="mode-grid">
        <label class="choice"
          ><input type="radio" bind:group={gameMode} value="CLASSIC" /><span
            ><strong>클래식</strong><small>CLASSIC</small><em>턴마다 한 발 사격</em></span
          ></label
        >
        <label class="choice"
          ><input type="radio" bind:group={gameMode} value="RAPID" /><span
            ><strong>신속전</strong><small>RAPID</small><em>고정 30초 턴</em></span
          ></label
        >
        <label class="choice"
          ><input type="radio" bind:group={gameMode} value="SALVO" /><span
            ><strong>일제사격</strong><small>SALVO</small><em>생존 함선당 한 발</em></span
          ></label
        >
      </div>
      <label class="duration-choice" for="turn-duration">
        <span><strong>턴 제한 시간</strong><small>TURN LIMIT</small></span>
        <select id="turn-duration" bind:value={turnDurationSeconds} disabled={gameMode === 'RAPID'}>
          <option value={0}>제한 없음</option>
          <option value={30}>30초</option>
          <option value={45}>45초</option>
          <option value={60}>60초</option>
          <option value={90}>90초</option>
          <option value={120}>120초</option>
        </select>
      </label>
    </fieldset>
    <Button variant="primary" size="lg" type="submit" loading={submitting} full
      >작전실 편성 <ArrowRight size={17} /></Button
    >
  </form>
</Modal>

<Modal
  open={openJoin}
  title="보안 코드로 참가"
  eyebrow="SECURE CHANNEL"
  description="초대받은 6자리 작전 코드를 입력하십시오."
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
      label="작전 코드"
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
      full><KeyRound size={17} /> 채널 접속</Button
    >
  </form>
</Modal>
