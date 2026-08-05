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
  const sessionCreated = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/sessions' && response.request().method() === 'POST'
  );
  await page.getByRole('button', { name: '작전 로비 입장' }).click();
  expect((await sessionCreated).status()).toBe(201);
  await expect(page).toHaveURL(/\/lobby$/);
  await expect(page.getByRole('heading', { name: '작전 로비' })).toBeVisible();
}

async function deploy(page: Page) {
  await expect(page.getByRole('heading', { name: '함대 배치' })).toBeVisible();
  await page.getByRole('button', { name: '자동 배치' }).click();
  await page.getByRole('button', { name: '배치 확정' }).click();
}

async function startOperation(host: Page, guest: Page) {
  await expect(
    host.getByRole('heading', { name: '모든 지휘관이 준비를 완료해야 합니다.' })
  ).toBeVisible();
  await host.getByRole('button', { name: '준비 완료' }).click();
  await guest.getByRole('button', { name: '준비 완료' }).click();
  await expect(host.getByRole('button', { name: '작전 시작' })).toBeEnabled();
  await host.getByRole('button', { name: '작전 시작' }).click();
  await host.getByRole('dialog').getByRole('button', { name: '작전 시작' }).click();
  await expect(host.getByRole('heading', { name: '함대 배치' })).toBeVisible();
  await expect(guest.getByRole('heading', { name: '함대 배치' })).toBeVisible();
}

test('room chat restores after refresh and surrender ends both clients immediately', async ({
  browser
}) => {
  const firstContext: BrowserContext = await browser.newContext();
  const secondContext: BrowserContext = await browser.newContext();
  const first = await firstContext.newPage();
  const second = await secondContext.newPage();

  await register(first, 'Alpha');
  await first.getByRole('button', { name: '작전실 생성' }).click();
  await first.getByLabel('작전실 이름').fill('Surrender Channel');
  await first.getByText('비공개', { exact: true }).click();
  await first.getByRole('button', { name: '작전실 편성' }).click();
  await expect(first).toHaveURL(/\/room\/[A-Z0-9]{6}$/);
  const roomCode = new URL(first.url()).pathname.split('/').at(-1)!;

  await second.goto(`/join/${roomCode}`);
  await second.getByLabel('지휘관 호출부호').fill('Bravo');
  await second.getByRole('button', { name: '초대 수락' }).click();
  await startOperation(first, second);
  await deploy(first);
  await deploy(second);
  await expect(first.getByText('상대 공격 보드')).toBeVisible();
  await expect(second.getByText('상대 공격 보드')).toBeVisible();

  await first.getByRole('button', { name: '전술 채팅 열기' }).click();
  await second.getByRole('button', { name: '전술 채팅 열기' }).click();
  await expect(first.getByText('보안 채널 동기화 중…')).toHaveCount(0);
  await expect(second.getByText('보안 채널 동기화 중…')).toHaveCount(0);
  const firstComposer = first.getByLabel('채팅 메시지');
  await firstComposer.fill('Hold position');
  await expect(second.getByText('Alpha 입력 중…')).toBeVisible();
  await second.keyboard.press('Escape');
  await expect(second.getByRole('button', { name: '전술 채팅 열기' })).toBeVisible();
  await firstComposer.fill('Hold position\nSector C4');
  await first.getByRole('button', { name: '채팅 전송' }).click();
  await expect(first.getByText('Hold position\nSector C4')).toBeVisible();
  await expect(second.getByLabel('읽지 않은 메시지 1개')).toBeVisible();
  await second.getByRole('button', { name: '전술 채팅 열기' }).click();
  await expect(second.getByText('Hold position\nSector C4')).toBeVisible();

  await second.reload();
  await expect(second.getByText('상대 공격 보드')).toBeVisible();
  await second.getByRole('button', { name: '전술 채팅 열기' }).click();
  await expect(second.getByText('Hold position\nSector C4')).toBeVisible();

  await first.getByRole('button', { name: '작전 포기' }).click();
  const modal = first.getByRole('dialog');
  await expect(modal.getByRole('heading', { name: '작전을 종료하시겠습니까?' })).toBeVisible();
  await expect(modal.getByText('기권하면 즉시 패배 처리되며 되돌릴 수 없습니다.')).toBeVisible();
  await modal.getByRole('button', { name: '기권' }).click();

  await expect(first.getByRole('heading', { name: '작전 패배' })).toBeVisible();
  await expect(second.getByRole('heading', { name: '작전 승리' })).toBeVisible();
  await expect(first.getByText('Defeat by Surrender')).toBeVisible();
  await expect(second.getByText('Victory by Surrender')).toBeVisible();
  await expect(second.getByText('Commander Alpha surrendered.', { exact: false })).toBeVisible();

  await firstContext.close();
  await secondContext.close();
});
