<script lang="ts">
  import { Hand, Keyboard, MousePointer2 } from '@lucide/svelte';
  import { t, type MessageKey } from '$lib/i18n';
  import { inputModality, type InputModality } from '$lib/stores';

  interface Props {
    context: 'placement' | 'targeting' | 'chat';
    compact?: boolean;
  }

  let { context, compact = false }: Props = $props();

  const labels: Record<InputModality, MessageKey> = {
    pointer: 'input.pointer',
    keyboard: 'input.keyboard',
    touch: 'input.touch'
  };
  const prompts: Record<Props['context'], Record<InputModality, MessageKey>> = {
    placement: {
      pointer: 'input.placement.pointer',
      keyboard: 'input.placement.keyboard',
      touch: 'input.placement.touch'
    },
    targeting: {
      pointer: 'input.targeting.pointer',
      keyboard: 'input.targeting.keyboard',
      touch: 'input.targeting.touch'
    },
    chat: {
      pointer: 'input.chat.pointer',
      keyboard: 'input.chat.keyboard',
      touch: 'input.chat.touch'
    }
  };

  let label = $derived($t(labels[$inputModality]));
  let prompt = $derived($t(prompts[context][$inputModality]));
</script>

<aside
  class:input-prompt--compact={compact}
  class="input-prompt"
  aria-label={$t('input.help', { modality: label })}
  aria-live="polite"
  data-testid={`input-prompt-${context}`}
  data-modality={$inputModality}
>
  <span aria-hidden="true">
    {#if $inputModality === 'keyboard'}
      <Keyboard size={14} />
    {:else if $inputModality === 'touch'}
      <Hand size={14} />
    {:else}
      <MousePointer2 size={14} />
    {/if}
  </span>
  <strong>{label}</strong>
  <em>{prompt}</em>
</aside>

<style>
  .input-prompt {
    display: flex;
    min-width: 0;
    align-items: center;
    gap: 7px;
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: 7px;
    color: var(--ink-300);
    background: rgba(3, 17, 25, 0.62);
  }
  .input-prompt > span {
    display: grid;
    flex: none;
    color: var(--cyan-300);
  }
  .input-prompt strong {
    flex: none;
    color: var(--cyan-200);
    font: 700 9px var(--font-display);
    letter-spacing: 0.08em;
  }
  .input-prompt em {
    min-width: 0;
    overflow: hidden;
    font-size: 10px;
    font-style: normal;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .input-prompt--compact {
    padding: 0;
    border: 0;
    background: transparent;
  }
  @media (max-width: 720px) {
    .input-prompt {
      align-items: flex-start;
    }
    .input-prompt em {
      line-height: 1.45;
      white-space: normal;
    }
  }
</style>
