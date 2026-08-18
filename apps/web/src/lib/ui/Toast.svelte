<script lang="ts">
  import type { Snippet } from 'svelte';
  import { AlertTriangle, CheckCircle2, Info, X } from '@lucide/svelte';
  import { t } from '$lib/i18n';
  interface Props {
    title: string;
    message: string;
    tone?: 'info' | 'success' | 'warning' | 'danger';
    action?: Snippet;
    onclose?: () => void;
  }
  let { title, message, tone = 'info', action, onclose }: Props = $props();
</script>

<article class={`ui-toast ui-toast--${tone}`} role={tone === 'danger' ? 'alert' : 'status'}>
  <span class="ui-toast__icon">
    {#if tone === 'success'}<CheckCircle2 size={18} />{:else if tone === 'info'}<Info
        size={18}
      />{:else}<AlertTriangle size={18} />{/if}
  </span>
  <div>
    <strong>{title}</strong>
    <p>{message}</p>
    {#if action}{@render action()}{/if}
  </div>
  {#if onclose}<button
      class="ui-icon-button"
      onclick={onclose}
      aria-label={$t('common.dismissNotification')}><X size={15} /></button
    >{/if}
</article>
