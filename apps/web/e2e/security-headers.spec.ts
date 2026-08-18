import { expect, test } from '@playwright/test';

test('document security policy permits nonce-based hydration and blocks unsafe scripts', async ({
  page
}) => {
  const policyViolations: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error' && /content security policy/i.test(message.text())) {
      policyViolations.push(message.text());
    }
  });

  const sessionProbe = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/sessions/current' &&
      response.request().method() === 'GET'
  );
  const response = await page.goto('/');
  await sessionProbe;
  expect(response).not.toBeNull();
  const headers = response!.headers();
  const csp = headers['content-security-policy'];

  expect(csp).toContain("default-src 'self'");
  expect(csp).toContain("script-src 'self'");
  expect(csp).toMatch(/'nonce-[^']+'/);
  expect(csp).not.toContain("script-src 'self' 'unsafe-inline'");
  expect(headers['strict-transport-security']).toBe('max-age=31536000; includeSubDomains');
  expect(headers['x-frame-options']).toBe('DENY');
  expect(headers['x-content-type-options']).toBe('nosniff');

  await page.locator('#nickname').fill('CspHydration');
  await page.getByRole('button', { name: '작전 로비 입장' }).click();
  await expect(page).toHaveURL(/\/lobby$/);
  expect(policyViolations).toEqual([]);
});
