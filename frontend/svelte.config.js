import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  kit: {
    // Static SPA build; exact-api serves the resulting `build/` directory.
    adapter: adapter({
      fallback: 'index.html'
    }),
    alias: {
      $lib: 'src/lib'
    }
  }
};

export default config;
