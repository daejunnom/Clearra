import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, searchForWorkspaceRoot } from 'vite';
import { frontendPaths } from '../../scripts/tools/clearra-frontend-paths.mjs';

import { wasmArtifactGuard } from './wasmArtifactGuard';

export default defineConfig(({ mode }) => {
  const frontend = frontendPaths('web', { requireOwner: true });
  return {
    cacheDir: frontend.viteCacheDir,
    plugins: [wasmArtifactGuard(), sveltekit()],
    server: {
      strictPort: false,
      fs: { allow: [searchForWorkspaceRoot(frontend.appRoot), frontend.frontendRoot] },
      // A long-running local audit must survive edits and server reconnects.
      // WASM generations use their own verified, non-reloading update channel.
      hmr: mode === 'local-recovery' || mode === 'local-audit' ? false : undefined
    }
  };
});
