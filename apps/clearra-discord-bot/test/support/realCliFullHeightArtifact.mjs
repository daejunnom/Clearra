import assert from 'node:assert/strict';
import { isAbsolute, join, resolve } from 'node:path';
import { expectedProductArtifactName } from '../../../../scripts/release/current-product-release.mjs';

export function checkedCliPath({ repository, path, version, release, platform, arch }) {
  assert.ok(isAbsolute(path ?? ''), 'an explicit already-built CLI path is required');
  const resolved = resolve(path);
  if (version !== undefined) {
    assert.equal(platform, 'linux', 'packaged CLI readback is Linux-only');
    assert.equal(arch, 'x64', 'packaged CLI readback requires Linux x64');
    assert.equal(version, release.version, 'packaged CLI version must match the checked-out source');
    assert.equal(resolved, join(repository, 'dist', expectedProductArtifactName('linux-cli', release)),
      'packaged CLI must be the exact versioned repository export');
  } else {
    const binary = platform === 'win32' ? 'clearra.exe' : 'clearra';
    assert.ok(['debug', 'release'].some(profile =>
      join(repository, 'build', 'cargo', 'default', profile, binary) === resolved),
    'CLI must be in the canonical development build target');
  }
  return resolved;
}
