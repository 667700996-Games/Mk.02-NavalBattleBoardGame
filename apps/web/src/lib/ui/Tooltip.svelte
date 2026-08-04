<script lang="ts">
  import type { Snippet } from 'svelte';
  interface Props {
    text: string;
    children?: Snippet;
    side?: 'top' | 'bottom';
  }
  let { text, children, side = 'bottom' }: Props = $props();
</script>

<span class={`ui-tooltip ui-tooltip--${side}`}>
  {@render children?.()}
  <span role="tooltip">{text}</span>
</span>
