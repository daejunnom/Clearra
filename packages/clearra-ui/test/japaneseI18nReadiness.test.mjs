import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { build } from 'esbuild';

const bundle = await build({
  bundle: true,
  format: 'esm',
  logLevel: 'silent',
  platform: 'node',
  stdin: {
    contents: `
      export * from '../src/lib/i18n/languageManifest.ts';
      export * from '../src/lib/i18n/japaneseWorkspaceCatalog.ts';
      export {
        preferredWorkspaceLanguage,
        workspaceCatalogReadiness,
        WORKSPACE_MESSAGE_KEYS
      } from '../src/lib/workspace/workspaceI18n.ts';
    `,
    resolveDir: fileURLToPath(new URL('.', import.meta.url)),
    sourcefile: 'japanese-i18n-readiness-entry.ts'
  },
  write: false
});

const production = await import(
  `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`
);

test('Japanese GUI locale is known but remains unavailable while its catalog is incomplete', () => {
  assert.deepEqual(production.RELEASED_WORKSPACE_LANGUAGES, ['en', 'ko']);
  assert.equal(production.UI_LANGUAGE_MANIFEST.ja.status, 'planned');
  assert.equal(production.matchKnownWorkspaceLocale('ja_JP'), 'ja');
  assert.equal(production.matchKnownWorkspaceLocale('jp'), null);
  assert.equal(production.matchReleasedWorkspaceLanguage('ja-JP'), null);
  assert.equal(production.preferredWorkspaceLanguage('ja-JP'), 'en');

  const readiness = production.workspaceCatalogReadiness(
    production.japaneseWorkspaceMessages
  );
  assert.equal(readiness.complete, false);
  assert.ok(readiness.translated > 0);
  assert.ok(readiness.translated < readiness.required);
  assert.deepEqual(readiness.unexpectedKeys, []);
  assert.deepEqual(readiness.placeholderMismatches, []);
  assert.ok(readiness.missingKeys.includes('solutionCopyFailed'));
  assert.equal(
    production.WORKSPACE_MESSAGE_KEYS.length,
    readiness.required
  );
});
