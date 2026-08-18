<script lang="ts">
  import { t } from '$lib/i18n';

  interface Props {
    width?: string;
    height?: string;
    radius?: string;
    label?: string;
  }
  let { width = '100%', height = '16px', radius = '8px', label }: Props = $props();
</script>

<span class="ui-skeleton" style={`width:${width};height:${height};border-radius:${radius}`}>
  <span class="sr-only">{label ?? $t('common.loading')}</span>
</span>
