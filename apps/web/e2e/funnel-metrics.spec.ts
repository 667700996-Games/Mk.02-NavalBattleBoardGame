import { expect, test, type Page } from '@playwright/test';

type Outcome = 'reached' | 'failed' | 'abandoned';

async function metrics(page: Page): Promise<string> {
  return page.evaluate(async () => {
    const response = await fetch('/api/metrics');
    if (!response.ok) throw new Error(`metrics returned ${response.status}`);
    return response.text();
  });
}

function funnelValue(text: string, stage: string, outcome: Outcome): number {
  const prefix = `mk01_new_player_funnel_events_total{stage="${stage}",outcome="${outcome}"} `;
  const line = text.split('\n').find((candidate) => candidate.startsWith(prefix));
  if (!line) throw new Error(`missing funnel series: ${stage}/${outcome}`);
  return Number(line.slice(prefix.length));
}

function failureValue(text: string, reason: string): number {
  const prefix = `mk01_new_player_funnel_failures_total{reason="${reason}"} `;
  const line = text.split('\n').find((candidate) => candidate.startsWith(prefix));
  if (!line) throw new Error(`missing failure series: ${reason}`);
  return Number(line.slice(prefix.length));
}

test('new-player funnel exposes reached, failed, and abandoned checkpoints', async ({ page }) => {
  await page.goto('/api/metrics');
  const baseline = await page.locator('body').innerText();
  const initial = {
    landing: funnelValue(baseline, 'landing', 'reached'),
    tutorialStarted: funnelValue(baseline, 'tutorial_started', 'reached'),
    tutorialCompleted: funnelValue(baseline, 'tutorial_completed', 'reached'),
    tutorialAbandoned: funnelValue(baseline, 'tutorial_started', 'abandoned'),
    sessionCreated: funnelValue(baseline, 'session_created', 'reached'),
    sessionFailed: funnelValue(baseline, 'session_created', 'failed'),
    sessionFailureReason: failureValue(baseline, 'session_creation'),
    lobbyEntered: funnelValue(baseline, 'lobby_entered', 'reached'),
    roomJoined: funnelValue(baseline, 'room_joined', 'reached'),
    placementCompleted: funnelValue(baseline, 'placement_completed', 'reached'),
    firstAttack: funnelValue(baseline, 'first_attack', 'reached'),
    matchCompleted: funnelValue(baseline, 'match_completed', 'reached')
  };

  await page.goto('/');
  await expect
    .poll(async () => funnelValue(await metrics(page), 'landing', 'reached'))
    .toBeGreaterThan(initial.landing);

  const nickname = page.locator('#nickname');
  await nickname.fill('Funnel Cadet');
  await expect(nickname).toHaveValue('Funnel Cadet');
  await page.route('**/api/sessions', (route) => route.abort('failed'));
  await page.getByRole('button', { name: '플레이 방식 선택' }).click();
  await expect(page.getByText('지휘관 등록에 실패했습니다.', { exact: true })).toBeVisible();
  await expect
    .poll(async () => funnelValue(await metrics(page), 'session_created', 'failed'))
    .toBeGreaterThan(initial.sessionFailed);
  await expect
    .poll(async () => failureValue(await metrics(page), 'session_creation'))
    .toBeGreaterThan(initial.sessionFailureReason);
  await page.unroute('**/api/sessions');

  await page.getByRole('button', { name: '플레이 방식 선택' }).click();
  await expect(page).toHaveURL(/\/play$/);
  await expect
    .poll(async () => funnelValue(await metrics(page), 'session_created', 'reached'))
    .toBeGreaterThan(initial.sessionCreated);

  await page.getByRole('button', { name: '튜토리얼 선택' }).click();
  await expect(page.getByRole('heading', { name: '작전 지휘 튜토리얼' })).toBeVisible();
  await expect
    .poll(async () => funnelValue(await metrics(page), 'tutorial_started', 'reached'))
    .toBeGreaterThan(initial.tutorialStarted);
  await page.getByRole('button', { name: '플레이 방식 다시 선택' }).click();
  await expect(page).toHaveURL(/\/play$/);
  await expect
    .poll(async () => funnelValue(await metrics(page), 'tutorial_started', 'abandoned'))
    .toBeGreaterThan(initial.tutorialAbandoned);

  await page.getByRole('button', { name: '튜토리얼 선택' }).click();
  for (let step = 0; step < 4; step += 1) {
    await page.getByRole('button', { name: /다음 훈련/ }).click();
  }
  await page.getByRole('button', { name: '훈련 완료' }).click();
  await expect(page).toHaveURL(/\/play$/);
  await expect
    .poll(async () => funnelValue(await metrics(page), 'tutorial_completed', 'reached'))
    .toBeGreaterThan(initial.tutorialCompleted);

  await page.getByRole('button', { name: '멀티 플레이 선택' }).click();
  await expect(page.getByRole('heading', { name: '작전 로비' })).toBeVisible();
  await expect
    .poll(async () => funnelValue(await metrics(page), 'lobby_entered', 'reached'))
    .toBeGreaterThan(initial.lobbyEntered);

  await page.goto('/single-player');
  await page.getByRole('button', { name: /신병 RECRUIT/ }).click();
  await expect(page.getByRole('heading', { name: '함대 배치' })).toBeVisible();
  await expect(page.locator('.launch-sequence')).toBeHidden();
  await expect
    .poll(async () => funnelValue(await metrics(page), 'room_joined', 'reached'))
    .toBeGreaterThan(initial.roomJoined);

  await page.getByRole('button', { name: '자동 배치' }).click();
  await expect(page.getByText('5/5 함선 배치')).toBeVisible();
  await page.getByRole('button', { name: '배치 확정' }).click();
  await expect(page.getByText('상대 공격 보드')).toBeVisible();
  await expect
    .poll(async () => funnelValue(await metrics(page), 'placement_completed', 'reached'))
    .toBeGreaterThan(initial.placementCompleted);

  await expect(page.getByRole('heading', { name: /공격 좌표를 지정하십시오/ })).toBeVisible({
    timeout: 15_000
  });
  const target = page.getByTestId('target-cell-0-0');
  await target.click();
  await page.getByRole('button', { name: '공격 실행' }).click();
  await expect(target).toHaveAttribute('aria-label', /A1, (빗나감|명중|격침)/);
  await expect
    .poll(async () => funnelValue(await metrics(page), 'first_attack', 'reached'))
    .toBeGreaterThan(initial.firstAttack);

  await page.getByRole('button', { name: '작전 포기' }).click();
  await page.getByRole('dialog').getByRole('button', { name: '기권' }).click();
  await expect(page.getByRole('heading', { name: /작전 (승리|패배)/ })).toBeVisible();
  await expect
    .poll(async () => funnelValue(await metrics(page), 'match_completed', 'reached'))
    .toBeGreaterThan(initial.matchCompleted);
});
