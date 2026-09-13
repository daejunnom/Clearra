import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, searchForWorkspaceRoot } from 'vite';
import { frontendPaths } from '../../scripts/tools/clearra-frontend-paths.mjs';

import { wasmArtifactGuard } from './wasmArtifactGuard';

export default defineConfig(({ mode }) => {
  const frontend = frontendPaths('web', { requireOwner: true });
  return {
    cacheDir: frontend.viteCacheDir,
    // Linked workspace imports are otherwise discovered after the first page
    // has loaded. With audit HMR disabled, that re-optimization can mix two
    // Svelte runtime generations until a manual reload (blank first page).
    optimizeDeps: { include: ['@lucide/svelte', 'tetris-fumen', '@tauri-apps/api/core'] },
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
