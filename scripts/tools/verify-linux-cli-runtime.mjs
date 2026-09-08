#!/usr/bin/env node
// Run the already packaged CLI in the deployment's slim base image. This is a
// runtime/ABI smoke, not another Cargo build or an independent release authority.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { lstat, readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const BOOKWORM_CLI_PROBES = Object.freeze([
  Object.freeze(['--format', 'json', 'rules', 'list']),
  Object.freeze(['--format', 'json', 'pc', '--lines', '2', '--queue', 'IJLOO', '--fixed', '--no-hold']),
  Object.freeze(['finesse', 'search', '--base-mask', '0', '--target-mask', '0xf',
    '--height', '1', '--queue', 'I', '--no-hold', '--pattern-knowledge', 'oracle',
    '--rule', 'srs-plus', '--workers', '1', '--format', 'json']),
]);

export function assertBookwormRuntime({ platform, arch, osRelease }) {
  if (platform !== 'linux' || arch !== 'x64' ||
      !/^ID=debian$/mu.test(osRelease) || !/^VERSION_CODENAME=bookworm$/mu.test(osRelease)) {
    throw new Error('CLI runtime smoke requires the x86_64 Debian Bookworm deployment baseline');
  }
}

export function verifyRuntimeProbeOutputs(outputs, sourceCommit) {
  if (!/^[0-9a-f]{40}$/u.test(sourceCommit ?? '') ||
      !Array.isArray(outputs) || outputs.length !== BOOKWORM_CLI_PROBES.length) {
    throw new Error('CLI runtime smoke requires an exact source and every probe result');
  }
  for (const output of outputs) {
    const result = JSON.parse(output);
    const expected = {
      source_commit: sourceCommit,
      engine_build_id: sourceCommit,
      contract_schema_version: 'clearra.search.contract.v2',
      supply_semantics_id: 'clearra.supply.projected-terminal-lookahead.v1',
      artifact_schema_version: 'clearra.solution-data.v1',
    };
    for (const [key, value] of Object.entries(expected)) {
      if (result?.runtime_identity?.[key] !== value) {
        throw new Error(`CLI runtime smoke identity differs: ${key}`);
      }
    }
  }
  if (JSON.parse(outputs[2])?.mode !== 'search') {
    throw new Error('Bookworm runtime did not execute the finesse search backend');
  }
}

async function main(args) {
  if (args.length !== 4 || args[0] !== '--version' || args[2] !== '--source-commit' ||
      !/^\d+\.\d+\.\d+(?:[-.][0-9A-Za-z.-]+)?$/u.test(args[1]) ||
      !/^[0-9a-f]{40}$/u.test(args[3])) {
    throw new Error('usage: verify-linux-cli-runtime.mjs --version VERSION --source-commit SHA');
  }
  assertBookwormRuntime({ platform: process.platform, arch: process.arch,
    osRelease: await readFile('/etc/os-release', 'utf8') });
  const executable = resolve(`dist/Clearra-CLI-v${args[1]}-linux-x86_64`);
  const metadata = await lstat(executable);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size < 1) {
    throw new Error('Bookworm CLI input must be the nonempty packaged regular file');
  }
  const digest = async () => createHash('sha256').update(await readFile(executable)).digest('hex');
  const before = await digest();
  const outputs = BOOKWORM_CLI_PROBES.map((probe) => execFileSync(executable, probe, {
    encoding: 'utf8', timeout: 60_000, maxBuffer: 8 * 1024 * 1024,
  }));
  verifyRuntimeProbeOutputs(outputs, args[3]);
  if (before !== await digest()) throw new Error('Packaged CLI changed during Bookworm runtime smoke');
  console.log(`bookworm_cli_runtime=verified source_commit=${args[3]} cli_sha256=${before} probes=${outputs.length} rebuild_count=0`);
}

if (resolve(process.argv[1] ?? '') === fileURLToPath(import.meta.url)) {
  main(process.argv.slice(2)).catch((error) => {
    console.error(`bookworm_cli_runtime=failed reason=${error.message}`);
    process.exitCode = 2;
  });
}
