<script lang="ts">
  import { Accessibility, Headphones, Music2, Smartphone, Volume2, Waves } from '@lucide/svelte';
  import { formatNumber, t, type MessageKey } from '$lib/i18n';
  import { sounds } from '$lib/sound';
  import { preferences, type AudioMix } from '$lib/stores';

  const channels: {
    key: keyof AudioMix;
    label: MessageKey;
    help: MessageKey;
    preview: 'music' | 'effects' | 'ambience' | 'voice';
  }[] = [
    { key: 'master', label: 'audio.master', help: 'audio.masterHelp', preview: 'effects' },
    { key: 'music', label: 'audio.music', help: 'audio.musicHelp', preview: 'music' },
    { key: 'effects', label: 'audio.effects', help: 'audio.effectsHelp', preview: 'effects' },
    { key: 'ambience', label: 'audio.ambience', help: 'audio.ambienceHelp', preview: 'ambience' },
    { key: 'voice', label: 'audio.voice', help: 'audio.voiceHelp', preview: 'voice' }
  ];

  function setMix(key: keyof AudioMix, event: Event) {
    const level = Number((event.currentTarget as HTMLInputElement).value);
    preferences.update((current) => ({
      ...current,
      audioMix: { ...current.audioMix, [key]: level }
    }));
  }
</script>

<section class="audio-settings panel" aria-labelledby="audio-title">
  <header>
    <span><Headphones size={20} /></span>
    <div>
      <small>{$t('audio.eyebrow')}</small>
      <h2 id="audio-title">{$t('audio.title')}</h2>
      <p>{$t('audio.description')}</p>
    </div>
    <label class="switch">
      <input
        type="checkbox"
        aria-label={$t('settings.sound')}
        bind:checked={$preferences.sound}
        onchange={() => $preferences.sound && sounds.select()}
      />
      <span></span><em>{$preferences.sound ? $t('common.on') : $t('common.off')}</em>
    </label>
  </header>

  <div class="mixers" aria-label={$t('audio.mixer')}>
    {#each channels as channel (channel.key)}
      <label class="mixer">
        <span>
          <strong>{$t(channel.label)}</strong>
          <small>{$t(channel.help)}</small>
        </span>
        <input
          type="range"
          min="0"
          max="1"
          step="0.05"
          value={$preferences.audioMix[channel.key]}
          aria-label={$t(channel.label)}
          oninput={(event) => setMix(channel.key, event)}
        />
        <output>{formatNumber($preferences.audioMix[channel.key], { style: 'percent' })}</output>
        <button
          type="button"
          disabled={!$preferences.sound}
          aria-label={$t('audio.previewChannel', { channel: $t(channel.label) })}
          onclick={() => sounds.preview(channel.preview)}>{$t('audio.preview')}</button
        >
      </label>
    {/each}
  </div>

  <div class="audio-options">
    <label>
      <span class="option-icon"><Accessibility size={19} /></span>
      <span
        ><strong>{$t('audio.accessibilityCues')}</strong><small
          >{$t('audio.accessibilityCuesHelp')}</small
        ></span
      >
      <span class="switch">
        <input
          type="checkbox"
          aria-label={$t('audio.accessibilityCues')}
          bind:checked={$preferences.audioCues}
          onchange={() => $preferences.audioCues && sounds.preview('voice')}
        />
        <span></span><em>{$preferences.audioCues ? $t('common.on') : $t('common.off')}</em>
      </span>
    </label>
    <label>
      <span class="option-icon"><Smartphone size={19} /></span>
      <span><strong>{$t('audio.haptics')}</strong><small>{$t('audio.hapticsHelp')}</small></span>
      <span class="switch">
        <input
          type="checkbox"
          aria-label={$t('audio.haptics')}
          bind:checked={$preferences.haptics}
          onchange={() => $preferences.haptics && sounds.select()}
        />
        <span></span><em>{$preferences.haptics ? $t('common.on') : $t('common.off')}</em>
      </span>
    </label>
  </div>

  <aside>
    <span><Music2 size={16} /><Waves size={16} /><Volume2 size={16} /></span>
    <p><strong>{$t('audio.lifecycleTitle')}</strong>{$t('audio.lifecycleHelp')}</p>
  </aside>
</section>

<style>
  .audio-settings {
    padding: var(--space-3);
  }
  header {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: start;
    gap: 0.8rem;
  }
  header > span,
  .option-icon {
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
  .mixers {
    display: grid;
    gap: 0.55rem;
    margin-top: var(--space-3);
  }
  .mixer {
    display: grid;
    grid-template-columns: minmax(12rem, 1fr) minmax(8rem, 1.4fr) 3rem auto;
    align-items: center;
    gap: 0.8rem;
    padding: 0.75rem 0.85rem;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    background: rgba(3, 15, 24, 0.48);
  }
  .mixer strong,
  .mixer small {
    display: block;
  }
  .mixer small {
    margin-top: 0.15rem;
    color: var(--ink-400);
    font-size: 0.72rem;
  }
  .mixer input {
    width: 100%;
    accent-color: var(--cyan-400);
  }
  .mixer output {
    color: var(--cyan-200);
    font: 700 0.78rem var(--font-display);
    text-align: right;
  }
  .mixer button {
    min-height: 2rem;
    padding: 0.3rem 0.65rem;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-xs);
    color: var(--cyan-200);
    background: rgba(10, 55, 70, 0.48);
    cursor: pointer;
  }
  .mixer button:disabled {
    color: var(--ink-500);
    cursor: not-allowed;
  }
  .audio-options {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.65rem;
    margin-top: 0.65rem;
  }
  .audio-options > label {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    align-items: center;
    gap: 0.75rem;
    padding: 0.8rem;
    border: 1px solid var(--line);
    border-radius: var(--radius-sm);
    background: rgba(3, 15, 24, 0.48);
  }
  .audio-options strong,
  .audio-options small {
    display: block;
  }
  .audio-options small {
    margin-top: 0.2rem;
    color: var(--ink-400);
    font-size: 0.72rem;
  }
  aside {
    display: flex;
    align-items: center;
    gap: 0.7rem;
    margin-top: 0.8rem;
    color: var(--ink-300);
    font-size: 0.78rem;
  }
  aside > span {
    display: flex;
    gap: 0.3rem;
    color: var(--safe);
  }
  aside p {
    margin: 0;
  }
  aside strong {
    margin-right: 0.35rem;
    color: var(--safe);
  }
  @media (max-width: 760px) {
    header {
      grid-template-columns: auto 1fr;
    }
    header > .switch {
      grid-column: 1/-1;
      justify-self: start;
    }
    .mixer {
      grid-template-columns: 1fr auto;
    }
    .mixer input {
      grid-column: 1/-1;
    }
    .mixer button {
      grid-column: 1/-1;
    }
    .audio-options {
      grid-template-columns: 1fr;
    }
  }
</style>
