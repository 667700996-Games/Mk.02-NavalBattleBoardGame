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
  await page.waitForLoadState('networkidle');
  await expectNoHorizontalOverflow(page);
}

async function expectNoHorizontalOverflow(page: Page) {
  await expect
    .poll(() =>
      page.evaluate(
        () =>
          Math.max(document.documentElement.scrollWidth, document.body.scrollWidth) <=
          document.documentElement.clientWidth + 1
      )
    )
    .toBe(true);
}

async function deploy(page: Page) {
  await expect(page.getByRole('heading', { name: '함대 배치' })).toBeVisible();
  await expect(page.locator('.launch-sequence')).toBeHidden();
  const autoDeploy = page.getByRole('button', { name: '자동 배치' });
  await expect(autoDeploy).toBeEnabled();
  await autoDeploy.click();
  await expect(page.getByText('5/5 함선 배치')).toBeVisible();
  const confirm = page.getByRole('button', { name: '배치 확정' });
  await expect(confirm).toBeEnabled();
  await confirm.click();
}

async function startOperation(host: Page, guest: Page) {
  await expect(
    host.getByRole('heading', { name: '모든 지휘관이 준비를 완료해야 합니다.' })
  ).toBeVisible();
  await expect(
    guest.getByRole('heading', { name: '모든 지휘관이 준비를 완료해야 합니다.' })
  ).toBeVisible();
  await expectNoHorizontalOverflow(host);
  await expectNoHorizontalOverflow(guest);
  await host.getByRole('button', { name: '준비 완료' }).click();
  await expect(host.getByRole('button', { name: '작전 시작' })).toBeDisabled();
  await guest.getByRole('button', { name: '준비 완료' }).click();
  await expect(host.getByRole('button', { name: '작전 시작' })).toBeEnabled();
  await expect(guest.getByText('방장의 작전 개시 대기')).toBeVisible();
  await host.getByRole('button', { name: '작전 시작' }).click();
  const modal = host.getByRole('dialog');
  await expect(modal.getByRole('heading', { name: '작전을 시작하시겠습니까?' })).toBeVisible();
  await modal.getByRole('button', { name: '작전 시작' }).click();
  await expect(host.getByRole('heading', { name: '함대 배치' })).toBeVisible();
  await expect(guest.getByRole('heading', { name: '함대 배치' })).toBeVisible();
  await expectNoHorizontalOverflow(host);
  await expectNoHorizontalOverflow(guest);
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
  const fireButton = page.getByRole('button', { name: '공격 실행' });
  for (let attempt = 0; attempt < 2; attempt += 1) {
    await expect(fireButton).toBeEnabled();
    await fireButton.click();
    try {
      await expect
        .poll(
          async () => {
            if ((await page.getByRole('heading', { name: /작전 (승리|패배)/ }).count()) > 0) {
              return true;
            }
            return !((await cell.getAttribute('class')) ?? '').includes('cell--selected');
          },
          { timeout: 2_000 }
        )
        .toBe(true);
      break;
    } catch (error) {
      if (attempt === 1) throw error;
    }
  }
  await expect
    .poll(
      async () => {
        if ((await page.getByRole('heading', { name: /작전 (승리|패배)/ }).count()) > 0) {
          return 'finished';
        }
        return (await cell.getAttribute('class')) ?? '';
      },
      { timeout: 20_000 }
    )
    .toMatch(/cell--(hit|sunk)|finished/);
}

async function expectSharedTurn(first: Page, second: Page) {
  await expect
    .poll(async () => {
      const [firstVersion, secondVersion, firstMine, secondMine] = await Promise.all([
        first.locator('.combat-strip span').last().textContent(),
        second.locator('.combat-strip span').last().textContent(),
        first.locator('.turn-banner--mine').isVisible(),
        second.locator('.turn-banner--mine').isVisible()
      ]);
      return firstVersion === secondVersion && firstMine !== secondMine;
    })
    .toBe(true);
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
  browser,
  browserName
}) => {
  const firstContext: BrowserContext = await browser.newContext(
    browserName === 'chromium' ? { permissions: ['clipboard-read', 'clipboard-write'] } : undefined
  );
  const secondContext: BrowserContext = await browser.newContext();
  const first = await firstContext.newPage();
  const second = await secondContext.newPage();
  const violations: string[] = [];
  auditFrames(first, violations);
  auditFrames(second, violations);

  await register(first, 'Alpha');
  await first.getByRole('button', { name: '작전실 생성' }).click();
  await first.getByLabel('작전실 이름').fill('E2E North Sea');
  await first.getByRole('radio', { name: /비공개/ }).check();
  await first.getByRole('button', { name: '작전실 편성' }).click();
  await expect(
    first.getByRole('heading', { name: '상대 지휘관의 입장을 기다리고 있습니다.' })
  ).toBeVisible();
  await expectNoHorizontalOverflow(first);
  const roomCode = new URL(first.url()).pathname.split('/').at(-1)!;

  await second.goto(`/join/${roomCode}`);
  await second.getByLabel('지휘관 호출부호').fill('Bravo');
  await second.getByRole('button', { name: '초대 수락' }).click();
  await startOperation(first, second);
  await deploy(first);
  await deploy(second);

  await expect(first.getByText('상대 공격 보드')).toBeVisible();
  await expect(second.getByText('상대 공격 보드')).toBeVisible();
  await expectNoHorizontalOverflow(first);
  await expectNoHorizontalOverflow(second);
  const firstFleet = await shipCoordinates(first);
  const secondFleet = await shipCoordinates(second);
  expect(firstFleet).toHaveLength(17);
  expect(secondFleet).toHaveLength(17);

  let firstShots = 0;
  let secondShots = 0;
  let refreshed = false;
  for (let turn = 0; turn < 34; turn += 1) {
    await expectSharedTurn(first, second);
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
      await expectNoHorizontalOverflow(first);
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
  await expectNoHorizontalOverflow(first);
  await expectNoHorizontalOverflow(second);
  const firstWon = await first.getByRole('heading', { name: '작전 승리' }).isVisible();
  const secondWon = await second.getByRole('heading', { name: '작전 승리' }).isVisible();
  expect(firstWon).not.toBe(secondWon);
  expect(violations).toEqual([]);

  await first.getByRole('link', { name: '전투 복기' }).click();
  await expect(first.getByRole('heading', { name: '전투 복기' })).toBeVisible();
  await expect(first.getByRole('heading', { name: '검증된 밸런스 기록' })).toBeVisible();
  await expect(first.getByText('RULESET V1 · PIN VERIFIED')).toBeVisible();
  await expect(
    first.getByText('SHA-256 6e6a17885e5203e30456ec9fe2f6d663541ec6d01df153cf352bac0314aafa76')
  ).toBeVisible();
  await expect(first.getByText(/항공모함 5칸/)).toBeVisible();
  const replayUrl = first.url();
  const copyReplayLink = first.getByRole('button', { name: '복기 링크 복사' });
  await expect(copyReplayLink).toBeVisible();
  await expect(first.getByText('참가자 세션만 열람할 수 있습니다.')).toBeVisible();
  await copyReplayLink.click();
  if (browserName === 'chromium') {
    await expect(first.getByText('참가자 전용 링크 복사됨')).toBeVisible();
    expect(await first.evaluate(() => navigator.clipboard.readText())).toBe(replayUrl);
  } else {
    await expect(
      first.getByText(/참가자 전용 링크 복사됨|주소창에서 링크를 복사해 주세요/)
    ).toBeVisible();
  }
  await expect(first.getByRole('heading', { name: '전술 분석' })).toBeVisible();
  await expect(first.locator('.analysis-card')).toHaveCount(2);
  await expect(first.locator('.phase-accuracy progress')).toHaveCount(6);
  await expect(first.getByRole('heading', { name: '결정적 전환점' })).toBeVisible();
  await expect(first.getByText('다음 교전 개선 제안')).toHaveCount(2);
  const finishingMoment = first.getByRole('button', { name: '승부를 끝낸 일격 사건 보기' });
  await expect(finishingMoment).toBeVisible();
  await finishingMoment.click();
  await expect(first.locator('.event-log li.active')).toBeVisible();
  await expectNoHorizontalOverflow(first);

  await first.locator('.replay-heading-actions').getByRole('link', { name: '전투 기록' }).click();
  await expect(first.getByRole('heading', { name: '전투 기록' })).toBeVisible();
  await expect(first.getByRole('region', { name: '현재 시즌 및 이벤트' })).toBeVisible();
  await expect(first.getByRole('heading', { name: '창립 함대 시즌' })).toBeVisible();
  await expect(first.getByText(/RULESET V1/).first()).toBeVisible();
  await expectNoHorizontalOverflow(first);

  await firstContext.close();
  await secondContext.close();
});
