import { expect, test, type Page } from '@playwright/test';

const ADMIN_TOKEN = 'integration-admin-token-32-characters-long';
const OPERATOR_ID = 'e2e.operator';

async function api(
  page: Page,
  path: string,
  init: { method?: string; body?: unknown; admin?: boolean; operator?: boolean } = {}
): Promise<{ status: number; body: unknown }> {
  return page.evaluate(
    async ({ path: requestPath, init: requestInit, token, operatorId }) => {
      const headers: Record<string, string> = {};
      if (requestInit.body !== undefined) headers['content-type'] = 'application/json';
      if (requestInit.admin) headers.authorization = `Bearer ${token}`;
      if (requestInit.operator) headers['x-operator-id'] = operatorId;
      const response = await fetch(`/api${requestPath}`, {
        method: requestInit.method ?? 'GET',
        credentials: 'include',
        headers,
        body: requestInit.body === undefined ? undefined : JSON.stringify(requestInit.body)
      });
      return {
        status: response.status,
        body: response.status === 204 ? null : await response.json()
      };
    },
    { path, init, token: ADMIN_TOKEN, operatorId: OPERATOR_ID }
  );
}

async function createAccount(page: Page, nickname: string, handle: string) {
  await page.goto('/');
  expect((await api(page, '/sessions', { method: 'POST', body: { nickname } })).status).toBe(201);
  const upgraded = await api(page, '/accounts/upgrade', {
    method: 'POST',
    body: { handle }
  });
  expect(upgraded.status).toBe(200);
  return upgraded.body as {
    account: { id: string; handle: string };
    recoveryKey: string;
  };
}

test('Cloudflare safety, moderation, support and live-content operations are durable', async ({
  browser
}) => {
  const alphaContext = await browser.newContext();
  const bravoContext = await browser.newContext();
  const alpha = await alphaContext.newPage();
  const bravo = await bravoContext.newPage();
  const suffix = crypto.randomUUID().replaceAll('-', '').slice(0, 8);

  await alpha.goto('/');
  expect(await api(alpha, '/health')).toMatchObject({
    status: 200,
    body: { status: 'ok', storage: 'durable-objects-sqlite', runtime: 'cloudflare-workers' }
  });
  expect(await api(alpha, '/ready')).toMatchObject({
    status: 200,
    body: { status: 'ready', storage: 'durable-objects-sqlite', runtime: 'cloudflare-workers' }
  });
  const metrics = await alpha.evaluate(async () => {
    const response = await fetch('/api/metrics');
    return {
      status: response.status,
      contentType: response.headers.get('content-type'),
      text: await response.text()
    };
  });
  expect(metrics).toMatchObject({ status: 200, contentType: 'text/plain; version=0.0.4' });
  expect(metrics.text).toContain('mk01_matchmaking_queue_depth');
  expect(metrics.text).toContain('mk01_new_player_funnel_events_total');

  const alphaAccount = await createAccount(alpha, 'Ops Alpha', `OpsA${suffix}`);
  const bravoAccount = await createAccount(bravo, 'Ops Bravo', `OpsB${suffix}`);

  const created = await api(alpha, '/rooms', {
    method: 'POST',
    body: { name: 'Safety Evidence', visibility: 'PRIVATE' }
  });
  expect(created.status).toBe(201);
  const alphaSnapshot = (created.body as { snapshot: { room: { code: string } } }).snapshot;
  const joined = await api(bravo, '/rooms/join', {
    method: 'POST',
    body: { code: alphaSnapshot.room.code }
  });
  expect(joined.status).toBe(200);
  const bravoSnapshot = joined.body as {
    roomId: string;
    selfPlayerId: string;
    players: Array<{ id: string; nickname: string }>;
  };
  const targetPlayer = bravoSnapshot.players.find(
    (player) => player.id === bravoSnapshot.selfPlayerId
  )!;
  const alphaCurrent = await api(alpha, `/rooms/${bravoSnapshot.roomId}`);
  const targetFromAlpha = (
    alphaCurrent.body as { players: Array<{ id: string; nickname: string }> }
  ).players.find((player) => player.nickname === targetPlayer.nickname)!;
  const report = await api(alpha, '/reports', {
    method: 'POST',
    body: {
      roomId: bravoSnapshot.roomId,
      targetPlayerId: targetFromAlpha.id,
      category: 'CHAT',
      details: 'Repeated abusive tactical messages'
    }
  });
  expect(report).toMatchObject({ status: 201, body: { report: { status: 'OPEN' } } });
  const reportId = (report.body as { report: { reportId: string } }).report.reportId;

  const cases = await api(alpha, '/admin/moderation/reports?status=OPEN', { admin: true });
  expect(cases).toMatchObject({
    status: 200,
    body: { cases: [{ report: { id: reportId, targetIdentityId: bravoAccount.account.id } }] }
  });
  const support = await api(alpha, `/admin/support/accounts?query=${bravoAccount.account.handle}`, {
    admin: true
  });
  expect(support).toMatchObject({
    status: 200,
    body: {
      account: { id: bravoAccount.account.id },
      sessions: [{ nickname: bravoAccount.account.handle }]
    }
  });

  const suspended = await api(alpha, `/admin/moderation/reports/${reportId}/actions`, {
    method: 'POST',
    admin: true,
    operator: true,
    body: { action: 'SUSPEND', reason: 'Verified repeated abuse', durationHours: 1 }
  });
  expect(suspended).toMatchObject({
    status: 200,
    body: { action: { action: 'SUSPEND', targetIdentityId: bravoAccount.account.id } }
  });
  const suspensionActionId = (suspended.body as { action: { id: string } }).action.id;
  expect(
    (
      await api(bravo, '/accounts/login', {
        method: 'POST',
        body: {
          accountId: bravoAccount.account.id,
          recoveryKey: bravoAccount.recoveryKey
        }
      })
    ).body
  ).toMatchObject({ code: 'ACCOUNT_SUSPENDED' });

  expect(
    (
      await api(alpha, `/admin/moderation/reports/${reportId}/actions`, {
        method: 'POST',
        admin: true,
        operator: true,
        body: {
          action: 'REVERSE',
          reason: 'Appeal evidence accepted',
          reversesActionId: suspensionActionId
        }
      })
    ).status
  ).toBe(200);
  const relogin = await api(bravo, '/accounts/login', {
    method: 'POST',
    body: { accountId: bravoAccount.account.id, recoveryKey: bravoAccount.recoveryKey }
  });
  expect(relogin.status).toBe(201);

  const now = new Date().toISOString();
  const contentHistory = await api(alpha, '/admin/content/revisions?limit=1', { admin: true });
  const currentRevision = (contentHistory.body as { currentRevision: number }).currentRevision;
  const publishedRevision = currentRevision + 1;
  const payload = {
    activateAt: now,
    season: {
      id: 'FOUNDERS_SEASON',
      title: '창립 함대 시즌',
      description: '정식 함대 지휘 체계를 확립하고 첫 시즌 전공을 기록하십시오.',
      startsAt: '2026-08-01T00:00:00.000Z',
      endsAt: '2026-10-31T23:59:59.000Z'
    },
    events: [],
    featureFlags: { missionsEnabled: true, eventBannerEnabled: false },
    tuning: {
      dailyDeploymentRewardXp: 125,
      dailyAccuracyRewardXp: 175,
      weeklySupremacyRewardXp: 450
    },
    changeNote: 'Cloudflare E2E content revision'
  };
  expect(
    (
      await api(alpha, '/admin/content/validate', {
        method: 'POST',
        admin: true,
        operator: true,
        body: { expectedRevision: currentRevision, payload }
      })
    ).body
  ).toMatchObject({ valid: true, candidateRevision: publishedRevision });
  expect(
    (
      await api(alpha, '/admin/content/revisions', {
        method: 'POST',
        admin: true,
        operator: true,
        body: { expectedRevision: currentRevision, payload }
      })
    ).status
  ).toBe(201);
  expect(await api(alpha, '/content/live')).toMatchObject({
    status: 200,
    body: { revision: publishedRevision, featureFlags: { eventBannerEnabled: false } }
  });
  const profile = await api(alpha, '/profile');
  expect(profile).toMatchObject({
    status: 200,
    body: { liveContent: { revision: publishedRevision } }
  });
  expect(
    (profile.body as { missions: Array<{ id: string; rewardXp: number }> }).missions.find(
      (mission) => mission.id === 'DAILY_DEPLOYMENT'
    )
  ).toMatchObject({ rewardXp: 125 });
  expect(
    (
      await api(alpha, '/admin/content/rollback', {
        method: 'POST',
        admin: true,
        operator: true,
        body: {
          expectedRevision: publishedRevision,
          targetRevision: 0,
          changeNote: 'Restore baseline after E2E validation'
        }
      })
    ).status
  ).toBe(200);

  const supportRevocation = await api(
    alpha,
    `/admin/support/accounts/${bravoAccount.account.id}/sessions/revoke`,
    {
      method: 'POST',
      admin: true,
      operator: true,
      body: { reason: 'Customer-requested security reset' }
    }
  );
  expect(supportRevocation).toMatchObject({
    status: 200,
    body: { action: { action: 'REVOKE_ALL_SESSIONS', affectedSessionIds: [expect.any(String)] } }
  });
  expect((await api(bravo, '/sessions/current')).status).toBe(401);
  expect(
    await api(alpha, `/admin/support/accounts?query=${bravoAccount.account.id}`, { admin: true })
  ).toMatchObject({
    status: 200,
    body: { actions: [{ action: 'REVOKE_ALL_SESSIONS' }] }
  });

  const signals = await api(alpha, '/admin/integrity/signals', { admin: true });
  expect(signals).toMatchObject({ status: 200, body: { signals: [], nextBefore: null } });

  await alphaContext.close();
  await bravoContext.close();
  expect(alphaAccount.account.id).toMatch(/^[0-9a-f-]{36}$/);
});
