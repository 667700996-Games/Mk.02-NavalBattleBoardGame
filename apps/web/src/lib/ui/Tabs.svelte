<script lang="ts">
  interface TabItem {
    id: string;
    label: string;
    meta?: string;
  }
  interface Props {
    items: TabItem[];
    value: string;
    label: string;
    onchange: (id: string) => void;
    compact?: boolean;
  }
  let { items, value, label, onchange, compact = false }: Props = $props();
</script>

<div class:ui-tabs--compact={compact} class="ui-tabs" role="tablist" aria-label={label}>
  {#each items as item (item.id)}
    <button
      type="button"
      role="tab"
      aria-selected={value === item.id}
      class:active={value === item.id}
      onclick={() => onchange(item.id)}
    >
      <span>{item.label}</span>{#if item.meta}<small>{item.meta}</small>{/if}
    </button>
  {/each}
</div>
