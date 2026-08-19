import { expect, test, type Page } from '@playwright/test';

async function api(
  page: Page,
  path: string,
  init: { method?: string; body?: unknown } = {}
): Promise<{ status: number; body: unknown }> {
  return page.evaluate(
    async ({ path: requestPath, init: requestInit }) => {
      const response = await fetch(`/api${requestPath}`, {
        method: requestInit.method ?? 'GET',
        credentials: 'include',
        headers:
          requestInit.body === undefined ? undefined : { 'content-type': 'application/json' },
        body: requestInit.body === undefined ? undefined : JSON.stringify(requestInit.body)
      });
      return {
        status: response.status,
        body: response.status === 204 ? null : await response.json()
      };
    },
    { path, init }
  );
}

test('Cloudflare account upgrade rotates credentials and supports login and remote revocation', async ({
  browser
}) => {
  const handle = `CF${crypto.randomUUID().replaceAll('-', '').slice(0, 14)}`;
  const firstContext = await browser.newContext();
  const secondContext = await browser.newContext();
  const first = await firstContext.newPage();
  const second = await secondContext.newPage();
  await Promise.all([first.goto('/'), second.goto('/')]);

  const guest = await api(first, '/sessions', {
    method: 'POST',
    body: { nickname: 'AccountAlpha' }
  });
  expect(guest.status).toBe(201);
  const firstSessionId = (guest.body as { id: string }).id;

  const upgraded = await api(first, '/accounts/upgrade', {
    method: 'POST',
    body: { handle }
  });
  expect(upgraded.status).toBe(200);
  expect(JSON.stringify(upgraded.body)).not.toContain('Hash');
  const account = (
    upgraded.body as { account: { id: string; handle: string }; recoveryKey: string }
  ).account;
  const recoveryKey = (upgraded.body as { recoveryKey: string }).recoveryKey;
  expect(account.handle).toBe(handle);
  expect(recoveryKey).toMatch(/^[A-Za-z0-9_-]{43}$/);

  const rotatedSession = await api(first, '/sessions/current');
  expect(rotatedSession.status).toBe(200);
  expect((rotatedSession.body as { nickname: string }).nickname).toBe(handle);

  const room = await api(first, '/rooms', {
    method: 'POST',
    body: { name: 'Revocation boundary', visibility: 'PRIVATE' }
  });
  expect(room.status).toBe(201);
  await first.evaluate(
    () =>
      new Promise<void>((resolve, reject) => {
        const socket = new WebSocket(`${location.origin.replace(/^http/, 'ws')}/ws`, 'mk01.v3');
        (window as typeof window & { revokedSocket?: WebSocket }).revokedSocket = socket;
        socket.addEventListener('open', () => resolve(), { once: true });
        socket.addEventListener('error', () => reject(new Error('WebSocket did not open')), {
          once: true
        });
      })
  );

  const login = await api(second, '/accounts/login', {
    method: 'POST',
    body: { accountId: account.id, recoveryKey }
  });
  expect(login.status).toBe(201);
  const secondSessionId = (login.body as { id: string }).id;

  const sessions = await api(second, '/accounts/sessions');
  expect(sessions.status).toBe(200);
  expect((sessions.body as { currentSessionId: string }).currentSessionId).toBe(secondSessionId);
  expect(
    (sessions.body as { sessions: Array<{ id: string }> }).sessions.map((item) => item.id)
  ).toEqual(expect.arrayContaining([firstSessionId, secondSessionId]));

  const exported = await api(second, '/accounts/export');
  expect(exported.status).toBe(200);
  expect(exported.body).toMatchObject({
    formatVersion: 1,
    credentialsExcluded: true,
    account: { id: account.id, handle }
  });
  expect(JSON.stringify(exported.body)).not.toMatch(/recoveryKey|tokenHash|recoveryKeyHash/);

  expect(
    (await api(second, `/accounts/sessions/${firstSessionId}`, { method: 'DELETE' })).status
  ).toBe(204);
  await expect
    .poll(() =>
      first.evaluate(
        () =>
          (window as typeof window & { revokedSocket?: WebSocket }).revokedSocket?.readyState ===
          WebSocket.CLOSED
      )
    )
    .toBe(true);
  expect((await api(first, '/sessions/current')).status).toBe(401);

  const wrongRecoveryKey = `${recoveryKey.slice(0, -1)}${recoveryKey.endsWith('A') ? 'B' : 'A'}`;
  expect(
    (
      await api(second, '/accounts', {
        method: 'DELETE',
        body: { recoveryKey: wrongRecoveryKey, confirmation: 'DELETE' }
      })
    ).status
  ).toBe(401);
  const deletion = await api(second, '/accounts', {
    method: 'DELETE',
    body: { recoveryKey, confirmation: 'DELETE' }
  });
  expect(deletion.status).toBe(200);
  expect(deletion.body).toMatchObject({
    stats: { sessionsDeleted: 1, roomsAnonymized: 1 }
  });
  expect((await api(second, '/sessions/current')).status).toBe(401);
  expect(
    (
      await api(second, '/accounts/login', {
        method: 'POST',
        body: { accountId: account.id, recoveryKey }
      })
    ).status
  ).toBe(401);

  await firstContext.close();
  await secondContext.close();
});
