import { expect, test } from '@playwright/test';

test('cosmetic loadout persists, drives every presentation surface, and preserves fog of war', async ({
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
  await page.locator('#nickname').fill(`Present-${testInfo.project.name}`.slice(0, 16));
  await page.getByRole('button', { name: '플레이 방식 선택' }).click();
  await page.getByRole('button', { name: '멀티 플레이 선택' }).click();
  await expect(page).toHaveURL(/\/lobby$/);
  const settingsProbe = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/sessions/current' &&
      response.request().method() === 'GET'
  );
  await page.goto('/settings');
  await settingsProbe;
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', 'true');
  await expect(page.getByRole('heading', { name: '함대 프레젠테이션' })).toBeVisible();

  await page.getByLabel('함대 마감').selectOption('ember');
  await expect(page.locator('html')).toHaveAttribute('data-fleet-skin', 'ember');
  await page.getByLabel('해양 전장').selectOption('ice');
  await expect(page.locator('html')).toHaveAttribute('data-board-theme', 'ice');
  await page.getByLabel('충격 시그니처').selectOption('plasma');
  await expect(page.locator('html')).toHaveAttribute('data-effect-theme', 'plasma');
  await page.getByLabel('지휘관 엠블럼').selectOption('compass');
  await expect(page.locator('html')).toHaveAttribute('data-profile-emblem', 'compass');
  await page.getByLabel('인터페이스 프레임').selectOption('veteran');
  await expect(page.locator('html')).toHaveAttribute('data-presentation-frame', 'veteran');
  await page.getByLabel('효과 품질').selectOption('low');

  const html = page.locator('html');
  await expect(html).toHaveAttribute('data-fleet-skin', 'ember');
  await expect(html).toHaveAttribute('data-board-theme', 'ice');
  await expect(html).toHaveAttribute('data-effect-theme', 'plasma');
  await expect(html).toHaveAttribute('data-profile-emblem', 'compass');
  await expect(html).toHaveAttribute('data-presentation-frame', 'veteran');
  await expect(html).toHaveAttribute('data-effect-quality', 'low');

  const reloadProbe = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/sessions/current' &&
      response.request().method() === 'GET'
  );
  await page.reload();
  await reloadProbe;
  await expect(page.getByLabel('함대 마감')).toHaveValue('ember');
  await expect(page.getByLabel('해양 전장')).toHaveValue('ice');
  await expect(page.getByLabel('충격 시그니처')).toHaveValue('plasma');
  await expect(page.getByLabel('지휘관 엠블럼')).toHaveValue('compass');
  await expect(page.getByLabel('인터페이스 프레임')).toHaveValue('veteran');
  await expect(page.getByLabel('효과 품질')).toHaveValue('low');

  await page.goto('/single-player');
  await page.getByRole('button', { name: /신병 RECRUIT/ }).click();
  await expect(page.getByRole('heading', { name: '함대 배치' })).toBeVisible();
  await expect(page.locator('.launch-sequence')).toBeHidden();
  await page.getByRole('button', { name: '자동 배치' }).click();
  await page.getByRole('button', { name: '배치 확정' }).click();
  await expect(page.getByText('상대 공격 보드')).toBeVisible();

  const targets = page.locator('[data-testid^="target-cell-"]');
  await expect(targets).toHaveCount(100);
  const targetLabels = await targets.evaluateAll((cells) =>
    cells.map((cell) => cell.getAttribute('aria-label') ?? '')
  );
  expect(targetLabels.every((label) => label.includes('미공격 좌표'))).toBe(true);
  expect(targetLabels.some((label) => /항공모함|전함|순양함|잠수함|구축함/.test(label))).toBe(
    false
  );

  const hullFill = await page
    .locator('.vessel--deployed .vessel__hull')
    .first()
    .evaluate((hull) => getComputedStyle(hull).fill);
  expect(hullFill).toBe('rgb(83, 51, 38)');
});
