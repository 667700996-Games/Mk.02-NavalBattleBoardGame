import AxeBuilder from '@axe-core/playwright';
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
  await page.getByRole('button', { name: '플레이 방식 선택' }).click();
  await page.getByRole('button', { name: '멀티 플레이 선택' }).click();
  await expect(page).toHaveURL(/\/lobby$/);
}

test('ranked queue measures RTT and sends only player-controlled preferences', async ({
  page,
  browserName
}, testInfo) => {
  const suffix = crypto.randomUUID().replaceAll('-', '').slice(0, 3);
  const handle = `Rank${browserName}${testInfo.repeatEachIndex}${suffix}`;
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
  await expect(page.getByText(/정밀 범위/)).toBeVisible();
  await expect(page.getByText('레이팅 1,500')).toBeVisible();
  await page.getByRole('button', { name: '매칭 취소' }).click();
  await expect(page.getByRole('heading', { name: '랭크 교전' })).toBeVisible();

  await page.goto('/stats');
  await expect(page.getByText('PROVISIONAL 1500 RP')).toBeVisible();
  await expect(page.getByRole('heading', { name: '시즌 지휘관 순위' })).toBeVisible();
  await expect(page.getByText('공개 가능한 배치 완료 지휘관이 없습니다')).toBeVisible();
  await expect(page.getByRole('button', { name: '공개 중' })).toBeVisible();
  await page.getByRole('button', { name: '공개 중' }).click();
  await expect(page.getByRole('button', { name: '비공개' })).toBeVisible();
});

test('mobile ranked leaderboard keeps privacy controls readable without overflow', async ({
  page,
  browserName
}, testInfo) => {
  test.skip(browserName !== 'chromium', 'One mobile layout run covers the shared responsive CSS.');
  const suffix = crypto.randomUUID().replaceAll('-', '').slice(0, 4);
  const handle = `BoardMobile${testInfo.repeatEachIndex}${suffix}`;
  await register(page, handle);
  const upgraded = await page.request.post('/api/accounts/upgrade', { data: { handle } });
  expect(upgraded.ok()).toBe(true);
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto('/stats');
  await expect(page.getByRole('heading', { name: '시즌 지휘관 순위' })).toBeVisible();
  const visibility = page.getByRole('button', { name: '공개 중' });
  const season = page.getByLabel('조회할 랭크 시즌');
  await expect(visibility).toBeVisible();
  await expect(season).toBeVisible();
  expect((await visibility.boundingBox())?.height).toBeGreaterThanOrEqual(44);
  expect((await season.boundingBox())?.height).toBeGreaterThanOrEqual(44);
  const dimensions = await page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    clientWidth: document.documentElement.clientWidth
  }));
  expect(dimensions.scrollWidth).toBeLessThanOrEqual(dimensions.clientWidth + 1);
  const audit = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22a', 'wcag22aa'])
    .analyze();
  expect(audit.violations).toEqual([]);
});
