import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';

const webPort = Number(process.env.WEB_PORT ?? 5173);
const serverOrigin = process.env.SERVER_ORIGIN ?? 'http://127.0.0.1:8080';

export default defineConfig({
  plugins: [sveltekit()],
  server: {
    host: true,
    allowedHosts: true,
    port: webPort,
    strictPort: true,
    proxy: {
      '/api': serverOrigin,
      '/ws': {
        target: serverOrigin.replace(/^http/, 'ws'),
        ws: true
      }
    }
  },
  test: {
    include: ['src/**/*.test.ts'],
    environment: 'node'
  }
});
