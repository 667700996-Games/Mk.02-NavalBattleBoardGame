<script lang="ts">
  import { resolve } from '$app/paths';
  import type { Snippet } from 'svelte';

  interface Props {
    children?: Snippet;
    variant?: 'primary' | 'secondary' | 'outline' | 'danger' | 'success';
    size?: 'sm' | 'md' | 'lg';
    type?: 'button' | 'submit' | 'reset';
    href?: string;
    loading?: boolean;
    disabled?: boolean;
    full?: boolean;
    ariaLabel?: string;
    class?: string;
    onclick?: (event: MouseEvent) => void;
  }

  let {
    children,
    variant = 'secondary',
    size = 'md',
    type = 'button',
    href,
    loading = false,
    disabled = false,
    full = false,
    ariaLabel,
    class: className = '',
    onclick
  }: Props = $props();

  let classes = $derived(
    `ui-button ui-button--${variant} ui-button--${size}${full ? ' ui-button--full' : ''} ${className}`
  );
</script>

{#if href && !disabled && !loading}
  <a class={classes} href={resolve(href as '/')} aria-label={ariaLabel} {onclick}>
    <span class="ui-button__content">{@render children?.()}</span>
  </a>
{:else}
  <button
    class={classes}
    {type}
    disabled={disabled || loading}
    aria-busy={loading}
    aria-label={ariaLabel}
    {onclick}
  >
    {#if loading}<span class="ui-button__loader" aria-hidden="true"></span>{/if}
    <span class="ui-button__content">{@render children?.()}</span>
  </button>
{/if}
