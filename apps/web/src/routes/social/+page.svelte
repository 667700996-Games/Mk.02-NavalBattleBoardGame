<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import { ArrowLeft } from '@lucide/svelte';
  import { api } from '$lib/api';
  import SocialHub from '$lib/components/social/SocialHub.svelte';
  import { t } from '$lib/i18n';
  import { session } from '$lib/stores';
  import { Button } from '$lib/ui';

  let ready = $state(false);

  onMount(async () => {
    try {
      session.set(await api.currentSession());
      ready = true;
    } catch {
      await goto(resolve('/'));
    }
  });
</script>

<svelte:head><title>{$t('social.metaTitle')}</title></svelte:head>

<main class="social-page shell">
  <nav aria-label={$t('social.navigation')}>
    <Button variant="secondary" onclick={() => goto(resolve('/lobby'))}
      ><ArrowLeft size={17} /> {$t('social.backToLobby')}</Button
    >
  </nav>
  {#if ready}<SocialHub session={$session} />{/if}
</main>

<style>
  .social-page {
    min-height: 100vh;
    padding-block: var(--space-6) var(--space-9);
  }
  nav {
    display: flex;
    margin-bottom: var(--space-3);
  }
</style>
