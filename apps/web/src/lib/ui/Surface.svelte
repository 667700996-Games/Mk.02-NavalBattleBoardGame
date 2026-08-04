<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    children?: Snippet;
    tone?: 'default' | 'elevated' | 'interactive' | 'quiet';
    padding?: 'none' | 'sm' | 'md' | 'lg';
    class?: string;
  }

  let { children, tone = 'default', padding = 'md', class: className = '' }: Props = $props();
</script>

<div class={`ui-surface ui-surface--${tone} ui-surface--${padding} ${className}`}>
  <span class="ui-surface__edge" aria-hidden="true"></span>
  <div class="ui-surface__content">{@render children?.()}</div>
</div>
