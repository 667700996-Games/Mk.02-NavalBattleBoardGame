import { expect, test, type Locator, type Page } from '@playwright/test';

async function register(page: Page) {
  const sessionProbe = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/sessions/current' &&
      response.request().method() === 'GET'
  );
  await page.goto('/');
  await sessionProbe;
  await page.locator('#nickname').fill('InputCaptain');
  await page.getByRole('button', { name: '플레이 방식 선택' }).click();
  await page.getByRole('button', { name: '싱글 플레이 선택' }).click();
  await expect(page).toHaveURL(/\/single-player$/);
}

async function expectPrompt(prompt: Locator, modality: string, copy: RegExp) {
  await expect(prompt).toHaveAttribute('data-modality', modality);
  await expect(prompt).toContainText(copy);
}

async function useTouch(page: Page) {
  await page.dispatchEvent('body', 'pointerdown', {
    bubbles: true,
    pointerId: 7,
    pointerType: 'touch',
    isPrimary: true
  });
}

async function usePointer(page: Page) {
  await page.dispatchEvent('body', 'pointerdown', {
    bubbles: true,
    pointerId: 8,
    pointerType: 'mouse',
    isPrimary: true
  });
}

test('placement, targeting and chat prompts follow mouse, keyboard and touch input', async ({
  page,
  browserName
}) => {
  test.skip(browserName !== 'chromium', 'Input modality behavior is covered once in Chromium.');

  await register(page);
  await page.getByRole('button', { name: /신병 RECRUIT/ }).click();
  await expect(page.getByRole('heading', { name: '함대 배치' })).toBeVisible();
  await expect(page.locator('.launch-sequence')).toBeHidden();

  const placement = page.getByTestId('input-prompt-placement');
  await expectPrompt(placement, 'pointer', /마우스.*함선 선택.*해역 클릭/);
  await page.keyboard.press('Tab');
  await expectPrompt(placement, 'keyboard', /키보드.*방향키.*Space 배치/);
  await useTouch(page);
  await expectPrompt(placement, 'touch', /터치.*함선 탭.*해역 탭/);

  await page.getByRole('button', { name: '자동 배치' }).click();
  await expect(page.getByText('5/5 함선 배치')).toBeVisible();
  await page.getByRole('button', { name: '배치 확정' }).click();
  await expect(page.getByText('상대 공격 보드')).toBeVisible();

  const targeting = page.getByTestId('input-prompt-targeting');
  await usePointer(page);
  await expectPrompt(targeting, 'pointer', /마우스.*공격 보드 클릭.*공격 실행/);
  await page.keyboard.press('ArrowRight');
  await expectPrompt(targeting, 'keyboard', /키보드.*방향키.*Space 좌표 선택/);
  await useTouch(page);
  await expectPrompt(targeting, 'touch', /터치.*공격 좌표 탭.*공격 실행 탭/);

  await page.getByRole('button', { name: '전술 채팅 열기' }).click();
  const chat = page.getByTestId('input-prompt-chat');
  await expectPrompt(chat, 'pointer', /마우스.*메시지 입력.*Enter 전송/);
  await page.keyboard.press('Tab');
  await expectPrompt(chat, 'keyboard', /키보드.*Shift\+Enter 줄바꿈.*Escape 닫기/);
  await useTouch(page);
  await expectPrompt(chat, 'touch', /터치.*전송 버튼.*빠른 신호/);
});
