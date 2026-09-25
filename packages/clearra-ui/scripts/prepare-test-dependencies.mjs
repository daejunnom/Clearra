import { spawnSync } from 'node:child_process';
import { lstat } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  ACCEPTED_CTK3_MANIFEST,
  verifyAcceptedCtk3Dist,
} from '../../../scripts/tools/accepted-ctk3-dist.mjs';

const self = fileURLToPath(import.meta.url);
const root = resolve(dirname(self), '../../..');
const dist = resolve(root, 'packages/ctk3/dist');

async function hasManifest(directory) {
  try {
    await lstat(resolve(directory, ACCEPTED_CTK3_MANIFEST));
    return true;
  } catch (error) {
    if (error?.code === 'ENOENT') return false;
    throw error;
  }
}

/** Build a source dependency, or verify and reuse an immutable accepted input.
 * An invalid/missing accepted input never falls back to rebuilding its bytes.
 */
export async function prepareUiTestDependencies({
  directory = dist,
  environment = process.env,
  build = buildCtk3,
} = {}) {
  const explicitRun = environment.CLEARRA_ACCEPTED_RUN_ID;
  const explicitAttempt = environment.CLEARRA_ACCEPTED_RUN_ATTEMPT;
  if (Boolean(explicitRun) !== Boolean(explicitAttempt)) {
    throw new Error('Accepted CTK3 test input requires both run ID and run attempt');
  }
  if (explicitRun || await hasManifest(directory)) {
    await verifyAcceptedCtk3Dist(
      directory,
      environment.CLEARRA_SOURCE_COMMIT,
      explicitRun ?? environment.GITHUB_RUN_ID,
      explicitAttempt ?? environment.GITHUB_RUN_ATTEMPT,
    );
    return 'accepted-verified';
  }
  await build();
  return 'source-built';
}

function buildCtk3() {
  const result = spawnSync(process.execPath, [resolve(root, 'packages/ctk3/scripts/build.mjs')], {
    cwd: root,
    env: process.env,
    stdio: 'inherit',
    windowsHide: true,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    throw new Error(`CTK3 test dependency build failed: exit=${result.status} signal=${result.signal ?? 'none'}`);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === self) {
  if (process.argv.length !== 2) throw new Error('UI test dependency paths are not configurable');
  await prepareUiTestDependencies().then(mode => {
    process.stdout.write(`ui_test_dependency=ctk3 mode=${mode}\n`);
  }).catch(error => {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  });
}
