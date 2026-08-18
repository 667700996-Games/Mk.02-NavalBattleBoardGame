import AxeBuilder from '@axe-core/playwright';
import { expect, test, type BrowserContext, type Page } from '@playwright/test';

const WCAG_TAGS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22a', 'wcag22aa'];

async function audit(page: Page, state: string) {
  const result = await new AxeBuilder({ page }).withTags(WCAG_TAGS).analyze();
  const details = result.violations
    .map(
      (violation) =>
        `${violation.id} (${violation.impact ?? 'unknown'}): ${violation.nodes
          .map((node) => node.target.join(' '))
          .join(', ')}`
    )
    .join('\n');
  expect(result.violations, `${state} WCAG violations:\n${details}`).toEqual([]);
}

async function register(page: Page, nickname: string) {
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
  await expect(page.getByRole('heading', { name: '작전 로비' })).toBeVisible();
}

async function deploy(page: Page) {
  await expect(page.getByRole('heading', { name: '함대 배치' })).toBeVisible();
  await expect(page.locator('.launch-sequence')).toBeHidden();
  await page.getByRole('button', { name: '자동 배치' }).click();
  await expect(page.getByText('5/5 함선 배치')).toBeVisible();
  const confirm = page.getByRole('button', { name: '배치 확정' });
  await expect(confirm).toBeEnabled();
  await confirm.click();
}

test('complete keyboard flow passes automated WCAG 2.2 AA checks', async ({
  browser,
  browserName
}) => {
  test.skip(browserName !== 'chromium', 'The semantic audit runs once in the Chromium project.');

  const firstContext: BrowserContext = await browser.newContext();
  const secondContext: BrowserContext = await browser.newContext();
  const first = await firstContext.newPage();
  const second = await secondContext.newPage();

  await first.goto('/');
  await expect(first.getByRole('heading', { name: /보이지 않는 함대를/ })).toBeVisible();
  await audit(first, 'landing');

  await register(first, 'A11yAlpha');
  await audit(first, 'lobby');

  const createTrigger = first.getByRole('button', { name: '작전실 생성' });
  await createTrigger.focus();
  await first.keyboard.press('Enter');
  const createDialog = first.getByRole('dialog');
  await expect(createDialog.getByRole('heading', { name: '새 작전실 편성' })).toBeVisible();
  await expect(createDialog.getByRole('button', { name: '닫기' })).toBeFocused();
  await audit(first, 'create room dialog');
  await first.keyboard.press('Shift+Tab');
  await expect(createDialog.getByRole('button', { name: '작전실 편성' })).toBeFocused();
  await first.keyboard.press('Tab');
  await expect(createDialog.getByRole('button', { name: '닫기' })).toBeFocused();
  await first.keyboard.press('Escape');
  await expect(createDialog).toBeHidden();
  await expect(createTrigger).toBeFocused();

  await createTrigger.press('Enter');
  await first.getByLabel('작전실 이름').fill('WCAG Fleet');
  await first.getByRole('radio', { name: /비공개/ }).check();
  await first.getByRole('button', { name: '작전실 편성' }).click();
  await expect(
    first.getByRole('heading', { name: '상대 지휘관의 입장을 기다리고 있습니다.' })
  ).toBeVisible();
  await audit(first, 'waiting room');
  const roomCode = new URL(first.url()).pathname.split('/').at(-1)!;

  await second.goto(`/join/${roomCode}`);
  await expect(second.getByRole('heading', { name: '작전 참가 요청' })).toBeVisible();
  await audit(second, 'join invitation');
  await second.getByLabel('지휘관 호출부호').fill('A11yBravo');
  await second.getByRole('button', { name: '초대 수락' }).click();

  await expect(
    first.getByRole('heading', { name: '모든 지휘관이 준비를 완료해야 합니다.' })
  ).toBeVisible();
  await first.getByRole('button', { name: '준비 완료' }).click();
  await second.getByRole('button', { name: '준비 완료' }).click();
  const startTrigger = first.getByRole('button', { name: '작전 시작' });
  await expect(startTrigger).toBeEnabled();
  await startTrigger.focus();
  await startTrigger.press('Enter');
  const startDialog = first.getByRole('dialog');
  await expect(
    startDialog.getByRole('heading', { name: '작전을 시작하시겠습니까?' })
  ).toBeVisible();
  await audit(first, 'start confirmation dialog');
  await startDialog.getByRole('button', { name: '작전 시작' }).click();

  await expect(first.getByRole('heading', { name: '함대 배치' })).toBeVisible();
  await expect(second.getByRole('heading', { name: '함대 배치' })).toBeVisible();
  await expect(first.locator('.launch-sequence')).toBeHidden();
  await audit(first, 'fleet placement');
  const firstPlacementCell = first.getByTestId('placement-cell-0-0');
  const nextPlacementCell = first.getByTestId('placement-cell-0-1');
  await firstPlacementCell.focus();
  await first.keyboard.press('ArrowRight');
  await expect(nextPlacementCell).toBeFocused();
  await first.keyboard.press('Space');
  await expect(first.getByText('1/5 함선 배치')).toBeVisible();

  await deploy(first);
  await deploy(second);
  await expect(first.getByText('상대 공격 보드')).toBeVisible();
  await expect(second.getByText('상대 공격 보드')).toBeVisible();
  await audit(first, 'battle host');
  await audit(second, 'battle guest');
  await expect(first.locator('.sr-only[aria-live="assertive"]')).toHaveCount(1);

  const active = (await first.locator('.turn-banner--mine').isVisible()) ? first : second;
  const targetCell = active.getByTestId('target-cell-0-0');
  const adjacentCell = active.getByTestId('target-cell-0-1');
  await targetCell.focus();
  await active.keyboard.press('ArrowRight');
  await expect(adjacentCell).toBeFocused();
  await active.keyboard.press('Space');
  await expect(active.getByRole('button', { name: '공격 실행' })).toBeEnabled();

  const chatToggle = active.getByRole('button', { name: '전술 채팅 열기' });
  await chatToggle.focus();
  await active.keyboard.press('Enter');
  const chatInput = active.getByRole('textbox', { name: '채팅 메시지' });
  await expect(chatInput).toBeFocused();
  await chatInput.fill('<invalid>');
  await active.keyboard.press('Enter');
  await expect(active.getByRole('alert')).toContainText('HTML 문법');
  await audit(active, 'chat error and live log');
  await chatInput.fill('키보드 교신 확인');
  await active.keyboard.press('Enter');
  await expect(active.getByText('키보드 교신 확인', { exact: true })).toBeVisible();
  await active.keyboard.press('Escape');
  await expect(chatToggle).toBeFocused();

  const surrenderTrigger = active.getByRole('button', { name: '작전 포기' });
  await surrenderTrigger.focus();
  await surrenderTrigger.press('Enter');
  const surrenderDialog = active.getByRole('dialog');
  await expect(
    surrenderDialog.getByRole('heading', { name: '작전을 종료하시겠습니까?' })
  ).toBeVisible();
  await audit(active, 'surrender confirmation dialog');
  await surrenderDialog.getByRole('button', { name: '기권' }).click();
  await expect(first.getByRole('heading', { name: /작전 (승리|패배)/ })).toBeVisible();
  await expect(second.getByRole('heading', { name: /작전 (승리|패배)/ })).toBeVisible();
  await audit(first, 'result host');
  await audit(second, 'result guest');

  await firstContext.close();
  await secondContext.close();
});
