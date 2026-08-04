<script lang="ts">
  import type { Snippet } from 'svelte';
  interface Props {
    label: string;
    trigger?: Snippet;
    children?: Snippet;
    align?: 'start' | 'end';
  }
  let { label, trigger, children, align = 'end' }: Props = $props();
  let open = $state(false);
  let root: HTMLDivElement;

  function windowClick(event: MouseEvent) {
    if (open && root && !root.contains(event.target as Node)) open = false;
  }
  function keydown(event: KeyboardEvent) {
    if (open && event.key === 'Escape') open = false;
  }
</script>

<svelte:window onclick={windowClick} onkeydown={keydown} />
<div bind:this={root} class={`ui-dropdown ui-dropdown--${align}`}>
  <button
    type="button"
    class="ui-dropdown__trigger"
    aria-label={label}
    aria-expanded={open}
    onclick={() => (open = !open)}
  >
    {@render trigger?.()}
  </button>
  {#if open}<div class="ui-dropdown__menu" role="menu" tabindex="-1">
      {@render children?.()}
    </div>{/if}
</div>
