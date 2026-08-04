import { expect, test, type BrowserContext, type Page } from '@playwright/test';

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
}

async function deploy(page: Page) {
  await expect(page.getByRole('heading', { name: '함대 배치' })).toBeVisible();
  await page.getByRole('button', { name: '자동 배치' }).click();
  await page.getByRole('button', { name: '배치 확정' }).click();
}

async function fire(page: Page, row: number, col: number) {
  const cell = page.getByTestId(`target-cell-${row}-${col}`);
  await cell.click();
  await page.getByRole('button', { name: '공격 실행' }).click();
  await expect.poll(() => cell.getAttribute('class')).toMatch(/cell--(miss|hit|sunk)/);
}

test('ready cancellation, tactical signals, deadline recovery and timeout defeat stay synchronized', async ({
  browser
}) => {
  const firstContext: BrowserContext = await browser.newContext();
  const secondContext: BrowserContext = await browser.newContext();
  const first = await firstContext.newPage();
  const second = await secondContext.newPage();

  await register(first, 'TimerAlpha');
  await first.getByRole('button', { name: '작전실 생성' }).click();
  await first.getByLabel('작전실 이름').fill('Deadline Protocol');
  await first.getByText('비공개', { exact: true }).click();
  await first.getByRole('button', { name: '작전실 편성' }).click();
  await expect(first).toHaveURL(/\/room\/[A-Z0-9]{6}$/);
  const roomCode = new URL(first.url()).pathname.split('/').at(-1)!;

  await second.goto(`/join/${roomCode}`);
  await second.getByLabel('지휘관 호출부호').fill('TimerBravo');
  await second.getByRole('button', { name: '초대 수락' }).click();
  await expect(second.getByRole('heading', { name: '함대 배치' })).toBeVisible();
  await expect(first.getByRole('heading', { name: '함대 배치' })).toBeVisible();

  await deploy(first);
  await expect(first.getByRole('heading', { name: '함대 배치 확정 완료' })).toBeVisible();
  await first.getByRole('button', { name: '전술 채팅 열기' }).click();
  await second.getByRole('button', { name: '전술 채팅 열기' }).click();
  await first.getByRole('button', { name: '준비 완료 취소' }).click();
  const unreadyDialog = first.getByRole('dialog');
  await expect(
    unreadyDialog.getByRole('heading', { name: '준비 상태를 해제하시겠습니까?' })
  ).toBeVisible();
  await expect(
    unreadyDialog.getByText('준비를 취소하면 함선 배치를 다시 수정할 수 있습니다.')
  ).toBeVisible();
  await unreadyDialog.getByRole('button', { name: '준비 취소' }).click();

  await expect(first.getByRole('heading', { name: '함대 배치' })).toBeVisible();
  await expect(first.getByText('준비 상태를 해제했습니다.', { exact: false })).toBeVisible();
  await expect(
    second.getByText('TimerAlpha 지휘관이 준비 상태를 해제', { exact: false })
  ).toBeVisible();
  await first.getByRole('button', { name: '채팅 닫기' }).click();
  await second.getByRole('button', { name: '채팅 닫기' }).click();
  await first.getByRole('button', { name: '초기화' }).click();
  await first.getByRole('button', { name: '자동 배치' }).click();
  await first.getByRole('button', { name: '배치 확정' }).click();

  await deploy(second);
  await expect(first.getByText('상대 공격 보드')).toBeVisible();
  await expect(second.getByText('상대 공격 보드')).toBeVisible();
  await expect(first.locator('.turn-clock strong')).toHaveText(/00:(0\d|10)/);
  await expect(second.getByText('ELAPSED')).toBeVisible();

  await first.getByRole('button', { name: '전술 채팅 열기' }).click();
  await second.getByRole('button', { name: '전술 채팅 열기' }).click();
  await first.getByRole('button', { name: '빠른 명령 선택' }).click();
  await first
    .getByRole('dialog')
    .getByRole('button', { name: /나이스 샷/ })
    .click();
  await expect(second.getByText('나이스 샷', { exact: true })).toBeVisible();
  await expect(second.getByText('QUICK COMMAND', { exact: true })).toBeVisible();

  await second.getByRole('button', { name: '이모지 선택' }).click();
  await second.getByRole('dialog').getByRole('button', { name: '🎯 이모지 전송' }).click();
  await expect(
    first.locator('.chat-message--emoji').getByText('🎯', { exact: true })
  ).toBeVisible();
  await first.getByRole('button', { name: '채팅 닫기' }).click();
  await second.getByRole('button', { name: '채팅 닫기' }).click();

  const attacker = (await first.locator('.turn-banner--mine').count()) ? first : second;
  const victim = attacker === first ? second : first;
  await fire(attacker, 0, 0);
  await expect(victim.locator('.turn-banner--mine')).toBeVisible();

  await victim.reload();
  await expect(victim.getByText('상대 공격 보드')).toBeVisible();
  await expect(victim.locator('.turn-banner--mine')).toBeVisible();
  await expect(victim.locator('.turn-clock strong')).toHaveText(/00:(0\d|10)/);

  for (let timeout = 1; timeout <= 3; timeout += 1) {
    if (timeout === 3) {
      await expect(victim.getByRole('heading', { name: '작전 패배' })).toBeVisible({
        timeout: 10_000
      });
      break;
    }
    await expect(attacker.locator('.turn-banner--mine')).toBeVisible({ timeout: 10_000 });
    await expect(victim.getByText(`TIMEOUT ${timeout}/3`)).toBeVisible();
    await fire(attacker, timeout, 0);
    await expect(victim.locator('.turn-banner--mine')).toBeVisible();
  }

  await expect(attacker.getByRole('heading', { name: '작전 승리' })).toBeVisible();
  await expect(attacker.getByText('Victory by Timeout')).toBeVisible();
  await expect(victim.getByText('Defeat by Timeout')).toBeVisible();
  await expect(victim.getByText('3회 연속 작전 시간 초과로 교전이 종료되었습니다.')).toBeVisible();
  await expect(victim.getByText('00:', { exact: false })).toBeVisible();

  await attacker.getByRole('button', { name: '전술 채팅 열기' }).click();
  await victim.getByRole('button', { name: '전술 채팅 열기' }).click();
  await attacker.getByRole('button', { name: '빠른 명령 선택' }).click();
  await attacker
    .getByRole('dialog')
    .getByRole('button', { name: /굿게임/ })
    .click();
  await expect(victim.getByText('굿게임', { exact: true })).toBeVisible();

  await firstContext.close();
  await secondContext.close();
});
