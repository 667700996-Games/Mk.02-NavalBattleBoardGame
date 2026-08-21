import { expect, test } from '@playwright/test';

test('Korean UI loads only bounded WOFF2 subset assets', async ({ page, browserName }) => {
  test.skip(browserName !== 'chromium', 'Chromium is the canonical resource-transfer audit.');
  const fontAssets = new Map<string, Promise<number>>();
  page.on('response', (response) => {
    const url = response.url();
    if (response.ok() && /\.(?:woff2?|ttf)(?:$|\?)/.test(url) && !fontAssets.has(url)) {
      fontAssets.set(
        url,
        response.body().then((body) => body.length)
      );
    }
  });

  await page.goto('/');
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', 'true');
  const loaded = await page.evaluate(async () => {
    await document.fonts.ready;
    const regular = await document.fonts.load('400 16px "KKorean"', '가');
    const bold = await document.fonts.load('700 16px "KKorean"', '가');
    return {
      family: getComputedStyle(document.documentElement).fontFamily,
      regular: regular.length,
      bold: bold.length
    };
  });

  await page.locator('#nickname').fill('FontCadet');
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
  await expect(page.getByRole('heading', { name: '오디오 및 햅틱' })).toBeVisible();
  await page.evaluate(() => document.fonts.ready);

  await expect.poll(() => fontAssets.size).toBeGreaterThan(0);
  // Route changes can surface memory-cache responses more than once. A font URL is
  // downloaded at most once in a fresh context, so audit the unique asset payload.
  const urls = [...fontAssets.keys()];
  expect(urls.every((url) => url.endsWith('.woff2'))).toBe(true);
  expect(urls.some((url) => /ibm-plex-sans-kr-korean-(?:400|700)-normal/.test(url))).toBe(false);
  expect(loaded.family).toContain('KLatin');
  expect(loaded.family).toContain('KKorean');
  expect(loaded.regular).toBeGreaterThan(0);
  expect(loaded.bold).toBeGreaterThan(0);

  const transferredBytes = (await Promise.all(fontAssets.values())).reduce(
    (total, bytes) => total + bytes,
    0
  );
  console.info(`Font delivery: ${urls.length} unique WOFF2 assets, ${transferredBytes} bytes`);
  expect(transferredBytes).toBeLessThanOrEqual(500_000);
});
