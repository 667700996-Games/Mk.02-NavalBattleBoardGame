import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';

const webPort = Number(process.env.WEB_PORT ?? 5173);
const serverOrigin = process.env.SERVER_ORIGIN ?? 'http://127.0.0.1:8080';
const proxy = {
  '/api': serverOrigin,
  '/ws': {
    target: serverOrigin.replace(/^http/, 'ws'),
    ws: true
  }
};

export default defineConfig({
  plugins: [sveltekit()],
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          const locale = id.match(/\/i18n\/messages\/(ko-KR|en-US)\.json$/)?.[1];
          return locale ? `locale-${locale}` : undefined;
        }
      }
    }
  },
  server: {
    host: true,
    allowedHosts: true,
    port: webPort,
    strictPort: true,
    proxy
  },
  preview: {
    host: true,
    allowedHosts: true,
    port: webPort,
    strictPort: true,
    proxy
  },
  test: {
    include: ['src/**/*.test.ts'],
    environment: 'node',
    coverage: {
      provider: 'v8',
      include: [
        'src/lib/performance.ts',
        'src/lib/protocol.ts',
        'src/lib/game/placement.ts',
        'src/lib/game/replay-analysis.ts',
        'src/lib/components/lobby/LobbyCommandDashboard.svelte',
        'src/lib/components/lobby/LobbyRoomOperations.svelte'
      ],
      reporter: ['text', 'json-summary', 'lcov'],
      reportsDirectory: 'coverage',
      thresholds: {
        statements: 85,
        branches: 78,
        functions: 75,
        lines: 87,
        'src/lib/performance.ts': {
          statements: 85,
          branches: 70,
          functions: 85,
          lines: 90
        },
        'src/lib/protocol.ts': {
          statements: 90,
          branches: 88,
          functions: 85,
          lines: 92
        },
        'src/lib/game/placement.ts': {
          statements: 90,
          branches: 80,
          functions: 85,
          lines: 92
        },
        'src/lib/game/replay-analysis.ts': {
          statements: 95,
          branches: 85,
          functions: 95,
          lines: 95
        },
        'src/lib/components/lobby/*.svelte': {
          statements: 85,
          branches: 80,
          functions: 60,
          lines: 85
        }
      }
    }
  }
});
