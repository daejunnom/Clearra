import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { assertBookwormRuntime, BOOKWORM_CLI_PROBES, verifyRuntimeProbeOutputs } from './verify-linux-cli-runtime.mjs';

const source = 'b'.repeat(40);
const identity = {
  source_commit: source, engine_build_id: source,
  contract_schema_version: 'clearra.search.contract.v2',
  supply_semantics_id: 'clearra.supply.projected-terminal-lookahead.v1',
  artifact_schema_version: 'clearra.solution-data.v1',
};
// Match the public CLI JSON envelope: the mode belongs to finesse_report,
// never to the root shared by rules, PC and finesse results.
const outputs = () => [
  { runtime_identity: identity },
  { runtime_identity: identity },
  { runtime_identity: identity, finesse_report: { mode: 'search' } },
].map((result) => JSON.stringify(result));

test('all slim-runtime probes require the exact packaged product identity', () => {
  verifyRuntimeProbeOutputs(outputs(), source);
  assert.equal(BOOKWORM_CLI_PROBES.length, 3);
  assert.ok(BOOKWORM_CLI_PROBES.every((probe) => probe.includes('json')));
  for (let probe = 0; probe < BOOKWORM_CLI_PROBES.length; probe += 1) {
    for (const key of Object.keys(identity)) {
      const bad = outputs();
      bad[probe] = JSON.stringify({ ...JSON.parse(bad[probe]), runtime_identity: { ...identity, [key]: 'wrong' } });
      assert.throws(() => verifyRuntimeProbeOutputs(bad, source), /identity differs/u);
    }
  }
  assert.throws(() => verifyRuntimeProbeOutputs(outputs().slice(1), source), /every probe/u);
  assert.throws(() => verifyRuntimeProbeOutputs(outputs(), 'HEAD'), /exact source/u);
  assert.throws(() => verifyRuntimeProbeOutputs(['not json', ...outputs().slice(1)], source));
  assert.throws(() => verifyRuntimeProbeOutputs([...outputs().slice(0, 2), JSON.stringify({ runtime_identity: identity })], source), /finesse/u);
});

test('the finesse smoke accepts the nested CLI report without a root mode', () => {
  const actualShape = outputs();
  assert.equal(Object.hasOwn(JSON.parse(actualShape[2]), 'mode'), false);
  verifyRuntimeProbeOutputs(actualShape, source);
});

test('a flattened mode cannot substitute for a missing or wrong finesse report', () => {
  for (const report of [undefined, null, 'search', {}, { mode: 'score' }, { mode: null }]) {
    const bad = outputs();
    bad[2] = JSON.stringify({ runtime_identity: identity, mode: 'search', finesse_report: report });
    assert.throws(() => verifyRuntimeProbeOutputs(bad, source), /finesse_report\.mode/u);
  }
});

test('host Ubuntu or the wrong architecture cannot stand in for the deployment baseline', () => {
  const baseline = { platform: 'linux', arch: 'x64', osRelease: 'ID=debian\nVERSION_CODENAME=bookworm\n' };
  assertBookwormRuntime(baseline);
  for (const changed of [{ arch: 'arm64' }, { platform: 'win32' },
    { osRelease: 'ID=ubuntu\nVERSION_CODENAME=noble\n' },
    { osRelease: 'ID=debian\nVERSION_CODENAME=bullseye\n' }]) {
    assert.throws(() => assertBookwormRuntime({ ...baseline, ...changed }), /Bookworm/u);
  }
});

test('parallel release CLI uses the Cloud compiler baseline and then the slim runtime, without a second compiler', async () => {
  const workflow = await readFile(new URL('../../.github/workflows/release-cli.yml', import.meta.url), 'utf8');
  const dockerfile = await readFile(new URL('../../apps/clearra-discord-bot/Dockerfile.current-job-service', import.meta.url), 'utf8');
  const job = workflow.split('  linux-cli:')[1].split('  discord-bot:')[0];
  const builder = dockerfile.match(/^FROM (\S+) AS clearra-build$/mu)?.[1];
  const runtime = dockerfile.match(/^FROM (\S+) AS runtime$/mu)?.[1];
  assert.equal(builder, 'rust:1.96-bookworm');
  assert.equal(runtime, 'node:22-bookworm-slim');
  assert.ok(job.includes(`container: ${builder}`));
  assert.ok(job.includes(`uses: docker://${runtime}`));
  assert.match(job, /needs: \[metadata, ctk3\]/u);
  assert.equal((job.match(/bash scripts\/tools\/package-release-cli.sh/gu) ?? []).length, 1);
  const script = await readFile(new URL('./verify-linux-cli-runtime.mjs', import.meta.url), 'utf8');
  assert.doesNotMatch(script, /spawn.*cargo|execFile.*cargo|cargo build/u);
});
