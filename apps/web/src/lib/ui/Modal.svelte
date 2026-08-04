<script lang="ts">
  import type { Snippet } from 'svelte';
  import { X } from '@lucide/svelte';

  interface Props {
    open: boolean;
    title: string;
    eyebrow?: string;
    description?: string;
    children?: Snippet;
    onclose: () => void;
  }

  let { open, title, eyebrow, description, children, onclose }: Props = $props();
  let dialog = $state<HTMLDivElement>();
  let previousFocus: HTMLElement | null = null;
  const titleId = 'command-modal-title';

  $effect(() => {
    if (!open || !dialog) return;
    const activeDialog = dialog;
    previousFocus = document.activeElement as HTMLElement | null;
    queueMicrotask(() => {
      activeDialog
        .querySelector<HTMLElement>('input, button, select, textarea, a[href], [tabindex="0"]')
        ?.focus();
    });
    return () => previousFocus?.focus();
  });

  function handleKeydown(event: KeyboardEvent) {
    if (!open || !dialog) return;
    if (event.key === 'Escape') {
      event.preventDefault();
      onclose();
      return;
    }
    if (event.key !== 'Tab') return;
    const focusable = Array.from(
      dialog.querySelectorAll<HTMLElement>(
        'input:not(:disabled), button:not(:disabled), select:not(:disabled), textarea:not(:disabled), a[href], [tabindex="0"]'
      )
    );
    if (!focusable.length) return;
    const first = focusable[0];
    const last = focusable.at(-1)!;
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <div
    class="ui-modal-backdrop"
    role="presentation"
    onclick={(event) => event.currentTarget === event.target && onclose()}
  >
    <div
      bind:this={dialog}
      class="ui-modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby={titleId}
    >
      <span class="ui-modal__scan" aria-hidden="true"></span>
      <button
        class="ui-icon-button ui-modal__close"
        type="button"
        onclick={onclose}
        aria-label="닫기"
      >
        <X size={17} />
      </button>
      {#if eyebrow}<p class="ui-kicker">{eyebrow}</p>{/if}
      <h2 id={titleId}>{title}</h2>
      {#if description}<p class="ui-modal__description">{description}</p>{/if}
      <div class="ui-modal__body">{@render children?.()}</div>
    </div>
  </div>
{/if}
