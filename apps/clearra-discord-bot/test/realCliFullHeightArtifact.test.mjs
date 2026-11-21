import assert from 'node:assert/strict';
import { join, resolve } from 'node:path';
import test from 'node:test';
import { checkedCliPath } from './support/realCliFullHeightArtifact.mjs';

const repository = resolve('example-repository');
const release = { version: '0.9.0', tag: 'v0.9.0' };
const path = join(repository, 'dist', 'Clearra-CLI-v0.9.0-linux-x86_64');
const input = { repository, release, path, version: release.version, platform: 'linux', arch: 'x64' };

test('packaged readback admits only the named Linux x64 export matching the source release', () => {
  assert.equal(checkedCliPath(input), path);
  assert.throws(() => checkedCliPath({ ...input, version: undefined }), /canonical development build target/u);
  assert.throws(() => checkedCliPath({ ...input, version: '' }), /must match the checked-out source/u);
  assert.throws(() => checkedCliPath({ ...input, version: '0.8.4' }), /must match the checked-out source/u);
  assert.throws(() => checkedCliPath({ ...input, platform: 'win32' }), /Linux-only/u);
  assert.throws(() => checkedCliPath({ ...input, arch: 'arm64' }), /requires Linux x64/u);
  assert.throws(() => checkedCliPath({ ...input, path: join('dist', 'Clearra-CLI-v0.9.0-linux-x86_64') }),
    /explicit already-built CLI path/u);
  assert.throws(() => checkedCliPath({ ...input, path: join(repository, '..', 'dist', 'Clearra-CLI-v0.9.0-linux-x86_64') }),
    /must be the exact versioned repository export/u);
});

test('unmarked readback retains the canonical native debug and release paths', () => {
  const nativePath = join(repository, 'build', 'cargo', 'default', 'debug', 'clearra');
  assert.equal(checkedCliPath({ ...input, path: nativePath, version: undefined }), nativePath);
  assert.throws(() => checkedCliPath({ ...input, path: nativePath }), /must be the exact versioned repository export/u);
  const windowsPath = join(repository, 'build', 'cargo', 'default', 'release', 'clearra.exe');
  assert.equal(checkedCliPath({ ...input, path: windowsPath, platform: 'win32', version: undefined }), windowsPath);
});

test('managed development readback is not enabled by a path or flag alone', () => {
  const nativePath = join(repository, 'build', 'cargo', 'default', 'debug', 'clearra');
  assert.throws(() => checkedCliPath({
    ...input, path: nativePath, version: undefined,
    environment: { CLEARRA_REAL_PC24_MANAGED_CLI: '1', CARGO_TARGET_DIR: resolve('arbitrary-target') },
  }));
});
