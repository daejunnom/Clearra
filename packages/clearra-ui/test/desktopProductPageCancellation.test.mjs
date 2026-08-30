import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { build } from 'esbuild';

const bundle = await build({
  bundle: true,
  format: 'esm',
  logLevel: 'silent',
  platform: 'node',
  plugins: [
    {
      name: 'desktop-invoke-test-double',
      setup(builder) {
        builder.onResolve({ filter: /^@tauri-apps\/api\/core$/ }, () => ({
          path: 'desktop-invoke-test-double',
          namespace: 'clearra-test'
        }));
        builder.onLoad(
          { filter: /^desktop-invoke-test-double$/, namespace: 'clearra-test' },
          () => ({
            contents:
              'export const invoke = (command, arguments_) => globalThis.__clearraDesktopInvoke(command, arguments_);',
            loader: 'js'
          })
        );
      }
    }
  ],
  stdin: {
    contents:
      "export { loadNextProductPage, loadProductMemberPage } from './src/lib/host/clearraDesktopHost.ts';",
    loader: 'ts',
    resolveDir: fileURLToPath(new URL('..', import.meta.url))
  },
  write: false
});
const production = await import(
  `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`
);

test('Desktop page abort sends release while the exact replay invoke is pending', async () => {
  const calls = [];
  let rejectPageGet = null;
  globalThis.__clearraDesktopInvoke = (command, arguments_) => {
    calls.push({ command, arguments_ });
    if (command === 'product_page_get') {
      return new Promise((_resolve, reject) => {
        rejectPageGet = reject;
      });
    }
    if (command === 'product_page_release') {
      rejectPageGet?.(new Error('portfolio-page-replay-cancelled'));
      return Promise.resolve();
    }
    return Promise.reject(new Error(`unexpected Desktop command: ${command}`));
  };

  try {
    const controller = new AbortController();
    const pending = production.loadProductMemberPage(
      '184467440737095516160',
      '1',
      controller.signal
    );
    await new Promise((resolve) => setImmediate(resolve));
    controller.abort();

    await assert.rejects(pending, (error) => {
      assert.equal(error.name, 'AbortError');
      return true;
    });
    assert.deepEqual(
      calls.map(({ command }) => command),
      ['product_page_get', 'product_page_release']
    );
    assert.deepEqual(calls[0].arguments_, {
      alternativeIndex: '184467440737095516160',
      memberPageNumber: '1'
    });
  } finally {
    delete globalThis.__clearraDesktopInvoke;
  }
});

test('Desktop next-page abort sends release while enumeration is pending', async () => {
  const calls = [];
  let rejectPageNext = null;
  globalThis.__clearraDesktopInvoke = (command, arguments_) => {
    calls.push({ command, arguments_ });
    if (command === 'product_page_next') {
      return new Promise((_resolve, reject) => {
        rejectPageNext = reject;
      });
    }
    if (command === 'product_page_release') {
      rejectPageNext?.(new Error('portfolio-page-enumeration-cancelled'));
      return Promise.resolve();
    }
    return Promise.reject(new Error(`unexpected Desktop command: ${command}`));
  };

  try {
    const controller = new AbortController();
    const pending = production.loadNextProductPage(17, controller.signal);
    await new Promise((resolve) => setImmediate(resolve));
    controller.abort();

    await assert.rejects(pending, (error) => {
      assert.equal(error.name, 'AbortError');
      return true;
    });
    assert.deepEqual(
      calls.map(({ command }) => command),
      ['product_page_next', 'product_page_release']
    );
    assert.deepEqual(calls[0].arguments_, { maximumWorkSteps: 17 });
  } finally {
    delete globalThis.__clearraDesktopInvoke;
  }
});

test('Desktop next-page completion removes its abort listener', async () => {
  const calls = [];
  globalThis.__clearraDesktopInvoke = (command, arguments_) => {
    calls.push({ command, arguments_ });
    if (command === 'product_page_next') return Promise.resolve('{}');
    return Promise.reject(new Error(`unexpected Desktop command: ${command}`));
  };

  try {
    const controller = new AbortController();
    assert.deepEqual(await production.loadNextProductPage(23, controller.signal), {});
    controller.abort();
    await new Promise((resolve) => setImmediate(resolve));

    assert.deepEqual(calls, [
      {
        command: 'product_page_next',
        arguments_: { maximumWorkSteps: 23 }
      }
    ]);
  } finally {
    delete globalThis.__clearraDesktopInvoke;
  }
});

test('an already-aborted Desktop page request never enters Tauri', async () => {
  const calls = [];
  globalThis.__clearraDesktopInvoke = (command) => {
    calls.push(command);
    return Promise.resolve();
  };

  try {
    const controller = new AbortController();
    controller.abort();
    await assert.rejects(
      production.loadProductMemberPage('1', '1', controller.signal),
      (error) => {
        assert.equal(error.name, 'AbortError');
        return true;
      }
    );
    assert.deepEqual(calls, []);
  } finally {
    delete globalThis.__clearraDesktopInvoke;
  }
});
