<script lang="ts">
  import { untrack } from 'svelte';
  import { Check, Dices, Grip, RotateCw, Trash2 } from '@lucide/svelte';
  import GridBoard from './GridBoard.svelte';
  import InputPrompt from './InputPrompt.svelte';
  import Vessel from './Vessel.svelte';
  import {
    autoPlaceFleet,
    rotatePlacement,
    validateFleet,
    validatePlacement
  } from '$lib/game/placement';
  import {
    fleetForBalance,
    type BalanceManifest,
    type Coordinate,
    type Orientation,
    type ShipKind,
    type ShipPlacement
  } from '$lib/types';
  import { sounds } from '$lib/sound';
  import { preferences } from '$lib/stores';
  import { formatNumber, shipMessageKey, t, type MessageKey } from '$lib/i18n';

  interface Props {
    balance: BalanceManifest;
    initialPlacement?: ShipPlacement[] | null;
    confirmed?: boolean;
    submitting?: boolean;
    onconfirm: (placements: ShipPlacement[]) => void;
  }
  let {
    balance,
    initialPlacement = null,
    confirmed = false,
    submitting = false,
    onconfirm
  }: Props = $props();

  let balanceFleet = $derived(fleetForBalance(balance));

  let placements = $state<ShipPlacement[]>(
    untrack(() => (initialPlacement ? structuredClone(initialPlacement) : []))
  );
  let selectedKind = $state<ShipKind | null>(
    untrack(
      () =>
        fleetForBalance(balance).find(
          (ship) => !placements.some((placement) => placement.kind === ship.kind)
        )?.kind ?? 'CARRIER'
    )
  );
  let orientation = $state<Orientation>('HORIZONTAL');
  let hover = $state<Coordinate | null>(null);
  let noticeKey = $state<MessageKey>('placement.initialNotice');
  let noticeShip = $state<ShipKind | null>(null);
  let autoDeploying = $state(false);

  let candidate = $derived<ShipPlacement | null>(
    selectedKind && hover ? { kind: selectedKind, origin: hover, orientation } : null
  );
  let preview = $derived(
    candidate ? validatePlacement(candidate, placements, balance) : { valid: true, cells: [] }
  );
  let fleet = $derived(validateFleet(placements, balance));

  function selectShip(kind: ShipKind) {
    if (confirmed || autoDeploying) return;
    selectedKind = kind;
    orientation =
      placements.find((placement) => placement.kind === kind)?.orientation ?? orientation;
    sounds.select();
  }

  function place(coordinate: Coordinate) {
    if (!selectedKind || confirmed || autoDeploying) return;
    const next: ShipPlacement = { kind: selectedKind, origin: coordinate, orientation };
    const validation = validatePlacement(next, placements, balance);
    if (!validation.valid) {
      noticeKey = validation.reason === 'OVERLAP' ? 'placement.overlap' : 'placement.outOfBounds';
      noticeShip = null;
      return;
    }
    placements = [...placements.filter((placement) => placement.kind !== selectedKind), next];
    noticeKey = 'placement.placed';
    noticeShip = selectedKind;
    selectedKind =
      balanceFleet.find((ship) => !placements.some((placement) => placement.kind === ship.kind))
        ?.kind ?? selectedKind;
    if (selectedKind)
      orientation =
        placements.find((placement) => placement.kind === selectedKind)?.orientation ?? orientation;
    sounds.place();
  }

  function rotate() {
    if (!selectedKind || confirmed || autoDeploying) return;
    const existing = placements.find((placement) => placement.kind === selectedKind);
    if (!existing) {
      orientation = orientation === 'HORIZONTAL' ? 'VERTICAL' : 'HORIZONTAL';
      return;
    }
    const rotated = rotatePlacement(existing);
    const validation = validatePlacement(rotated, placements, balance);
    if (!validation.valid) {
      noticeKey = 'placement.rotationBlocked';
      noticeShip = null;
      return;
    }
    placements = [...placements.filter((placement) => placement.kind !== selectedKind), rotated];
    orientation = rotated.orientation;
    noticeKey = 'placement.rotated';
    noticeShip = selectedKind;
    sounds.rotate();
    sounds.select();
  }

  async function autoPlace() {
    if (confirmed || autoDeploying) return;
    const deployed = autoPlaceFleet(Math.random, balance);
    autoDeploying = true;
    placements = [];
    noticeKey = 'placement.autoRunning';
    noticeShip = null;
    if ($preferences.reducedMotion) {
      placements = deployed;
    } else {
      for (const [index] of deployed.entries()) {
        await new Promise((resolve) => setTimeout(resolve, 85));
        placements = [...deployed.slice(0, index + 1)];
        sounds.place();
      }
    }
    selectedKind = 'CARRIER';
    orientation = deployed[0]?.orientation ?? 'HORIZONTAL';
    noticeKey = 'placement.autoComplete';
    autoDeploying = false;
  }

  function reset() {
    if (autoDeploying) return;
    placements = [];
    selectedKind = 'CARRIER';
    orientation = 'HORIZONTAL';
    noticeKey = 'placement.resetNotice';
    noticeShip = null;
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
      <p class="eyebrow">{$t('placement.eyebrow')}</p>
      <h2 id="placement-title">{$t('placement.title')}</h2>
      <p>{$t('placement.description')}</p>
    </div>
    <span class:success={fleet.valid} class="status-pill"
      ><span class="status-dot"></span>{$t('placement.status', {
        placed: formatNumber(placements.length),
        total: formatNumber(balanceFleet.length)
      })}</span
    >
  </header>

  <div class="deployment-steps" aria-label={$t('placement.progress')}>
    <span class="active"
      ><i>01</i><strong>{$t('placement.selectShip')}</strong><small
        >{$t('placement.selectShipCode')}</small
      ></span
    >
    <span class:active={placements.length > 0}
      ><i>02</i><strong>{$t('placement.mapSector')}</strong><small
        >{$t('placement.mapSectorCode')}</small
      ></span
    >
    <span class:active={fleet.valid}
      ><i>03</i><strong>{$t('placement.lockFormation')}</strong><small
        >{$t('placement.lockFormationCode')}</small
      ></span
    >
  </div>

  <div class="placement__layout">
    <div class="placement__board panel">
      <div class="board-toolbar">
        <span
          ><i></i>{$t('placement.sectorOrientation', {
            size: formatNumber(balance.boardSize),
            orientation:
              orientation === 'HORIZONTAL' ? $t('placement.horizontal') : $t('placement.vertical')
          })}</span
        ><InputPrompt context="placement" compact />
      </div>
      <GridBoard
        {balance}
        mode="placement"
        label={$t('placement.boardLabel')}
        {placements}
        previewCells={preview.cells ?? []}
        previewKind={selectedKind}
        previewValid={preview.valid}
        selectedShipKind={selectedKind}
        interactive={!confirmed}
        disabled={confirmed || submitting}
        oncell={place}
        onhover={(coordinate) => (hover = coordinate)}
        ondropcell={place}
        onshipdrag={selectShip}
      />
      <p class:danger={!preview.valid} class="placement-notice" aria-live="polite">
        {$t(noticeKey, {
          ship: noticeShip ? $t(shipMessageKey(noticeShip)) : ''
        })}
      </p>
    </div>

    <aside class="fleet-dock panel">
      <div class="fleet-dock__heading">
        <div>
          <span>{$t('placement.manifestCode')}</span><strong>{$t('placement.manifest')}</strong>
        </div>
        <Grip size={18} />
      </div>
      <div class="fleet-list">
        {#each balanceFleet as ship (ship.kind)}
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
              ><strong>{$t(shipMessageKey(ship.kind))}</strong><small
                >{$t('placement.cells', { count: formatNumber(ship.size) })}</small
              ></span
            >
            <span class="ship-shape" style={`--ship-cells: ${ship.size}`} aria-hidden="true"
              ><Vessel
                kind={ship.kind}
                state={placed ? 'deployed' : 'docked'}
                renderMode="manifest"
              /></span
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
          disabled={confirmed || autoDeploying || !selectedKind}
          ><RotateCw size={15} /> {$t('placement.rotate')}</button
        >
        <button
          class="button button--small"
          type="button"
          onclick={autoPlace}
          disabled={confirmed || autoDeploying}><Dices size={15} /> {$t('placement.auto')}</button
        >
        <button
          class="button button--small button--danger"
          type="button"
          onclick={reset}
          disabled={confirmed || autoDeploying || placements.length === 0}
          ><Trash2 size={15} /> {$t('placement.reset')}</button
        >
      </div>
      <div class="confirm-zone">
        <p>
          {fleet.valid ? $t('placement.ready') : $t('placement.incomplete')}
        </p>
        <button
          class="button button--primary button--wide"
          type="button"
          disabled={!fleet.valid || confirmed || submitting}
          onclick={() => {
            sounds.confirm();
            onconfirm(placements);
          }}
          ><Check size={17} />
          {submitting
            ? $t('placement.confirming')
            : confirmed
              ? $t('placement.confirmed')
              : $t('placement.confirm')}</button
        >
        <small>{$t('placement.lockWarning')}</small>
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
  .board-toolbar :global(.input-prompt) {
    max-width: 70%;
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

  .placement {
    position: relative;
    overflow-x: clip;
    padding: 4px 0 28px;
  }
  .placement::before {
    position: absolute;
    z-index: -1;
    top: 52px;
    right: -14%;
    bottom: 2%;
    left: -14%;
    content: '';
    opacity: 0.35;
    pointer-events: none;
    background: radial-gradient(ellipse at center, rgba(35, 129, 140, 0.12), transparent 62%);
  }
  .placement__heading {
    padding-bottom: 18px;
    border-bottom: 1px solid var(--line);
  }
  .placement__heading h2 {
    font-family: var(--font-display);
    font-size: clamp(30px, 4vw, 44px);
    font-weight: 600;
    letter-spacing: 0.02em;
  }
  .placement__heading p:last-child {
    color: var(--ink-400);
  }
  .deployment-steps {
    border: 0;
    border-top: 1px solid var(--line);
    border-bottom: 1px solid var(--line);
    border-radius: 0;
    background: transparent;
  }
  .deployment-steps span {
    min-height: 62px;
    border-right: 1px solid var(--line);
    background: rgba(2, 12, 18, 0.25);
  }
  .deployment-steps span.active {
    background: linear-gradient(90deg, rgba(83, 233, 232, 0.08), transparent);
  }
  .placement__layout {
    grid-template-columns: minmax(0, 1.5fr) minmax(330px, 0.5fr);
    gap: 16px;
  }
  .placement__board {
    border-radius: 10px 3px 10px 3px;
    border-color: rgba(83, 233, 232, 0.24);
    background: linear-gradient(145deg, rgba(6, 29, 38, 0.9), rgba(2, 13, 20, 0.94));
  }
  .placement__board :global(.board-wrap) {
    margin-inline: auto;
  }
  .board-toolbar {
    padding: 0 3px 8px;
    border-bottom: 1px solid var(--line);
  }
  .board-toolbar span {
    color: var(--tactical);
    font-size: 10px;
  }
  .placement-notice {
    min-height: 23px;
    padding: 7px 9px;
    border-left: 2px solid var(--tactical);
    background: rgba(83, 233, 232, 0.045);
    color: var(--ink-300);
  }
  .placement-notice.danger {
    border-color: var(--critical);
    color: #ff9ca5;
    background: rgba(238, 86, 103, 0.06);
  }
  .fleet-dock {
    border-radius: 10px 3px 10px 3px;
    border-color: rgba(130, 188, 199, 0.18);
    background: linear-gradient(165deg, rgba(8, 29, 37, 0.92), rgba(2, 13, 20, 0.96));
  }
  .fleet-dock__heading {
    border-bottom-color: var(--line);
  }
  .fleet-dock__heading strong {
    font-family: var(--font-display);
    font-size: 17px;
    letter-spacing: 0.04em;
  }
  .fleet-item {
    grid-template-columns: minmax(0, 1fr) 126px;
    border-radius: 5px 2px 5px 2px;
    background: rgba(2, 15, 22, 0.62);
  }
  .fleet-item:hover,
  .fleet-item.selected {
    border-color: var(--line-active);
    background: rgba(83, 233, 232, 0.08);
    transform: translateX(0);
  }
  .fleet-item.placed {
    border-left: 2px solid var(--safe);
  }
  .fleet-item:nth-child(4) .ship-shape {
    transform: none;
  }
  .ship-shape {
    display: block;
    width: calc(21px * var(--ship-cells));
    max-width: 110px;
    aspect-ratio: 25 / 8;
    justify-self: center;
    align-self: center;
  }
  .ship-shape :global(.vessel) {
    width: 100%;
    height: 100%;
  }
  .fleet-item.selected .ship-shape :global(.vessel) {
    filter: drop-shadow(0 0 5px rgba(83, 233, 232, 0.42));
  }
  .confirm-zone {
    background: linear-gradient(180deg, rgba(83, 233, 232, 0.04), transparent);
  }
  .confirm-zone .button--primary {
    min-height: 48px;
  }
  @media (max-width: 930px) {
    .placement__layout {
      grid-template-columns: 1fr;
    }
  }
</style>
