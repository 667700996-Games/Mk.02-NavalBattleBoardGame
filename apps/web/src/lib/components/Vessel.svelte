<script lang="ts">
  import type { ShipKind } from '$lib/types';

  interface Props {
    kind: ShipKind;
    orientation?: 'HORIZONTAL' | 'VERTICAL';
    state?: 'docked' | 'deployed' | 'preview' | 'invalid' | 'sunk';
    label?: string;
  }

  let {
    kind,
    orientation = 'HORIZONTAL',
    state = 'deployed',
    label
  }: Props = $props();
</script>

<span
  class:vertical={orientation === 'VERTICAL'}
  class={`vessel vessel--${kind} vessel--${state}`}
  aria-label={label}
  role={label ? 'img' : undefined}
>
  <svg viewBox={orientation === 'VERTICAL' ? '0 0 64 200' : '0 0 200 64'} aria-hidden={!label}>
    <g transform={orientation === 'VERTICAL' ? 'translate(0 200) rotate(-90)' : undefined}>
      <path class="vessel__wake" d="M8 32H192" />
      {#if kind === 'CARRIER'}
        <path class="vessel__hull" d="M8 32 25 12h150l17 20-17 20H25Z" />
        <path class="vessel__deck" d="M29 17h132l14 15-14 15H29l10-15Z" />
        <path class="vessel__island" d="M119 19h22v26h-22z" />
        <path class="vessel__detail" d="M47 27h55M47 37h55M151 27h9M151 37h9" />
      {:else if kind === 'BATTLESHIP'}
        <path class="vessel__hull" d="M10 32 28 16h142l20 16-20 16H28Z" />
        <path class="vessel__deck" d="M33 20h108l15 12-15 12H33l10-12Z" />
        <path class="vessel__island" d="M92 18h25v28H92z" />
        <path class="vessel__detail" d="M49 26h25M49 38h25M128 26h22M128 38h22" />
        <circle class="vessel__turret" cx="61" cy="32" r="7" />
        <circle class="vessel__turret" cx="151" cy="32" r="6" />
      {:else if kind === 'CRUISER'}
        <path class="vessel__hull" d="M12 32 29 19h142l17 13-17 13H29Z" />
        <path class="vessel__deck" d="M36 23h102l12 9-12 9H36l9-9Z" />
        <path class="vessel__island" d="M100 20h19v24h-19z" />
        <path class="vessel__detail" d="M53 28h32M53 36h32M132 28h17M132 36h17" />
      {:else if kind === 'SUBMARINE'}
        <path class="vessel__hull" d="M15 32c10-14 27-21 54-21h61c25 0 43 7 55 21-12 14-30 21-55 21H69c-27 0-44-7-54-21Z" />
        <path class="vessel__deck" d="M75 27h46v10H75z" />
        <path class="vessel__island" d="M101 13h18v14h-18z" />
        <path class="vessel__detail" d="M37 32h24M134 32h27" />
      {:else}
        <path class="vessel__hull" d="M7 32 39 22h129l25 10-25 10H39Z" />
        <path class="vessel__deck" d="M48 25h80l14 7-14 7H48l8-7Z" />
        <path class="vessel__island" d="M90 21h17v22H90z" />
        <path class="vessel__detail" d="M57 29h22M57 35h22M119 29h25M119 35h25" />
      {/if}
    </g>
  </svg>
</span>

<style>
  .vessel {
    position: relative;
    display: block;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    color: #b8e9eb;
    filter: drop-shadow(0 4px 5px rgba(0, 0, 0, 0.45));
  }

  .vessel svg {
    display: block;
    width: 100%;
    height: 100%;
    overflow: visible;
  }

  .vessel__hull {
    fill: #163b4b;
    stroke: currentColor;
    stroke-width: 1.6;
    vector-effect: non-scaling-stroke;
  }

  .vessel__deck {
    fill: #265c6c;
    stroke: rgba(206, 248, 247, 0.42);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
  }

  .vessel__island {
    fill: #0c2633;
    stroke: rgba(205, 247, 246, 0.56);
    stroke-width: 1.2;
    vector-effect: non-scaling-stroke;
  }

  .vessel__detail,
  .vessel__wake {
    fill: none;
    stroke: rgba(207, 249, 247, 0.56);
    stroke-width: 1.6;
    vector-effect: non-scaling-stroke;
  }

  .vessel__wake {
    opacity: 0.2;
    stroke-dasharray: 4 8;
  }

  .vessel__turret {
    fill: #4e8791;
    stroke: rgba(225, 255, 253, 0.58);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
  }

  .vessel--docked {
    color: #88bfc5;
    filter: none;
  }

  .vessel--preview {
    color: #9effe3;
    filter: drop-shadow(0 0 6px rgba(79, 226, 173, 0.36));
  }

  .vessel--preview .vessel__hull,
  .vessel--preview .vessel__deck {
    fill: rgba(22, 129, 122, 0.76);
  }

  .vessel--invalid {
    color: #ff9ba6;
    filter: drop-shadow(0 0 6px rgba(238, 86, 103, 0.42));
  }

  .vessel--invalid .vessel__hull,
  .vessel--invalid .vessel__deck {
    fill: rgba(127, 33, 51, 0.82);
  }

  .vessel--sunk {
    color: #ff8d98;
    filter: saturate(0.55) drop-shadow(0 0 5px rgba(238, 86, 103, 0.42));
  }

  .vessel--sunk .vessel__hull,
  .vessel--sunk .vessel__deck {
    fill: #3d2931;
  }

  .vessel--sunk .vessel__detail {
    stroke: rgba(255, 157, 166, 0.7);
  }
</style>
