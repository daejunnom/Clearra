import assert from 'node:assert/strict';
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
