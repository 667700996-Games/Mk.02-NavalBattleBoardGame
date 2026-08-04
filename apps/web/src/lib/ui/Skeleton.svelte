<script lang="ts">
  interface Props {
    width?: string;
    height?: string;
    radius?: string;
    label?: string;
  }
  let { width = '100%', height = '16px', radius = '8px', label = '불러오는 중' }: Props = $props();
</script>

<span class="ui-skeleton" style={`width:${width};height:${height};border-radius:${radius}`}>
  <span class="sr-only">{label}</span>
</span>
