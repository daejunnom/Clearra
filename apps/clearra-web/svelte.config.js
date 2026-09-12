import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { frontendConfigPaths } from '../../scripts/tools/clearra-frontend-paths.mjs';

const frontend = frontendConfigPaths('web');

const deploymentBase = process.env.CLEARRA_WEB_BASE_PATH || '';
if (
  deploymentBase !== '' &&
  (!deploymentBase.startsWith('/') || deploymentBase.endsWith('/') || deploymentBase.includes('..'))
) {
  throw new Error('CLEARRA_WEB_BASE_PATH must be empty or an absolute path without a trailing slash');
}

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  kit: {
    outDir: frontend.kitOutDir,
    adapter: adapter({ pages: frontend.exportDir, assets: frontend.exportDir, fallback: 'index.html' }),
    files: {
      assets: process.env.CLEARRA_WEB_PUBLIC_DIR || 'static'
    },
    paths: {
      base: deploymentBase,
      relative: deploymentBase === ''
    }
  }
};

export default config;
