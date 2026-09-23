import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig, searchForWorkspaceRoot, type Plugin } from 'vite';
import { fetchQualifiedFrontendAccelerator, frontendAcceleratorAssets,
  frontendPaths, frontendUiSourceAliases } from '../../scripts/tools/clearra-frontend-paths.mjs';

import { wasmArtifactGuard } from './wasmArtifactGuard';

function localAcceleratorReleaseMirror(): Plugin {
  return {
    name: 'clearra-local-accelerator-release-mirror',
    configureServer(server) {
      const base = process.env.CLEARRA_WEB_BASE_PATH || '';
      server.middlewares.use(async (request, response, next) => {
        try {
          const pathname = new URL(request.url || '/', 'http://localhost').pathname;
          if (!pathname.startsWith(`${base}/accel/`)) return next();
          if (request.method !== 'GET') { response.writeHead(405).end(); return; }
          const asset = (await frontendAcceleratorAssets())
            .find(item => `${base}${item.pathname}` === pathname);
          if (!asset) { response.writeHead(404).end(); return; }
          const bytes = await fetchQualifiedFrontendAccelerator(asset);
          response.writeHead(200, {
            'Content-Type': 'application/octet-stream',
            'Content-Length': bytes.byteLength,
            'Cache-Control': 'no-store'
          }).end(bytes);
        } catch {
          response.writeHead(502).end('Qualified accelerator source unavailable');
        }
      });
    }
  };
}

export default defineConfig(({ mode }) => {
  const frontend = frontendPaths('web', { requireOwner: true });
  return {
    cacheDir: frontend.viteCacheDir,
    // Linked workspace imports are otherwise discovered after the first page
    // has loaded. With audit HMR disabled, that re-optimization can mix two
    // Svelte runtime generations until a manual reload (blank first page).
    optimizeDeps: { include: ['@lucide/svelte', 'tetris-fumen', '@tauri-apps/api/core'] },
    plugins: [localAcceleratorReleaseMirror(), wasmArtifactGuard(), sveltekit()],
    resolve: { alias: frontendUiSourceAliases() },
    ssr: { noExternal: true },
    server: {
      strictPort: false,
      fs: { allow: [searchForWorkspaceRoot(frontend.appRoot), frontend.frontendRoot] },
      // A long-running local audit must survive edits and server reconnects.
      // WASM generations use their own verified, non-reloading update channel.
      // Reloading server-side artifact validation must not reload an active page.
      hmr: mode === 'local-recovery' || mode === 'local-audit' ? false : undefined
    }
  };
});
