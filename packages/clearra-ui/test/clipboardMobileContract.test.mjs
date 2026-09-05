import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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
      export { writeClipboardText } from './src/lib/workspace/clipboardText.ts';
      export { fieldImportFailureMessageKey } from './src/lib/workspace/fieldImportFailure.ts';
      export {
        workspaceMessage,
        workspaceSolutionCopyFailureKey
      } from './src/lib/workspace/workspaceI18n.ts';
      export { encodeSolutionPagesForClipboard } from './src/lib/workspace/solutionExportAsync.ts';
    `,
    loader: 'ts',
    resolveDir: fileURLToPath(new URL('..', import.meta.url))
  },
  write: false
});
const production = await import(
  `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`
);
const { writeClipboardText } = production;

test('clipboard adapter falls back from ClipboardItem write to writeText', async () => {
  const calls = [];
  class TestClipboardItem {
    constructor(items) {
      this.items = items;
    }
  }
  await writeClipboardText('fallback text', undefined, {
    navigator: {
      clipboard: {
        async write() {
          calls.push('write');
          throw new Error('embedded write denied');
        },
        async writeText(value) {
          calls.push(`writeText:${value}`);
        }
      }
    },
    ClipboardItem: TestClipboardItem,
    Blob
  });
  assert.deepEqual(calls, ['write', 'writeText:fallback text']);
});

test('single, all, and failed-queue copy use the same clipboard adapter', () => {
  const single = source('SolutionCopyButton.svelte');
  const all = source('SolutionCopyAllButton.svelte');
  const download = source('SolutionDownloadButton.svelte');
  const results = source('ResultWorkspace.svelte');
  const i18n = source('workspaceI18n.ts');
  for (const [name, text] of [['single', single], ['all', all], ['failed', results]]) {
    assert.match(text, /writeClipboardText\(/u, name);
    assert.doesNotMatch(text, /navigator\.clipboard/u, name);
  }
  assert.match(single, /fumenCommentTooLong/u);
  assert.match(all, /workspaceSolutionCopyFailureKey/u);
  assert.match(i18n, /invalid-fumen-comment[\s\S]*invalidFumenComment/u);
  assert.match(i18n, /fumen-page-limit[\s\S]*fumenExportPageLimit/u);
  assert.match(download, /fumenCommentTooLong/u);
  assert.match(download, /invalidFumenComment/u);
  assert.match(single, /error instanceof Error[\s\S]*error\.message/u);
  assert.match(all, /error instanceof Error[\s\S]*error\.message/u);
  assert.match(download, /error instanceof Error \? error\.message/u);
});

test('copy-all maps a 4,097-page Fumen rejection to stable EN and KO guidance', async () => {
  const page = { height: 1, initialMask: 0n, placements: [] };
  await assert.rejects(
    () =>
      production.encodeSolutionPagesForClipboard(
        Array.from({ length: 4_097 }, () => page),
        'fumen'
      ),
    (error) => {
      const code = error instanceof Error ? error.message : '';
      const key = production.workspaceSolutionCopyFailureKey(code);
      assert.equal(code, 'fumen-page-limit');
      assert.equal(key, 'fumenExportPageLimit');
      assert.match(production.workspaceMessage('en', key), /4,096 pages/u);
      assert.match(production.workspaceMessage('ko', key), /4,096페이지/u);
      return true;
    }
  );
});

test('stable Fumen ingress failures retain user-facing EN and KO messages', () => {
  assert.equal(
    production.fieldImportFailureMessageKey(new Error('fumen-input-too-large')),
    'fumenInputTooLarge'
  );
  assert.equal(
    production.fieldImportFailureMessageKey(new Error('fumen-page-limit')),
    'fumenPageLimit'
  );
  assert.match(production.workspaceMessage('en', 'fumenInputTooLarge'), /too large/i);
  assert.match(production.workspaceMessage('ko', 'fumenPageLimit'), /4,096/u);
  for (const locale of ['en', 'ko']) {
    const publicImportFailure = production.workspaceMessage(locale, 'fieldImportInvalid');
    assert.doesNotMatch(publicImportFailure, /\bctk[12]\b/iu);
    assert.match(publicImportFailure, /Fumen/u);
    assert.match(publicImportFailure, /CTK3/u);
  }
  for (const file of [
    'WorkspaceBoardEditor.svelte',
    'CtkDrawerWorkspace.svelte',
    'PlayerWorkspace.svelte'
  ]) {
    assert.match(source(file), /fieldImportFailureMessageKey\(error/u, file);
  }
  assert.match(source('player/PlayerControls.svelte'), /label\(fieldFailureKey\)/u);
});

test('solution controls are overflow-safe at 320, 360, 390, and 430 CSS widths', () => {
  const toolbar = source('SolutionCopyFormatControl.svelte');
  const single = source('SolutionCopyButton.svelte');
  const all = source('SolutionCopyAllButton.svelte');
  const download = source('SolutionDownloadButton.svelte');
  const results = source('ResultWorkspace.svelte');

  assert.match(toolbar, /@media \(max-width: 520px\)/u);
  assert.match(toolbar, /grid-template-columns: repeat\(2, minmax\(0, 1fr\)\)/u);
  assert.match(toolbar, /@media \(max-width: 360px\)/u);
  assert.match(toolbar, /grid-template-columns: minmax\(0, 1fr\)/u);
  assert.match(toolbar, /max-width: 100%/u);
  for (const width of [320, 360, 390, 430]) {
    assert.ok(width <= 520, `${width}px uses the bounded mobile grid`);
  }
  for (const [name, text] of [
    ['format', toolbar],
    ['single', single],
    ['all', all],
    ['download', download],
    ['failed queue', results]
  ]) {
    assert.match(text, /@media \(pointer: coarse\)/u, name);
    assert.match(text, /min-height: 44px/u, name);
  }
});

function source(file) {
  return readFileSync(new URL(`../src/lib/workspace/${file}`, import.meta.url), 'utf8');
}
