<script lang="ts">
  import type { Snippet } from 'svelte';
  interface Props {
    children?: Snippet;
    level?: 1 | 2 | 3 | 4;
    display?: boolean;
    class?: string;
  }
  let { children, level = 2, display = false, class: className = '' }: Props = $props();
</script>

<svelte:element this={`h${level}`} class:display class={`ui-heading ${className}`}>
  {@render children?.()}
</svelte:element>
