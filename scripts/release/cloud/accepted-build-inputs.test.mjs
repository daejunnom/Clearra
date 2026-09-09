import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { canonicalJson, sealCanonicalReport } from '../canonical-release-evidence.mjs';
import { CANONICAL_ACCEPTANCE_REQUIRED_JOB_NAMES } from '../canonical-acceptance-evidence.mjs';
import { ACCEPTED_CTK3_MANIFEST, sealAcceptedCtk3Dist } from '../../tools/accepted-ctk3-dist.mjs';
import { CLOUD_INPUT_MANIFEST, createAcceptedCloudInputs, verifyAcceptedCloudInputs, sha256File } from './accepted-build-inputs.mjs';
import { createAcceptedBuildImageAuthority, verifyAcceptedBuildImageAuthority, verifyTransportProvenance } from './accepted-build-image-authority.mjs';

const SOURCE = '1'.repeat(40);
const PROJECT = 'clearra-production';
const authority = { repository: 'daejunnom/Clearra', version: '0.8.0', basePath: '/Clearra', sourceCommit: SOURCE, runId: '42', runAttempt: '1' };
const hash = (value) => createHash('sha256').update(value).digest('hex');
const rootUrl = new URL('../../../', import.meta.url);
const config = (await readFile(new URL('apps/clearra-discord-bot/cloudbuild-accepted-job-service.yaml', rootUrl), 'utf8')).replaceAll('\r\n', '\n');
const bootstrap = config.split('      - |\n')[1].split('\n      - ${_INPUT_MANIFEST_SHA256}')[0]
  .replace(/^        /gmu, '').replaceAll('$$', '$');

async function fixture(t) {
  const root = await mkdtemp(join(tmpdir(), 'clearra-accepted-cloud-test-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const ctk = join(root, 'accepted-ctk3');
  const cli = join(root, 'linux-cli');
  await mkdir(ctk); await mkdir(cli);
  for (const name of ['decodeWorker.js', 'index.cjs', 'index.d.ts', 'index.js']) await writeFile(join(ctk, name), `fixture:${name}`);
  const ctkManifest = await sealAcceptedCtk3Dist(ctk, SOURCE, '42', '1');
  const cliBytes = Buffer.from('synthetic CLI bytes, never executable');
  await writeFile(join(cli, 'Clearra-CLI-v0.8.0-linux-x86_64'), cliBytes);
  const evidence = sealCanonicalReport({
    schema_id: 'clearra.canonical-acceptance-evidence.v1', repository: authority.repository,
    release_version: authority.version, pages_base_path: authority.basePath, source_commit: SOURCE,
    run_id: '42', run_attempt: '1', workflow_path: '.github/workflows/release-cli.yml', status: 'passed',
    jobs: CANONICAL_ACCEPTANCE_REQUIRED_JOB_NAMES.map((name, i) => ({name, job_id: String(i + 1), status: 'passed'})),
    accepted_inputs: { ctk3_manifest_sha256: hash(canonicalJson(ctkManifest)), pages_identity_sha256: 'a'.repeat(64),
      wasm_build_receipt_sha256: 'b'.repeat(64), gate_index_sha256: 'c'.repeat(64) },
    final_source_fragments: { toolchains: {source_commit: SOURCE}, canonical_gate: {source_commit: SOURCE, status: 'passed', readiness_open_count: 0},
      surface_reports: ['desktop', 'discord', 'native', 'wasm'].map((surface) => ({surface, source_commit: SOURCE, status: 'passed', sha256: 'd'.repeat(64)})),
      release_artifacts: ['linux-cli', 'windows-cli', 'windows-gui'].map((role) => ({role, source_commit: SOURCE,
        name: role === 'linux-cli' ? 'Clearra-CLI-v0.8.0-linux-x86_64' : role, size_bytes: cliBytes.length, sha256: hash(cliBytes)})) },
  });
  const evidencePath = join(root, 'canonical.json');
  const sourcePath = join(root, 'exact-source.tar.gz');
  await writeFile(evidencePath, `${canonicalJson(evidence)}\n`);
  await writeFile(sourcePath, 'synthetic exact source archive');
  const options = { ...authority, canonicalAcceptanceEvidencePath: evidencePath, acceptedCtk3DistPath: ctk,
    linuxCliDirectory: cli, exactSourceArchivePath: sourcePath, outputDirectory: join(root, 'inputs') };
  return { root, options, evidence, ctk, cli, evidencePath, sourcePath };
}

async function sealedFixture(t) {
  const f = await fixture(t);
  const {manifest, manifestSha256} = await createAcceptedCloudInputs(f.options);
  return {...f, manifest, manifestSha256, expected: {...authority, manifestSha256}};
}

test('copies exactly accepted CLI and CTK3 bytes while preserving the independent source archive', async (t) => {
  const f = await sealedFixture(t);
  assert.equal(await sha256File(f.sourcePath), f.manifest.files.find((e) => e.path === 'exact-source.tar.gz').sha256);
  assert.equal(await readFile(join(f.root, 'inputs/clearra'), 'utf8'), 'synthetic CLI bytes, never executable');
  assert.deepEqual(await verifyAcceptedCloudInputs(f.options.outputDirectory, f.expected), f.manifest);
  await assert.rejects(createAcceptedCloudInputs(f.options), /exist/u);
});

test('the same source from a different acceptance run, attempt, or identity is rejected', async (t) => {
  const f = await sealedFixture(t);
  for (const change of [{sourceCommit: '2'.repeat(40)}, {runId: '43'}, {runAttempt: '2'}, {manifestSha256: 'a'.repeat(64)}]) {
    await assert.rejects(verifyAcceptedCloudInputs(f.options.outputDirectory, {...f.expected, ...change}));
  }
});

for (const target of ['clearra', 'exact-source.tar.gz', 'canonical-acceptance.json', 'ctk3/index.js', `ctk3/${ACCEPTED_CTK3_MANIFEST}`]) {
  test(`post-download tampering is rejected: ${target}`, async (t) => {
    const f = await sealedFixture(t);
    await writeFile(join(f.root, 'inputs', target), 'tampered');
    await assert.rejects(verifyAcceptedCloudInputs(f.options.outputDirectory, f.expected), /payload differs/u);
  });
}

test('resealing the transport cannot make a different CLI become accepted', async (t) => {
  const f = await sealedFixture(t);
  await writeFile(join(f.root, 'inputs/clearra'), 'different CLI');
  const entry = f.manifest.files.find((file) => file.path === 'clearra');
  entry.sha256 = hash('different CLI'); entry.size_bytes = Buffer.byteLength('different CLI');
  const path = join(f.root, 'inputs', CLOUD_INPUT_MANIFEST);
  await writeFile(path, `${canonicalJson(f.manifest)}\n`);
  await assert.rejects(verifyAcceptedCloudInputs(f.options.outputDirectory, {...f.expected, manifestSha256: await sha256File(path)}), /canonical acceptance/u);
});

test('a valid CTK3 seal is insufficient unless canonical acceptance binds that exact manifest', async (t) => {
  const f = await fixture(t);
  await rm(join(f.ctk, ACCEPTED_CTK3_MANIFEST));
  await writeFile(join(f.ctk, 'index.js'), 'changed CTK3');
  await sealAcceptedCtk3Dist(f.ctk, SOURCE, '42', '1');
  await assert.rejects(createAcceptedCloudInputs(f.options), /CTK3 manifest differs/u);
});

test('extra input files, CLI artifacts, and linked input directories fail closed', async (t) => {
  const f = await sealedFixture(t);
  await writeFile(join(f.root, 'inputs/unexpected.txt'), 'unexpected');
  await assert.rejects(verifyAcceptedCloudInputs(f.options.outputDirectory, f.expected), /Unexpected/u);
  await writeFile(join(f.cli, 'other-cli'), 'other');
  await assert.rejects(createAcceptedCloudInputs({...f.options, outputDirectory: join(f.root, 'new-inputs')}), /file set differs/u);
  const linked = join(f.root, 'linked');
  await symlink(f.options.outputDirectory, linked, 'junction');
  await assert.rejects(verifyAcceptedCloudInputs(linked, f.expected), /link/u);
});

async function buildFixture(t) {
  const f = await sealedFixture(t);
  const inputArchivePath = join(f.root, 'transport.tar.gz');
  await writeFile(inputArchivePath, 'sealed transport bytes');
  const archiveHash = await sha256File(inputArchivePath);
  const tag = `asia-northeast1-docker.pkg.dev/${PROJECT}/clearra/clearra-current-job:source-${SOURCE}`;
  const build = { id: '12345678-1234-4234-8234-123456789abc', projectId: PROJECT, status: 'SUCCESS',
    substitutions: { _IMAGE_NAME: 'clearra-current-job', _REGION: 'asia-northeast1', _REPOSITORY: 'clearra', _SOURCE_COMMIT: SOURCE, _TAG: `source-${SOURCE}`,
      _PRODUCT_VERSION: '0.8.0', _ACCEPTED_RUN_ID: '42', _ACCEPTED_RUN_ATTEMPT: '1', _INPUT_MANIFEST_SHA256: f.manifestSha256,
      _CLI_SHA256: f.manifest.files.find((e) => e.path === 'clearra').sha256 },
    images: [tag], results: {images: [{name: tag, digest: `sha256:${'a'.repeat(64)}`}]},
    options: {sourceProvenanceHash: ['SHA256']},
    sourceProvenance: {resolvedStorageSource: {bucket: 'clearra-cloud_cloudbuild', object: 'source/transport.tgz', generation: '7'},
      fileHashes: {'gs://clearra-cloud_cloudbuild/source/transport.tgz#7': {fileHash: [{type: 'SHA256', value: Buffer.from(archiveHash, 'hex').toString('base64')}]}}},
    steps: [{id: 'verify-accepted-inputs', name: 'node:22-bookworm-slim', status: 'SUCCESS'},
      {id: 'package-accepted-runtime', name: 'gcr.io/cloud-builders/docker', status: 'SUCCESS', args: ['build', '-f', 'source/apps/clearra-discord-bot/Dockerfile.accepted-job-service']}] };
  const buildReadbackPath = join(f.root, 'build.json');
  await writeFile(buildReadbackPath, JSON.stringify(build));
  return {...f, build, archiveHash, buildOptions: {...f.expected, projectId: PROJECT, exactSourceArchivePath: f.sourcePath,
    inputDirectory: f.options.outputDirectory, inputArchivePath, buildReadbackPath,
    expectedStorageSource: {...build.sourceProvenance.resolvedStorageSource}}};
}

test('v2 binds accepted inputs, actual fetched transport, generation and immutable image; approval rechecks all bytes', async (t) => {
  const f = await buildFixture(t);
  const report = await createAcceptedBuildImageAuthority(f.buildOptions);
  assert.equal(report.schema_id, 'clearra.cloud-build-image-authority.v2');
  assert.equal(report.product_rebuild_count, 0);
  assert.equal(report.cloud_input_archive_sha256, f.archiveHash);
  const path = join(f.root, 'authority.json');
  await writeFile(path, `${canonicalJson(report)}\n`);
  await verifyAcceptedBuildImageAuthority(path, f.buildOptions);
  await writeFile(f.buildOptions.inputArchivePath, 'changed after approval');
  await assert.rejects(verifyAcceptedBuildImageAuthority(path, f.buildOptions), /fetched archive/u);
});

for (const [label, mutate] of [
  ['missing provenance', (b) => {delete b.sourceProvenance.fileHashes;}],
  ['wrong archive hash', (b) => {Object.values(b.sourceProvenance.fileHashes)[0].fileHash[0].value = Buffer.alloc(32).toString('base64');}],
  ['wrong storage path', (b) => {b.sourceProvenance.resolvedStorageSource.object = 'other.tgz';}],
  ['wrong storage generation', (b) => {b.sourceProvenance.resolvedStorageSource.generation = '8';}],
  ['unresolved generation', (b) => {delete b.sourceProvenance.resolvedStorageSource.generation;}],
  ['hash not requested', (b) => {b.options.sourceProvenanceHash = [];}],
  ['different accepted run', (b) => {b.substitutions._ACCEPTED_RUN_ID = '43';}],
  ['masked verification failure', (b) => {b.steps[0].allowFailure = true;}],
  ['missing verification', (b) => {b.steps.shift();}],
  ['source-build fallback', (b) => {b.steps[1].args[2] = 'source/apps/clearra-discord-bot/Dockerfile.current-job-service';}],
]) {
  test(`Cloud authority rejects ${label}`, async (t) => {
    const f = await buildFixture(t); mutate(f.build);
    await writeFile(f.buildOptions.buildReadbackPath, JSON.stringify(f.build));
    await assert.rejects(createAcceptedBuildImageAuthority(f.buildOptions));
  });
}

test('provenance accepts only the generation-qualified path and exact base64 SHA256 of the uploaded archive', () => {
  const b = { options: {sourceProvenanceHash: ['SHA256']}, sourceProvenance: {resolvedStorageSource: {bucket: 'bucket', object: 'source.tgz', generation: '42'},
    fileHashes: {'gs://bucket/source.tgz#42': {fileHash: [{type: 'SHA256', value: Buffer.from('a'.repeat(64), 'hex').toString('base64')}]}}} };
  const expected = {...b.sourceProvenance.resolvedStorageSource};
  verifyTransportProvenance(b, 'a'.repeat(64), expected);
  b.sourceProvenance.fileHashes['gs://bucket/source.tgz#42'].fileHash.push({...b.sourceProvenance.fileHashes['gs://bucket/source.tgz#42'].fileHash[0]});
  assert.throws(() => verifyTransportProvenance(b, 'a'.repeat(64), expected), /SHA-256 differs/u);
});

test('the real Cloud bootstrap verifies the source before extraction and executes the real input verifier', async (t) => {
  const f = await fixture(t);
  const snapshot = join(f.root, 'snapshot'); await mkdir(snapshot);
  const visited = new Set();
  async function copyModule(url) {
    if (visited.has(url.href)) return;
    visited.add(url.href);
    const text = await readFile(url, 'utf8');
    const relative = fileURLToPath(url).slice(fileURLToPath(rootUrl).length);
    const path = join(snapshot, relative); await mkdir(dirname(path), {recursive: true}); await writeFile(path, text);
    for (const match of text.matchAll(/\bfrom\s+['"](\.[^'"]+\.mjs)['"]/gu)) await copyModule(new URL(match[1], url));
  }
  await copyModule(new URL('scripts/release/cloud/accepted-build-inputs.mjs', rootUrl));
  const packed = spawnSync('tar', ['-czf', f.sourcePath, '-C', snapshot, '.'], {encoding:'utf8', windowsHide:true});
  assert.equal(packed.status, 0, packed.stderr);
  const {manifestSha256} = await createAcceptedCloudInputs(f.options);
  const run = (manifestHash = manifestSha256) => spawnSync(process.execPath, ['--input-type=module', '-e', bootstrap, manifestHash, SOURCE, '42', '1'],
    {cwd:f.root, encoding:'utf8', timeout:30_000, windowsHide:true});
  const bad = run('0'.repeat(64)); assert.notEqual(bad.status, 0); assert.match(bad.stderr, /manifest hash mismatch/u);
  const result = run(); assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /cloud_inputs=verified rebuild_count=0/u);
  assert.notEqual(run().status, 0, 'source extraction cannot overwrite an existing directory');
});

function assertPackagingFlow(workflow, cloudConfig, dockerfile) {
  const candidate = workflow.split('\n  candidate:')[1].split('\n  promote:')[0];
  const download = candidate.split('      - name: Download the already accepted Linux CLI\n')[1]?.split('\n      - name:')[0] ?? '';
  for (const line of [
    '        uses: actions/download-artifact@v4',
    '          name: clearra-linux-cli-v${{ steps.accepted-inputs.outputs.release_version }}-run-${{ needs.authority.outputs.accepted_run_id }}-attempt-${{ needs.authority.outputs.accepted_run_attempt }}',
    '          path: evidence/accepted-linux-cli',
    '          github-token: ${{ github.token }}',
    '          repository: ${{ github.repository }}',
    '          run-id: ${{ needs.authority.outputs.accepted_run_id }}',
  ]) assert.ok(download.split('\n').includes(line), line);
  assert.doesNotMatch(download, /pattern:|continue-on-error|merge-multiple:/u);
  assert.match(candidate, /gcloud storage cp evidence\/cloud-build-inputs\.tar\.gz "\$cloud_input_object"[\s\S]*--if-generation-match=0/u);
  assert.match(candidate, /gcloud storage objects describe "\$cloud_input_object" --format='value\(generation\)'/u);
  assert.match(candidate, /gcloud builds submit "\$cloud_input_object"/u);
  assert.match(candidate, /--storage-source-uri "\$cloud_input_object"[\s\S]*--storage-source-generation "\$cloud_input_generation"/u);
  assert.match(candidate, /--config=.*cloudbuild-accepted-job-service\.yaml/u);
  assert.match(candidate, /tar -czf evidence\/cloud-build-inputs\.tar\.gz -C evidence\/cloud-build-inputs inputs/u);
  assert.match(candidate, /accepted-build-inputs\.mjs create/u);
  assert.match(candidate, /accepted-build-image-authority\.mjs create/u);
  assert.doesNotMatch(candidate, /cargo build|npm run build|cloudbuild-current-job-service|continue-on-error/u);
  const promote = workflow.split('\n  promote:')[1].split('\n  sync-observe:')[0];
  assert.ok(promote.indexOf('accepted-build-image-authority.mjs verify') > promote.indexOf('prepared artifact verification failed'));
  assert.ok(promote.indexOf('accepted Cloud product authority verification failed') < promote.indexOf('invoke-freeze-v080.ps1'));
  assert.match(cloudConfig, /sourceProvenanceHash: \[SHA256\]/u);
  assert.match(cloudConfig, /source\/apps\/clearra-discord-bot\/Dockerfile\.accepted-job-service/u);
  assert.doesNotMatch(cloudConfig, /allowFailure|allowExitCodes|cargo build|npm run build|Dockerfile\.current-job-service/u);
  assert.equal((cloudConfig.match(/^  - id:/gmu) ?? []).length, 2);
  const from = [...dockerfile.matchAll(/^FROM (\S+) AS /gmu)].map((match) => match[1]);
  assert.deepEqual(from, ['node:22-bookworm-slim', 'node:22-bookworm-slim']);
  assert.doesNotMatch(dockerfile, /cargo|rustup|npm run build|npm exec|npx|--from=clearra-build/u);
  assert.match(dockerfile, /RUN npm ci --omit=dev --ignore-scripts/u);
  assert.match(dockerfile, /COPY inputs\/ctk3\/ .\/packages\/ctk3\/dist\//u);
  assert.match(dockerfile, /COPY inputs\/clearra \/usr\/local\/bin\/clearra/u);
  assert.match(dockerfile, /verify-linux-cli-runtime\.mjs/u);
  assert.match(dockerfile, /finesse_report\?\.mode!=='score'/u);
  assert.equal((dockerfile.match(/sha256sum --check --strict/gu) ?? []).length, 2);
  assert.match(dockerfile, /^USER node$/mu);
}

test('production only packages exact-run accepted products and verifies the handoff before runtime mutation', async () => {
  const read = async (path) => (await readFile(new URL(path, rootUrl), 'utf8')).replaceAll('\r\n', '\n');
  const workflow = await read('.github/workflows/discord-deploy.yml');
  const dockerfile = await read('apps/clearra-discord-bot/Dockerfile.accepted-job-service');
  assertPackagingFlow(workflow, config, dockerfile);
  for (const changed of [
    workflow.replace('name: clearra-linux-cli-v${{ steps.accepted-inputs.outputs.release_version }}', 'name: clearra-linux-cli-v0.8.0'),
    workflow.replace('gcloud builds submit "$cloud_input_object"', 'gcloud builds submit evidence/exact-source.tar.gz'),
    workflow.replace('accepted-build-image-authority.mjs verify', 'echo skip-verification'),
  ]) assert.throws(() => assertPackagingFlow(changed, config, dockerfile));
  assert.throws(() => assertPackagingFlow(workflow, config.replace('sourceProvenanceHash: [SHA256]', ''), dockerfile));
  assert.throws(() => assertPackagingFlow(workflow, config, dockerfile + '\nRUN cargo build\n'));
  assert.throws(() => assertPackagingFlow(workflow, config, dockerfile.replace('--omit=dev --ignore-scripts', '--omit=dev')));
  assert.throws(() => assertPackagingFlow(workflow, config + '\n  allowFailure: true\n', dockerfile));
  const recovery = await read('scripts/release/discord-deployment-recovery.mjs');
  for (const name of ['Build the exact source archive once in Cloud Build', 'Download the already accepted Linux CLI',
    'Seal accepted Cloud build inputs without recompilation', 'Package accepted products in Cloud Build without recompilation']) assert.ok(recovery.includes(`"${name}"`));
  const standalone = await read('apps/clearra-discord-bot/Dockerfile.current-job-service');
  assert.match(standalone, /^FROM rust:1\.96-bookworm AS clearra-build$/mu);
  const release = await read('.github/workflows/release-cli.yml');
  assert.match(release.split('\n  linux-cli:')[1].split('\n  discord-bot:')[0], /container: rust:1\.96-bookworm/u);
});
