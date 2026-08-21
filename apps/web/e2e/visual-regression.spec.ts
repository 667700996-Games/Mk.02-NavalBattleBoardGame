import { expect, test } from '@playwright/test';

async function settleVisualState(page: import('@playwright/test').Page) {
  const notificationCloseButtons = page.locator('.toast-stack .ui-icon-button');
  while ((await notificationCloseButtons.count()) > 0) {
    await notificationCloseButtons.first().click();
  }
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
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', 'true');
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
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', 'true');
  await page
    .locator('#nickname')
    .fill(testInfo.project.name === 'mobile-chromium' ? 'GoldenMobile' : 'GoldenDesktop');
  const roomList = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/rooms' && response.request().method() === 'GET'
  );
  await page.getByRole('button', { name: '플레이 방식 선택' }).click();
  await page.getByRole('button', { name: '멀티 플레이 선택' }).click();
  await expect(page).toHaveURL(/\/lobby$/);
  await roomList;
  await expect(page.getByRole('heading', { name: '활성 작전 없음' })).toBeVisible();
  await settleVisualState(page);
  await expect(page).toHaveScreenshot('lobby-empty.png', { fullPage: true });
});

test('practice combat and after-action report match approved goldens', async ({
  page
}, testInfo) => {
  const sessionProbe = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/sessions/current' &&
      response.request().method() === 'GET'
  );
  await page.goto('/');
  await sessionProbe;
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', 'true');
  await page
    .locator('#nickname')
    .fill(testInfo.project.name === 'mobile-chromium' ? 'CombatMobile' : 'CombatDesktop');
  await page.getByRole('button', { name: '플레이 방식 선택' }).click();
  await page.getByRole('button', { name: '싱글 플레이 선택' }).click();
  await page.getByRole('button', { name: /신병 RECRUIT/ }).click();
  await expect(page.getByRole('heading', { name: '함대 배치' })).toBeVisible();
  await expect(page.locator('.launch-sequence')).toBeHidden();
  for (const row of [0, 2, 4, 6, 8]) {
    await page.getByTestId(`placement-cell-${row}-0`).click();
  }
  await expect(page.getByText('5/5 함선 배치')).toBeVisible();
  await expect(page.locator('.placement .board-cell.cell--ship')).toHaveCount(17);
  await page.getByRole('button', { name: '배치 확정' }).click();
  await expect(page.getByRole('heading', { name: /공격 좌표를 지정하십시오/ })).toBeVisible({
    timeout: 30_000
  });

  const firstTarget = page.getByTestId('target-cell-0-0');
  await firstTarget.click();
  await page.getByRole('button', { name: '공격 실행' }).click();
  await expect(firstTarget).toHaveAttribute('aria-label', /A1, 명중/);
  await expect(page.locator('.fire-sequence')).toBeHidden({ timeout: 10_000 });
  await expect(page.locator('.combat-event')).toBeHidden({ timeout: 10_000 });
  await expect(page.getByRole('heading', { name: /공격 좌표를 지정하십시오/ })).toBeVisible({
    timeout: 30_000
  });
  await settleVisualState(page);
  await expect(page).toHaveScreenshot('battle-practice-hit.png', { fullPage: true });

  await page.getByRole('button', { name: '작전 포기' }).first().click();
  const surrenderDialog = page.getByRole('dialog');
  await expect(
    surrenderDialog.getByRole('heading', { name: '작전을 종료하시겠습니까?' })
  ).toBeVisible();
  await surrenderDialog.getByRole('button', { name: '기권' }).click();
  await expect(page.getByRole('heading', { name: '작전 패배' })).toBeVisible();
  await settleVisualState(page);
  await expect(page).toHaveScreenshot('result-defeat.png', { fullPage: true });
});
