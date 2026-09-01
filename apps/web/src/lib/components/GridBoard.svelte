<script lang="ts">
  import { Crosshair, Flame, Waves } from '@lucide/svelte';
  import { cellsForPlacement } from '$lib/game/placement';
  import { shipName, t } from '$lib/i18n';
  import Vessel from './Vessel.svelte';
  import {
    coordinateKey,
    coordinateLabel,
    type AttackOutcome,
    type BalanceManifest,
    type Coordinate,
    type OwnBoardSnapshot,
    type ShipKind,
    type ShipPlacement,
    type TargetBoardSnapshot
  } from '$lib/types';

  interface Props {
    balance?: BalanceManifest | null;
    mode: 'placement' | 'own' | 'target';
    label: string;
    placements?: ShipPlacement[];
    ownBoard?: OwnBoardSnapshot | null;
    targetBoard?: TargetBoardSnapshot | null;
    selected?: Coordinate | null;
    selectedShipKind?: ShipKind | null;
    previewKind?: ShipKind | null;
    previewCells?: Coordinate[];
    previewValid?: boolean;
    interactive?: boolean;
    allowMarkedSelection?: boolean;
    disabled?: boolean;
    oncell?: (coordinate: Coordinate) => void;
    onhover?: (coordinate: Coordinate | null) => void;
    ondropcell?: (coordinate: Coordinate) => void;
    onshipdrag?: (kind: ShipKind) => void;
  }

  let {
    balance = null,
    mode,
    label,
    placements = [],
    ownBoard = null,
    targetBoard = null,
    selected = null,
    selectedShipKind = null,
    previewKind = null,
    previewCells = [],
    previewValid = true,
    interactive = false,
    allowMarkedSelection = false,
    disabled = false,
    oncell,
    onhover,
    ondropcell,
    onshipdrag
  }: Props = $props();

  let hovered = $state<Coordinate | null>(null);
  let activeCell = $state('0:0');

  let boardSize = $derived(balance?.boardSize ?? 10);
  let grid = $derived(
    Array.from({ length: boardSize }, (_, row) =>
      Array.from({ length: boardSize }, (_, col) => ({ row, col }))
    )
  );

  function placementKind(coordinate: Coordinate): ShipKind | null {
    return (
      placements.find((placement) =>
        cellsForPlacement(placement, balance).some(
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

  let vessels = $derived.by(() => {
    if (mode === 'placement') {
      return placements.map((placement) => {
        const cells = cellsForPlacement(placement, balance);
        return {
          kind: placement.kind,
          row: Math.min(...cells.map((cell) => cell.row)),
          col: Math.min(...cells.map((cell) => cell.col)),
          length: cells.length,
          vertical: placement.orientation === 'VERTICAL',
          state: 'deployed' as const
        };
      });
    }
    if (mode === 'own') {
      return (ownBoard?.ships ?? []).map((ship) => {
        const first = ship.cells[0];
        const second = ship.cells[1];
        const vertical = Boolean(first && second && first.col === second.col);
        return {
          kind: ship.kind,
          row: Math.min(...ship.cells.map((cell) => cell.row)),
          col: Math.min(...ship.cells.map((cell) => cell.col)),
          length: ship.cells.length,
          vertical,
          state: ship.sunk ? ('sunk' as const) : ('deployed' as const)
        };
      });
    }
    return [];
  });

  let previewVessel = $derived.by(() => {
    if (mode !== 'placement' || !previewCells.length || !previewKind) return null;
    return {
      kind: previewKind,
      row: Math.min(...previewCells.map((cell) => cell.row)),
      col: Math.min(...previewCells.map((cell) => cell.col)),
      length: previewCells.length,
      vertical: previewCells.every((cell) => cell.col === previewCells[0]?.col),
      state: previewValid ? ('preview' as const) : ('invalid' as const)
    };
  });

  function handleKeyboard(event: KeyboardEvent, coordinate: Coordinate) {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      if (!disabled) oncell?.(coordinate);
      return;
    }
    const direction =
      event.key === 'ArrowUp'
        ? { row: -1, col: 0 }
        : event.key === 'ArrowDown'
          ? { row: 1, col: 0 }
          : event.key === 'ArrowLeft'
            ? { row: 0, col: -1 }
            : event.key === 'ArrowRight'
              ? { row: 0, col: 1 }
              : null;
    if (!direction) return;
    event.preventDefault();
    const board = (event.currentTarget as HTMLElement).closest('.board-grid');
    let next = {
      row: coordinate.row + direction.row,
      col: coordinate.col + direction.col
    };
    while (next.row >= 0 && next.row < boardSize && next.col >= 0 && next.col < boardSize) {
      const key = coordinateKey(next);
      const cell = board?.querySelector<HTMLButtonElement>(`[data-cell="${key}"]`);
      if (cell && !cell.disabled) {
        activeCell = key;
        cell.focus();
        return;
      }
      next = { row: next.row + direction.row, col: next.col + direction.col };
    }
  }

  function ariaDescription(coordinate: Coordinate): string {
    const attack = attackAt(coordinate);
    const kind = mode === 'placement' ? placementKind(coordinate) : ownShipKind(coordinate);
    const parts = [coordinateLabel(coordinate)];
    if (kind) parts.push(shipName(kind));
    if (attack)
      parts.push(
        attack === 'MISS' ? $t('board.miss') : attack === 'HIT' ? $t('board.hit') : $t('board.sunk')
      );
    if (mode === 'target' && !attack) parts.push($t('board.unattacked'));
    return parts.join(', ');
  }

  function rowLabel(index: number): string {
    return index >= 0 && index < 26 ? String.fromCharCode('A'.charCodeAt(0) + index) : '?';
  }
</script>

<div
  class:board-disabled={disabled}
  class:board-wrap--placement={mode === 'placement'}
  class="board-wrap"
>
  <span class="board-wrap__bezel" aria-hidden="true"></span>
  <span class="board-hover-readout" aria-live="polite"
    >{hovered ? coordinateLabel(hovered) : '— —'}</span
  >
  <div
    class="board-grid"
    style={`--board-size:${boardSize}`}
    role="grid"
    tabindex={interactive && !disabled ? undefined : 0}
    aria-label={label}
    onmouseleave={() => {
      hovered = null;
      onhover?.(null);
    }}
  >
    <div class="board-row" role="row" aria-hidden="true">
      <span class="axis axis--corner"><Crosshair size={10} /></span>
      {#each Array.from({ length: boardSize }) as _, col (col)}
        <span class="axis axis--col">{col + 1}</span>
      {/each}
    </div>
    <div class="board-vessels" aria-hidden="true">
      {#each vessels as vessel (vessel.kind)}
        <div
          class="vessel-slot"
          class:vessel-slot--selected={mode === 'placement' && vessel.kind === selectedShipKind}
          style={`grid-row: ${vessel.row + 1} / span ${vessel.vertical ? vessel.length : 1}; grid-column: ${vessel.col + 1} / span ${vessel.vertical ? 1 : vessel.length};`}
        >
          <Vessel
            kind={vessel.kind}
            orientation={vessel.vertical ? 'VERTICAL' : 'HORIZONTAL'}
            state={vessel.state}
          />
        </div>
      {/each}
      {#if previewVessel}
        <div
          class="vessel-slot vessel-slot--preview"
          style={`grid-row: ${previewVessel.row + 1} / span ${previewVessel.vertical ? previewVessel.length : 1}; grid-column: ${previewVessel.col + 1} / span ${previewVessel.vertical ? 1 : previewVessel.length};`}
        >
          <Vessel
            kind={previewVessel.kind}
            orientation={previewVessel.vertical ? 'VERTICAL' : 'HORIZONTAL'}
            state={previewVessel.state}
          />
        </div>
      {/if}
    </div>
    {#each grid as row, rowIndex (rowIndex)}
      <div class="board-row" role="row">
        <span class="axis axis--row" aria-hidden="true">{rowLabel(rowIndex)}</span>
        {#each row as coordinate (coordinateKey(coordinate))}
          {@const attack = attackAt(coordinate)}
          {@const kind = mode === 'placement' ? placementKind(coordinate) : ownShipKind(coordinate)}
          {@const preview = isPreview(coordinate)}
          {@const isSelected = selected?.row === coordinate.row && selected?.col === coordinate.col}
          {@const cellKey = coordinateKey(coordinate)}
          <button
            class:cell--ship={Boolean(kind)}
            class:cell--ship-selected={mode === 'placement' && kind === selectedShipKind}
            class:cell--preview={preview && previewValid}
            class:cell--invalid={preview && !previewValid}
            class:cell--selected={isSelected}
            class:cell--marked={Boolean(attack)}
            class:cell--miss={attack === 'MISS'}
            class:cell--hit={attack === 'HIT'}
            class:cell--sunk={attack === 'SUNK'}
            class:cell--interactive={interactive && (!attack || allowMarkedSelection) && !disabled}
            class="board-cell"
            type="button"
            role="gridcell"
            data-cell={cellKey}
            data-testid={`${mode}-cell-${coordinate.row}-${coordinate.col}`}
            aria-label={ariaDescription(coordinate)}
            aria-selected={isSelected}
            tabindex={interactive &&
            !disabled &&
            (!attack || allowMarkedSelection) &&
            activeCell === cellKey
              ? 0
              : -1}
            disabled={disabled ||
              (mode === 'target' && ((!allowMarkedSelection && Boolean(attack)) || !interactive))}
            draggable={mode === 'placement' && Boolean(kind) && !disabled}
            onclick={() => oncell?.(coordinate)}
            onkeydown={(event) => handleKeyboard(event, coordinate)}
            onmouseenter={() => {
              hovered = coordinate;
              onhover?.(coordinate);
            }}
            onfocus={() => {
              activeCell = cellKey;
              hovered = coordinate;
              onhover?.(coordinate);
            }}
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
            {#if attack === 'MISS'}<span class="miss-marker"><i></i><Waves size={13} /></span>{/if}
            {#if attack === 'HIT' || attack === 'SUNK'}<span class="hit-marker"
                ><i></i><Flame size={14} /></span
              >{/if}
          </button>
        {/each}
      </div>
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
  .board-wrap--placement {
    max-width: 780px;
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
    background: radial-gradient(ellipse, rgba(25, 200, 218, 0.16), transparent 72%);
    opacity: 0.22;
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
    --water-glow: rgba(34, 180, 190, 0.18);
    --water-line: rgba(34, 101, 119, 0.08);
    --water-tint-a: rgba(4, 35, 48, 0.72);
    --water-tint-b: rgba(1, 16, 26, 0.84);
    position: relative;
    display: grid;
    grid-template-columns: 28px repeat(var(--board-size), 1fr);
    grid-template-rows: 28px repeat(var(--board-size), 1fr);
    width: 100%;
    aspect-ratio: 1;
    padding: 5px;
    border: 1px solid rgba(100, 193, 208, 0.3);
    overflow: hidden;
    border-radius: 8px 3px 8px 3px;
    background:
      radial-gradient(circle at 28% 16%, var(--water-glow), transparent 30%),
      repeating-linear-gradient(120deg, var(--water-line) 0 1px, transparent 1px 7px),
      linear-gradient(135deg, var(--water-tint-a), var(--water-tint-b)),
      url('/art/ocean-command-surface-v1.webp') center / cover;
    background-blend-mode: screen, screen, multiply, normal;
    box-shadow:
      inset 0 0 45px rgba(30, 166, 190, 0.055),
      0 18px 48px rgba(0, 0, 0, 0.2);
    user-select: none;
    touch-action: manipulation;
    contain: layout paint;
  }

  .board-hover-readout {
    position: absolute;
    z-index: 8;
    top: 19px;
    right: 22px;
    color: var(--cyan-300);
    font: 700 10px var(--font-display);
    letter-spacing: 0.14em;
    opacity: 0.86;
  }

  .board-row {
    display: contents;
  }

  .board-vessels {
    position: absolute;
    z-index: 3;
    top: 33px;
    right: 5px;
    bottom: 5px;
    left: 33px;
    display: grid;
    grid-template-columns: repeat(var(--board-size), minmax(0, 1fr));
    grid-template-rows: repeat(var(--board-size), minmax(0, 1fr));
    pointer-events: none;
  }

  .vessel-slot {
    position: relative;
    z-index: 1;
    min-width: 0;
    min-height: 0;
    padding: 0;
  }

  .vessel-slot--preview {
    z-index: 4;
    padding: 0;
  }
  .vessel-slot--selected {
    z-index: 5;
  }
  .vessel-slot--selected :global(.vessel) {
    filter: drop-shadow(0 0 7px rgba(255, 209, 107, 0.48));
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
      linear-gradient(145deg, rgba(8, 55, 68, 0.39), rgba(4, 31, 45, 0.56));
    cursor: default;
    transition:
      background 0.12s ease,
      box-shadow 0.12s ease,
      transform 0.12s ease;
  }
  .board-row > .board-cell:first-of-type {
    border-left: 0;
  }
  .board-cell:disabled {
    opacity: 1;
  }

  .board-cell.cell--marked {
    z-index: 6;
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
  .cell--ship-selected {
    z-index: 2;
    background: rgba(237, 181, 82, 0.15);
    box-shadow: inset 0 0 0 1px rgba(255, 209, 107, 0.56);
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
    animation: target-lock 560ms var(--ease-out) infinite;
    will-change: transform, opacity;
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
    animation: water-ring 520ms ease-out 1 both;
  }
  .hit-marker {
    color: #fff0d8;
    background: radial-gradient(
      circle,
      rgba(255, 79, 56, 0.72),
      rgba(255, 99, 48, 0.2) 38%,
      transparent 70%
    );
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
    animation: impact-core 420ms ease-out 1 both;
  }
  .hit-marker :global(svg) {
    position: relative;
    z-index: 2;
  }
  .cell--sunk {
    background: rgba(111, 23, 38, 0.4);
  }

  :global(html[data-board-theme='sonar']) .board-grid {
    --water-glow: rgba(72, 244, 236, 0.28);
    --water-line: rgba(71, 217, 223, 0.14);
    --water-tint-a: rgba(0, 55, 59, 0.58);
    --water-tint-b: rgba(0, 19, 24, 0.82);
  }

  :global(html[data-board-theme='ice']) .board-grid {
    --water-glow: rgba(197, 238, 255, 0.22);
    --water-line: rgba(165, 209, 235, 0.13);
    --water-tint-a: rgba(31, 66, 91, 0.62);
    --water-tint-b: rgba(5, 25, 41, 0.84);
  }

  :global(html[data-effect-theme='plasma']) .hit-marker {
    color: #f4e9ff;
    background: radial-gradient(
      circle,
      rgba(174, 91, 255, 0.78),
      rgba(82, 73, 242, 0.22) 40%,
      transparent 70%
    );
  }

  :global(html[data-effect-theme='plasma']) .hit-marker i {
    background: #f8efff;
    box-shadow:
      0 0 8px #f2dbff,
      0 0 17px #a86cff,
      0 0 24px rgba(91, 81, 240, 0.7);
  }

  :global(html[data-effect-theme='ordnance']) .hit-marker {
    color: #fff6d8;
    background: radial-gradient(
      circle,
      rgba(255, 190, 57, 0.78),
      rgba(186, 67, 22, 0.26) 42%,
      transparent 72%
    );
  }

  :global(html[data-effect-theme='ordnance']) .hit-marker i {
    background: #fff8db;
    box-shadow:
      0 0 7px #fff0ae,
      0 0 14px #ffb434,
      0 0 22px rgba(181, 47, 20, 0.66);
  }

  :global(html[data-effect-quality='low']) .board-wrap::after {
    opacity: 0.12;
  }

  :global(html[data-effect-quality='low']) .hit-marker,
  :global(html[data-effect-quality='low']) .vessel-slot--selected :global(.vessel) {
    filter: none;
  }

  :global(html[data-effect-quality='low']) .hit-marker i {
    box-shadow: 0 0 8px currentColor;
    animation-iteration-count: 1;
  }

  :global(html[data-effect-quality='minimal']) .board-grid {
    background:
      radial-gradient(circle at 28% 16%, rgba(30, 130, 151, 0.12), transparent 30%),
      linear-gradient(135deg, #062631, #03131e);
    background-blend-mode: normal;
    box-shadow: none;
  }

  :global(html[data-effect-quality='minimal']) .board-wrap::after {
    display: none;
  }

  :global(html[data-effect-quality='minimal']) .hit-marker,
  :global(html[data-effect-quality='minimal']) .vessel-slot--selected :global(.vessel) {
    filter: none;
  }

  :global(html[data-effect-quality='minimal']) .hit-marker i,
  :global(html[data-effect-quality='minimal']) .miss-marker i,
  :global(html[data-effect-quality='minimal']) .cell--selected::before {
    animation: none;
    box-shadow: none;
  }
  .board-disabled {
    filter: saturate(0.72);
    opacity: 0.86;
  }
  @keyframes target-lock {
    50% {
      transform: scale(0.72);
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
  @media (prefers-reduced-motion: reduce) {
    .cell--selected::before,
    .miss-marker i,
    .hit-marker i {
      animation: none;
    }
  }
  @container (max-width:430px) {
    .board-grid {
      grid-template-columns: 22px repeat(10, 1fr);
      grid-template-rows: 22px repeat(10, 1fr);
      padding: 3px;
      border-radius: 9px;
    }
    .vessel-slot {
      padding: 2%;
    }
  }
</style>
