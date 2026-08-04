<script lang="ts">
  import type { Snippet } from 'svelte';
  import Surface from './Surface.svelte';

  interface Props {
    children?: Snippet;
    title?: string;
    eyebrow?: string;
    action?: Snippet;
    class?: string;
    padding?: 'none' | 'sm' | 'md' | 'lg';
  }

  let { children, title, eyebrow, action, class: className = '', padding = 'md' }: Props = $props();
</script>

<Surface class={`ui-panel ${className}`} {padding}>
  {#if title || eyebrow || action}
    <header class="ui-panel__header">
      <div>
        {#if eyebrow}<p class="ui-kicker">{eyebrow}</p>{/if}
        {#if title}<h2>{title}</h2>{/if}
      </div>
      {@render action?.()}
    </header>
  {/if}
  {@render children?.()}
</Surface>
