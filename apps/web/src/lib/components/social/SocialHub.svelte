<script lang="ts">
  import { goto } from '$app/navigation';
  import { resolve } from '$app/paths';
  import { onMount } from 'svelte';
  import { Check, Clock3, Radio, ShieldCheck, UserPlus, Users, X } from '@lucide/svelte';
  import { api } from '$lib/api';
  import { formatRelativeTime, localizeError, t } from '$lib/i18n';
  import { gameSnapshot } from '$lib/stores';
  import type { Session, SocialAction, SocialOverview, SocialRelationship } from '$lib/types';
  import { Badge, Button, Surface } from '$lib/ui';

  interface Props {
    session: Session | null;
  }

  let { session }: Props = $props();
  let overview = $state<SocialOverview | null>(null);
  let friendHandle = $state('');
  let busy = $state(false);
  let loading = $state(false);
  let error = $state('');
  let notice = $state('');

  const friends = $derived(
    overview?.relationships.filter((relationship) => relationship.friendState === 'FRIEND') ?? []
  );
  const requests = $derived(
    overview?.relationships.filter(
      (relationship) =>
        relationship.friendState === 'INCOMING' ||
        relationship.partyState === 'INCOMING_INVITE' ||
        relationship.gameInvite?.direction === 'INCOMING'
    ) ?? []
  );

  onMount(() => {
    if (!session?.accountId) return;
    let active = true;
    let timer: ReturnType<typeof setInterval>;
    void load().then(() => {
      if (active) timer = setInterval(() => void load(true), 8_000);
    });
    return () => {
      active = false;
      if (timer) clearInterval(timer);
    };
  });

  async function load(silent = false) {
    if (!session?.accountId) return;
    if (!silent) loading = true;
    try {
      overview = await api.socialOverview();
      if (!silent) error = '';
    } catch (caught) {
      if (!silent) error = localizeError(caught, 'social.loadError');
    } finally {
      if (!silent) loading = false;
    }
  }

  async function act(action: SocialAction, successKey?: 'social.updated' | 'social.inviteSent') {
    busy = true;
    error = '';
    notice = '';
    try {
      const response = await api.applySocialAction(action);
      overview = response.overview;
      if (response.joinCode) {
        const snapshot = await api.joinRoom(response.joinCode);
        gameSnapshot.set(snapshot);
        await goto(resolve('/room/[code]', { code: snapshot.room.code }));
        return;
      }
      notice = $t(successKey ?? 'social.updated');
    } catch (caught) {
      error = localizeError(caught, 'social.actionError');
    } finally {
      busy = false;
    }
  }

  async function requestFriend() {
    const targetHandle = friendHandle.trim();
    if (!targetHandle) return;
    await act({ action: 'FRIEND_REQUEST', targetHandle });
    friendHandle = '';
  }

  async function updatePrivacy(
    field: 'allowFriendRequests' | 'showPresence' | 'allowGameInvites',
    enabled: boolean
  ) {
    if (!overview) return;
    busy = true;
    error = '';
    try {
      overview = await api.setSocialPrivacy({
        allowFriendRequests:
          field === 'allowFriendRequests' ? enabled : overview.privacy.allowFriendRequests,
        showPresence: field === 'showPresence' ? enabled : overview.privacy.showPresence,
        allowGameInvites: field === 'allowGameInvites' ? enabled : overview.privacy.allowGameInvites
      });
      notice = $t('social.privacySaved');
    } catch (caught) {
      error = localizeError(caught, 'social.actionError');
    } finally {
      busy = false;
    }
  }

  async function inviteToGame(relationship: SocialRelationship) {
    busy = true;
    error = '';
    notice = '';
    try {
      const created = await api.createRoom($t('social.directRoomName'), 'PRIVATE', {
        mode: 'CLASSIC',
        turnDurationSeconds: 60
      });
      const response = await api.applySocialAction({
        action: 'GAME_INVITE',
        targetAccountId: relationship.targetIdentityId,
        roomId: created.snapshot.room.id
      });
      overview = response.overview;
      gameSnapshot.set(created.snapshot);
      await goto(resolve('/room/[code]', { code: created.snapshot.room.code }));
    } catch (caught) {
      error = localizeError(caught, 'social.actionError');
      try {
        const recovered = await api.recover();
        if (recovered) await api.leaveRoom(recovered.room.id);
      } catch {
        // A failed invitation must not hide the original actionable error.
      }
    } finally {
      busy = false;
    }
  }

  function recentLabel(value: string): string {
    const minutes = Math.round((new Date(value).getTime() - Date.now()) / 60_000);
    if (Math.abs(minutes) < 60) return formatRelativeTime(minutes, 'minute');
    return formatRelativeTime(Math.round(minutes / 60), 'hour');
  }
</script>

<section class="social-hub" aria-labelledby="social-title">
  <div class="social-heading">
    <div>
      <p class="eyebrow">{$t('social.eyebrow')}</p>
      <h2 id="social-title"><Users size={21} /> {$t('social.title')}</h2>
      <p>{$t('social.description')}</p>
    </div>
    {#if overview}<Badge tone="cyan">{$t('social.friendCount', { count: friends.length })}</Badge
      >{/if}
  </div>

  {#if !session?.accountId}
    <Surface tone="quiet" padding="md" class="social-account-callout">
      <ShieldCheck size={22} />
      <div>
        <strong>{$t('social.accountRequired')}</strong>
        <p>{$t('social.accountRequiredHelp')}</p>
      </div>
      <a href={resolve('/settings')}>{$t('social.openAccountSettings')}</a>
    </Surface>
  {:else if loading && !overview}
    <Surface tone="quiet" padding="md"><p aria-live="polite">{$t('social.loading')}</p></Surface>
  {:else}
    {#if error}<p class="social-message social-message--error" role="alert">{error}</p>{/if}
    {#if notice}<p class="social-message" role="status">{notice}</p>{/if}

    <div class="social-grid">
      <Surface tone="elevated" padding="md" class="social-card">
        <h3><UserPlus size={18} /> {$t('social.addFriend')}</h3>
        <form
          class="friend-search"
          onsubmit={(event) => {
            event.preventDefault();
            void requestFriend();
          }}
        >
          <label for="friend-handle">{$t('social.handle')}</label>
          <div>
            <input
              id="friend-handle"
              bind:value={friendHandle}
              maxlength="16"
              autocomplete="off"
              placeholder={$t('social.handlePlaceholder')}
            /><Button type="submit" disabled={busy || !friendHandle.trim()}
              >{$t('social.sendRequest')}</Button
            >
          </div>
        </form>

        <h3 class="section-title"><Radio size={18} /> {$t('social.requests')}</h3>
        {#if requests.length === 0}
          <p class="empty">{$t('social.noRequests')}</p>
        {:else}
          <ul class="social-list">
            {#each requests as relationship (relationship.targetIdentityId)}
              <li>
                <div>
                  <strong>{relationship.targetNickname}</strong>
                  <small
                    >{relationship.friendState === 'INCOMING'
                      ? $t('social.friendRequest')
                      : relationship.partyState === 'INCOMING_INVITE'
                        ? $t('social.partyRequest')
                        : $t('social.gameRequest', {
                            room: relationship.gameInvite?.roomName ?? ''
                          })}</small
                  >
                </div>
                <div class="row-actions">
                  {#if relationship.friendState === 'INCOMING' && relationship.friendRequestId}
                    <button
                      aria-label={$t('social.accept')}
                      disabled={busy}
                      onclick={() =>
                        act({
                          action: 'FRIEND_RESPOND',
                          targetAccountId: relationship.targetIdentityId,
                          requestId: relationship.friendRequestId!,
                          accept: true
                        })}><Check size={16} /></button
                    >
                    <button
                      aria-label={$t('social.decline')}
                      disabled={busy}
                      onclick={() =>
                        act({
                          action: 'FRIEND_RESPOND',
                          targetAccountId: relationship.targetIdentityId,
                          requestId: relationship.friendRequestId!,
                          accept: false
                        })}><X size={16} /></button
                    >
                  {:else if relationship.partyState === 'INCOMING_INVITE' && relationship.partyId}
                    <button
                      aria-label={$t('social.accept')}
                      disabled={busy}
                      onclick={() =>
                        act({
                          action: 'PARTY_RESPOND',
                          targetAccountId: relationship.targetIdentityId,
                          partyId: relationship.partyId!,
                          accept: true
                        })}><Check size={16} /></button
                    >
                    <button
                      aria-label={$t('social.decline')}
                      disabled={busy}
                      onclick={() =>
                        act({
                          action: 'PARTY_RESPOND',
                          targetAccountId: relationship.targetIdentityId,
                          partyId: relationship.partyId!,
                          accept: false
                        })}><X size={16} /></button
                    >
                  {:else if relationship.gameInvite}
                    <button
                      aria-label={$t('social.accept')}
                      disabled={busy}
                      onclick={() =>
                        act({
                          action: 'GAME_INVITE_RESPOND',
                          targetAccountId: relationship.targetIdentityId,
                          inviteId: relationship.gameInvite!.id,
                          accept: true
                        })}><Check size={16} /></button
                    >
                    <button
                      aria-label={$t('social.decline')}
                      disabled={busy}
                      onclick={() =>
                        act({
                          action: 'GAME_INVITE_RESPOND',
                          targetAccountId: relationship.targetIdentityId,
                          inviteId: relationship.gameInvite!.id,
                          accept: false
                        })}><X size={16} /></button
                    >
                  {/if}
                </div>
              </li>
            {/each}
          </ul>
        {/if}
      </Surface>

      <Surface tone="elevated" padding="md" class="social-card">
        <h3><Users size={18} /> {$t('social.friends')}</h3>
        {#if friends.length === 0}<p class="empty">{$t('social.noFriends')}</p>{:else}
          <ul class="social-list">
            {#each friends as relationship (relationship.targetIdentityId)}
              <li class="friend-row">
                <div>
                  <strong>{relationship.targetNickname}</strong><small
                    class="presence presence--{relationship.presence.toLowerCase()}"
                    >{$t(
                      `social.presence.${relationship.presence}` as 'social.presence.OFFLINE'
                    )}</small
                  >
                </div>
                <div class="friend-actions">
                  {#if relationship.partyState === 'NONE'}<button
                      disabled={busy}
                      onclick={() =>
                        act({
                          action: 'PARTY_INVITE',
                          targetAccountId: relationship.targetIdentityId
                        })}>{$t('social.partyInvite')}</button
                    >{:else}<button
                      disabled={busy}
                      onclick={() =>
                        act({
                          action: 'PARTY_LEAVE',
                          targetAccountId: relationship.targetIdentityId
                        })}>{$t('social.partyLeave')}</button
                    >{/if}
                  <button
                    disabled={busy || relationship.gameInvite !== null}
                    onclick={() => inviteToGame(relationship)}>{$t('social.gameInvite')}</button
                  >
                  <button
                    class="danger"
                    disabled={busy}
                    onclick={() =>
                      act({
                        action: 'FRIEND_REMOVE',
                        targetAccountId: relationship.targetIdentityId
                      })}>{$t('social.remove')}</button
                  >
                </div>
              </li>
            {/each}
          </ul>
        {/if}
      </Surface>

      <Surface tone="quiet" padding="md" class="social-card">
        <h3><Clock3 size={18} /> {$t('social.recentPlayers')}</h3>
        {#if overview?.recentPlayers.length === 0}<p class="empty">
            {$t('social.noRecentPlayers')}
          </p>{:else}
          <ul class="recent-list">
            {#each overview?.recentPlayers ?? [] as player (player.accountId)}
              <li>
                <div>
                  <strong>{player.handle}</strong><small>{recentLabel(player.lastPlayedAt)}</small>
                </div>
                {#if player.blocked}<Badge tone="danger">{$t('social.blocked')}</Badge
                  >{:else if player.muted}<Badge tone="warning">{$t('social.muted')}</Badge
                  >{:else if player.friend}<Badge tone="success">{$t('social.friend')}</Badge
                  >{:else}<button
                    disabled={busy}
                    onclick={() => act({ action: 'FRIEND_REQUEST', targetHandle: player.handle })}
                    >{$t('social.add')}</button
                  >{/if}
              </li>
            {/each}
          </ul>
        {/if}
      </Surface>

      <Surface tone="quiet" padding="md" class="social-card privacy-card">
        <h3><ShieldCheck size={18} /> {$t('social.privacy')}</h3>
        {#if overview}
          <label
            ><input
              type="checkbox"
              checked={overview.privacy.allowFriendRequests}
              disabled={busy}
              onchange={(event) =>
                updatePrivacy('allowFriendRequests', event.currentTarget.checked)}
            /><span
              ><strong>{$t('social.allowFriendRequests')}</strong><small
                >{$t('social.allowFriendRequestsHelp')}</small
              ></span
            ></label
          >
          <label
            ><input
              type="checkbox"
              checked={overview.privacy.showPresence}
              disabled={busy}
              onchange={(event) => updatePrivacy('showPresence', event.currentTarget.checked)}
            /><span
              ><strong>{$t('social.showPresence')}</strong><small
                >{$t('social.showPresenceHelp')}</small
              ></span
            ></label
          >
          <label
            ><input
              type="checkbox"
              checked={overview.privacy.allowGameInvites}
              disabled={busy}
              onchange={(event) => updatePrivacy('allowGameInvites', event.currentTarget.checked)}
            /><span
              ><strong>{$t('social.allowGameInvites')}</strong><small
                >{$t('social.allowGameInvitesHelp')}</small
              ></span
            ></label
          >
        {/if}
      </Surface>
    </div>
  {/if}
</section>

<style>
  .social-hub {
    margin-top: var(--space-6);
  }
  .social-heading {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: var(--space-4);
    margin-bottom: var(--space-4);
  }
  .social-heading h2,
  :global(.social-card) h3 {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin: 0;
  }
  .social-heading p:not(.eyebrow),
  :global(.social-account-callout) p {
    margin: 0.35rem 0 0;
    color: var(--text-muted);
  }
  .social-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-4);
  }
  :global(.social-card) {
    min-width: 0;
  }
  .social-account-callout {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  :global(.social-account-callout) div {
    flex: 1;
  }
  :global(.social-account-callout) a {
    color: var(--cyan-300);
    font-weight: 700;
  }
  .friend-search {
    margin-top: var(--space-3);
  }
  .friend-search > label {
    display: block;
    margin-bottom: 0.4rem;
    color: var(--text-muted);
    font-size: 0.78rem;
    font-weight: 700;
    text-transform: uppercase;
  }
  .friend-search > div {
    display: flex;
    gap: 0.5rem;
  }
  .friend-search input {
    min-width: 0;
    flex: 1;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: rgba(2, 12, 21, 0.7);
    color: var(--text);
    padding: 0.7rem 0.8rem;
  }
  .section-title {
    margin-top: var(--space-5) !important;
  }
  .social-list,
  .recent-list {
    display: grid;
    gap: 0.55rem;
    margin: var(--space-3) 0 0;
    padding: 0;
    list-style: none;
  }
  .social-list li,
  .recent-list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.7rem;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    background: rgba(4, 19, 31, 0.55);
  }
  .social-list strong,
  .social-list small,
  .recent-list strong,
  .recent-list small {
    display: block;
  }
  .social-list small,
  .recent-list small {
    margin-top: 0.2rem;
    color: var(--text-muted);
    font-size: 0.75rem;
  }
  .row-actions,
  .friend-actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 0.4rem;
  }
  .row-actions button,
  .friend-actions button,
  .recent-list button {
    min-height: 2rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-xs);
    background: rgba(13, 44, 64, 0.8);
    color: var(--text);
    padding: 0.35rem 0.55rem;
    font-size: 0.72rem;
    font-weight: 700;
    cursor: pointer;
  }
  .row-actions button {
    display: grid;
    place-items: center;
    min-width: 2rem;
    padding: 0.35rem;
  }
  button.danger {
    color: var(--danger-300);
  }
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .presence::before {
    content: '';
    display: inline-block;
    width: 0.48rem;
    height: 0.48rem;
    margin-right: 0.35rem;
    border-radius: 50%;
    background: var(--text-muted);
  }
  .presence--online::before {
    background: var(--success-400);
    box-shadow: 0 0 0.5rem var(--success-400);
  }
  .presence--in_game::before {
    background: var(--cyan-300);
    box-shadow: 0 0 0.5rem var(--cyan-300);
  }
  :global(.privacy-card) label {
    display: flex;
    align-items: flex-start;
    gap: 0.7rem;
    margin-top: 0.9rem;
  }
  :global(.privacy-card) input {
    width: 1.1rem;
    height: 1.1rem;
    accent-color: var(--cyan-400);
  }
  :global(.privacy-card) strong,
  :global(.privacy-card) small {
    display: block;
  }
  :global(.privacy-card) small {
    margin-top: 0.2rem;
    color: var(--text-muted);
  }
  .empty {
    color: var(--text-muted);
  }
  .social-message {
    margin: 0 0 var(--space-3);
    color: var(--success-300);
  }
  .social-message--error {
    color: var(--danger-300);
  }
  @media (max-width: 800px) {
    .social-grid {
      grid-template-columns: 1fr;
    }
    .social-heading {
      align-items: flex-start;
    }
  }
  @media (max-width: 520px) {
    .friend-search > div,
    .friend-row {
      align-items: stretch !important;
      flex-direction: column;
    }
    .friend-actions {
      justify-content: flex-start;
    }
    .social-account-callout {
      align-items: flex-start;
      flex-wrap: wrap;
    }
  }
</style>
