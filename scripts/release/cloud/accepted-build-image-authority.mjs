#!/usr/bin/env node
// v2 is opt-in for the accepted-product route; retain the historical source-only
// authority reader unchanged so prior release/recovery evidence remains valid.
import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';
import { canonicalJson, sealCanonicalReport, verifyCanonicalReportHash } from '../canonical-release-evidence.mjs';
import { createCloudBuildImageAuthority } from './cloud-build-image-authority-v080.mjs';
import { sha256File, verifyAcceptedCloudInputs } from './accepted-build-inputs.mjs';

export const ACCEPTED_BUILD_AUTHORITY = 'clearra.cloud-build-image-authority.v2';

export async function createAcceptedBuildImageAuthority(options) {
  const manifest = await verifyAcceptedCloudInputs(options.inputDirectory, options);
  const legacy = { ...await createCloudBuildImageAuthority(options) };
  delete legacy.report_sha256;
  const sourceEntry = manifest.files.find((entry) => entry.path === 'exact-source.tar.gz');
  if (sourceEntry.sha256 !== legacy.exact_source_archive_sha256) {
    throw new Error('Cloud transport exact source differs from the Oracle source archive');
  }
  const cliHash = manifest.files.find((entry) => entry.path === 'clearra').sha256;
  const build = JSON.parse(await readFile(options.buildReadbackPath, 'utf8'));
  const expected = { _INPUT_MANIFEST_SHA256: options.manifestSha256, _CLI_SHA256: cliHash,
    _PRODUCT_VERSION: manifest.version, _ACCEPTED_RUN_ID: options.runId, _ACCEPTED_RUN_ATTEMPT: options.runAttempt };
  for (const [key, value] of Object.entries(expected)) {
    if (build.substitutions?.[key] !== value) throw new Error(`Cloud packaging substitution differs: ${key}`);
  }
  if (!Array.isArray(build.steps) || build.steps.length !== 2 ||
      build.steps[0]?.id !== 'verify-accepted-inputs' || build.steps[0]?.name !== 'node:22-bookworm-slim' ||
      build.steps[1]?.id !== 'package-accepted-runtime' || build.steps[1]?.name !== 'gcr.io/cloud-builders/docker' ||
      build.steps.some((step) => step.status !== 'SUCCESS' || step.allowFailure === true ||
        (step.allowExitCodes?.length ?? 0) !== 0) ||
      build.steps[1]?.args?.[0] !== 'build' || build.steps[1]?.args?.[1] !== '-f' ||
      build.steps[1]?.args?.[2] !== 'source/apps/clearra-discord-bot/Dockerfile.accepted-job-service') {
    throw new Error('Cloud packaging must verify inputs and package successfully without a source-build fallback');
  }
  const archiveHash = await sha256File(options.inputArchivePath);
  verifyTransportProvenance(build, archiveHash);
  return sealCanonicalReport({ ...legacy, schema_id: ACCEPTED_BUILD_AUTHORITY,
    accepted_run_id: options.runId, accepted_run_attempt: options.runAttempt,
    cloud_input_archive_sha256: archiveHash, cloud_input_manifest_sha256: options.manifestSha256,
    accepted_linux_cli_sha256: cliHash,
    accepted_ctk3_manifest_sha256: manifest.files.find((file) => file.path === 'ctk3/clearra-accepted-ctk3.v2.json').sha256,
    resolved_storage_source: build.sourceProvenance.resolvedStorageSource,
    product_rebuild_count: 0,
  });
}

export function verifyTransportProvenance(build, archiveHash) {
  const source = build.sourceProvenance?.resolvedStorageSource;
  if (!source || !/^[1-9][0-9]*$/u.test(String(source.generation ?? '')) ||
      !build.options?.sourceProvenanceHash?.includes('SHA256')) {
    throw new Error('Cloud packaging requires resolved source generation and SHA256 provenance');
  }
  const hashes = build.sourceProvenance?.fileHashes;
  const name = `gs://${source.bucket}/${source.object}#${source.generation}`;
  if (!hashes || Object.keys(hashes).length !== 1 || !Array.isArray(hashes[name]?.fileHash)) {
    throw new Error('Cloud input archive provenance path differs');
  }
  const sha256 = hashes[name].fileHash.filter((hash) => hash.type === 'SHA256');
  if (!/^[0-9a-f]{64}$/u.test(archiveHash) || sha256.length !== 1 ||
      sha256[0].value !== Buffer.from(archiveHash, 'hex').toString('base64')) {
    throw new Error('Cloud fetched archive SHA-256 differs from the sealed local transport');
  }
}

export async function verifyAcceptedBuildImageAuthority(reportPath, options) {
  await sha256File(reportPath); // Reject links before reading the report.
  const report = JSON.parse(await readFile(reportPath, 'utf8'));
  verifyCanonicalReportHash(report, 'accepted Cloud image authority');
  const actual = await createAcceptedBuildImageAuthority({ ...options, manifestSha256: report.cloud_input_manifest_sha256 });
  if (canonicalJson(actual) !== canonicalJson(report)) throw new Error('Accepted Cloud image authority differs');
  return actual;
}

async function main() {
  const { values, positionals } = parseArgs({ strict: true, allowPositionals: true, options: Object.fromEntries([
    'source-commit', 'project', 'exact-source-archive', 'build-readback', 'input-directory',
    'input-archive', 'manifest-sha256', 'run-id', 'run-attempt', 'output', 'report',
  ].map((key) => [key, { type: 'string' }])) });
  const options = { sourceCommit: values['source-commit'], projectId: values.project,
    exactSourceArchivePath: values['exact-source-archive'], buildReadbackPath: values['build-readback'],
    inputDirectory: values['input-directory'], inputArchivePath: values['input-archive'],
    manifestSha256: values['manifest-sha256'], runId: values['run-id'], runAttempt: values['run-attempt'] };
  if (positionals.length !== 1) throw new Error('Use create or verify for accepted Cloud image authority');
  if (positionals[0] === 'create') {
    const report = await createAcceptedBuildImageAuthority(options);
    await writeFile(values.output, `${canonicalJson(report)}\n`, { flag: 'wx', mode: 0o600 });
    console.log(`${ACCEPTED_BUILD_AUTHORITY} ${report.report_sha256}`);
  } else if (positionals[0] === 'verify') {
    await verifyAcceptedBuildImageAuthority(values.report, options);
    console.log('accepted_cloud_image_authority=verified');
  } else throw new Error('Use create or verify for accepted Cloud image authority');
}

if (resolve(process.argv[1] ?? '') === fileURLToPath(import.meta.url)) {
  main().catch((error) => { console.error(`accepted_cloud_image_authority=failed reason=${error.message}`); process.exitCode = 2; });
}
