import { expect, test, type BrowserContext, type Page } from '@playwright/test';

async function register(page: Page, nickname: string) {
  await page.goto('/');
  await page.locator('#nickname').fill(nickname);
  await page.getByRole('button', { name: '작전 로비 입장' }).click();
  await expect(page.getByRole('heading', { name: '작전 로비' })).toBeVisible();
}

async function deploy(page: Page) {
  await expect(page.getByRole('heading', { name: '함대 배치' })).toBeVisible();
  await page.getByRole('button', { name: '자동 배치' }).click();
  await page.getByRole('button', { name: '배치 확정' }).click();
}

async function shipCoordinates(page: Page): Promise<string[]> {
  return page
    .locator('[data-testid^="own-cell-"].cell--ship')
    .evaluateAll((cells) =>
      cells.map((cell) => (cell as HTMLElement).dataset.testid!.replace('own-cell-', ''))
    );
}

async function fire(page: Page, target: string) {
  const [row, col] = target.split('-');
  const cell = page.getByTestId(`target-cell-${row}-${col}`);
  await cell.click();
  await page.getByRole('button', { name: '공격 실행' }).click();
  await expect
    .poll(async () => {
      if ((await page.getByRole('heading', { name: /작전 (승리|패배)/ }).count()) > 0) {
        return 'finished';
      }
      return (await cell.getAttribute('class')) ?? '';
    })
    .toMatch(/cell--(hit|sunk)|finished/);
}

function auditFrames(page: Page, violations: string[]) {
  page.on('websocket', (socket) => {
    socket.on('framereceived', ({ payload }) => {
      try {
        const event = JSON.parse(String(payload));
        const snapshot = event.type === 'room:created' ? event.payload?.snapshot : event.payload;
        if (snapshot?.targetBoard && Object.hasOwn(snapshot.targetBoard, 'ships')) {
          violations.push('targetBoard exposed ships');
        }
        if (snapshot?.players?.some((player: object) => Object.hasOwn(player, 'sessionId'))) {
          violations.push('public player exposed sessionId');
        }
      } catch {
        // Ping/pong or non-JSON frames are not application state.
      }
    });
  });
}

test('two isolated browser sessions complete a secure game and recover after refresh', async ({
  browser
}) => {
  const firstContext: BrowserContext = await browser.newContext();
  const secondContext: BrowserContext = await browser.newContext();
  const first = await firstContext.newPage();
  const second = await secondContext.newPage();
  const violations: string[] = [];
  auditFrames(first, violations);
  auditFrames(second, violations);

  await register(first, 'Alpha');
  await first.getByRole('button', { name: '작전실 생성' }).click();
  await first.getByLabel('작전실 이름').fill('E2E North Sea');
  await first.getByText('비공개', { exact: true }).click();
  await first.getByRole('button', { name: '작전실 편성' }).click();
  await expect(first.getByRole('heading', { name: '상대 지휘관을 기다리는 중' })).toBeVisible();
  const roomCode = new URL(first.url()).pathname.split('/').at(-1)!;

  await second.goto(`/join/${roomCode}`);
  await second.getByLabel('지휘관 호출부호').fill('Bravo');
  await second.getByRole('button', { name: '초대 수락' }).click();
  await deploy(first);
  await deploy(second);

  await expect(first.getByText('상대 공격 보드')).toBeVisible();
  await expect(second.getByText('상대 공격 보드')).toBeVisible();
  const firstFleet = await shipCoordinates(first);
  const secondFleet = await shipCoordinates(second);
  expect(firstFleet).toHaveLength(17);
  expect(secondFleet).toHaveLength(17);

  let firstShots = 0;
  let secondShots = 0;
  let refreshed = false;
  for (let turn = 0; turn < 34; turn += 1) {
    if (await first.locator('.turn-banner--mine').isVisible()) {
      await fire(first, secondFleet[firstShots++]);
    } else {
      await expect(second.locator('.turn-banner--mine')).toBeVisible();
      await fire(second, firstFleet[secondShots++]);
    }

    if (!refreshed && firstShots + secondShots >= 4) {
      const attacksBefore = await first
        .locator(
          '[data-testid^="target-cell-"].cell--hit, [data-testid^="target-cell-"].cell--sunk'
        )
        .count();
      await first.reload();
      await expect(first.getByText('상대 공격 보드')).toBeVisible();
      await expect(
        first.locator(
          '[data-testid^="target-cell-"].cell--hit, [data-testid^="target-cell-"].cell--sunk'
        )
      ).toHaveCount(attacksBefore);
      refreshed = true;
    }

    if ((await first.getByRole('heading', { name: /작전 (승리|패배)/ }).count()) > 0) break;
  }

  await expect(first.getByRole('heading', { name: /작전 (승리|패배)/ })).toBeVisible();
  await expect(second.getByRole('heading', { name: /작전 (승리|패배)/ })).toBeVisible();
  const firstWon = await first.getByRole('heading', { name: '작전 승리' }).isVisible();
  const secondWon = await second.getByRole('heading', { name: '작전 승리' }).isVisible();
  expect(firstWon).not.toBe(secondWon);
  expect(violations).toEqual([]);

  await firstContext.close();
  await secondContext.close();
});
