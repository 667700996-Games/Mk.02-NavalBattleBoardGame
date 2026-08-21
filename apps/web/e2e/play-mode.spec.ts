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

test('tutorial choice can return to play selection while training is in progress', async ({
  page
}) => {
  await registerToPlaySelection(page, 'TutorialCaptain');

  await page.getByRole('button', { name: '튜토리얼 선택' }).click();

  await expect(page).toHaveURL(/\/tutorial$/);
  await expect(page.getByRole('heading', { name: '작전 지휘 튜토리얼' })).toBeVisible();
  await expect(
    page.getByText('한 판을 시작하기 전에 핵심 판단과 복구 규칙을 직접 확인합니다.')
  ).toHaveCount(0);
  const modeNavigation = page.locator('.tutorial-mode-nav');
  const lessonNavigation = page.locator('.tutorial-actions');
  await expect(lessonNavigation).toBeVisible();
  expect(
    await modeNavigation.evaluate(
      (navigation, actions) =>
        Boolean(navigation.compareDocumentPosition(actions) & Node.DOCUMENT_POSITION_FOLLOWING),
      await lessonNavigation.elementHandle()
    )
  ).toBe(true);
  await page.getByRole('button', { name: /다음 훈련/ }).click();
  await page.getByRole('button', { name: '플레이 방식 다시 선택' }).click();
  await expect(page).toHaveURL(/\/play$/);
});

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

test('multiplayer choice opens a focused lobby with right-aligned history and settings', async ({
  page
}) => {
  await registerToPlaySelection(page, 'MultiCaptain');

  await page.getByRole('button', { name: '멀티 플레이 선택' }).click();

  await expect(page).toHaveURL(/\/lobby$/);
  await expect(page.getByRole('heading', { name: '작전 로비' })).toBeVisible();
  await expect(page.getByRole('button', { name: '작전실 생성' })).toBeVisible();
  await expect(page.getByText('AI 연습 교전', { exact: true })).toHaveCount(0);
  await expect(page.locator('.dashboard-side')).toHaveCount(0);
  await expect(page.getByText(/MultiCaptain 지휘관, 신호를 선택/)).toHaveCount(0);
  const headerInner = page.locator('.app-header__inner');
  await expect(headerInner.locator(':scope > :last-child')).toHaveClass(/nav-links/);
  await expect(headerInner.locator('.nav-links .nav-link')).toHaveText(['전투 기록', '설정']);
  await expect(page.locator('.app-header').getByRole('link', { name: '플레이 선택' })).toHaveCount(
    0
  );
});
