<script lang="ts">
  import { Crosshair, Flame, Waves } from '@lucide/svelte';
  import { cellsForPlacement } from '$lib/game/placement';
  import {
    FLEET,
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
    let next = coordinate;
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
  <div
    class="board-grid"
    role="grid"
    tabindex="0"
    aria-label={label}
    onmouseleave={() => onhover?.(null)}
  >
    <span class="axis axis--corner" aria-hidden="true"><Crosshair size={10} /></span>
    {#each Array.from({ length: 10 }) as _, col}
      <span class="axis axis--col" aria-hidden="true">{col + 1}</span>
    {/each}
    {#each grid as row, rowIndex}
      <span class="axis axis--row" aria-hidden="true">{ROW_LABELS[rowIndex]}</span>
      {#each row as coordinate}
        {@const attack = attackAt(coordinate)}
        {@const kind = mode === 'placement' ? placementKind(coordinate) : ownShipKind(coordinate)}
        {@const preview = isPreview(coordinate)}
        {@const isSelected = selected?.row === coordinate.row && selected?.col === coordinate.col}
        <button
          class:cell--ship={Boolean(kind)}
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
          {#if kind}<span class="ship-segment" title={shipName(kind)}></span>{/if}
          {#if attack === 'MISS'}<span class="miss-marker"><Waves size={13} /></span>{/if}
          {#if attack === 'HIT' || attack === 'SUNK'}<span class="hit-marker"
              ><Flame size={14} /></span
            >{/if}
        </button>
      {/each}
    {/each}
  </div>
</div>

<style>
  .board-wrap {
    width: 100%;
    max-width: 590px;
    container-type: inline-size;
  }
  .board-grid {
    display: grid;
    grid-template-columns: 28px repeat(10, 1fr);
    grid-template-rows: 28px repeat(10, 1fr);
    width: 100%;
    aspect-ratio: 1;
    padding: 5px;
    border: 1px solid rgba(74, 179, 209, 0.28);
    border-radius: 12px;
    background: linear-gradient(135deg, rgba(5, 34, 50, 0.98), rgba(3, 20, 32, 0.98));
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
    border-top: 1px solid rgba(73, 155, 184, 0.15);
    border-left: 1px solid rgba(73, 155, 184, 0.15);
    color: white;
    background: rgba(8, 45, 63, 0.5);
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
    background: rgba(40, 107, 131, 0.36);
  }
  .ship-segment {
    position: absolute;
    inset: 18%;
    border: 1px solid rgba(144, 214, 227, 0.45);
    border-radius: 3px;
    background: linear-gradient(145deg, rgba(124, 175, 189, 0.65), rgba(42, 82, 99, 0.72));
    box-shadow:
      inset 0 1px rgba(255, 255, 255, 0.22),
      0 2px 6px rgba(0, 0, 0, 0.25);
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
  .cell--sunk .ship-segment {
    background: #492832;
    border-color: #ff7584;
  }
  .cell--sunk {
    background: rgba(111, 23, 38, 0.4);
  }
  .board-disabled {
    filter: saturate(0.72);
    opacity: 0.86;
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
