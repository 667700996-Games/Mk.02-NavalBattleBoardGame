import { defineConfig, devices } from '@playwright/test';

const serverPort = Number(process.env.VISUAL_SERVER_PORT ?? 28083);
const webPort = Number(process.env.VISUAL_WEB_PORT ?? 25176);
const serverOrigin = `http://127.0.0.1:${serverPort}`;
const webOrigin = `http://127.0.0.1:${webPort}`;

export default defineConfig({
  testDir: './e2e',
  testMatch: 'visual-regression.spec.ts',
  timeout: 90_000,
  workers: 1,
  retries: 0,
  reporter: [['list'], ['html', { open: 'never', outputFolder: 'playwright-visual-report' }]],
  snapshotPathTemplate: '{testDir}/visual-regression.spec.ts-snapshots/{arg}-{projectName}{ext}',
  expect: {
    timeout: 10_000,
    toHaveScreenshot: {
      animations: 'disabled',
      caret: 'hide',
      maxDiffPixelRatio: 0.01,
      threshold: 0.2
    }
  },
  use: {
    baseURL: webOrigin,
    colorScheme: 'dark',
    reducedMotion: 'reduce',
    locale: 'ko-KR',
    timezoneId: 'Asia/Seoul'
  },
  webServer: [
    {
      command: 'cargo run -p mk01-server',
      cwd: '../..',
      url: `${serverOrigin}/api/health`,
      env: {
        STORAGE_MODE: 'memory',
        SERVER_PORT: String(serverPort),
        PUBLIC_BASE_URL: webOrigin,
        ALLOWED_ORIGINS: webOrigin,
        RUST_LOG: 'warn'
      },
      reuseExistingServer: false,
      timeout: 120_000
    },
    {
      command: 'npm run dev',
      cwd: '.',
      url: webOrigin,
      env: { WEB_PORT: String(webPort), SERVER_ORIGIN: serverOrigin },
      reuseExistingServer: false,
      timeout: 120_000
    }
  ],
  projects: [
    {
      name: 'desktop-chromium',
      use: { ...devices['Desktop Chrome'], viewport: { width: 1440, height: 1000 } }
    },
    {
      name: 'mobile-chromium',
      use: { ...devices['Pixel 7'] }
    }
  ]
});
