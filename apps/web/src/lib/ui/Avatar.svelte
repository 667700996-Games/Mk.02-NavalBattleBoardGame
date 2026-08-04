<script lang="ts">
  interface Props {
    name: string;
    status?: 'online' | 'reconnecting' | 'offline' | 'none';
    size?: 'sm' | 'md' | 'lg';
    label?: string;
  }
  let { name, status = 'none', size = 'md', label }: Props = $props();
  let initials = $derived(name.trim().slice(0, 2).toUpperCase() || '—');
</script>

<span class={`ui-avatar ui-avatar--${size}`} aria-label={label ?? name} title={label ?? name}>
  <span>{initials}</span>
  {#if status !== 'none'}<i class={`ui-avatar__status ui-avatar__status--${status}`}></i>{/if}
</span>
