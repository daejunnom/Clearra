import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

import { wasmArtifactGuard } from './wasmArtifactGuard';

export default defineConfig({
  plugins: [wasmArtifactGuard(), sveltekit()],
  server: {
    strictPort: false
  }
});
