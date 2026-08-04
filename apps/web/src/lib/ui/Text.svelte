<script lang="ts">
  import type { Snippet } from 'svelte';
  interface Props {
    children?: Snippet;
    tone?: 'default' | 'muted' | 'cyan' | 'danger' | 'success';
    size?: 'sm' | 'md' | 'lg';
    as?: 'p' | 'span' | 'small';
    class?: string;
  }
  let {
    children,
    tone = 'default',
    size = 'md',
    as = 'p',
    class: className = ''
  }: Props = $props();
</script>

<svelte:element this={as} class={`ui-text ui-text--${tone} ui-text--${size} ${className}`}>
  {@render children?.()}
</svelte:element>
