import { expect, test, type APIRequestContext } from '@playwright/test';

async function metrics(request: APIRequestContext): Promise<string> {
  const response = await request.get('/api/metrics');
  expect(response.ok()).toBe(true);
  return response.text();
}

function histogramCount(text: string, name: string, route: string, tier: string): number {
  const prefix = `${name}_count{route="${route}",device_tier="${tier}"} `;
  const line = text.split('\n').find((candidate) => candidate.startsWith(prefix));
  return line ? Number(line.slice(prefix.length)) : 0;
}

test('real browser vitals and attack latency reach anonymous histograms', async ({
  page,
  request
}, testInfo) => {
  test.skip(testInfo.project.name !== 'chromium', 'canonical desktop RUM acceptance profile');
  const baseline = await metrics(request);
  const initial = {
    lcp: histogramCount(baseline, 'mk01_rum_lcp_milliseconds', 'landing', 'desktop'),
    cls: histogramCount(baseline, 'mk01_rum_cls_milli', 'landing', 'desktop'),
    inp: histogramCount(baseline, 'mk01_rum_inp_milliseconds', 'landing', 'desktop'),
    battle: histogramCount(baseline, 'mk01_rum_battle_interaction_milliseconds', 'room', 'desktop')
  };

  await page.goto('/');
  const nickname = page.locator('#nickname');
  await expect(nickname).toBeVisible();
  await page.evaluate(() => {
    document.querySelector('#nickname')?.addEventListener(
      'pointerdown',
      () => {
        const until = performance.now() + 40;
        while (performance.now() < until) {
          // Deterministically create one measurable Event Timing interaction.
        }
      },
      { once: true }
    );
  });
  await nickname.click();
  await nickname.fill('RUM Cadet');
  await page.waitForTimeout(250);
  await page.evaluate(() => window.dispatchEvent(new PageTransitionEvent('pagehide')));

  for (const [metric, name] of [
    ['lcp', 'mk01_rum_lcp_milliseconds'],
    ['cls', 'mk01_rum_cls_milli'],
    ['inp', 'mk01_rum_inp_milliseconds']
  ] as const) {
    await expect
      .poll(async () => histogramCount(await metrics(request), name, 'landing', 'desktop'))
      .toBeGreaterThan(initial[metric]);
  }

  await page.getByRole('button', { name: '작전 로비 입장' }).click();
  await expect(page.getByRole('heading', { name: '작전 로비' })).toBeVisible();
  await page.getByRole('button', { name: /신병 RECRUIT/ }).click();
  await expect(page.getByRole('heading', { name: '함대 배치' })).toBeVisible();
  await expect(page.locator('.launch-sequence')).toBeHidden();
  await page.getByRole('button', { name: '자동 배치' }).click();
  await expect(page.getByText('5/5 함선 배치')).toBeVisible();
  await page.getByRole('button', { name: '배치 확정' }).click();
  await expect(page.getByRole('heading', { name: /공격 좌표를 지정하십시오/ })).toBeVisible({
    timeout: 30_000
  });
  const target = page.getByTestId('target-cell-0-0');
  await target.click();
  await page.getByRole('button', { name: '공격 실행' }).click();
  await expect(target).toHaveAttribute('aria-label', /A1, 명중/);
  await expect
    .poll(async () =>
      histogramCount(
        await metrics(request),
        'mk01_rum_battle_interaction_milliseconds',
        'room',
        'desktop'
      )
    )
    .toBeGreaterThan(initial.battle);
});
