import { defineConfig, devices } from '@playwright/test';

const serverPort = Number(process.env.E2E_SERVER_PORT ?? 18080);
const webPort = Number(process.env.E2E_WEB_PORT ?? 15173);
const serverOrigin = `http://127.0.0.1:${serverPort}`;
const webOrigin = `http://127.0.0.1:${webPort}`;

export default defineConfig({
  testDir: './e2e',
  timeout: 120_000,
  fullyParallel: false,
  workers: 1,
  expect: { timeout: 10_000 },
  retries: process.env.CI ? 2 : 0,
  reporter: [['list'], ['html', { open: 'never' }]],
  use: {
    baseURL: webOrigin,
    trace: 'on-first-retry'
  },
  webServer: [
    {
      command: 'cargo run -p mk01-server',
      cwd: '../..',
      url: `${serverOrigin}/api/health`,
      env: {
        STORAGE_MODE: 'memory',
        SERVER_PORT: String(serverPort),
        TURN_DURATION_SECONDS: '10',
        PUBLIC_BASE_URL: webOrigin,
        ALLOWED_ORIGINS: `${webOrigin},http://localhost:${webPort}`,
        RUST_LOG: 'warn'
      },
      reuseExistingServer: !process.env.CI,
      timeout: 120_000
    },
    {
      command: 'npm run dev',
      cwd: '.',
      url: webOrigin,
      env: {
        WEB_PORT: String(webPort),
        SERVER_ORIGIN: serverOrigin
      },
      reuseExistingServer: !process.env.CI,
      timeout: 120_000
    }
  ],
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
    { name: 'firefox', use: { ...devices['Desktop Firefox'] } },
    { name: 'webkit', use: { ...devices['Desktop Safari'] } },
    {
      name: 'mobile-chrome',
      use: { ...devices['Pixel 7'] },
      testMatch: /(full-game|responsive)\.spec\.ts/
    },
    {
      name: 'mobile-safari',
      use: { ...devices['iPhone 13'] },
      testMatch: /(full-game|responsive)\.spec\.ts/
    },
    {
      name: 'tablet-chrome',
      use: { ...devices['iPad Pro 11'] },
      testMatch: /(full-game|responsive)\.spec\.ts/
    }
  ]
});
