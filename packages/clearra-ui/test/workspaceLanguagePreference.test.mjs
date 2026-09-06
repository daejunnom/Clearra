import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { build } from 'esbuild';

const bundle = await build({
  bundle: true,
  format: 'esm',
  logLevel: 'silent',
  platform: 'node',
  entryPoints: [fileURLToPath(new URL('../src/lib/workspace/workspaceLanguagePreference.ts', import.meta.url))],
  write: false
});

test('denied preference storage cannot abort mounting or discard the session language', async () => {
  const originalStorage = Object.getOwnPropertyDescriptor(globalThis, 'localStorage');
  const originalNavigator = Object.getOwnPropertyDescriptor(globalThis, 'navigator');
  const denied = () => { throw new DOMException('Storage denied', 'SecurityError'); };
  try {
    Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { language: 'en-US' } });
    Object.defineProperty(globalThis, 'localStorage', { configurable: true, get: denied });
    const preferences = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
    assert.equal(preferences.readWorkspaceLanguage(), 'en');
    assert.doesNotThrow(() => preferences.persistWorkspaceLanguage('ko'));
    assert.equal(preferences.readWorkspaceLanguage(), 'ko');

    // Some hosts allow reading but reject writes: an absent stored value must
    // still retain the language selected during this app session.
    Object.defineProperty(globalThis, 'localStorage', {
      configurable: true,
      value: { getItem: () => null, setItem: denied }
    });
    assert.equal(preferences.readWorkspaceLanguage(), 'ko');

    // A stale persisted EN preference must not replace KO after a denied write.
    Object.defineProperty(globalThis, 'localStorage', {
      configurable: true,
      value: { getItem: () => 'en', setItem: denied }
    });
    preferences.persistWorkspaceLanguage('ko');
    assert.equal(preferences.readWorkspaceLanguage(), 'ko');

    Object.defineProperty(globalThis, 'localStorage', {
      configurable: true,
      value: { getItem: () => 'en', setItem: () => {} }
    });
    preferences.persistWorkspaceLanguage('en');
    assert.equal(preferences.readWorkspaceLanguage(), 'en');
  } finally {
    if (originalStorage) Object.defineProperty(globalThis, 'localStorage', originalStorage);
    else delete globalThis.localStorage;
    if (originalNavigator) Object.defineProperty(globalThis, 'navigator', originalNavigator);
    else delete globalThis.navigator;
  }
});
