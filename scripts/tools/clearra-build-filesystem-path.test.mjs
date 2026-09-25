import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, existsSync, realpathSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';
import { resolveBuildFilesystemPath } from './clearra-build-filesystem-path.mjs';

function fixture(body) {
  const root = mkdtempSync(join(tmpdir(), 'clearra-physical-path-'));
  try { body(root); } finally { rmSync(root, { recursive: true, force: true }); }
}

test('missing output leaves preserve the physical ancestor without creating files', () => fixture(root => {
  const parent = join(root, 'MixedCase');
  mkdirSync(parent);
  const child = join(parent, 'frontend', 'web', 'svelte-kit');
  assert.equal(resolveBuildFilesystemPath(child), resolve(realpathSync.native(parent), 'frontend/web/svelte-kit'));
  assert.equal(existsSync(child), false);
}));

test('Windows case-folded owner identity becomes the exact physical compiler path',
  { skip: process.platform !== 'win32' }, () => fixture(root => {
    const physical = join(root, 'Clearra', 'PathProbe');
    mkdirSync(physical, { recursive: true });
    const expected = realpathSync.native(physical);
    assert.equal(resolveBuildFilesystemPath(physical.toLowerCase()), expected);
    assert.equal(resolveBuildFilesystemPath(join(physical.toLowerCase(), 'generated/app.js')),
      join(expected, 'generated/app.js'));
  }));

test('link or junction aliases cannot become compiler-path authority', () => fixture(root => {
  const owner = join(root, 'owner'); const foreign = join(root, 'foreign');
  mkdirSync(owner); mkdirSync(foreign);
  symlinkSync(foreign, join(owner, 'escape'), process.platform === 'win32' ? 'junction' : 'dir');
  assert.throws(() => resolveBuildFilesystemPath(join(owner, 'escape/generated')), /link|reparse|symlink/i);
}));

test('a native resolution to a different physical directory is rejected', t => fixture(root => {
  const owner = join(root, 'owner'); const foreign = join(root, 'foreign');
  mkdirSync(owner); mkdirSync(foreign);
  const nativeRealpath = realpathSync.native;
  const physicalForeign = nativeRealpath(foreign);
  const mocked = t.mock.method(realpathSync, 'native', (path, ...options) =>
    path === owner ? physicalForeign : nativeRealpath(path, ...options));
  try {
    assert.throws(() => resolveBuildFilesystemPath(join(owner, 'generated')), /physical ancestor/);
    assert.equal(existsSync(join(foreign, 'generated')), false);
  } finally { mocked.mock.restore(); }
}));

test('broken links cannot be treated as missing output ancestors', () => fixture(root => {
  const missing = join(root, 'missing');
  symlinkSync(missing, join(root, 'broken'), process.platform === 'win32' ? 'junction' : 'dir');
  assert.throws(() => resolveBuildFilesystemPath(join(root, 'broken/generated')), /link|reparse|symlink/i);
  assert.equal(existsSync(missing), false);
}));

test('Windows short-name aliases resolve both existing and missing compiler paths',
  { skip: process.platform !== 'win32' }, t => fixture(root => {
    const parent = join(root, 'Long Compiler Directory');
    mkdirSync(parent);
    const expected = realpathSync.native(parent);
    const alias = execFileSync(process.env.ComSpec ?? 'cmd.exe',
      ['/d', '/s', '/c', 'for %I in ("%CLEARRA_TEST_LONG_PATH%") do @echo %~sI'],
      // cmd.exe parses this fixed command itself; do not add CRT quote escapes.
      { encoding: 'utf8', windowsHide: true, windowsVerbatimArguments: true, timeout: 10000,
        env: { ...process.env, CLEARRA_TEST_LONG_PATH: parent } }).trim();
    assert.ok(alias, 'the native short-name query returned a path');
    assert.equal(realpathSync.native(alias), expected, 'the query returned a real alias of the fixture');
    if (alias.toLowerCase() === expected.toLowerCase()) {
      t.skip('this Windows volume has no distinct 8.3 alias');
      return;
    }
    assert.equal(resolveBuildFilesystemPath(alias), expected);
    const child = join(alias, 'generated', 'app.js');
    assert.equal(resolveBuildFilesystemPath(child), join(expected, 'generated', 'app.js'));
    assert.equal(existsSync(child), false);
  }));
