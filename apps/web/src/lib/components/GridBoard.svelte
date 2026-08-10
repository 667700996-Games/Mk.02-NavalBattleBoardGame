<script lang="ts">
  import { Crosshair, Flame, Waves } from '@lucide/svelte';
  import { cellsForPlacement } from '$lib/game/placement';
  import {
    ROW_LABELS,
    coordinateKey,
    coordinateLabel,
    shipName,
    type AttackOutcome,
    type Coordinate,
    type OwnBoardSnapshot,
    type ShipKind,
    type ShipPlacement,
    type TargetBoardSnapshot
  } from '$lib/types';

  interface Props {
    mode: 'placement' | 'own' | 'target';
    label: string;
    placements?: ShipPlacement[];
    ownBoard?: OwnBoardSnapshot | null;
    targetBoard?: TargetBoardSnapshot | null;
    selected?: Coordinate | null;
    previewCells?: Coordinate[];
    previewValid?: boolean;
    interactive?: boolean;
    disabled?: boolean;
    oncell?: (coordinate: Coordinate) => void;
    onhover?: (coordinate: Coordinate | null) => void;
    ondropcell?: (coordinate: Coordinate) => void;
    onshipdrag?: (kind: ShipKind) => void;
  }

  let {
    mode,
    label,
    placements = [],
    ownBoard = null,
    targetBoard = null,
    selected = null,
    previewCells = [],
    previewValid = true,
    interactive = false,
    disabled = false,
    oncell,
    onhover,
    ondropcell,
    onshipdrag
  }: Props = $props();

  const grid = Array.from({ length: 10 }, (_, row) =>
    Array.from({ length: 10 }, (_, col) => ({ row, col }))
  );

  function placementKind(coordinate: Coordinate): ShipKind | null {
    return (
      placements.find((placement) =>
        cellsForPlacement(placement).some(
          (cell) => cell.row === coordinate.row && cell.col === coordinate.col
        )
      )?.kind ?? null
    );
  }

  function shipAt(coordinate: Coordinate): {
    kind: ShipKind;
    index: number;
    size: number;
    vertical: boolean;
  } | null {
    if (mode === 'placement') {
      const placement = placements.find((item) =>
        cellsForPlacement(item).some(
          (cell) => cell.row === coordinate.row && cell.col === coordinate.col
        )
      );
      if (!placement) return null;
      const cells = cellsForPlacement(placement);
      return {
        kind: placement.kind,
        index: cells.findIndex(
          (cell) => cell.row === coordinate.row && cell.col === coordinate.col
        ),
        size: cells.length,
        vertical: placement.orientation === 'VERTICAL'
      };
    }

    const ship = ownBoard?.ships.find((item) =>
      item.cells.some((cell) => cell.row === coordinate.row && cell.col === coordinate.col)
    );
    if (!ship) return null;
    const index = ship.cells.findIndex(
      (cell) => cell.row === coordinate.row && cell.col === coordinate.col
    );
    const first = ship.cells[0];
    const second = ship.cells[1];
    return {
      kind: ship.kind,
      index,
      size: ship.cells.length,
      vertical: Boolean(first && second && first.col === second.col)
    };
  }

  function segmentClass(coordinate: Coordinate): string {
    const ship = shipAt(coordinate);
    if (!ship) return '';
    if (ship.index === 0) return 'ship-segment--bow';
    if (ship.index === ship.size - 1) return 'ship-segment--stern';
    return 'ship-segment--mid';
  }

  function ownShipKind(coordinate: Coordinate): ShipKind | null {
    return (
      ownBoard?.ships.find((ship) =>
        ship.cells.some((cell) => cell.row === coordinate.row && cell.col === coordinate.col)
      )?.kind ?? null
    );
  }

  function attackAt(coordinate: Coordinate): AttackOutcome | null {
    const attacks = mode === 'own' ? ownBoard?.attacksReceived : targetBoard?.attacks;
    return (
      attacks?.find(
        (attack) =>
          attack.coordinate.row === coordinate.row && attack.coordinate.col === coordinate.col
      )?.outcome ?? null
    );
  }

  function isPreview(coordinate: Coordinate): boolean {
    return previewCells.some((cell) => cell.row === coordinate.row && cell.col === coordinate.col);
  }

  function handleKeyboard(event: KeyboardEvent, coordinate: Coordinate) {
    let next: Coordinate;
    if (event.key === 'ArrowUp') next = { ...coordinate, row: Math.max(0, coordinate.row - 1) };
    else if (event.key === 'ArrowDown')
      next = { ...coordinate, row: Math.min(9, coordinate.row + 1) };
    else if (event.key === 'ArrowLeft')
      next = { ...coordinate, col: Math.max(0, coordinate.col - 1) };
    else if (event.key === 'ArrowRight')
      next = { ...coordinate, col: Math.min(9, coordinate.col + 1) };
    else if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      if (!disabled) oncell?.(coordinate);
      return;
    } else return;
    event.preventDefault();
    const board = (event.currentTarget as HTMLElement).closest('.board-grid');
    board?.querySelector<HTMLButtonElement>(`[data-cell="${next.row}:${next.col}"]`)?.focus();
  }

  function ariaDescription(coordinate: Coordinate): string {
    const attack = attackAt(coordinate);
    const kind = mode === 'placement' ? placementKind(coordinate) : ownShipKind(coordinate);
    const parts = [coordinateLabel(coordinate)];
    if (kind) parts.push(shipName(kind));
    if (attack) parts.push(attack === 'MISS' ? '빗나감' : attack === 'HIT' ? '명중' : '격침');
    if (mode === 'target' && !attack) parts.push('미공격 좌표');
    return parts.join(', ');
  }
</script>

<div class:board-disabled={disabled} class="board-wrap">
  <span class="board-wrap__bezel" aria-hidden="true"></span>
  <div
    class="board-grid"
    role="grid"
    tabindex="0"
    aria-label={label}
    onmouseleave={() => onhover?.(null)}
  >
    <span class="axis axis--corner" aria-hidden="true"><Crosshair size={10} /></span>
    {#each Array.from({ length: 10 }) as _, col (col)}
      <span class="axis axis--col" aria-hidden="true">{col + 1}</span>
    {/each}
    {#each grid as row, rowIndex (rowIndex)}
      <span class="axis axis--row" aria-hidden="true">{ROW_LABELS[rowIndex]}</span>
      {#each row as coordinate (coordinateKey(coordinate))}
        {@const attack = attackAt(coordinate)}
        {@const kind = mode === 'placement' ? placementKind(coordinate) : ownShipKind(coordinate)}
        {@const ship = shipAt(coordinate)}
        {@const preview = isPreview(coordinate)}
        {@const isSelected = selected?.row === coordinate.row && selected?.col === coordinate.col}
        <button
          class:cell--ship={Boolean(kind)}
          class:cell--ship-vertical={Boolean(ship?.vertical)}
          class:cell--preview={preview && previewValid}
          class:cell--invalid={preview && !previewValid}
          class:cell--selected={isSelected}
          class:cell--miss={attack === 'MISS'}
          class:cell--hit={attack === 'HIT'}
          class:cell--sunk={attack === 'SUNK'}
          class:cell--interactive={interactive && !attack && !disabled}
          class="board-cell"
          type="button"
          role="gridcell"
          data-cell={coordinateKey(coordinate)}
          data-testid={`${mode}-cell-${coordinate.row}-${coordinate.col}`}
          aria-label={ariaDescription(coordinate)}
          disabled={disabled || (mode === 'target' && Boolean(attack))}
          draggable={mode === 'placement' && Boolean(kind) && !disabled}
          onclick={() => oncell?.(coordinate)}
          onkeydown={(event) => handleKeyboard(event, coordinate)}
          onmouseenter={() => onhover?.(coordinate)}
          onfocus={() => onhover?.(coordinate)}
          ondragstart={() => kind && onshipdrag?.(kind)}
          ondragover={(event) => {
            if (mode === 'placement') event.preventDefault();
            onhover?.(coordinate);
          }}
          ondrop={(event) => {
            event.preventDefault();
            ondropcell?.(coordinate);
          }}
        >
          {#if kind}<span
              class={`ship-segment ${segmentClass(coordinate)}`}
              title={shipName(kind)}
              aria-hidden="true"><i></i><b></b></span
            >{/if}
          {#if attack === 'MISS'}<span class="miss-marker"><i></i><Waves size={13} /></span>{/if}
          {#if attack === 'HIT' || attack === 'SUNK'}<span class="hit-marker"
              ><i></i><Flame size={14} /></span
            >{/if}
        </button>
      {/each}
    {/each}
  </div>
</div>

<style>
  .board-wrap {
    position: relative;
    width: 100%;
    max-width: 620px;
    padding: 10px;
    container-type: inline-size;
    isolation: isolate;
    perspective: 1100px;
  }
  .board-wrap::before,
  .board-wrap::after {
    position: absolute;
    z-index: -1;
    content: '';
    pointer-events: none;
  }
  .board-wrap::before {
    inset: 2px;
    border: 1px solid rgba(91, 226, 237, 0.18);
    border-radius: 12px 5px 12px 5px;
    background: linear-gradient(145deg, rgba(26, 89, 108, 0.19), rgba(1, 8, 15, 0.82));
    box-shadow:
      0 28px 70px rgba(0, 0, 0, 0.42),
      0 0 50px rgba(24, 208, 226, 0.055);
  }
  .board-wrap::after {
    inset: 12% 12% -4%;
    border-radius: 50%;
    background: rgba(25, 200, 218, 0.14);
    filter: blur(38px);
    opacity: 0.26;
  }
  .board-wrap__bezel {
    position: absolute;
    z-index: 4;
    inset: 2px;
    border-radius: 12px 5px 12px 5px;
    pointer-events: none;
    background:
      linear-gradient(var(--cyan-300), var(--cyan-300)) 14px 14px / 28px 1px no-repeat,
      linear-gradient(var(--cyan-300), var(--cyan-300)) 14px 14px / 1px 28px no-repeat,
      linear-gradient(var(--cyan-300), var(--cyan-300)) calc(100% - 14px) calc(100% - 14px) / 28px
        1px no-repeat,
      linear-gradient(var(--cyan-300), var(--cyan-300)) calc(100% - 14px) calc(100% - 14px) / 1px
        28px no-repeat;
    opacity: 0.36;
  }
  .board-grid {
    position: relative;
    display: grid;
    grid-template-columns: 28px repeat(10, 1fr);
    grid-template-rows: 28px repeat(10, 1fr);
    width: 100%;
    aspect-ratio: 1;
    padding: 5px;
    border: 1px solid rgba(100, 193, 208, 0.3);
    overflow: hidden;
    border-radius: 8px 3px 8px 3px;
    background:
      radial-gradient(circle at 28% 16%, rgba(34, 180, 190, 0.12), transparent 30%),
      repeating-linear-gradient(120deg, rgba(34, 101, 119, 0.06) 0 1px, transparent 1px 7px),
      linear-gradient(135deg, rgba(4, 35, 48, 0.99), rgba(1, 16, 26, 0.99));
    box-shadow:
      inset 0 0 45px rgba(30, 166, 190, 0.055),
      0 18px 48px rgba(0, 0, 0, 0.2);
    user-select: none;
    touch-action: manipulation;
  }
  .axis {
    display: grid;
    place-items: center;
    color: #7292a3;
    font-family: Rajdhani, sans-serif;
    font-size: clamp(8px, 2.5cqw, 11px);
    font-weight: 600;
  }
  .axis--corner {
    color: #3d7085;
  }
  .board-cell {
    position: relative;
    min-width: 0;
    min-height: 0;
    padding: 0;
    overflow: hidden;
    border: 0;
    border-top: 1px solid rgba(99, 176, 190, 0.16);
    border-left: 1px solid rgba(99, 176, 190, 0.16);
    color: white;
    background:
      radial-gradient(circle at 30% 24%, rgba(124, 228, 232, 0.04), transparent 42%),
      linear-gradient(145deg, rgba(8, 55, 68, 0.54), rgba(4, 31, 45, 0.7));
    cursor: default;
    transition:
      background 0.12s ease,
      box-shadow 0.12s ease,
      transform 0.12s ease;
  }
  .board-cell:nth-child(11n + 1) {
    border-left: 0;
  }
  .board-cell:disabled {
    opacity: 1;
  }
  .board-cell::after {
    position: absolute;
    inset: 17%;
    content: '';
    border: 1px solid transparent;
    border-radius: 3px;
    pointer-events: none;
  }
  .board-cell::before {
    position: absolute;
    z-index: 6;
    inset: 8%;
    content: '';
    border: 1px solid transparent;
    border-radius: 50%;
    pointer-events: none;
  }
  .cell--interactive:not(:disabled) {
    cursor: crosshair;
  }
  .cell--interactive:not(:disabled):hover,
  .cell--interactive:not(:disabled):focus-visible {
    z-index: 2;
    background: rgba(32, 158, 187, 0.27);
    box-shadow:
      inset 0 0 0 1px var(--cyan-400),
      0 0 18px rgba(57, 224, 235, 0.14);
  }
  .cell--interactive:not(:disabled):hover::after {
    border-color: rgba(165, 247, 247, 0.55);
  }
  .cell--ship {
    background: rgba(46, 116, 132, 0.16);
  }
  .ship-segment {
    position: absolute;
    z-index: 2;
    inset: 17% -1px;
    display: block;
    border: 1px solid rgba(157, 218, 224, 0.52);
    border-radius: 0;
    background: linear-gradient(
      90deg,
      rgba(133, 204, 211, 0.75),
      rgba(46, 91, 106, 0.96) 56%,
      rgba(20, 52, 66, 0.98)
    );
    box-shadow:
      inset 0 1px rgba(255, 255, 255, 0.28),
      inset 0 -2px rgba(1, 17, 25, 0.52),
      0 2px 6px rgba(0, 0, 0, 0.35);
    transform: translateZ(4px);
  }
  .ship-segment::before {
    position: absolute;
    top: 25%;
    right: 16%;
    bottom: 25%;
    left: 18%;
    content: '';
    border: 1px solid rgba(210, 246, 245, 0.28);
    border-radius: 2px;
    background: linear-gradient(90deg, transparent, rgba(166, 229, 230, 0.16), transparent);
  }
  .ship-segment i {
    position: absolute;
    z-index: 1;
    top: 20%;
    bottom: 20%;
    left: 42%;
    width: 18%;
    border: 1px solid rgba(190, 236, 235, 0.32);
    border-radius: 2px;
    background: rgba(11, 45, 59, 0.72);
  }
  .ship-segment b {
    position: absolute;
    z-index: 2;
    top: 42%;
    right: 5%;
    left: 5%;
    height: 16%;
    background: rgba(213, 250, 248, 0.36);
  }
  .ship-segment--bow {
    border-radius: 60% 8% 8% 60%;
    clip-path: polygon(0 50%, 18% 15%, 100% 15%, 100% 85%, 18% 85%);
  }
  .ship-segment--stern {
    border-radius: 8% 60% 60% 8%;
    clip-path: polygon(0 15%, 82% 15%, 100% 50%, 82% 85%, 0 85%);
  }
  .cell--ship-vertical .ship-segment {
    inset: -1px 17%;
    background: linear-gradient(
      180deg,
      rgba(134, 204, 211, 0.75),
      rgba(46, 91, 106, 0.96) 56%,
      rgba(20, 52, 66, 0.98)
    );
  }
  .cell--ship-vertical .ship-segment::before {
    top: 18%;
    right: 25%;
    bottom: 18%;
    left: 25%;
  }
  .cell--ship-vertical .ship-segment i {
    top: 42%;
    right: 20%;
    bottom: auto;
    left: 20%;
    width: auto;
    height: 18%;
  }
  .cell--ship-vertical .ship-segment b {
    top: 5%;
    right: auto;
    bottom: 5%;
    left: 42%;
    width: 16%;
    height: auto;
  }
  .cell--ship-vertical .ship-segment--bow {
    border-radius: 60% 60% 8% 8%;
    clip-path: polygon(50% 0, 85% 18%, 85% 100%, 15% 100%, 15% 18%);
  }
  .cell--ship-vertical .ship-segment--stern {
    border-radius: 8% 8% 60% 60%;
    clip-path: polygon(15% 0, 85% 0, 85% 82%, 50% 100%, 15% 82%);
  }
  .cell--preview {
    z-index: 2;
    background: rgba(32, 211, 194, 0.25);
    box-shadow: inset 0 0 0 1px var(--green-500);
  }
  .cell--invalid {
    z-index: 2;
    background: rgba(255, 72, 88, 0.25);
    box-shadow: inset 0 0 0 1px var(--red-500);
  }
  .cell--selected {
    z-index: 2;
    box-shadow:
      inset 0 0 0 2px var(--amber-500),
      0 0 15px rgba(255, 180, 60, 0.2);
  }
  .cell--selected::before {
    border-color: rgba(255, 190, 77, 0.8);
    animation: target-lock 900ms var(--ease-out) infinite;
  }
  .miss-marker,
  .hit-marker {
    position: absolute;
    z-index: 4;
    inset: 0;
    display: grid;
    place-items: center;
  }
  .miss-marker {
    color: #83cce0;
    background: radial-gradient(circle, rgba(96, 184, 207, 0.18), transparent 65%);
  }
  .miss-marker i {
    position: absolute;
    width: 36%;
    aspect-ratio: 1;
    border: 1px solid rgba(144, 230, 244, 0.72);
    border-radius: 50%;
    animation: water-ring 1.8s ease-out infinite;
  }
  .hit-marker {
    color: #fff0d8;
    background: radial-gradient(
      circle,
      rgba(255, 79, 56, 0.72),
      rgba(255, 99, 48, 0.2) 38%,
      transparent 70%
    );
    filter: drop-shadow(0 0 5px #ff5e3b);
  }
  .hit-marker i {
    position: absolute;
    width: 34%;
    aspect-ratio: 1;
    border-radius: 50%;
    background: #fff3cf;
    box-shadow:
      0 0 8px #fff0c2,
      0 0 16px #ff7b3e,
      0 0 24px rgba(255, 70, 33, 0.75);
    animation: impact-core 1.7s ease-in-out infinite;
  }
  .hit-marker :global(svg) {
    position: relative;
    z-index: 2;
  }
  .cell--sunk .ship-segment {
    background: linear-gradient(90deg, #64343e, #2b2029);
    border-color: #ff7584;
    filter: saturate(0.7);
  }
  .cell--sunk {
    background: rgba(111, 23, 38, 0.4);
  }
  .board-disabled {
    filter: saturate(0.72);
    opacity: 0.86;
  }
  @keyframes target-lock {
    50% {
      inset: 20%;
      opacity: 0.28;
    }
  }
  @keyframes water-ring {
    from {
      transform: scale(0.4);
      opacity: 0.85;
    }
    to {
      transform: scale(2.6);
      opacity: 0;
    }
  }
  @keyframes impact-core {
    50% {
      transform: scale(1.35);
      opacity: 0.65;
    }
  }
  @container (max-width:430px) {
    .board-grid {
      grid-template-columns: 22px repeat(10, 1fr);
      grid-template-rows: 22px repeat(10, 1fr);
      padding: 3px;
      border-radius: 9px;
    }
    .ship-segment {
      inset: 14%;
    }
  }
</style>
