<script lang="ts">
  import type { Snippet } from 'svelte';
  interface Props {
    children?: Snippet;
    ordered?: boolean;
    label?: string;
    class?: string;
  }
  let { children, ordered = false, label, class: className = '' }: Props = $props();
</script>

{#if ordered}
  <ol class={`ui-list ${className}`} aria-label={label}>{@render children?.()}</ol>
{:else}
  <ul class={`ui-list ${className}`} aria-label={label}>{@render children?.()}</ul>
{/if}
