import adapter from '@sveltejs/adapter-cloudflare';
import nodeAdapter from '@sveltejs/adapter-node';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

const selectedAdapter =
  process.env.MK01_WEB_ADAPTER === 'node'
    ? nodeAdapter()
    : adapter({ config: 'wrangler.svelte.jsonc' });

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  kit: {
    adapter: selectedAdapter,
    csp: {
      mode: 'auto',
      directives: {
        'default-src': ['self'],
        'base-uri': ['self'],
        'object-src': ['none'],
        'frame-ancestors': ['none'],
        'form-action': ['self'],
        'img-src': ['self', 'data:', 'blob:'],
        'font-src': ['self'],
        'style-src': ['self', 'unsafe-inline'],
        'script-src': ['self'],
        'connect-src': ['self', 'ws:', 'wss:'],
        'worker-src': ['self', 'blob:']
      }
    }
  }
};

export default config;
