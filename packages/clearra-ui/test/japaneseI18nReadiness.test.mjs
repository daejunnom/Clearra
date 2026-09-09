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
      export * from '../src/lib/i18n/componentCatalog.ts';
      export {
        preferredWorkspaceLanguage,
        workspaceMessage,
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

test('Japanese GUI locale remains planned after its complete catalog is prepared', () => {
  assert.deepEqual(production.RELEASED_WORKSPACE_LANGUAGES, ['en', 'ko']);
  assert.equal(production.UI_LANGUAGE_MANIFEST.ja.status, 'planned');
  assert.equal(production.matchKnownWorkspaceLocale('ja_JP'), 'ja');
  assert.equal(production.matchKnownWorkspaceLocale('jp'), null);
  assert.equal(production.matchReleasedWorkspaceLanguage('ja-JP'), null);
  assert.equal(production.preferredWorkspaceLanguage('ja-JP'), 'en');

  const readiness = production.workspaceCatalogReadiness(
    production.japaneseWorkspaceMessages
  );
  assert.equal(readiness.complete, true);
  assert.equal(readiness.translated, readiness.required);
  assert.deepEqual(readiness.unexpectedKeys, []);
  assert.deepEqual(readiness.placeholderMismatches, []);
  assert.deepEqual(readiness.missingKeys, []);
  assert.equal(
    production.WORKSPACE_MESSAGE_KEYS.length,
    readiness.required
  );
});

test('Japanese workspace phrases are translated while identifiers and native language names remain intact', () => {
  const invariantKeys = [
    'ctkDrawer', 'playerPps', 'playerScoreGuideline', 'playerB2b',
    'english', 'korean', 'queuePlaceholder', 'scoreProfileJstrisUltra',
    'srsPlusAllMini', 'progressBuild'
  ];
  const unchanged = [];
  for (const key of production.WORKSPACE_MESSAGE_KEYS) {
    const text = production.japaneseWorkspaceMessages[key];
    assert.ok(text.trim().length > 0, key);
    if (text === production.workspaceMessage('en', key)) unchanged.push(key);
    else if (!['playerDas', 'playerArr', 'ctkPageCount'].includes(key)) {
      assert.match(text, /[\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Han}]/u, key);
    }
  }
  assert.deepEqual(unchanged, invariantKeys);
});

test('readiness rejects missing keys and changed interpolation even after Japanese preparation', () => {
  const invalid = { ...production.japaneseWorkspaceMessages, playerInitialQueueTooLong: '最大{limit}個です。' };
  delete invalid.solutionCopyFailed;
  const readiness = production.workspaceCatalogReadiness(invalid);
  assert.equal(readiness.complete, false);
  assert.deepEqual(readiness.missingKeys, ['solutionCopyFailed']);
  assert.deepEqual(readiness.placeholderMismatches, ['playerInitialQueueTooLong']);
});

test('every component phrase contains all known locales and identical placeholders', () => {
  const placeholders = text => [...text.matchAll(/\{([a-z0-9_]+)\}/gi)].map(match => match[1]).sort();
  for (const [key, message] of Object.entries(production.COMPONENT_MESSAGES)) {
    assert.deepEqual(Object.keys(message).sort(), ['en', 'ja', 'ko'], key);
    for (const locale of ['ko', 'ja']) {
      assert.ok(message[locale].trim().length > 0, `${key}/${locale}`);
      assert.deepEqual(placeholders(message[locale]), placeholders(message.en), `${key}/${locale}`);
    }
    assert.notEqual(message.ja, message.en, `${key} must have a Japanese translation`);
  }
  assert.equal(production.componentMessage('en', 'workerAuthoritySummary', {
    requested: 4, effective: 3, reason: 'host limit'
  }), 'Workers: 4 requested · 3 effective · host limit');
});
