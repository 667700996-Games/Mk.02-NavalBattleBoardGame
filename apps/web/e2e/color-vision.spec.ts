import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

const WCAG_TAGS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22a', 'wcag22aa'];
const MODES = ['protanopia', 'deuteranopia', 'tritanopia'] as const;

async function audit(page: Page, state: string) {
  const result = await new AxeBuilder({ page }).withTags(WCAG_TAGS).analyze();
  const details = result.violations
    .map((violation) => `${violation.id}: ${violation.nodes.map((node) => node.target).join(', ')}`)
    .join('\n');
  expect(result.violations, `${state} violations:\n${details}`).toEqual([]);
}

async function assertSemanticPalette(page: Page, mode: (typeof MODES)[number]) {
  await expect(page.locator('html')).toHaveAttribute('data-color-vision', mode);
  const palette = await page.evaluate(() => {
    const style = getComputedStyle(document.documentElement);
    return ['--tactical', '--safe', '--warning', '--critical'].map((token) =>
      style.getPropertyValue(token).trim()
    );
  });
  expect(new Set(palette).size, `${mode} semantic colors must remain distinct`).toBe(4);
}

async function register(page: Page) {
  const sessionProbe = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/sessions/current' &&
      response.request().method() === 'GET'
  );
  await page.goto('/');
  await sessionProbe;
  await page.locator('#nickname').fill('ColorCaptain');
  await page.getByRole('button', { name: '작전 로비 입장' }).click();
  await expect(page).toHaveURL(/\/lobby$/);
}

async function openHydratedSettings(page: Page) {
  const sessionProbe = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/sessions/current' &&
      response.request().method() === 'GET'
  );
  await page.goto('/settings');
  await sessionProbe;
  await expect(page.getByRole('heading', { name: '환경 설정' })).toBeVisible();
}

test('color-vision presets persist and every combat state has non-color semantics', async ({
  page,
  browserName
}) => {
  test.skip(browserName !== 'chromium', 'Palette semantics are deterministic across renderers.');

  await register(page);
  await openHydratedSettings(page);
  const selector = page.getByLabel('색각 표시 프리셋');
  for (const mode of MODES) {
    await selector.selectOption(mode);
    await expect(selector).toHaveValue(mode);
    await expect
      .poll(() =>
        page.evaluate(
          () => JSON.parse(localStorage.getItem('mk01_preferences') ?? '{}').colorVision
        )
      )
      .toBe(mode);
    await assertSemanticPalette(page, mode);
    await audit(page, `${mode} settings`);
  }

  const reloadProbe = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/sessions/current' &&
      response.request().method() === 'GET'
  );
  await page.reload();
  await reloadProbe;
  await expect(selector).toHaveValue('tritanopia');
  await assertSemanticPalette(page, 'tritanopia');

  await page.goto('/lobby');
  await page.getByRole('button', { name: /신병 RECRUIT/ }).click();
  await expect(page.getByRole('heading', { name: '함대 배치' })).toBeVisible();
  await expect(page.locator('.launch-sequence')).toBeHidden();
  await page.getByRole('button', { name: '자동 배치' }).click();
  await expect(page.getByText('5/5 함선 배치')).toBeVisible();
  await page.getByRole('button', { name: '배치 확정' }).click();
  await expect(page.getByText('상대 공격 보드')).toBeVisible();

  for (const mode of MODES) {
    await page.evaluate((nextMode) => {
      const saved = JSON.parse(localStorage.getItem('mk01_preferences') ?? '{}');
      localStorage.setItem('mk01_preferences', JSON.stringify({ ...saved, colorVision: nextMode }));
    }, mode);
    await page.reload();
    await expect(page.getByText('상대 공격 보드')).toBeVisible();
    await assertSemanticPalette(page, mode);
    await audit(page, `${mode} battle`);
  }

  await expect(page.getByText('MISS', { exact: true })).toBeVisible();
  await expect(page.getByText('HIT', { exact: true })).toBeVisible();
  await expect(page.getByText('SUNK', { exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: /공격 좌표를 지정하십시오/ })).toBeVisible({
    timeout: 15_000
  });
  const target = page.getByTestId('target-cell-0-0');
  await expect(target).toHaveAttribute('aria-label', /A1, 미공격 좌표/);
  await target.click();
  await page.getByRole('button', { name: '공격 실행' }).click();
  await expect(target).toHaveAttribute('aria-label', /A1, (빗나감|명중|격침)/);
  const outcome = await target.getAttribute('aria-label');
  await expect(
    target.locator(outcome?.includes('빗나감') ? '.miss-marker' : '.hit-marker')
  ).toBeVisible();
});
