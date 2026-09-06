import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

import { wasmArtifactGuard } from './wasmArtifactGuard';

export default defineConfig(({ mode }) => ({
  plugins: [wasmArtifactGuard(), sveltekit()],
  server: {
    strictPort: false,
    // A long-running local audit must survive edits and server reconnects.
    // WASM generations use their own verified, non-reloading update channel.
    hmr: mode === 'local-recovery' || mode === 'local-audit' ? false : undefined
  }
}));
