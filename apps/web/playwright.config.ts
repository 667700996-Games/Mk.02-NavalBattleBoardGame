import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  timeout: 120_000,
  fullyParallel: false,
  workers: 1,
  expect: { timeout: 10_000 },
  retries: process.env.CI ? 2 : 0,
  reporter: [['list'], ['html', { open: 'never' }]],
  use: {
    baseURL: 'http://127.0.0.1:5173',
    trace: 'on-first-retry'
  },
  webServer: [
    {
      command: 'cargo run -p mk01-server',
      cwd: '../..',
      url: 'http://127.0.0.1:8080/api/health',
      env: {
        STORAGE_MODE: 'memory',
        SERVER_PORT: '8080',
        TURN_DURATION_SECONDS: '10',
        RUST_LOG: 'warn'
      },
      reuseExistingServer: !process.env.CI,
      timeout: 120_000
    },
    {
      command: 'npm run dev',
      cwd: '.',
      url: 'http://127.0.0.1:5173',
      reuseExistingServer: !process.env.CI,
      timeout: 120_000
    }
  ],
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
    { name: 'mobile', use: { ...devices['iPhone 13'] }, testMatch: /responsive\.spec\.ts/ }
  ]
});
