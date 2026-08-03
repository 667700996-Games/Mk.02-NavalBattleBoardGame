<script lang="ts">
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import {
    ArrowRight,
    Clock3,
    Copy,
    DoorOpen,
    LockKeyhole,
    Plus,
    Radio,
    RefreshCw,
    Search,
    Users,
    X
  } from '@lucide/svelte';
  import { api, ApiError } from '$lib/api';
  import { realtime } from '$lib/realtime';
  import { gameSnapshot, session } from '$lib/stores';
  import type { RoomSummary, RoomVisibility } from '$lib/types';

  let rooms: RoomSummary[] = [];
  let loading = true;
  let error = '';
  let showCreate = false;
  let showJoin = false;
  let roomName = '북태평양 교전';
  let visibility: RoomVisibility = 'PUBLIC';
  let roomCode = '';
  let submitting = false;
  let matching = false;
  let queuedAt: Date | null = null;
  let elapsed = 0;

  onMount(() => {
    let refreshTimer: ReturnType<typeof setInterval>;
    let queueTimer: ReturnType<typeof setInterval>;
    let unsubscribe: (() => void) | undefined;
    (async () => {
      try {
        const current = await api.currentSession();
        session.set(current);
        const recovered = await api.recover();
        if (recovered && !['FINISHED', 'CANCELLED'].includes(recovered.room.status)) {
          gameSnapshot.set(recovered);
          await goto(`/room/${recovered.room.code}`);
          return;
        }
        realtime.connect();
        await loadRooms();
        refreshTimer = setInterval(loadRooms, 7_500);
        unsubscribe = gameSnapshot.subscribe((snapshot) => {
          if (matching && snapshot?.room.status === 'PLACEMENT') {
            goto(`/room/${snapshot.room.code}`);
          }
        });
        queueTimer = setInterval(() => {
          elapsed = queuedAt ? Math.floor((Date.now() - queuedAt.getTime()) / 1000) : 0;
        }, 1_000);
      } catch {
        await goto('/');
      }
    })();
    return () => {
      if (refreshTimer) clearInterval(refreshTimer);
      if (queueTimer) clearInterval(queueTimer);
      unsubscribe?.();
    };
  });

  async function loadRooms() {
    try {
      rooms = (await api.listRooms()).rooms;
      error = '';
    } catch (caught) {
      error = caught instanceof ApiError ? caught.message : '공개 방 목록을 불러오지 못했습니다.';
    } finally {
      loading = false;
    }
  }

  async function createRoom() {
    submitting = true;
    error = '';
    try {
      const response = await api.createRoom(roomName, visibility);
      gameSnapshot.set(response.snapshot);
      await goto(`/room/${response.snapshot.room.code}`);
    } catch (caught) {
      error = caught instanceof ApiError ? caught.message : '작전실을 만들지 못했습니다.';
    } finally {
      submitting = false;
    }
  }

  async function joinRoom(code = roomCode) {
    submitting = true;
    error = '';
    try {
      const snapshot = await api.joinRoom(code);
      gameSnapshot.set(snapshot);
      await goto(`/room/${snapshot.room.code}`);
    } catch (caught) {
      error = caught instanceof ApiError ? caught.message : '작전실에 참가하지 못했습니다.';
    } finally {
      submitting = false;
    }
  }

  async function toggleMatchmaking() {
    if (matching) {
      await api.cancelMatchmaking();
      matching = false;
      queuedAt = null;
      return;
    }
    try {
      const response = await api.enqueueMatchmaking();
      if (response.snapshot) {
        gameSnapshot.set(response.snapshot);
        await goto(`/room/${response.snapshot.room.code}`);
      } else {
        matching = true;
        queuedAt = new Date(response.queuedAt ?? Date.now());
      }
    } catch (caught) {
      error = caught instanceof ApiError ? caught.message : '빠른 매칭을 시작하지 못했습니다.';
    }
  }

  const age = (createdAt: string) => {
    const minutes = Math.max(0, Math.floor((Date.now() - new Date(createdAt).getTime()) / 60_000));
    return minutes < 1 ? '방금 전' : `${minutes}분 전`;
  };
</script>

<svelte:head><title>작전 로비 · Mk.01</title></svelte:head>

<div class="lobby shell">
  <header class="lobby-heading">
    <div>
      <p class="eyebrow">OPERATIONS LOBBY</p>
      <h1 class="page-title">작전 로비</h1>
      <p>{$session?.nickname} 지휘관, 참가할 해역을 선택하십시오.</p>
    </div>
    <div class="lobby-heading__actions">
      <button class="button" onclick={() => (showJoin = true)}><DoorOpen size={17} /> 코드 참가</button>
      <button class="button button--primary" onclick={() => (showCreate = true)}><Plus size={17} /> 작전실 생성</button>
    </div>
  </header>

  {#if error}<div class="alert" role="alert">{error}</div>{/if}

  <section class="quick-match panel">
    <div class="quick-match__signal"><Radio size={28} /></div>
    <div>
      <span class="status-pill"><span class="status-dot"></span> AUTOMATED MATCHING</span>
      <h2>{matching ? '상대 지휘관 탐색 중' : '빠른 교전'}</h2>
      <p>{matching ? `${elapsed}초 경과 · 대기 중에도 취소할 수 있습니다.` : '대기 중인 지휘관과 즉시 1:1 비공개 작전실을 편성합니다.'}</p>
    </div>
    <button class:button--danger={matching} class="button button--primary" onclick={toggleMatchmaking}>
      {#if matching}<X size={17} /> 매칭 취소{:else}<Search size={17} /> 상대 찾기{/if}
    </button>
  </section>

  <section class="room-section" aria-labelledby="public-room-title">
    <div class="section-heading">
      <div>
        <p class="eyebrow">OPEN CHANNELS</p>
        <h2 id="public-room-title">공개 작전실</h2>
      </div>
      <button class="icon-button" onclick={loadRooms} aria-label="방 목록 새로고침" title="새로고침"><RefreshCw size={16} /></button>
    </div>

    <div class="room-list panel">
      <div class="room-list__head"><span>작전실</span><span>지휘관</span><span>생성</span><span></span></div>
      {#if loading}
        <div class="empty-state"><div class="spinner" aria-label="방 목록 불러오는 중"></div></div>
      {:else if rooms.length === 0}
        <div class="empty-state">
          <div><Radio size={30} class="muted" /><h3>현재 열린 작전실이 없습니다</h3><p class="muted">첫 작전실을 만들거나 빠른 교전을 시작해 보세요.</p></div>
        </div>
      {:else}
        {#each rooms as room (room.id)}
          <article class="room-row">
            <div class="room-name"><span class="room-signal"></span><div><strong>{room.name}</strong><small>CODE {room.code}</small></div></div>
            <span><Users size={15} /> {room.playerCount}/{room.capacity}</span>
            <span><Clock3 size={15} /> {age(room.createdAt)}</span>
            <button class="button button--small" onclick={() => joinRoom(room.code)} disabled={submitting}>참가 <ArrowRight size={14} /></button>
          </article>
        {/each}
      {/if}
    </div>
  </section>
</div>

{#if showCreate}
  <div class="modal-backdrop" role="presentation" onclick={(event) => event.currentTarget === event.target && (showCreate = false)}>
    <section class="modal panel" role="dialog" aria-modal="true" aria-labelledby="create-title">
      <button class="icon-button modal__close" onclick={() => (showCreate = false)} aria-label="닫기"><X size={16} /></button>
      <p class="eyebrow">NEW OPERATION</p><h2 id="create-title">새 작전실 생성</h2>
      <form onsubmit={(event) => { event.preventDefault(); createRoom(); }}>
        <div class="field"><label for="room-name">작전실 이름</label><input id="room-name" class="input" bind:value={roomName} minlength="2" maxlength="32" required /></div>
        <fieldset><legend>공개 범위</legend><label class="choice"><input type="radio" bind:group={visibility} value="PUBLIC" /><span><Radio size={17} /><strong>공개</strong><small>로비 목록에서 누구나 참가</small></span></label><label class="choice"><input type="radio" bind:group={visibility} value="PRIVATE" /><span><LockKeyhole size={17} /><strong>비공개</strong><small>초대 링크와 코드로만 참가</small></span></label></fieldset>
        <button class="button button--primary button--wide" type="submit" disabled={submitting}>{submitting ? '편성 중…' : '작전실 편성'} <ArrowRight size={17} /></button>
      </form>
    </section>
  </div>
{/if}

{#if showJoin}
  <div class="modal-backdrop" role="presentation" onclick={(event) => event.currentTarget === event.target && (showJoin = false)}>
    <section class="modal panel" role="dialog" aria-modal="true" aria-labelledby="join-title">
      <button class="icon-button modal__close" onclick={() => (showJoin = false)} aria-label="닫기"><X size={16} /></button>
      <p class="eyebrow">SECURE CHANNEL</p><h2 id="join-title">코드로 참가</h2><p class="muted">초대받은 6자리 작전 코드를 입력하십시오.</p>
      <form onsubmit={(event) => { event.preventDefault(); joinRoom(); }}>
        <div class="field"><label for="room-code">작전 코드</label><input id="room-code" class="input input-code" bind:value={roomCode} minlength="6" maxlength="6" placeholder="ABC123" autocomplete="off" required /></div>
        <button class="button button--primary button--wide" type="submit" disabled={submitting || roomCode.length !== 6}>채널 접속 <ArrowRight size={17} /></button>
      </form>
    </section>
  </div>
{/if}

<style>
  .lobby { padding: 64px 0 100px; }
  .lobby-heading { display: flex; align-items: end; justify-content: space-between; gap: 30px; margin-bottom: 34px; }.lobby-heading h1 { margin-bottom: 8px; }.lobby-heading p:last-child { margin: 0; color: var(--steel-300); }.lobby-heading__actions { display: flex; gap: 10px; }
  .alert { margin-bottom: 18px; padding: 12px 15px; border: 1px solid rgba(255,83,100,.35); border-radius: 10px; color: #ffb2bc; background: rgba(100,18,31,.24); font-size: 13px; }
  .quick-match { display: grid; grid-template-columns: auto 1fr auto; align-items: center; gap: 20px; padding: 24px; border-color: rgba(57,224,235,.25); overflow: hidden; }
  .quick-match__signal { position: relative; display: grid; width: 64px; height: 64px; place-items: center; border: 1px solid rgba(57,224,235,.34); border-radius: 50%; color: var(--cyan-400); background: radial-gradient(circle, rgba(57,224,235,.14), transparent 65%); }.quick-match__signal::after { position:absolute; inset:-8px; content:''; border:1px solid rgba(57,224,235,.09); border-radius:50%; }
  .quick-match h2 { margin: 10px 0 3px; font-size: 19px; }.quick-match p { margin: 0; color: #819cac; font-size: 12px; }
  .room-section { margin-top: 50px; }.section-heading { display:flex; align-items:end; justify-content:space-between; margin-bottom:16px; }.section-heading h2 { margin: 0; font-size: 24px; }
  .room-list { overflow: hidden; border-radius: var(--radius-md); }.room-list__head, .room-row { display:grid; grid-template-columns: minmax(240px,1fr) 130px 130px 90px; align-items:center; gap:15px; padding: 14px 20px; }.room-list__head { min-height:44px; color:#698697; background:rgba(4,15,24,.72); font-size:10px; font-weight:700; letter-spacing:.1em; text-transform:uppercase; }.room-row { min-height:75px; border-top:1px solid var(--line); }.room-row:hover { background:rgba(38,109,135,.07); }.room-row > span { display:flex; align-items:center; gap:7px; color:#8da8b7; font-size:12px; }.room-name { display:flex; align-items:center; gap:13px; }.room-name div { display:grid; gap:3px; }.room-name small { color:#627f90; font-family:Rajdhani; font-size:10px; letter-spacing:.12em; }.room-signal { width:8px; height:8px; border-radius:50%; background:var(--green-500); box-shadow:0 0 10px rgba(61,226,161,.7); }
  .modal-backdrop { position:fixed; z-index:80; inset:0; display:grid; place-items:center; padding:20px; background:rgba(0,6,10,.76); backdrop-filter:blur(8px); }.modal { position:relative; width:min(500px,100%); padding:30px; }.modal h2 { margin-bottom:12px; font-size:26px; }.modal__close { position:absolute; top:18px; right:18px; }.modal form { display:grid; gap:20px; margin-top:24px; }.modal fieldset { display:grid; grid-template-columns:1fr 1fr; gap:10px; padding:0; border:0; }.modal legend { margin-bottom:8px; color:#c9dce6; font-size:13px; font-weight:650; }.choice { position:relative; }.choice input { position:absolute; opacity:0; }.choice span { display:grid; grid-template-columns:auto 1fr; gap:3px 8px; min-height:88px; align-content:center; padding:14px; border:1px solid var(--line); border-radius:10px; cursor:pointer; }.choice svg { grid-row:1/3; color:var(--cyan-400); }.choice small { color:#708c9c; font-size:10px; }.choice input:checked + span { border-color:var(--cyan-400); background:rgba(22,199,217,.1); box-shadow:0 0 0 2px rgba(22,199,217,.08); }
  @media(max-width:720px){.lobby{padding-top:40px}.lobby-heading{display:block}.lobby-heading__actions{margin-top:22px}.lobby-heading__actions .button{flex:1;padding-inline:10px}.quick-match{grid-template-columns:auto 1fr}.quick-match>.button{grid-column:1/-1;width:100%}.room-list__head{display:none}.room-row{grid-template-columns:1fr auto; gap:10px; padding:16px}.room-row>span{display:none}.modal{padding:25px 20px}.modal fieldset{grid-template-columns:1fr}}
</style>

