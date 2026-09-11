import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { build } from 'esbuild';
import { compile } from 'svelte/compiler';

const bundle = await build({
  bundle: true,
  format: 'esm',
  platform: 'node',
  logLevel: 'silent',
  stdin: {
    contents: `
      export * from '../src/lib/i18n/runtimeShellCatalog.ts';
      export * from '../src/lib/i18n/languageManifest.ts';
      export { deferWasmTerminalResponse } from '../src/lib/wasm/wasmTerminalTranscript.ts';
    `,
    resolveDir: fileURLToPath(new URL('.', import.meta.url)),
    sourcefile: 'runtime-shell-i18n-test.ts'
  },
  write: false
});
const api = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
const read = (path) => readFileSync(new URL(`../src/lib/${path}`, import.meta.url), 'utf8');
const placeholders = (text) => [...text.matchAll(/\{\w+\}/gu)].map(([token]) => token).sort();

test('runtime shell catalog has exact EN/KO/JA and placeholder parity for every message', () => {
  const messages = Object.entries(api.RUNTIME_SHELL_MESSAGES);
  assert.ok(messages.length >= 100);
  for (const [key, locales] of messages) {
    assert.deepEqual(Object.keys(locales).sort(), ['en', 'ja', 'ko'], key);
    assert.ok(locales.ja.length > 0, key);
    assert.notEqual(locales.ja, locales.en, key);
    assert.deepEqual(placeholders(locales.ja), placeholders(locales.en), key);
    assert.deepEqual(placeholders(locales.ko), placeholders(locales.en), key);
    assert.equal(api.runtimeShellCopy('ja')[key], locales.ja);
  }
  assert.equal(api.runtimeShellText('ja', 'workersValue', { count: 4, mode: '並列' }), '4（並列）');
  assert.equal(api.runtimeShellText('en', 'workersValue', { count: 4, mode: 'parallel' }), '4 (parallel)');
});

test('known display states are translated without rewriting technical identifiers', () => {
  assert.equal(api.runtimeShellValue('ja', 'running'), '実行中');
  assert.equal(api.runtimeShellValue('ja', 'validation-failed'), '検証失敗');
  assert.equal(api.runtimeShellValue('ko', 'pending'), '대기 중');
  assert.equal(api.runtimeShellValue('ja', true), 'はい');
  for (const identifier of ['cpu', 'gpu', 'clearra-cli/CommandRequest', 'pc-path-family.v2', '0123456789abcdef']) {
    assert.equal(api.runtimeShellValue('ja', identifier), identifier);
  }
});

test('terminal localization preserves commands, deferred JSON keys, codes and response ownership', () => {
  const response = { status: 'success', message: 'job cancelled', total_solution_count: 2 };
  const deferred = api.deferWasmTerminalResponse(response);
  const command = '$ clearra pc --queue IOTSZJL --lang en --format json';
  const lines = [
    'clearra web runtime ready', command, 'job 42 started',
    'E_WASM_WORKER_MESSAGE_INVALID: WASM worker returned an invalid message',
    'E_WASM_PREPARATION_PROGRESS_STALLED: WASM preparation did not complete within 500 ms; the worker tree was force-terminated.',
    'job cancelled; computation scope released', deferred
  ];
  const japanese = api.formatRuntimeShellTranscript('ja', lines);
  assert.ok(japanese.includes(command));
  assert.ok(japanese.includes('ジョブ42を開始しました。'));
  assert.ok(japanese.includes('E_WASM_WORKER_MESSAGE_INVALID: WASMワーカーが無効なメッセージを返しました。'));
  assert.ok(japanese.includes('500ミリ秒以内'));
  assert.ok(japanese.includes(JSON.stringify(response, null, 2)));
  assert.deepEqual(response, { status: 'success', message: 'job cancelled', total_solution_count: 2 });
  assert.equal(lines[0], 'clearra web runtime ready');
  assert.equal(deferred.response, null);
  const english = api.formatRuntimeShellTranscript('en', lines);
  assert.ok(english.includes('job 42 started'));
  assert.ok(english.includes(JSON.stringify(response, null, 2)));
});

test('terminal formatting failure is translated and retries preserve deferred structured results', () => {
  const cyclic = {};
  cyclic.cycle = cyclic;
  const deferred = api.deferWasmTerminalResponse(cyclic);
  assert.match(api.formatRuntimeShellTranscript('ja', [deferred]), /^E_WASM_TERMINAL_FORMAT: 応答テキスト/u);
  assert.equal(deferred.response, cyclic);
  delete cyclic.cycle;
  assert.equal(api.formatRuntimeShellTranscript('ja', [deferred]), '{}');
  assert.equal(deferred.response, null);
});

test('host helper and terminal literal inventory has reviewed Japanese source coverage', () => {
  const english = new Set(Object.values(api.RUNTIME_SHELL_MESSAGES).map((locale) => locale.en));
  for (const path of [
    'wasm/WasmTerminalWorkerController.ts', 'wasm/wasmWorkerStore.ts',
    'stores/desktopJobStore.ts', 'wasm/wasmTerminalTranscript.ts'
  ]) {
    for (const [, source] of read(path).matchAll(/'([^'\n]+)'/g)) {
      if (/[A-Za-z]{2,} [A-Za-z]{2,}/u.test(source)) assert.ok(english.has(source), `${path}: ${source}`);
    }
  }
});

test('all three shells compile with catalog-backed labels and accept released Japanese', () => {
  for (const path of [
    'components/DesktopHostShell.svelte', 'render/RenderStatusPanel.svelte', 'wasm/WasmTerminalShell.svelte'
  ]) {
    const source = read(path);
    assert.match(source, /matchReleasedWorkspaceLanguage\(/u, path);
    assert.match(source, /runtimeShellCopy\(locale\)/u, path);
    assert.doesNotThrow(() => compile(source, { filename: path, generate: 'server' }));
    const markup = source.replace(/<script[\s\S]*?<\/script>/u, '').split('<style>')[0];
    assert.doesNotMatch(markup, /aria-label="[A-Za-z ]+"/u, path);
    for (const [, raw] of markup.matchAll(/>([^<>{]+)</g)) {
      const text = raw.trim();
      if (!/[A-Za-z]/u.test(text)) continue;
      assert.ok(['Clearra', 'WebGPU', 'PNG/GIF', 'clearra-cli/CommandRequest'].includes(text), `${path}: ${text}`);
    }
  }
  assert.equal(api.UI_LANGUAGE_MANIFEST.ja.status, 'released');
  assert.deepEqual(api.RELEASED_WORKSPACE_LANGUAGES, ['en', 'ko', 'ja']);
  assert.equal(api.matchReleasedWorkspaceLanguage('ja'), 'ja');
  assert.equal(api.matchReleasedWorkspaceLanguage('ja-JP'), 'ja');
});
