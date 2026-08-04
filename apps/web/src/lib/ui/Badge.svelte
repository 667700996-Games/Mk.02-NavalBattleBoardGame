<script lang="ts">
  import type { Snippet } from 'svelte';
  interface Props {
    children?: Snippet;
    tone?: 'neutral' | 'cyan' | 'blue' | 'success' | 'warning' | 'danger';
    pulse?: boolean;
    class?: string;
  }
  let { children, tone = 'neutral', pulse = false, class: className = '' }: Props = $props();
</script>

<span class={`ui-badge ui-badge--${tone} ${className}`}>
  {#if pulse}<i class="ui-badge__signal" aria-hidden="true"></i>{/if}
  {@render children?.()}
</span>
