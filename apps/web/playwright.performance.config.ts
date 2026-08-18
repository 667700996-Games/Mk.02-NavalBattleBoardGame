import { defineConfig } from '@playwright/test';

const serverPort = Number(process.env.PERFORMANCE_SERVER_PORT ?? 18081);
const webPort = Number(process.env.PERFORMANCE_WEB_PORT ?? 15174);
const serverOrigin = `http://127.0.0.1:${serverPort}`;
const webOrigin = `http://127.0.0.1:${webPort}`;

export default defineConfig({
  testDir: './performance',
  timeout: 120_000,
  fullyParallel: false,
  workers: 1,
  retries: process.env.CI ? 1 : 0,
  reporter: [['list'], ['html', { outputFolder: 'playwright-performance-report', open: 'never' }]],
  use: { baseURL: webOrigin, trace: 'retain-on-failure' },
  projects: [{ name: 'chromium-performance', use: { browserName: 'chromium' } }],
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
        ALLOWED_ORIGINS: webOrigin,
        RUST_LOG: 'warn'
      },
      reuseExistingServer: false,
      timeout: 120_000
    },
    {
      command: 'npm run preview',
      cwd: '.',
      url: webOrigin,
      env: { WEB_PORT: String(webPort), SERVER_ORIGIN: serverOrigin },
      reuseExistingServer: false,
      timeout: 120_000
    }
  ]
});
