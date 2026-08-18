import { expect, test } from '@playwright/test';

async function activateLocale(page: import('@playwright/test').Page, locale: 'en-US' | 'en-XA') {
  await page.evaluate((nextLocale) => localStorage.setItem('mk01_locale', nextLocale), locale);
  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-locale', locale);
}

test('launch locales persist and pseudolocalization expands without viewport overflow', async ({
  page
}) => {
  await page.goto('/');
  await activateLocale(page, 'en-US');

  await expect(page.locator('html')).toHaveAttribute('lang', 'en-US');
  await expect(page.getByRole('heading', { name: /Command the unseen fleet/ })).toBeVisible();
  const sourceTitle = (await page.locator('.display-title').innerText())
    .replace(/\s+/g, ' ')
    .trim();

  await activateLocale(page, 'en-XA');
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  await expect(page.locator('.display-title')).toContainText('⟦');
  const pseudoTitle = (await page.locator('.display-title').innerText())
    .replace(/\s+/g, ' ')
    .trim();
  expect(pseudoTitle.length).toBeGreaterThanOrEqual(Math.ceil(sourceTitle.length * 1.25));

  const dimensions = await page.evaluate(() => ({
    documentWidth: document.documentElement.scrollWidth,
    viewportWidth: document.documentElement.clientWidth,
    bodyWidth: document.body.scrollWidth
  }));
  expect(dimensions.documentWidth).toBeLessThanOrEqual(dimensions.viewportWidth + 1);
  expect(dimensions.bodyWidth).toBeLessThanOrEqual(dimensions.viewportWidth + 1);

  const selector = page.locator('.locale-control select');
  await expect(selector).toHaveValue('en-XA');
  await selector.selectOption('ko-KR');
  await expect(page.locator('html')).toHaveAttribute('lang', 'ko-KR');
  await expect.poll(() => page.evaluate(() => localStorage.getItem('mk01_locale'))).toBe('ko-KR');
});
