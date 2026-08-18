import { expect, test, type Page } from '@playwright/test';

async function register(page: Page, nickname: string) {
  const sessionProbe = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/sessions/current' &&
      response.request().method() === 'GET'
  );
  await page.goto('/');
  await sessionProbe;
  await page.locator('#nickname').fill(nickname);
  await page.getByRole('button', { name: '작전 로비 입장' }).click();
  await expect(page).toHaveURL(/\/lobby$/);
}

test('ranked queue measures RTT and sends only player-controlled preferences', async ({
  page,
  browserName
}, testInfo) => {
  const handle = `Rank${browserName}${testInfo.repeatEachIndex}`;
  await register(page, handle);
  const upgraded = await page.request.post('/api/accounts/upgrade', {
    data: { handle }
  });
  expect(upgraded.ok()).toBe(true);
  await page.goto('/lobby');
  await expect(page.getByText(`${handle} 지휘관`)).toBeVisible();

  await page.getByRole('button', { name: '랭크', exact: true }).click();
  await expect(page.getByRole('heading', { name: '랭크 교전' })).toBeVisible();
  await page.getByLabel('랭크 매칭 리전').selectOption('JAPAN');
  await page.getByRole('button', { name: 'RTT 측정' }).click();
  await expect(page.getByRole('button', { name: /ms 재측정/ })).toBeVisible();

  const queueRequest = page.waitForRequest(
    (request) =>
      new URL(request.url()).pathname === '/api/matchmaking/ranked' && request.method() === 'POST'
  );
  await page.getByRole('button', { name: '상대 찾기' }).click();
  const submitted = (await queueRequest).postDataJSON() as Record<string, unknown>;
  expect(submitted.pool).toBe('RANKED');
  expect(submitted.region).toBe('JAPAN');
  expect(typeof submitted.latencyMs).toBe('number');
  expect(submitted).not.toHaveProperty('rating');
  expect(submitted).not.toHaveProperty('partyId');

  await expect(page.getByRole('heading', { name: '상대 지휘관 탐색 중' })).toBeVisible();
  await expect(page.getByText(/EXACT 범위/)).toBeVisible();
  await expect(page.getByText('RATING 1500')).toBeVisible();
  await page.getByRole('button', { name: '매칭 취소' }).click();
  await expect(page.getByRole('heading', { name: '랭크 교전' })).toBeVisible();

  await page.goto('/stats');
  await expect(page.getByText('PROVISIONAL 1500 RP')).toBeVisible();
});
