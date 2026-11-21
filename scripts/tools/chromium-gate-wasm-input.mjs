import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { lstat, realpath } from 'node:fs/promises';
import { join } from 'node:path';
import {
  ACCEPTED_WASM_BUILD_RECEIPT,
  verifyAcceptedWasmBuild,
} from '../release/accepted-wasm-build.mjs';
import { assertNoBuildLinks, buildPathIdentity } from './clearra-build-policy.mjs';
import { inspectVerifiedClearraWasmDirectory } from './import-verified-clearra-wasm.mjs';

const SOURCE = /^[a-f0-9]{40}$/u;
const HASH = /^[1-9][0-9]{0,19}$/u;

/** Read the release gate's existing immutable payload. Do not stage, rebind or
 * build it here; browser URLs are fed from these captured bytes. */
export async function readChromiumGateWasmInput({
  owner,
  repositoryRoot,
  environment = process.env,
}) {
  const source = environment.CLEARRA_SOURCE_COMMIT;
  assert.match(source ?? '', SOURCE, 'Browser gate requires an exact commit');
  assert.equal(environment.GITHUB_ACTIONS, 'true', 'Gate-staged Chromium is for Actions');
  assert.equal(environment.GITHUB_SHA, source, 'Wrong Actions checkout source');
  assert.equal(environment.CLEARRA_ENGINE_BUILD_ID, source, 'Wrong WASM engine identity');
  const checkoutRef = environment.GITHUB_REF;
  const repository = environment.GITHUB_REPOSITORY;
  assert.match(repository ?? '', /^[a-zA-Z0-9_.-]+\/[a-zA-Z0-9_.-]+$/u, 'Missing Actions repository');
  const staged = join(owner.cargoTarget, 'clearra-web-public', 'wasm');
  assertNoBuildLinks(staged);
  assert.equal(buildPathIdentity(await realpath(staged)), buildPathIdentity(staged),
    'WASM must be the managed frontend staging path');

  let receipt = null;
  let artifactInput;
  if (owner.purpose === 'product') {
    assert.equal(checkoutRef, 'refs/heads/main', 'Canonical WASM gate must run from main');
    assert.equal(environment.GITHUB_EVENT_NAME, 'workflow_dispatch');
    assert.equal(environment.GITHUB_WORKFLOW_REF, `${repository}/.github/workflows/release-cli.yml@${checkoutRef}`);
    assert.ok(environment.CLEARRA_ACCEPTED_WASM_DIR, 'Canonical WASM gate requires producer artifact');
    assert.match(environment.GITHUB_RUN_ID ?? '', HASH);
    assert.match(environment.GITHUB_RUN_ATTEMPT ?? '', HASH);
    assert.equal(environment.CLEARRA_ACCEPTED_RUN_ID, environment.GITHUB_RUN_ID);
    assert.equal(environment.CLEARRA_ACCEPTED_RUN_ATTEMPT, environment.GITHUB_RUN_ATTEMPT);
    receipt = await verifyAcceptedWasmBuild(staged, source, environment.GITHUB_RUN_ID, environment.GITHUB_RUN_ATTEMPT);
    artifactInput = 'canonical-wasm-producer';
  } else {
    assert.equal(owner.purpose, 'experiment');
    assert.equal(environment.GITHUB_WORKFLOW_REF, `${repository}/.github/workflows/candidate-preflight.yml@${checkoutRef}`);
    assert.ok(!environment.CLEARRA_ACCEPTED_WASM_DIR &&
      !environment.CLEARRA_ACCEPTED_RUN_ID && !environment.CLEARRA_ACCEPTED_RUN_ATTEMPT,
    'Candidate Chromium gate must not consume accepted-run inputs');
    const acceptedReceipt = await lstat(join(staged, ACCEPTED_WASM_BUILD_RECEIPT)).catch(error => {
      if (error.code !== 'ENOENT') throw error;
      return null;
    });
    assert.equal(acceptedReceipt, null, 'Candidate Chromium gate found a canonical receipt');
    artifactInput = 'unqualified-same-job';
  }

  const artifact = await inspectVerifiedClearraWasmDirectory({
    sourceDirectory: staged, repositoryRoot, environment,
  });
  // A previous verification of the directory does not prove the bytes
  // actually served. Cross-check the captured snapshot against its receipt.
  if (receipt) {
    for (const [name, bytes] of artifact.files) {
      const expected = receipt.files.find(file => file.path === name);
      const actual = createHash('sha256').update(bytes).digest('hex');
      assert.ok(expected && expected.sha256 === actual && expected.size === bytes.byteLength,
        'Chromium staged bytes differ from the verified WASM producer: ' + name);
    }
  }
  return { ...artifact, artifactInput, acceptedPayloadSha256: receipt?.payload_sha256 ?? null };
}
