<script lang="ts">
  interface Props {
    label?: string;
  }
  let { label }: Props = $props();
</script>

<div class="ui-divider" role="separator">
  {#if label}<span>{label}</span>{/if}
</div>
