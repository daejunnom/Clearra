import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, searchForWorkspaceRoot } from 'vite';
import { frontendPaths, frontendUiSourceAliases } from '../../scripts/tools/clearra-frontend-paths.mjs';
import { frontendDependencyPolicy } from '../../scripts/tools/clearra-frontend-dependencies.mjs';

export default defineConfig(({ command }) => {
  const frontend = frontendPaths('desktop', { requireOwner: true });
  return {
    cacheDir: frontend.viteCacheDir,
    plugins: [sveltekit()],
    resolve: { alias: frontendUiSourceAliases() },
    ...frontendDependencyPolicy(command),
    server: {
      strictPort: false,
      fs: { allow: [searchForWorkspaceRoot(frontend.appRoot), frontend.frontendRoot] }
    }
  };
});
