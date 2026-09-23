import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, searchForWorkspaceRoot } from 'vite';
import { frontendPaths, frontendUiSourceAliases } from '../../scripts/tools/clearra-frontend-paths.mjs';

export default defineConfig(() => {
  const frontend = frontendPaths('desktop', { requireOwner: true });
  return {
    cacheDir: frontend.viteCacheDir,
    plugins: [sveltekit()],
    resolve: { alias: frontendUiSourceAliases() },
    // Managed product builds place SvelteKit output outside the workspace.
    // Bundle the complete SSR dependency graph so prerendering never tries to
    // resolve transitive packages by walking from that isolated directory.
    ssr: { noExternal: true },
    server: {
      strictPort: false,
      fs: { allow: [searchForWorkspaceRoot(frontend.appRoot), frontend.frontendRoot] }
    }
  };
});
