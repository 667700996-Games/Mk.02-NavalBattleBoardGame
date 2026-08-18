import { expect, test, type Page } from '@playwright/test';

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

async function startOperation(host: Page, guest: Page) {
  await expect(
    host.getByRole('heading', { name: '모든 지휘관이 준비를 완료해야 합니다.' })
  ).toBeVisible();
  await expect(
    guest.getByRole('heading', { name: '모든 지휘관이 준비를 완료해야 합니다.' })
  ).toBeVisible();
  await host.getByRole('button', { name: '준비 완료' }).click();
  await guest.getByRole('button', { name: '준비 완료' }).click();
  await expect(host.getByRole('button', { name: '작전 시작' })).toBeEnabled();
  await host.getByRole('button', { name: '작전 시작' }).click();
  await host.getByRole('dialog').getByRole('button', { name: '작전 시작' }).click();
  for (const page of [host, guest]) {
    await expect(page.getByRole('heading', { name: '함대 배치' })).toBeVisible();
    const autoDeploy = page.getByRole('button', { name: '자동 배치' });
    await expect(autoDeploy).toBeEnabled();
    await autoDeploy.click();
    await expect(page.getByText('5/5 함선 배치')).toBeVisible();
    const confirm = page.getByRole('button', { name: '배치 확정' });
    await expect(confirm).toBeEnabled();
    await confirm.click();
  }
  await expect(host.getByText('상대 공격 보드')).toBeVisible();
  await expect(guest.getByText('상대 공격 보드')).toBeVisible();
}

test('public battles expose only the delayed visibility-filtered spectator projection', async ({
  browser,
  browserName
}) => {
  const hostContext = await browser.newContext();
  const guestContext = await browser.newContext();
  const viewerContext = await browser.newContext();
  const host = await hostContext.newPage();
  const guest = await guestContext.newPage();
  const viewer = await viewerContext.newPage();
  const operationName = `Public Watch ${browserName}`;

  await register(host, 'Watch Alpha');
  await host.getByRole('button', { name: '작전실 생성' }).click();
  const roomCreated = host.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/rooms' && response.request().method() === 'POST'
  );
  const roomName = host.getByLabel('작전실 이름');
  await roomName.fill(operationName);
  await roomName.press('Enter');
  expect((await roomCreated).status()).toBe(201);
  await expect(host).toHaveURL(/\/room\/[A-Z0-9]{6}$/);
  const roomCode = new URL(host.url()).pathname.split('/').at(-1)!;

  await guest.goto(`/join/${roomCode}`);
  await guest.getByLabel('지휘관 호출부호').fill('Watch Bravo');
  await guest.getByRole('button', { name: '초대 수락' }).click();
  await startOperation(host, guest);

  await register(viewer, 'Watch Observer');
  await expect(viewer.getByRole('heading', { name: '지연 관전 채널' })).toBeVisible();
  const responsePromise = viewer.waitForResponse((response) =>
    /\/api\/games\/.+\/spectate$/.test(new URL(response.url()).pathname)
  );
  const operationCard = viewer.locator('.room-card').filter({ hasText: operationName });
  await operationCard.getByRole('button', { name: '관전 시작' }).click();
  const response = await responsePromise;
  expect(response.status()).toBe(200);
  const payload = await response.json();
  expect(payload.delaySeconds).toBe(30);
  expect(payload.players).toHaveLength(2);
  for (const forbidden of [
    'boards',
    'ships',
    'sessionId',
    'pendingPlacements',
    'chatMessages',
    'tokenHash'
  ]) {
    expect(JSON.stringify(payload)).not.toContain(`"${forbidden}"`);
  }

  await expect(viewer.getByRole('heading', { name: operationName })).toBeVisible();
  await expect(viewer.getByText('보안 지연 중')).toBeVisible();
  await expect(viewer.getByText('공정성 보호 지연 30초')).toBeVisible();
  await expect(viewer.locator('.spectator-board')).toHaveCount(2);
  await expect(viewer.locator('.spectator-board .vessel-slot')).toHaveCount(0);
  await expect
    .poll(() =>
      viewer.evaluate(
        () =>
          Math.max(document.documentElement.scrollWidth, document.body.scrollWidth) <=
          document.documentElement.clientWidth + 1
      )
    )
    .toBe(true);

  await hostContext.close();
  await guestContext.close();
  await viewerContext.close();
});
