import { expect, test, type Page } from '@playwright/test';

async function registerToPlaySelection(page: Page, nickname: string) {
  const sessionProbe = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/sessions/current' &&
      response.request().method() === 'GET'
  );
  await page.goto('/');
  await sessionProbe;
  await page.locator('#nickname').fill(nickname);
  await page.getByRole('button', { name: '플레이 방식 선택' }).click();
  await expect(page).toHaveURL(/\/play$/);
  await expect(page.getByRole('heading', { name: '전투 방식을 선택하십시오.' })).toBeVisible();
}

test('single-player choice opens the dedicated AI tactical range', async ({ page }) => {
  await registerToPlaySelection(page, 'SoloCaptain');

  await page.getByRole('button', { name: '싱글 플레이 선택' }).click();

  await expect(page).toHaveURL(/\/single-player$/);
  await expect(page.getByRole('heading', { name: 'AI 전술 훈련장' })).toBeVisible();
  await expect(page.getByRole('button', { name: /신병 RECRUIT/ })).toBeVisible();
  await expect(page.getByRole('button', { name: /장교 OFFICER/ })).toBeVisible();
  await expect(page.getByRole('button', { name: /제독 ADMIRAL/ })).toBeVisible();

  await page.getByRole('button', { name: /신병 RECRUIT/ }).click();
  await expect(page).toHaveURL(/\/room\//);
  await expect(page.getByRole('heading', { name: '함대 배치' })).toBeVisible();
});

test('multiplayer choice opens a lobby without AI practice controls', async ({ page }) => {
  await registerToPlaySelection(page, 'MultiCaptain');

  await page.getByRole('button', { name: '멀티 플레이 선택' }).click();

  await expect(page).toHaveURL(/\/lobby$/);
  await expect(page.getByRole('heading', { name: '작전 로비' })).toBeVisible();
  await expect(page.getByRole('button', { name: '작전실 생성' })).toBeVisible();
  await expect(page.getByText('AI 연습 교전', { exact: true })).toHaveCount(0);
});
