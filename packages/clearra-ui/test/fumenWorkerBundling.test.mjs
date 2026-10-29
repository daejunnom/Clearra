import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { build } from 'vite';

test('Fumen reader and export workers bundle without recursive worker imports', async () => {
  const entry = fileURLToPath(new URL('../src/lib/workspace/fieldInterchange.ts', import.meta.url));
  const result = await build({
    configFile: false,
    logLevel: 'silent',
    build: {
      write: false,
      emptyOutDir: false,
      rollupOptions: { input: entry }
    }
  });
  assert.ok(!Array.isArray(result));
  const names = result.output.map((item) => item.fileName);
  assert.ok(names.some((name) => /fumenDocumentWorker-.*\.js$/u.test(name)));
  assert.ok(names.some((name) => /fieldDocumentExportWorker-.*\.js$/u.test(name)));
});
