import { expect, test, type Response } from '@playwright/test';

test('Korean UI loads only bounded WOFF2 subset assets', async ({ page, browserName }) => {
  test.skip(browserName !== 'chromium', 'Chromium is the canonical resource-transfer audit.');
  const fontResponses: Response[] = [];
  page.on('response', (response) => {
    if (/\.(?:woff2?|ttf)(?:$|\?)/.test(response.url())) fontResponses.push(response);
  });

  await page.goto('/');
  const loaded = await page.evaluate(async () => {
    await document.fonts.ready;
    const regular = await document.fonts.load('400 16px "K119"', '가');
    const bold = await document.fonts.load('700 16px "K119"', '가');
    return {
      family: getComputedStyle(document.documentElement).fontFamily,
      regular: regular.length,
      bold: bold.length
    };
  });

  await expect.poll(() => fontResponses.length).toBeGreaterThan(0);
  const urls = fontResponses.map((response) => response.url());
  expect(new Set(urls).size).toBe(urls.length);
  expect(urls.every((url) => url.endsWith('.woff2'))).toBe(true);
  expect(urls.some((url) => /ibm-plex-sans-kr-korean-(?:400|700)-normal/.test(url))).toBe(false);
  expect(loaded.family).toContain('KLatin');
  expect(loaded.family).toContain('K119');
  expect(loaded.regular).toBeGreaterThan(0);
  expect(loaded.bold).toBeGreaterThan(0);

  const transferredBytes = (
    await Promise.all(fontResponses.map((response) => response.body().then((body) => body.length)))
  ).reduce((total, bytes) => total + bytes, 0);
  expect(transferredBytes).toBeLessThanOrEqual(500_000);
});
