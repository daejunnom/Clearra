import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, searchForWorkspaceRoot } from 'vite';
import { frontendPaths } from '../../scripts/tools/clearra-frontend-paths.mjs';

export default defineConfig(() => {
  const frontend = frontendPaths('desktop', { requireOwner: true });
  return {
    cacheDir: frontend.viteCacheDir,
    plugins: [sveltekit()],
    server: {
      strictPort: false,
      fs: { allow: [searchForWorkspaceRoot(frontend.appRoot), frontend.frontendRoot] }
    }
  };
});
