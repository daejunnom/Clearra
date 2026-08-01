import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';

const root = dirname(fileURLToPath(import.meta.url));
const repository = resolve(root, '../../..');
const isolationHeaders = {
  'Cross-Origin-Opener-Policy': 'same-origin',
  'Cross-Origin-Embedder-Policy': 'require-corp'
};

export default defineConfig({
  root,
  publicDir: resolve(repository, 'apps/clearra-web/static'),
  build: {
    target: 'es2022'
  },
  server: {
    fs: { allow: [repository] },
    headers: isolationHeaders
  },
  preview: {
    headers: isolationHeaders
  }
});
