import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { frontendConfigPaths } from '../../scripts/tools/clearra-frontend-paths.mjs';

const frontend = frontendConfigPaths('desktop');

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  kit: {
    outDir: frontend.kitOutDir,
    adapter: adapter({ pages: frontend.exportDir, assets: frontend.exportDir, fallback: 'index.html' }),
    paths: { relative: true }
  }
};

export default config;
