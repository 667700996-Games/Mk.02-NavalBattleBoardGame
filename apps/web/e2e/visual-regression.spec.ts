import { expect, test } from '@playwright/test';

async function settleVisualState(page: import('@playwright/test').Page) {
  await page.evaluate(async () => {
    await document.fonts.ready;
    window.scrollTo(0, 0);
  });
}

test.beforeEach(async ({ page }) => {
  await page.clock.setFixedTime(new Date('2026-08-18T00:35:00Z'));
});

test('landing command surface matches its approved golden', async ({ page }) => {
  const sessionProbe = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/sessions/current' &&
      response.request().method() === 'GET'
  );
  await page.goto('/');
  await sessionProbe;
  await expect(page.getByRole('heading', { name: /보이지 않는 함대를/ })).toBeVisible();
  await settleVisualState(page);
  await expect(page).toHaveScreenshot('landing.png', { fullPage: true });
});

test('authenticated lobby matches its approved empty-channel golden', async ({
  page
}, testInfo) => {
  const sessionProbe = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/sessions/current' &&
      response.request().method() === 'GET'
  );
  await page.goto('/');
  await sessionProbe;
  await page
    .locator('#nickname')
    .fill(testInfo.project.name === 'mobile-chromium' ? 'GoldenMobile' : 'GoldenDesktop');
  const roomList = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/rooms' && response.request().method() === 'GET'
  );
  await page.getByRole('button', { name: '작전 로비 입장' }).click();
  await expect(page).toHaveURL(/\/lobby$/);
  await roomList;
  await expect(page.getByText('NO ACTIVE OPERATIONS DETECTED')).toBeVisible();
  await expect(page.getByText('실시간 동기화 중')).toBeVisible();
  await settleVisualState(page);
  await expect(page).toHaveScreenshot('lobby-empty.png', { fullPage: true });
});
