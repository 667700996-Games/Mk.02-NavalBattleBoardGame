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
  const confirm = page.getByRole('button', { name: '배치 확정' });
  await expect(confirm).toBeEnabled();
  await confirm.click();
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
  await first.locator('#turn-duration').evaluate((select: HTMLSelectElement) => {
    const option = new Option('10초 E2E fixture', '10');
    (option as HTMLOptionElement & { __value?: number }).__value = 10;
    select.add(option);
    option.selected = true;
    select.dispatchEvent(new Event('change', { bubbles: true }));
  });
  await first.getByRole('button', { name: '작전실 편성' }).click();
  await expect(first).toHaveURL(/\/room\/[A-Z0-9]{6}$/);
  const roomCode = new URL(first.url()).pathname.split('/').at(-1)!;

  await second.goto(`/join/${roomCode}`);
  await second.getByLabel('지휘관 호출부호').fill('TimerBravo');
  await second.getByRole('button', { name: '초대 수락' }).click();
  await expect(
    first.getByRole('heading', { name: '모든 지휘관이 준비를 완료해야 합니다.' })
  ).toBeVisible();
  await expect(
    second.getByRole('heading', { name: '모든 지휘관이 준비를 완료해야 합니다.' })
  ).toBeVisible();
  await expect(first.getByRole('button', { name: '작전 시작' })).toBeDisabled();
  await expect(second.getByRole('button', { name: '작전 시작' })).toHaveCount(0);

  await first.getByRole('button', { name: '준비 완료' }).click();
  await expect(first.getByRole('button', { name: '작전 시작' })).toBeDisabled();
  await second.getByRole('button', { name: '준비 완료' }).click();
  await expect(first.getByRole('button', { name: '작전 시작' })).toBeEnabled();
  await expect(second.getByText('방장의 작전 개시 대기')).toBeVisible();
  await expect(first.getByRole('heading', { name: '함대 배치' })).toHaveCount(0);

  await second.reload();
  await expect(second.getByRole('button', { name: '준비 취소' })).toBeVisible();
  await expect(second.getByText('GUEST', { exact: true })).toBeVisible();
  await second.getByRole('button', { name: '준비 취소' }).click();
  await expect(first.getByRole('button', { name: '작전 시작' })).toBeDisabled();
  await first.getByRole('button', { name: '전술 채팅 열기' }).click();
  await second.getByRole('button', { name: '전술 채팅 열기' }).click();
  await expect(
    first.getByText('TimerBravo 지휘관이 준비를 취소했습니다.', { exact: false })
  ).toBeVisible();
  await first.getByRole('button', { name: '채팅 닫기' }).click();
  await second.getByRole('button', { name: '채팅 닫기' }).click();
  await second.getByRole('button', { name: '준비 완료' }).click();
  await expect(first.getByRole('button', { name: '작전 시작' })).toBeEnabled();

  const forgedStartCode = await second.evaluate(async () => {
    const snapshot = await fetch('/api/games/recover').then((response) => response.json());
    return new Promise<string>((resolve, reject) => {
      const protocol = location.protocol === 'https:' ? 'wss:' : 'ws:';
      const socket = new WebSocket(`${protocol}//${location.host}/ws`);
      const timeout = window.setTimeout(
        () => reject(new Error('game:start rejection timeout')),
        5_000
      );
      socket.addEventListener('open', () => {
        socket.send(
          JSON.stringify({
            type: 'game:start',
            payload: {
              requestId: crypto.randomUUID(),
              roomId: snapshot.roomId,
              playerId: snapshot.selfPlayerId,
              roomVersion: snapshot.roomVersion
            }
          })
        );
      });
      socket.addEventListener('message', (event) => {
        const message = JSON.parse(String(event.data));
        if (message.type !== 'game:start:rejected') return;
        window.clearTimeout(timeout);
        socket.close();
        resolve(message.payload.code);
      });
    });
  });
  expect(forgedStartCode).toBe('NOT_HOST');
  await second.reload();
  await expect(second.getByRole('button', { name: '준비 취소' })).toBeVisible();
  await expect(first.getByRole('button', { name: '작전 시작' })).toBeEnabled();

  await first.getByRole('button', { name: '작전 시작' }).click();
  const startDialog = first.getByRole('dialog');
  await expect(
    startDialog.getByRole('heading', { name: '작전을 시작하시겠습니까?' })
  ).toBeVisible();
  await expect(
    startDialog.getByText('두 지휘관의 준비가 완료되었습니다.', { exact: false })
  ).toBeVisible();
  await startDialog.getByRole('button', { name: '작전 시작' }).click();
  await expect(first.getByRole('heading', { name: '함대 배치' })).toBeVisible();
  await expect(second.getByRole('heading', { name: '함대 배치' })).toBeVisible();

  await deploy(first);
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
        timeout: 15_000
      });
      break;
    }
    await expect(victim.getByText(`TIMEOUT ${timeout}/3`)).toBeVisible({ timeout: 15_000 });
    await expect(attacker.locator('.turn-banner--mine')).toBeVisible();
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
