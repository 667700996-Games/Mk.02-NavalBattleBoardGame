import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

async function registerAccount(page: Page, nickname: string, handle: string) {
  const sessionProbe = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/sessions/current' &&
      response.request().method() === 'GET'
  );
  await page.goto('/');
  await sessionProbe;
  await page.locator('#nickname').fill(nickname);
  await page.getByRole('button', { name: '작전 로비 입장' }).click();
  await expect(page).toHaveURL(/\/lobby$/);
  const upgraded = await page.request.post('/api/accounts/upgrade', { data: { handle } });
  expect(upgraded.ok()).toBe(true);
  await page.goto('/social');
  await expect(page.getByRole('heading', { name: '소셜 작전 허브' })).toBeVisible();
}

test('friends, party, privacy, presence and direct invite form one responsive flow', async ({
  browser,
  browserName
}) => {
  const alphaContext = await browser.newContext();
  const bravoContext = await browser.newContext();
  const alpha = await alphaContext.newPage();
  const bravo = await bravoContext.newPage();
  const suffix = crypto.randomUUID().replaceAll('-', '').slice(0, 4);
  const alphaHandle = `SocA${browserName}${suffix}`;
  const bravoHandle = `SocB${browserName}${suffix}`;

  await registerAccount(alpha, 'Social Alpha', alphaHandle);
  await registerAccount(bravo, 'Social Bravo', bravoHandle);

  const privacyToggle = bravo.getByRole('checkbox', { name: '친구 요청 허용' });
  await expect(privacyToggle).toBeChecked();
  const privacyDisabled = bravo.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/social/privacy' &&
      response.request().method() === 'PUT'
  );
  await privacyToggle.uncheck();
  expect((await privacyDisabled).ok()).toBe(true);
  await expect(bravo.getByText('개인정보 선택을 저장했습니다.')).toBeVisible();
  const privacyEnabled = bravo.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/social/privacy' &&
      response.request().method() === 'PUT'
  );
  await privacyToggle.check();
  expect((await privacyEnabled).ok()).toBe(true);

  await alpha.getByLabel('지휘관 핸들').fill(bravoHandle);
  await alpha.getByRole('button', { name: '요청 전송' }).click();
  await expect(alpha.getByText('소셜 상태를 갱신했습니다.')).toBeVisible();

  await bravo.reload();
  await expect(bravo.getByText('친구 요청', { exact: true })).toBeVisible();
  await bravo.getByRole('button', { name: '수락' }).click();
  await expect(bravo.getByText('온라인', { exact: true })).toBeVisible();

  await alpha.reload();
  await expect(alpha.getByText(bravoHandle, { exact: true })).toBeVisible();
  await alpha.getByRole('button', { name: '파티 초대' }).click();
  await bravo.reload();
  await expect(bravo.getByText('2인 파티 초대')).toBeVisible();
  await bravo.getByRole('button', { name: '수락' }).click();
  await expect(bravo.getByRole('button', { name: '파티 나가기' })).toBeVisible();

  await alpha.reload();
  await alpha.getByRole('button', { name: '게임 초대' }).click();
  await expect(alpha).toHaveURL(/\/room\/[A-Z0-9]{6}$/);
  await bravo.reload();
  await expect(bravo.getByText(/작전 초대$/)).toBeVisible();
  await bravo.getByRole('button', { name: '수락' }).click();
  await expect(bravo).toHaveURL(/\/room\/[A-Z0-9]{6}$/);
  await expect(bravo.getByText(alphaHandle, { exact: true })).toBeVisible();

  await bravo.goto('/social');
  const audit = await new AxeBuilder({ page: bravo })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22a', 'wcag22aa'])
    .analyze();
  expect(audit.violations).toEqual([]);
  expect(
    await bravo.evaluate(
      () => document.documentElement.scrollWidth <= document.documentElement.clientWidth + 1
    )
  ).toBe(true);

  await alphaContext.close();
  await bravoContext.close();
});
