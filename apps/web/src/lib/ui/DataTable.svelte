<script lang="ts">
  import type { Snippet } from 'svelte';
  interface Props {
    children?: Snippet;
    label: string;
    class?: string;
  }
  let { children, label, class: className = '' }: Props = $props();
</script>

<div class={`ui-table-wrap ${className}`} role="region" aria-label={label}>
  <table class="ui-table">{@render children?.()}</table>
</div>
