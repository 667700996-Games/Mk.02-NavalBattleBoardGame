import { expect, test } from '@playwright/test';

declare global {
  interface Window {
    __mk01Haptics: number[][];
  }
}

test('music, ambience, and interface masters decode after a user gesture in every desktop engine', async ({
  page
}, testInfo) => {
  const requested = new Set<string>();
  page.on('response', (response) => {
    const path = new URL(response.url()).pathname;
    if (path.startsWith('/audio/') && response.ok()) requested.add(path);
  });
  const sessionProbe = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/sessions/current' &&
      response.request().method() === 'GET'
  );
  await page.goto('/');
  await sessionProbe;
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', 'true');
  await page.locator('#nickname').fill(`Audio-${testInfo.project.name}`.slice(0, 16));
  await page.getByRole('button', { name: '작전 로비 입장' }).click();
  await expect(page).toHaveURL(/\/lobby$/);
  await expect
    .poll(
      () =>
        ['/audio/music-command-loop.mp3', '/audio/ambience-ocean-loop.mp3'].every((path) =>
          requested.has(path)
        ),
      { timeout: 20_000 }
    )
    .toBe(true);
  const settingsProbe = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/sessions/current' &&
      response.request().method() === 'GET'
  );
  await page.goto('/settings');
  await settingsProbe;
  await page.getByRole('button', { name: /마스터 채널 미리 듣기/ }).click();
  await expect.poll(() => requested.has('/audio/ui-confirm.mp3')).toBe(true);
  await expect
    .poll(async () => Number(await page.locator('html').getAttribute('data-audio-loaded-assets')))
    .toBeGreaterThanOrEqual(3);
  await expect(page.locator('html')).toHaveAttribute('data-audio-lifecycle', 'running');
});

test('file-backed audio, independent mixers, lifecycle recovery, cues, and optional haptics work together', async ({
  page,
  browserName
}) => {
  test.skip(
    browserName !== 'chromium',
    'Chromium provides the canonical Web Audio lifecycle fixture.'
  );

  await page.addInitScript(() => {
    window.__mk01Haptics = [];
    Object.defineProperty(navigator, 'vibrate', {
      configurable: true,
      value: (pattern: number | number[]) => {
        window.__mk01Haptics.push(Array.isArray(pattern) ? pattern : [pattern]);
        return true;
      }
    });
    const nativeMatchMedia = window.matchMedia.bind(window);
    window.matchMedia = (query: string) => {
      if (query !== '(pointer: coarse)') return nativeMatchMedia(query);
      return {
        matches: true,
        media: query,
        onchange: null,
        addListener: () => undefined,
        removeListener: () => undefined,
        addEventListener: () => undefined,
        removeEventListener: () => undefined,
        dispatchEvent: () => true
      };
    };
  });

  const audioRequests: string[] = [];
  page.on('request', (request) => {
    const path = new URL(request.url()).pathname;
    if (path.startsWith('/audio/')) audioRequests.push(path);
  });

  const sessionProbe = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/sessions/current' &&
      response.request().method() === 'GET'
  );
  await page.goto('/');
  await sessionProbe;
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', 'true');
  await page.locator('#nickname').fill('AudioCadet');
  await page.getByRole('button', { name: '작전 로비 입장' }).click();
  await expect(page).toHaveURL(/\/lobby$/);
  await expect
    .poll(() => audioRequests.some((path) => path.endsWith('music-command-loop.mp3')))
    .toBe(true);
  await expect
    .poll(() => audioRequests.some((path) => path.endsWith('ambience-ocean-loop.mp3')))
    .toBe(true);

  const settingsProbe = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/sessions/current' &&
      response.request().method() === 'GET'
  );
  await page.goto('/settings');
  await settingsProbe;
  await expect(page.getByRole('heading', { name: '오디오 및 햅틱' })).toBeVisible();

  const levels = {
    마스터: '0.65',
    음악: '0.25',
    효과음: '0.7',
    환경음: '0.35',
    '음성 및 큐': '0.6'
  } as const;
  for (const [label, value] of Object.entries(levels)) {
    await page.getByLabel(label, { exact: true }).fill(value);
    await expect(page.getByLabel(label, { exact: true })).toHaveValue(value);
  }

  const cues = page.getByLabel('접근성 오디오 큐');
  await cues.uncheck();
  await cues.check();
  const haptics = page.getByLabel('모바일 햅틱');
  await haptics.uncheck();
  await haptics.check();
  await expect.poll(() => page.evaluate(() => window.__mk01Haptics.length)).toBeGreaterThan(0);
  expect(await page.evaluate(() => window.__mk01Haptics.at(-1))).toEqual([8]);

  await expect(page.locator('html')).toHaveAttribute('data-audio-lifecycle', 'running');
  await page.evaluate(() => window.dispatchEvent(new Event('blur')));
  await expect(page.locator('html')).toHaveAttribute('data-audio-lifecycle', 'suspended');
  await page.evaluate(() => window.dispatchEvent(new Event('focus')));
  await expect(page.locator('html')).toHaveAttribute('data-audio-lifecycle', 'running');

  const revision = Number(await page.locator('html').getAttribute('data-audio-output-revision'));
  await page.evaluate(() => navigator.mediaDevices?.dispatchEvent(new Event('devicechange')));
  await expect
    .poll(async () => Number(await page.locator('html').getAttribute('data-audio-output-revision')))
    .toBe(revision + 1);

  const reloadProbe = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === '/api/sessions/current' &&
      response.request().method() === 'GET'
  );
  await page.reload();
  await reloadProbe;
  for (const [label, value] of Object.entries(levels)) {
    await expect(page.getByLabel(label, { exact: true })).toHaveValue(value);
  }
  await expect(cues).toBeChecked();
  await expect(haptics).toBeChecked();
  expect(audioRequests.some((path) => path.endsWith('ui-select.mp3'))).toBe(true);
  expect(audioRequests.some((path) => path.endsWith('cue-turn.mp3'))).toBe(true);
});
