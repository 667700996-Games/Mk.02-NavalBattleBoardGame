import { Buffer } from 'node:buffer';
import { readFileSync } from 'node:fs';
import {
  expect,
  test,
  type BrowserContextOptions,
  type CDPSession,
  type Page,
  type Response
} from '@playwright/test';

type ResourceKind = 'javascript' | 'css' | 'fonts' | 'images' | 'audio';

interface RuntimeBudget {
  viewport: { width: number; height: number };
  deviceScaleFactor: number;
  isMobile: boolean;
  hasTouch: boolean;
  cpuThrottlingRate: number;
  javascriptBytes: number;
  cssBytes: number;
  fontBytes: number;
  imageBytes: number;
  audioBytes: number;
  jsHeapPeakBytes: number;
  cpuTaskMilliseconds: number;
  longTaskMilliseconds: number;
  frameP95Milliseconds: number;
  webSocketBytes: number;
}

interface EffectMetrics {
  frames: number[];
  longTasks: number[];
}

const budgetConfig = JSON.parse(
  readFileSync(new URL('../../../config/performance-budgets.json', import.meta.url), 'utf8')
) as { tiers: Record<string, RuntimeBudget> };
const tiers = budgetConfig.tiers;

function resourceKind(response: Response): ResourceKind | null {
  if (new URL(response.url()).pathname.startsWith('/audio/')) return 'audio';
  const type = response.request().resourceType();
  if (type === 'script') return 'javascript';
  if (type === 'stylesheet') return 'css';
  if (type === 'font') return 'fonts';
  if (type === 'image') return 'images';
  if (type === 'media') return 'audio';
  return null;
}

function percentile(values: number[], fraction: number): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((left, right) => left - right);
  return sorted[Math.max(0, Math.ceil(sorted.length * fraction) - 1)];
}

async function metricValue(session: CDPSession, name: string): Promise<number> {
  const result = (await session.send('Performance.getMetrics')) as {
    metrics: Array<{ name: string; value: number }>;
  };
  return result.metrics.find((metric) => metric.name === name)?.value ?? 0;
}

async function expectNoHorizontalOverflow(page: Page): Promise<void> {
  try {
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            Math.max(document.documentElement.scrollWidth, document.body.scrollWidth) <=
            document.documentElement.clientWidth + 1
        )
      )
      .toBe(true);
  } catch (error) {
    const details = await page.evaluate(() => {
      const width = document.documentElement.clientWidth;
      return [...document.querySelectorAll<HTMLElement>('body *')]
        .map((element) => {
          const rect = element.getBoundingClientRect();
          return {
            element: `${element.tagName.toLowerCase()}.${element.className}`,
            left: Math.round(rect.left),
            right: Math.round(rect.right),
            scrollWidth: element.scrollWidth,
            clientWidth: element.clientWidth
          };
        })
        .filter(
          (entry) =>
            entry.right > width + 1 || entry.left < -1 || entry.scrollWidth > entry.clientWidth + 1
        )
        .slice(0, 20);
    });
    throw new Error(`Horizontal overflow sources: ${JSON.stringify(details)}`, { cause: error });
  }
}

async function expectReadableBattleControls(page: Page): Promise<void> {
  await expectNoHorizontalOverflow(page);
  const targetBox = await page.getByTestId('target-cell-0-0').boundingBox();
  const fireBox = await page.getByRole('button', { name: '공격 실행' }).boundingBox();
  if (!targetBox || !fireBox) throw new Error('Battle controls are not visibly measurable');
  expect(targetBox.width).toBeGreaterThanOrEqual(24);
  expect(targetBox.height).toBeGreaterThanOrEqual(24);
  expect(fireBox.height).toBeGreaterThanOrEqual(40);
  expect(fireBox.width).toBeGreaterThanOrEqual(120);
  const headingFontSize = await page
    .getByRole('heading', { name: /공격 좌표를 지정하십시오/ })
    .evaluate((heading) => Number.parseFloat(getComputedStyle(heading).fontSize));
  expect(headingFontSize).toBeGreaterThanOrEqual(14);
}

for (const [tierName, budget] of Object.entries(tiers)) {
  test(`${tierName} production gameplay stays inside runtime budgets`, async ({
    browser
  }, testInfo) => {
    const options: BrowserContextOptions = {
      baseURL: testInfo.project.use.baseURL,
      viewport: budget.viewport,
      deviceScaleFactor: budget.deviceScaleFactor,
      isMobile: budget.isMobile,
      hasTouch: budget.hasTouch,
      reducedMotion: 'no-preference',
      locale: 'ko-KR',
      timezoneId: 'Asia/Seoul'
    };
    const context = await browser.newContext(options);
    await context.addInitScript(() => {
      type InstrumentedWindow = Window & {
        __mk01Performance: { active: boolean; frames: number[]; longTasks: number[] };
        __mk01StartEffects: () => void;
        __mk01StopEffects: () => { frames: number[]; longTasks: number[] };
      };
      const target = window as InstrumentedWindow;
      target.__mk01Performance = { active: false, frames: [], longTasks: [] };
      target.__mk01StartEffects = () => {
        target.__mk01Performance = { active: true, frames: [], longTasks: [] };
      };
      target.__mk01StopEffects = () => {
        target.__mk01Performance.active = false;
        return {
          frames: [...target.__mk01Performance.frames],
          longTasks: [...target.__mk01Performance.longTasks]
        };
      };
      let previousFrame = 0;
      const sampleFrame = (timestamp: number) => {
        if (target.__mk01Performance.active && previousFrame > 0) {
          target.__mk01Performance.frames.push(timestamp - previousFrame);
        }
        previousFrame = timestamp;
        requestAnimationFrame(sampleFrame);
      };
      requestAnimationFrame(sampleFrame);
      if ('PerformanceObserver' in window) {
        new PerformanceObserver((list) => {
          if (!target.__mk01Performance.active) return;
          for (const entry of list.getEntries()) {
            target.__mk01Performance.longTasks.push(entry.duration);
          }
        }).observe({ type: 'longtask', buffered: true });
      }
    });

    const page = await context.newPage();
    const resourceResponses = new Map<string, { response: Response; kind: ResourceKind }>();
    let webSocketBytes = 0;
    page.on('response', (response) => {
      const kind = resourceKind(response);
      if (kind && !resourceResponses.has(response.url())) {
        resourceResponses.set(response.url(), { response, kind });
      }
    });
    page.on('websocket', (socket) => {
      socket.on('framesent', ({ payload }) => {
        webSocketBytes += Buffer.isBuffer(payload) ? payload.length : Buffer.byteLength(payload);
      });
      socket.on('framereceived', ({ payload }) => {
        webSocketBytes += Buffer.isBuffer(payload) ? payload.length : Buffer.byteLength(payload);
      });
    });

    const cdp = await context.newCDPSession(page);
    await cdp.send('Performance.enable');
    await cdp.send('Emulation.setCPUThrottlingRate', { rate: budget.cpuThrottlingRate });
    const taskStart = await metricValue(cdp, 'TaskDuration');
    const heapSamples: number[] = [];
    const sampleHeap = async () => heapSamples.push(await metricValue(cdp, 'JSHeapUsedSize'));

    const sessionProbe = page.waitForResponse(
      (response) =>
        new URL(response.url()).pathname === '/api/sessions/current' &&
        response.request().method() === 'GET'
    );
    await page.goto('/');
    await sessionProbe;
    await expect(page.locator('html')).toHaveAttribute('data-hydrated', 'true', {
      timeout: 30_000
    });
    await sampleHeap();
    await page.locator('#nickname').fill(`Perf ${tierName}`.slice(0, 16));
    await page.getByRole('button', { name: '작전 로비 입장' }).click();
    await expect(page.getByRole('heading', { name: '작전 로비' })).toBeVisible();
    await sampleHeap();
    await page.getByRole('button', { name: /신병 RECRUIT/ }).click();
    await expect(page.getByRole('heading', { name: '함대 배치' })).toBeVisible();
    await expect(page.locator('.launch-sequence')).toBeHidden();
    await sampleHeap();
    await page.getByRole('button', { name: '자동 배치' }).click();
    await expect(page.getByText('5/5 함선 배치')).toBeVisible();
    await page.getByRole('button', { name: '배치 확정' }).click();
    await expect(page.getByText('상대 공격 보드')).toBeVisible();
    const attackHeading = page.getByRole('heading', { name: /공격 좌표를 지정하십시오/ });
    await expect(attackHeading).toBeVisible({
      timeout: 30_000
    });
    await expectReadableBattleControls(page);
    await sampleHeap();

    await page.evaluate(() => {
      (window as Window & { __mk01StartEffects: () => void }).__mk01StartEffects();
    });
    for (let column = 0; column < 5; column += 1) {
      const target = page.getByTestId(`target-cell-0-${column}`);
      await target.click();
      await page.getByRole('button', { name: '공격 실행' }).click();
      const outcome = column === 4 ? '격침' : '명중';
      await expect(target).toHaveAttribute('aria-label', new RegExp(`A${column + 1}, ${outcome}`));
      if (column < 4) await expect(attackHeading).toBeVisible({ timeout: 15_000 });
    }
    await expectNoHorizontalOverflow(page);
    if (tierName === 'lowMobile') {
      await page.getByTestId('target-cell-0-4').scrollIntoViewIfNeeded();
      await testInfo.attach('low-mobile-carrier-sunk.png', {
        body: await page.screenshot(),
        contentType: 'image/png'
      });
    }
    await page.getByRole('button', { name: '작전 포기' }).click();
    await page.getByRole('dialog').getByRole('button', { name: '기권' }).click();
    await expect(page.getByRole('heading', { name: /작전 (승리|패배)/ })).toBeVisible();
    await expectNoHorizontalOverflow(page);
    if (tierName === 'lowMobile') {
      const closeNotification = page.getByRole('button', { name: '알림 닫기' });
      while ((await closeNotification.count()) > 0) await closeNotification.first().click();
      await page.evaluate(() => window.scrollTo({ top: 0 }));
      await testInfo.attach('low-mobile-result.png', {
        body: await page.screenshot(),
        contentType: 'image/png'
      });
    }
    await page.waitForTimeout(1_500);
    const effects = await page.evaluate(() =>
      (window as Window & { __mk01StopEffects: () => EffectMetrics }).__mk01StopEffects()
    );
    await page.evaluate(async () => {
      await document.fonts.ready;
    });
    await page.waitForTimeout(250);
    await sampleHeap();

    const resourceBytes: Record<ResourceKind, number> = {
      javascript: 0,
      css: 0,
      fonts: 0,
      images: 0,
      audio: 0
    };
    for (const { response, kind } of resourceResponses.values()) {
      const body = await response.body();
      resourceBytes[kind] += body.length;
    }
    const taskEnd = await metricValue(cdp, 'TaskDuration');
    const jsHeapPeakBytes = Math.max(...heapSamples);
    const report = {
      tier: tierName,
      cpuThrottlingRate: budget.cpuThrottlingRate,
      resourceBytes,
      jsHeapPeakBytes: Math.round(jsHeapPeakBytes),
      cpuTaskMilliseconds: Math.round((taskEnd - taskStart) * 1000),
      longTaskMilliseconds: Math.round(effects.longTasks.reduce((sum, value) => sum + value, 0)),
      frameP50Milliseconds: Number(percentile(effects.frames, 0.5).toFixed(2)),
      frameP90Milliseconds: Number(percentile(effects.frames, 0.9).toFixed(2)),
      frameP95Milliseconds: Number(percentile(effects.frames, 0.95).toFixed(2)),
      slowFrameRatio: Number(
        (effects.frames.filter((duration) => duration > 34).length / effects.frames.length).toFixed(
          3
        )
      ),
      sampledFrames: effects.frames.length,
      webSocketBytes
    };
    await testInfo.attach(`${tierName}-performance.json`, {
      body: JSON.stringify(report, null, 2),
      contentType: 'application/json'
    });
    console.log(JSON.stringify(report));

    expect(report.resourceBytes.javascript).toBeLessThanOrEqual(budget.javascriptBytes);
    expect(report.resourceBytes.css).toBeLessThanOrEqual(budget.cssBytes);
    expect(report.resourceBytes.fonts).toBeLessThanOrEqual(budget.fontBytes);
    expect(report.resourceBytes.images).toBeLessThanOrEqual(budget.imageBytes);
    expect(report.resourceBytes.audio).toBeLessThanOrEqual(budget.audioBytes);
    expect(report.jsHeapPeakBytes).toBeLessThanOrEqual(budget.jsHeapPeakBytes);
    expect(report.cpuTaskMilliseconds).toBeLessThanOrEqual(budget.cpuTaskMilliseconds);
    expect(report.longTaskMilliseconds).toBeLessThanOrEqual(budget.longTaskMilliseconds);
    expect(report.frameP95Milliseconds).toBeLessThanOrEqual(budget.frameP95Milliseconds);
    expect(report.webSocketBytes).toBeLessThanOrEqual(budget.webSocketBytes);
    expect(report.sampledFrames).toBeGreaterThan(20);

    await context.close();
  });
}
