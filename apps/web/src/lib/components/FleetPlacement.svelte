<script lang="ts">
  import { untrack } from 'svelte';
  import { Check, Dices, Grip, RotateCw, Trash2 } from '@lucide/svelte';
  import GridBoard from './GridBoard.svelte';
  import {
    autoPlaceFleet,
    rotatePlacement,
    validateFleet,
    validatePlacement
  } from '$lib/game/placement';
  import {
    FLEET,
    shipName,
    type Coordinate,
    type Orientation,
    type ShipKind,
    type ShipPlacement
  } from '$lib/types';
  import { sounds } from '$lib/sound';

  interface Props {
    initialPlacement?: ShipPlacement[] | null;
    confirmed?: boolean;
    submitting?: boolean;
    onconfirm: (placements: ShipPlacement[]) => void;
  }
  let {
    initialPlacement = null,
    confirmed = false,
    submitting = false,
    onconfirm
  }: Props = $props();

  let placements = $state<ShipPlacement[]>(
    untrack(() => (initialPlacement ? structuredClone(initialPlacement) : []))
  );
  let selectedKind = $state<ShipKind | null>(
    FLEET.find((ship) => !placements.some((placement) => placement.kind === ship.kind))?.kind ??
      'CARRIER'
  );
  let orientation = $state<Orientation>('HORIZONTAL');
  let hover = $state<Coordinate | null>(null);
  let notice = $state('함선을 선택하고 해역의 시작 좌표를 지정하십시오.');

  let candidate = $derived<ShipPlacement | null>(
    selectedKind && hover ? { kind: selectedKind, origin: hover, orientation } : null
  );
  let preview = $derived(
    candidate ? validatePlacement(candidate, placements) : { valid: true, cells: [] }
  );
  let fleet = $derived(validateFleet(placements));

  function selectShip(kind: ShipKind) {
    if (confirmed) return;
    selectedKind = kind;
    orientation =
      placements.find((placement) => placement.kind === kind)?.orientation ?? orientation;
    sounds.select();
  }

  function place(coordinate: Coordinate) {
    if (!selectedKind || confirmed) return;
    const next: ShipPlacement = { kind: selectedKind, origin: coordinate, orientation };
    const validation = validatePlacement(next, placements);
    if (!validation.valid) {
      notice =
        validation.reason === 'OVERLAP'
          ? '다른 함선과 겹치는 위치입니다.'
          : '보드 경계를 벗어나는 위치입니다.';
      return;
    }
    placements = [...placements.filter((placement) => placement.kind !== selectedKind), next];
    notice = `${shipName(selectedKind)} 배치 완료`;
    selectedKind =
      FLEET.find((ship) => !placements.some((placement) => placement.kind === ship.kind))?.kind ??
      selectedKind;
    if (selectedKind)
      orientation =
        placements.find((placement) => placement.kind === selectedKind)?.orientation ?? orientation;
    sounds.select();
  }

  function rotate() {
    if (!selectedKind || confirmed) return;
    const existing = placements.find((placement) => placement.kind === selectedKind);
    if (!existing) {
      orientation = orientation === 'HORIZONTAL' ? 'VERTICAL' : 'HORIZONTAL';
      return;
    }
    const rotated = rotatePlacement(existing);
    const validation = validatePlacement(rotated, placements);
    if (!validation.valid) {
      notice = '현재 위치에서는 회전할 공간이 부족합니다.';
      return;
    }
    placements = [...placements.filter((placement) => placement.kind !== selectedKind), rotated];
    orientation = rotated.orientation;
    notice = `${shipName(selectedKind)} 방향 전환`;
    sounds.select();
  }

  function autoPlace() {
    placements = autoPlaceFleet();
    selectedKind = 'CARRIER';
    orientation = placements[0].orientation;
    notice = '함대 자동 배치가 완료되었습니다. 확정 전까지 수정할 수 있습니다.';
  }

  function reset() {
    placements = [];
    selectedKind = 'CARRIER';
    orientation = 'HORIZONTAL';
    notice = '전체 배치를 초기화했습니다.';
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key.toLowerCase() === 'r') {
      event.preventDefault();
      rotate();
    }
    if (event.key === 'Escape') selectedKind = null;
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<section class="placement" aria-labelledby="placement-title">
  <header class="placement__heading">
    <div>
      <p class="eyebrow">FLEET DEPLOYMENT</p>
      <h2 id="placement-title">함대 배치</h2>
      <p>상대 지휘관에게 함선 좌표는 공개되지 않습니다.</p>
    </div>
    <span class:success={fleet.valid} class="status-pill"
      ><span class="status-dot"></span>{placements.length}/5 함선 배치</span
    >
  </header>

  <div class="deployment-steps" aria-label="함대 배치 진행 단계">
    <span class="active"><i>01</i><strong>함선 선택</strong><small>SELECT VESSEL</small></span>
    <span class:active={placements.length > 0}
      ><i>02</i><strong>해역 배치</strong><small>MAP SECTOR</small></span
    >
    <span class:active={fleet.valid}
      ><i>03</i><strong>작전 확정</strong><small>LOCK FORMATION</small></span
    >
  </div>

  <div class="placement__layout">
    <div class="placement__board panel">
      <div class="board-toolbar">
        <span
          ><i></i> SECTOR 10 × 10 / {orientation === 'HORIZONTAL' ? '가로 방향' : '세로 방향'}</span
        ><small>R · 회전 &nbsp; ESC · 선택 해제</small>
      </div>
      <GridBoard
        mode="placement"
        label="내 함대 배치 보드"
        {placements}
        previewCells={preview.cells ?? []}
        previewValid={preview.valid}
        interactive={!confirmed}
        disabled={confirmed || submitting}
        oncell={place}
        onhover={(coordinate) => (hover = coordinate)}
        ondropcell={place}
        onshipdrag={selectShip}
      />
      <p class:danger={!preview.valid} class="placement-notice" aria-live="polite">{notice}</p>
    </div>

    <aside class="fleet-dock panel">
      <div class="fleet-dock__heading">
        <div><span>FLEET MANIFEST</span><strong>함대 목록</strong></div>
        <Grip size={18} />
      </div>
      <div class="fleet-list">
        {#each FLEET as ship (ship.kind)}
          {@const placed = placements.find((placement) => placement.kind === ship.kind)}
          <button
            type="button"
            class:selected={selectedKind === ship.kind}
            class:placed={Boolean(placed)}
            class="fleet-item"
            onclick={() => selectShip(ship.kind)}
            disabled={confirmed}
            draggable={!confirmed}
            ondragstart={() => selectShip(ship.kind)}
          >
            <span class="fleet-item__meta"
              ><strong>{ship.name}</strong><small>{ship.size} CELLS</small></span
            >
            <span class="ship-shape" aria-hidden="true"
              >{#each Array.from({ length: ship.size }) as _, index (index)}<i></i>{/each}</span
            >
            {#if placed}<span class="placed-check"><Check size={15} /></span>{/if}
          </button>
        {/each}
      </div>
      <div class="fleet-actions">
        <button
          class="button button--small"
          type="button"
          onclick={rotate}
          disabled={confirmed || !selectedKind}><RotateCw size={15} /> 회전</button
        >
        <button class="button button--small" type="button" onclick={autoPlace} disabled={confirmed}
          ><Dices size={15} /> 자동 배치</button
        >
        <button
          class="button button--small button--danger"
          type="button"
          onclick={reset}
          disabled={confirmed || placements.length === 0}><Trash2 size={15} /> 초기화</button
        >
      </div>
      <div class="confirm-zone">
        <p>
          {fleet.valid
            ? '모든 함선이 교전 준비를 마쳤습니다.'
            : '다섯 척을 모두 유효한 위치에 배치하십시오.'}
        </p>
        <button
          class="button button--primary button--wide"
          type="button"
          disabled={!fleet.valid || confirmed || submitting}
          onclick={() => onconfirm(placements)}
          ><Check size={17} />
          {submitting ? '배치 확인 중…' : confirmed ? '배치 확정됨' : '배치 확정'}</button
        >
        <small>확정한 뒤에는 위치를 변경할 수 없습니다.</small>
      </div>
    </aside>
  </div>
</section>

<style>
  .placement__heading {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 20px;
    margin-bottom: 22px;
  }
  .placement__heading h2 {
    margin: 0 0 5px;
    font-size: 28px;
  }
  .placement__heading p:last-child {
    margin: 0;
    color: var(--steel-300);
    font-size: 12px;
  }
  .deployment-steps {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    margin-bottom: 16px;
    overflow: hidden;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: rgba(2, 12, 20, 0.5);
  }
  .deployment-steps span {
    position: relative;
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 1px 10px;
    align-items: center;
    min-height: 58px;
    padding: 10px 16px;
    border-right: 1px solid var(--line);
    color: var(--ink-500);
    transition: 280ms var(--ease-out);
  }
  .deployment-steps span:last-child {
    border-right: 0;
  }
  .deployment-steps span.active {
    color: var(--cyan-300);
    background: linear-gradient(90deg, rgba(40, 223, 232, 0.09), transparent);
  }
  .deployment-steps span.active::after {
    position: absolute;
    inset: auto 12% 0;
    height: 1px;
    content: '';
    background: var(--cyan-300);
    box-shadow: 0 0 10px var(--cyan-400);
  }
  .deployment-steps i {
    grid-row: 1 / 3;
    font-family: var(--font-display);
    font-size: 18px;
    font-style: normal;
  }
  .deployment-steps strong {
    color: var(--ink-200);
    font-size: 10px;
  }
  .deployment-steps small {
    font-family: var(--font-display);
    font-size: 7px;
    letter-spacing: 0.16em;
  }
  .placement__layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 352px;
    gap: 20px;
    align-items: start;
  }
  .placement__board {
    position: relative;
    padding: 18px;
    background: linear-gradient(145deg, rgba(9, 31, 44, 0.88), rgba(4, 15, 24, 0.9));
  }
  .board-toolbar {
    display: flex;
    justify-content: space-between;
    margin: 0 3px 12px;
    color: var(--ink-300);
    font-family: var(--font-display);
    font-size: 11px;
    letter-spacing: 0.07em;
  }
  .board-toolbar span {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .board-toolbar i {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--green-400);
    box-shadow: 0 0 8px var(--green-400);
  }
  .board-toolbar small {
    color: #617e8e;
  }
  .placement-notice {
    min-height: 18px;
    margin: 12px 3px 0;
    color: #87a4b3;
    font-size: 11px;
  }
  .fleet-dock {
    padding: 20px;
    background: linear-gradient(160deg, rgba(10, 32, 44, 0.94), rgba(3, 14, 22, 0.94));
  }
  .fleet-dock__heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-bottom: 15px;
    border-bottom: 1px solid var(--line);
    color: #6f8d9e;
  }
  .fleet-dock__heading div {
    display: grid;
    gap: 3px;
  }
  .fleet-dock__heading span {
    font-family: Rajdhani;
    font-size: 9px;
    letter-spacing: 0.18em;
  }
  .fleet-dock__heading strong {
    color: #d8e9f0;
    font-size: 15px;
  }
  .fleet-list {
    display: grid;
    gap: 7px;
    margin: 15px 0;
  }
  .fleet-item {
    position: relative;
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: center;
    gap: 12px;
    width: 100%;
    min-height: 66px;
    padding: 11px 13px;
    border: 1px solid var(--line);
    border-radius: 9px;
    color: #b9ced8;
    text-align: left;
    background: rgba(5, 21, 31, 0.6);
    cursor: grab;
    transition:
      transform 240ms var(--ease-out),
      border-color 240ms ease,
      background 240ms ease;
  }
  .fleet-item:hover,
  .fleet-item.selected {
    border-color: rgba(57, 224, 235, 0.55);
    background: rgba(22, 199, 217, 0.08);
    transform: translateX(-3px);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.18);
  }
  .fleet-item.placed {
    border-left: 2px solid var(--green-500);
  }
  .fleet-item:disabled {
    cursor: default;
  }
  .fleet-item__meta {
    display: grid;
    gap: 3px;
  }
  .fleet-item__meta strong {
    font-size: 12px;
  }
  .fleet-item__meta small {
    color: #5f7c8c;
    font-family: Rajdhani;
    font-size: 9px;
    letter-spacing: 0.12em;
  }
  .ship-shape {
    display: flex;
    gap: 2px;
  }
  .ship-shape i {
    display: block;
    width: 11px;
    height: 8px;
    border: 1px solid rgba(132, 198, 211, 0.42);
    border-radius: 2px;
    background: linear-gradient(180deg, #59869a, #244f63);
    box-shadow: inset 0 1px rgba(255, 255, 255, 0.15);
  }
  .placed-check {
    position: absolute;
    top: 6px;
    right: 7px;
    color: var(--green-500);
  }
  .fleet-actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 7px;
  }
  .fleet-actions .button:last-child {
    grid-column: 1/-1;
  }
  .confirm-zone {
    margin-top: 18px;
    padding-top: 17px;
    border-top: 1px solid var(--line);
  }
  .confirm-zone p {
    margin-bottom: 12px;
    color: #91aab7;
    font-size: 11px;
    line-height: 1.6;
  }
  .confirm-zone small {
    display: block;
    margin-top: 8px;
    color: #597787;
    text-align: center;
    font-size: 9px;
  }
  @media (max-width: 930px) {
    .placement__layout {
      grid-template-columns: 1fr;
    }
    .fleet-dock {
      display: grid;
      grid-template-columns: 1fr 1fr;
      gap: 15px;
    }
    .fleet-dock__heading {
      grid-column: 1/-1;
    }
    .fleet-list {
      grid-column: 1/2;
      margin: 0;
    }
    .fleet-actions,
    .confirm-zone {
      align-self: start;
      margin-top: 0;
    }
    .confirm-zone {
      grid-column: 2/3;
    }
    .fleet-actions {
      grid-column: 2/3;
      grid-row: 2;
    }
  }
  @media (max-width: 650px) {
    .placement__heading {
      display: block;
    }
    .placement__heading > .status-pill {
      margin-top: 14px;
    }
    .placement__board {
      padding: 8px;
    }
    .deployment-steps span {
      min-height: 48px;
      padding: 8px;
    }
    .deployment-steps strong,
    .deployment-steps small {
      display: none;
    }
    .deployment-steps i {
      grid-row: auto;
      text-align: center;
    }
    .fleet-dock {
      display: block;
      padding: 15px;
    }
    .fleet-list {
      margin: 15px 0;
    }
    .fleet-actions {
      display: flex;
      flex-wrap: wrap;
    }
    .fleet-actions .button {
      flex: 1;
    }
    .fleet-actions .button:last-child {
      grid-column: auto;
    }
    .confirm-zone {
      margin-top: 16px;
    }
    .placement-notice {
      padding-inline: 5px;
    }
  }
</style>
