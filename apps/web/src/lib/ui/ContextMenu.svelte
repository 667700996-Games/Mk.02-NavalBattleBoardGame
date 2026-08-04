<script lang="ts">
  import type { Snippet } from 'svelte';
  interface Props {
    children?: Snippet;
    menu?: Snippet;
    label: string;
  }
  let { children, menu, label }: Props = $props();
  let open = $state(false);
  let x = $state(0);
  let y = $state(0);

  function show(event: MouseEvent) {
    event.preventDefault();
    x = Math.min(event.clientX, innerWidth - 220);
    y = Math.min(event.clientY, innerHeight - 180);
    open = true;
  }
</script>

<svelte:window
  onclick={() => (open = false)}
  onkeydown={(event) => event.key === 'Escape' && (open = false)}
/>
<div class="ui-context-target" role="group" aria-label={label} oncontextmenu={show}>
  {@render children?.()}
</div>
{#if open}<div
    class="ui-context-menu"
    role="menu"
    aria-label={label}
    style={`left:${x}px;top:${y}px`}
  >
    {@render menu?.()}
  </div>{/if}
