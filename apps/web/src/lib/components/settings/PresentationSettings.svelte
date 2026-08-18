<script lang="ts">
  import { Gauge, Palette, ShieldCheck } from '@lucide/svelte';
  import { preferences, type CosmeticLoadout, type EffectQuality } from '$lib/stores';
  import { t } from '$lib/i18n';

  const cosmeticOptions = {
    fleetSkin: ['steel', 'arctic', 'ember'],
    boardTheme: ['abyss', 'sonar', 'ice'],
    effectTheme: ['signal', 'plasma', 'ordnance'],
    profileEmblem: ['anchor', 'trident', 'compass'],
    presentationFrame: ['command', 'stealth', 'veteran']
  } as const satisfies { [Key in keyof CosmeticLoadout]: readonly CosmeticLoadout[Key][] };

  function setQuality(event: Event) {
    const effectQuality = (event.currentTarget as HTMLSelectElement).value as EffectQuality;
    preferences.update((current) => ({ ...current, effectQuality }));
  }

  function setCosmetic<Key extends keyof CosmeticLoadout>(key: Key, event: Event) {
    const value = (event.currentTarget as HTMLSelectElement).value as CosmeticLoadout[Key];
    preferences.update((current) => ({
      ...current,
      cosmetics: { ...current.cosmetics, [key]: value }
    }));
  }
</script>

<section class="presentation-settings panel" aria-labelledby="presentation-title">
  <header>
    <span><Palette size={20} /></span>
    <div>
      <small>{$t('cosmetics.eyebrow')}</small>
      <h2 id="presentation-title">{$t('cosmetics.title')}</h2>
      <p>{$t('cosmetics.description')}</p>
    </div>
  </header>

  <div class="cosmetic-preview" aria-label={$t('cosmetics.preview')}>
    <span class="preview-emblem"><i></i></span>
    <div class="preview-board"><i class="preview-vessel"></i><b></b><em></em></div>
    <span class="preview-copy"
      ><strong>{$t('cosmetics.previewTitle')}</strong><small>{$t('cosmetics.previewSafe')}</small
      ></span
    >
  </div>

  <div class="cosmetic-grid">
    <label>
      <span
        ><strong>{$t('cosmetics.fleetSkin')}</strong><small>{$t('cosmetics.fleetSkinHelp')}</small
        ></span
      >
      <select
        value={$preferences.cosmetics.fleetSkin}
        aria-label={$t('cosmetics.fleetSkin')}
        onchange={(event) => setCosmetic('fleetSkin', event)}
      >
        {#each cosmeticOptions.fleetSkin as value (value)}<option {value}
            >{$t(`cosmetics.option.${value}` as 'cosmetics.option.steel')}</option
          >{/each}
      </select>
    </label>
    <label>
      <span
        ><strong>{$t('cosmetics.boardTheme')}</strong><small>{$t('cosmetics.boardThemeHelp')}</small
        ></span
      >
      <select
        value={$preferences.cosmetics.boardTheme}
        aria-label={$t('cosmetics.boardTheme')}
        onchange={(event) => setCosmetic('boardTheme', event)}
      >
        {#each cosmeticOptions.boardTheme as value (value)}<option {value}
            >{$t(`cosmetics.option.${value}` as 'cosmetics.option.abyss')}</option
          >{/each}
      </select>
    </label>
    <label>
      <span
        ><strong>{$t('cosmetics.effectTheme')}</strong><small
          >{$t('cosmetics.effectThemeHelp')}</small
        ></span
      >
      <select
        value={$preferences.cosmetics.effectTheme}
        aria-label={$t('cosmetics.effectTheme')}
        onchange={(event) => setCosmetic('effectTheme', event)}
      >
        {#each cosmeticOptions.effectTheme as value (value)}<option {value}
            >{$t(`cosmetics.option.${value}` as 'cosmetics.option.signal')}</option
          >{/each}
      </select>
    </label>
    <label>
      <span
        ><strong>{$t('cosmetics.profileEmblem')}</strong><small
          >{$t('cosmetics.profileEmblemHelp')}</small
        ></span
      >
      <select
        value={$preferences.cosmetics.profileEmblem}
        aria-label={$t('cosmetics.profileEmblem')}
        onchange={(event) => setCosmetic('profileEmblem', event)}
      >
        {#each cosmeticOptions.profileEmblem as value (value)}<option {value}
            >{$t(`cosmetics.option.${value}` as 'cosmetics.option.anchor')}</option
          >{/each}
      </select>
    </label>
    <label>
      <span
        ><strong>{$t('cosmetics.presentationFrame')}</strong><small
          >{$t('cosmetics.presentationFrameHelp')}</small
        ></span
      >
      <select
        value={$preferences.cosmetics.presentationFrame}
        aria-label={$t('cosmetics.presentationFrame')}
        onchange={(event) => setCosmetic('presentationFrame', event)}
      >
        {#each cosmeticOptions.presentationFrame as value (value)}<option {value}
            >{$t(`cosmetics.option.${value}` as 'cosmetics.option.command')}</option
          >{/each}
      </select>
    </label>
  </div>

  <div class="quality-row">
    <span><Gauge size={19} /></span>
    <div>
      <strong>{$t('cosmetics.effectQuality')}</strong>
      <p>{$t('cosmetics.effectQualityHelp')}</p>
    </div>
    <select
      value={$preferences.effectQuality}
      onchange={setQuality}
      aria-label={$t('cosmetics.effectQuality')}
    >
      <option value="high">{$t('cosmetics.quality.high')}</option>
      <option value="low">{$t('cosmetics.quality.low')}</option>
      <option value="minimal">{$t('cosmetics.quality.minimal')}</option>
    </select>
  </div>
  <aside>
    <ShieldCheck size={17} /><span
      ><strong>{$t('cosmetics.fairPlay')}</strong>{$t('cosmetics.fairPlayHelp')}</span
    >
  </aside>
</section>

<style>
  .presentation-settings {
    padding: var(--space-3);
  }
  header {
    display: flex;
    align-items: flex-start;
    gap: 0.8rem;
  }
  header > span {
    display: grid;
    place-items: center;
    width: 2.5rem;
    height: 2.5rem;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    color: var(--cyan-300);
    background: rgba(20, 75, 91, 0.38);
  }
  header small {
    color: var(--cyan-400);
    font: 0.72rem var(--font-display);
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }
  header h2 {
    margin: 0.2rem 0;
    font-family: var(--font-display);
  }
  header p {
    margin: 0;
    color: var(--ink-300);
  }
  .cosmetic-preview {
    position: relative;
    display: grid;
    grid-template-columns: auto 8rem 1fr;
    align-items: center;
    gap: 1rem;
    margin: var(--space-3) 0;
    padding: 1rem;
    overflow: hidden;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-sm);
    background: linear-gradient(135deg, rgba(14, 54, 70, 0.85), rgba(2, 12, 20, 0.94));
  }
  .preview-emblem {
    display: grid;
    place-items: center;
    width: 3.25rem;
    height: 3.25rem;
    border: 1px solid var(--cyan-400);
    border-radius: 50%;
  }
  .preview-emblem i {
    width: 1.3rem;
    height: 1.3rem;
    border: 2px solid var(--cyan-300);
    border-top-color: transparent;
    border-radius: 50% 50% 45% 45%;
    transform: rotate(45deg);
  }
  .preview-board {
    position: relative;
    height: 4rem;
    overflow: hidden;
    border: 1px solid rgba(111, 244, 246, 0.35);
    border-radius: 0.25rem;
    background:
      linear-gradient(rgba(3, 18, 28, 0.28), rgba(3, 18, 28, 0.78)),
      url('/art/ocean-command-surface-v1.webp') center/cover;
  }
  .preview-board::after {
    position: absolute;
    inset: 0;
    content: '';
    background-image:
      linear-gradient(rgba(111, 244, 246, 0.16) 1px, transparent 1px),
      linear-gradient(90deg, rgba(111, 244, 246, 0.16) 1px, transparent 1px);
    background-size: 20% 20%;
  }
  .preview-vessel {
    position: absolute;
    z-index: 2;
    top: 45%;
    left: 18%;
    width: 60%;
    height: 0.45rem;
    border: 1px solid var(--cyan-200);
    border-radius: 100% 35%;
    background: #265c6c;
    transform: rotate(-12deg);
  }
  .preview-board b,
  .preview-board em {
    position: absolute;
    z-index: 3;
    width: 0.8rem;
    height: 0.8rem;
    border-radius: 50%;
  }
  .preview-board b {
    top: 15%;
    right: 18%;
    border: 1px solid var(--cyan-300);
  }
  .preview-board em {
    right: 28%;
    bottom: 13%;
    background: var(--orange-400);
    box-shadow: 0 0 0.8rem var(--orange-400);
  }
  .preview-copy strong,
  .preview-copy small {
    display: block;
  }
  .preview-copy small {
    margin-top: 0.25rem;
    color: var(--ink-300);
  }
  .cosmetic-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.65rem;
  }
  .cosmetic-grid label,
  .quality-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.8rem;
    padding: 0.8rem;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    background: rgba(3, 15, 24, 0.48);
  }
  .cosmetic-grid strong,
  .cosmetic-grid small {
    display: block;
  }
  .cosmetic-grid small {
    margin-top: 0.2rem;
    color: var(--ink-400);
    font-size: 0.72rem;
  }
  select {
    min-height: 2.5rem;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-xs);
    color: var(--ink-100);
    background: #071a25;
    padding: 0.45rem 0.65rem;
  }
  .quality-row {
    grid-template-columns: auto minmax(0, 1fr) auto;
    margin-top: 0.65rem;
  }
  .quality-row > span {
    color: var(--cyan-300);
  }
  .quality-row p {
    margin: 0.2rem 0 0;
    color: var(--ink-400);
  }
  aside {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    margin-top: 0.8rem;
    color: var(--ink-300);
    font-size: 0.78rem;
  }
  aside :global(svg) {
    flex: none;
    color: var(--safe);
  }
  aside strong {
    margin-right: 0.35rem;
    color: var(--safe);
  }
  :global(html[data-board-theme='sonar']) .preview-board {
    background:
      linear-gradient(rgba(0, 70, 68, 0.18), rgba(0, 20, 22, 0.72)),
      url('/art/ocean-command-surface-v1.webp') center/cover;
  }
  :global(html[data-board-theme='ice']) .preview-board {
    background:
      linear-gradient(rgba(73, 111, 139, 0.2), rgba(9, 31, 48, 0.7)),
      url('/art/ocean-command-surface-v1.webp') center/cover;
  }
  :global(html[data-fleet-skin='arctic']) .preview-vessel {
    border-color: #e4f7ff;
    background: #7096a8;
  }
  :global(html[data-fleet-skin='ember']) .preview-vessel {
    border-color: #ffd4a6;
    background: #80503a;
  }
  :global(html[data-effect-theme='plasma']) .preview-board em {
    background: #b56fff;
    box-shadow: 0 0 0.8rem #a86cff;
  }
  :global(html[data-effect-theme='ordnance']) .preview-board em {
    background: #ffc653;
    box-shadow: 0 0 0.8rem #ff9d32;
  }
  :global(html[data-profile-emblem='trident']) .preview-emblem {
    border-radius: 28% 28% 50% 50%;
  }
  :global(html[data-profile-emblem='compass']) .preview-emblem {
    border-radius: 18%;
    transform: rotate(45deg);
  }
  :global(html[data-profile-emblem='compass']) .preview-emblem i {
    transform: rotate(0);
  }
  :global(html[data-presentation-frame='stealth']) .cosmetic-preview {
    border-radius: 3px;
    background: linear-gradient(135deg, #07181f, #02090e);
  }
  :global(html[data-presentation-frame='veteran']) .cosmetic-preview {
    border-color: rgba(235, 190, 100, 0.38);
    background: linear-gradient(135deg, #24201b, #090d10);
  }
  :global(html[data-effect-quality='minimal']) .preview-board {
    background: linear-gradient(135deg, #062631, #03131e);
  }
  :global(html[data-effect-quality='minimal']) .preview-board em {
    box-shadow: none;
  }
  @media (max-width: 650px) {
    .cosmetic-grid {
      grid-template-columns: 1fr;
    }
    .cosmetic-preview {
      grid-template-columns: auto 1fr;
    }
    .preview-copy {
      grid-column: 1/-1;
    }
    .quality-row {
      grid-template-columns: auto 1fr;
    }
    .quality-row select {
      grid-column: 1/-1;
      width: 100%;
    }
  }
</style>
