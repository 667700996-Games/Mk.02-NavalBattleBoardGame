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

async function createSession(page: Page, nickname: string) {
  await page.goto('/');
  const response = await api(page, '/sessions', { method: 'POST', body: { nickname } });
  expect(response.status).toBe(201);
}

test('Cloudflare matchmaking pairs two casual and two ranked sessions exactly once', async ({
  browser
}) => {
  const contexts = await Promise.all(Array.from({ length: 4 }, () => browser.newContext()));
  const [casualA, casualB, rankedA, rankedB] = await Promise.all(
    contexts.map((context) => context.newPage())
  );
  await Promise.all([
    createSession(casualA, 'QueueAlpha'),
    createSession(casualB, 'QueueBravo'),
    createSession(rankedA, 'RankAlpha'),
    createSession(rankedB, 'RankBravo')
  ]);

  const casualQueued = await api(casualA, '/matchmaking', { method: 'POST' });
  expect(casualQueued).toMatchObject({
    status: 200,
    body: { queued: true, ticket: { pool: 'CASUAL', region: 'AUTO', rating: null } }
  });
  const casualMatched = await api(casualB, '/matchmaking', { method: 'POST' });
  expect(casualMatched).toMatchObject({
    status: 200,
    body: {
      queued: false,
      matchQuality: { pool: 'CASUAL', phase: 'EXACT' },
      snapshot: { matchmakingQuality: { pool: 'CASUAL' } }
    }
  });
  const casualRecovered = await api(casualA, '/matchmaking', { method: 'POST' });
  const casualRoom = (
    casualMatched.body as { snapshot: { roomId: string; room: { code: string } } }
  ).snapshot;
  expect(casualRecovered).toMatchObject({
    status: 200,
    body: {
      queued: false,
      snapshot: { roomId: casualRoom.roomId, room: { code: casualRoom.room.code } }
    }
  });

  const suffix = crypto.randomUUID().replaceAll('-', '').slice(0, 8);
  for (const [page, handle] of [
    [rankedA, `RankA${suffix}`],
    [rankedB, `RankB${suffix}`]
  ] as const) {
    expect(
      (await api(page, '/accounts/upgrade', { method: 'POST', body: { handle } })).status
    ).toBe(200);
  }
  const rankedPreferences = {
    method: 'POST',
    body: { pool: 'RANKED', region: 'KOREA', latencyMs: 60 }
  };
  expect(await api(rankedA, '/matchmaking/ranked', rankedPreferences)).toMatchObject({
    status: 200,
    body: { queued: true, ticket: { rating: 1500, searchWindow: { phase: 'EXACT' } } }
  });
  const rankedMatched = await api(rankedB, '/matchmaking/ranked', rankedPreferences);
  expect(rankedMatched).toMatchObject({
    status: 200,
    body: {
      queued: false,
      matchQuality: { pool: 'RANKED', phase: 'EXACT', ratingDelta: 0 },
      snapshot: {
        matchmakingQuality: { pool: 'RANKED' },
        rankedMatch: { seasonId: 'FOUNDERS_SEASON', contentRevision: expect.any(Number) }
      }
    }
  });

  const rankedRoomId = (rankedMatched.body as { snapshot: { roomId: string } }).snapshot.roomId;
  const rankedRecovered = await api(rankedA, '/matchmaking/ranked', rankedPreferences);
  expect(rankedRecovered).toMatchObject({
    status: 200,
    body: { queued: false, snapshot: { roomId: rankedRoomId } }
  });

  await Promise.all(contexts.map((context) => context.close()));
});
