import { defineConfig, devices } from '@playwright/test';

const port = Number(process.env.CLOUDFLARE_E2E_PORT ?? 18788);
const origin = `http://127.0.0.1:${port}`;

export default defineConfig({
  testDir: './e2e',
  testMatch: /(?:cloudflare-account|full-game|chat-surrender)\.spec\.ts/,
  timeout: 180_000,
  fullyParallel: false,
  workers: 1,
  expect: { timeout: 15_000 },
  retries: process.env.CI ? 1 : 0,
  reporter: [['list'], ['html', { outputFolder: 'playwright-cloudflare-report', open: 'never' }]],
  metadata: { runtime: 'cloudflare' },
  use: {
    baseURL: origin,
    locale: 'ko-KR',
    trace: 'on-first-retry'
  },
  webServer: {
    command:
      `npm run build:web:cloudflare && ` + `npm --workspace @mk01/worker run dev -- --port ${port}`,
    cwd: '../..',
    url: `${origin}/api/health`,
    reuseExistingServer: !process.env.CI,
    timeout: 180_000
  },
  projects: [{ name: 'cloudflare-chromium', use: { ...devices['Desktop Chrome'] } }]
});
