<script lang="ts">
  import type { ShipKind } from '$lib/types';

  interface Props {
    kind: ShipKind;
    orientation?: 'HORIZONTAL' | 'VERTICAL';
    state?: 'docked' | 'deployed' | 'preview' | 'invalid' | 'sunk';
    renderMode?: 'board' | 'manifest';
    label?: string;
  }

  let {
    kind,
    orientation = 'HORIZONTAL',
    state = 'deployed',
    renderMode = 'board',
    label
  }: Props = $props();
</script>

<span
  class:vertical={orientation === 'VERTICAL'}
  class={`vessel vessel--${kind} vessel--${state}`}
  aria-label={label}
  role={label ? 'img' : undefined}
>
  <svg
    viewBox={orientation === 'VERTICAL' ? '0 0 64 200' : '0 0 200 64'}
    preserveAspectRatio={renderMode === 'board' ? 'none' : 'xMidYMid meet'}
    aria-hidden={!label}
  >
    <g transform={orientation === 'VERTICAL' ? 'translate(0 200) rotate(-90)' : undefined}>
      <path class="vessel__wake" d="M1 32H199" />
      {#if kind === 'CARRIER'}
        <path class="vessel__hull" d="M2 32 18 7h162l18 25-18 25H18Z" />
        <path class="vessel__deck" d="M18 15h157l15 17-15 17H18l13-17Z" />
        <path class="vessel__island" d="M119 17h24v30h-24z" />
        <path class="vessel__detail" d="M37 26h66M37 38h66M153 26h14M153 38h14" />
      {:else if kind === 'BATTLESHIP'}
        <path class="vessel__hull" d="M2 32 22 13h154l22 19-22 19H22Z" />
        <path class="vessel__deck" d="M27 18h123l19 14-19 14H27l12-14Z" />
        <path class="vessel__island" d="M89 16h28v32H89z" />
        <path class="vessel__detail" d="M41 25h31M41 39h31M128 25h28M128 39h28" />
        <circle class="vessel__turret" cx="61" cy="32" r="7" />
        <circle class="vessel__turret" cx="151" cy="32" r="6" />
      {:else if kind === 'CRUISER'}
        <path class="vessel__hull" d="M2 32 22 17h156l20 15-20 15H22Z" />
        <path class="vessel__deck" d="M28 21h118l17 11-17 11H28l11-11Z" />
        <path class="vessel__island" d="M98 18h22v28H98z" />
        <path class="vessel__detail" d="M43 27h36M43 37h36M132 27h22M132 37h22" />
      {:else if kind === 'SUBMARINE'}
        <path
          class="vessel__hull"
          d="M2 32C14 17 31 9 61 9h78c28 0 47 8 59 23-12 15-31 23-59 23H61C31 55 14 47 2 32Z"
        />
        <path class="vessel__deck" d="M64 27h73v10H64z" />
        <path class="vessel__island" d="M98 11h21v16H98z" />
        <path class="vessel__detail" d="M25 32h32M143 32h31" />
      {:else}
        <path class="vessel__hull" d="M2 32 35 20h139l24 12-24 12H35Z" />
        <path class="vessel__deck" d="M40 24h98l17 8-17 8H40l10-8Z" />
        <path class="vessel__island" d="M88 19h19v26H88z" />
        <path class="vessel__detail" d="M49 28h26M49 36h26M118 28h30M118 36h30" />
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
    filter: drop-shadow(0 3px 4px rgba(0, 0, 0, 0.45));
    transition: filter 160ms var(--ease-out);
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
    opacity: 0.14;
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
