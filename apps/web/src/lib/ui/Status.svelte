<script lang="ts">
  interface Props {
    label: string;
    value?: string;
    state?: 'online' | 'idle' | 'warning' | 'danger';
  }
  let { label, value, state = 'idle' }: Props = $props();
</script>

<span class={`ui-status ui-status--${state}`}>
  <i aria-hidden="true"></i><span>{label}</span>{#if value}<strong>{value}</strong>{/if}
</span>
