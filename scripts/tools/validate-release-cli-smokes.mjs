import { readFile } from 'node:fs/promises';

const TERMINAL_SUPPLY_MODE = '--validate-terminal-supply-json';
const TERMINAL_SUPPLY_EXPECTED_COUNT = 18;
const TERMINAL_SUPPLY_EXPECTED_HASH = 'cts1:8a7fc484d9b49994';
const TERMINAL_SUPPLY_INITIAL_MASK = 0x1c0701c07n;
const TERMINAL_SUPPLY_FULL_FIELD = (1n << 40n) - 1n;
const CANONICAL_PIECE_ORDER = Object.freeze(['I', 'O', 'T', 'S', 'Z', 'J', 'L']);
const FNV_OFFSET = 0xcbf29ce484222325n;
const FNV_PRIME = 0x100000001b3n;
const U64_MASK = (1n << 64n) - 1n;

const commandArguments = process.argv.slice(2);
if (commandArguments.length > 0) {
  if (
    commandArguments.length !== 3 ||
    commandArguments[0] !== TERMINAL_SUPPLY_MODE ||
    commandArguments[1] !== '--expected-source-commit' ||
    !/^[0-9a-f]{40}$/u.test(commandArguments[2])
  ) {
    throw new Error(`unsupported release CLI smoke validator arguments: ${commandArguments.join(' ')}`);
  }
  const raw = await readStandardInput();
  const evidence = validateTerminalSupplyJson(raw, commandArguments[2]);
  console.log(
    `Terminal-supply release asset JSON passed: solutions=${evidence.solutionCount} hash=${evidence.normalizedSetHash}`
  );
  process.exit(0);
}

const root = new URL('../../', import.meta.url);
const packageScript = await readFile(new URL('scripts/tools/package-release-cli.sh', root), 'utf8');
const workflow = await readFile(new URL('.github/workflows/release-cli.yml', root), 'utf8');

const linuxJob = section(workflow, '\n  linux-cli:', '\n  discord-bot:');
const windowsJob = section(workflow, '\n  windows-products:', '\n  publish:');
const publishJob = workflow.slice(workflow.indexOf('\n  publish:'));

requireText(packageScript, '--features wasm-cpu-runtime,webgpu-search', 'Linux publish features');
requireText(packageScript, 'CLEARRA_SOURCE_COMMIT', 'Linux source commit identity');
requireText(packageScript, 'CLEARRA_ENGINE_BUILD_ID', 'Linux engine build identity');
requireText(packageScript, 'json="$("$RELEASE_BINARY" "$@")"', 'installed Linux asset execution');
for (const smoke of ['rules', 'rules-export-srs-x', 'solver', 'pc-tiling', 'failed-queue', 'build-probability', 'pc-srs-x', 'terminal-supply-p0']) {
  requireText(packageScript, `run_json_smoke ${smoke}`, `Linux ${smoke} smoke`);
}
for (const semanticMarker of [
  '"actual_backend":"wasm-cpu-build-probability"',
  '"probability_calculated":true',
  '"rule_profile":"srs-x"',
  '"effective_kick_model":"srs-x"',
  '"coverage_probability":"not-calculated"',
  '"probability_calculated":false',
  '"probability_complete":false',
  '"supply_probability_complete":false',
  '"resource_probability_complete":false'
]) {
  requireText(packageScript, semanticMarker, `Linux semantic assertion ${semanticMarker}`);
}
for (const marker of [
  'embedded?.id !== "srs-x"',
  'embedded?.source_rule !== "srs-x"',
  'embedded?.entries?.length !== 80',
  'halfTurnCount !== 24',
  '{"action":"export","profile":"srs-x"}'
]) {
  requireText(packageScript, marker, `Linux SRS-X export assertion ${marker}`);
}
requireText(
  packageScript,
  '"resource_report":{"truncated":false,"truncation_reason":null,"probability_complete":false}',
  'Linux tiling non-calculation resource assertion'
);
const linuxTerminalSmoke = section(
  packageScript,
  '\nrun_json_smoke terminal-supply-p0',
  "\nprintf 'cli_release_binary="
);
requireTerminalSupplySmoke(linuxTerminalSmoke, 'Linux');
requireText(
  packageScript,
  'node "$ROOT/scripts/tools/validate-release-cli-smokes.mjs"',
  'Linux terminal-supply executable validator invocation'
);
requireText(packageScript, TERMINAL_SUPPLY_MODE, 'Linux terminal-supply validator mode');
requireText(
  packageScript,
  '--expected-source-commit "$CLEARRA_SOURCE_COMMIT"',
  'Linux terminal-supply expected source identity'
);
for (const marker of [
  '--format json --include-solution-data pc-scenario',
  '--field 0x1c0701c07 --visible-height 4 --queue STOILJZ',
  '--max-pieces 7 --exact-pieces 7',
  '--backend cpu --workers 1',
  '"unique_solution_count":18',
  '"normalized_solution_set_hash":"cts1:8a7fc484d9b49994"'
]) {
  requireText(linuxTerminalSmoke, marker, `Linux terminal-supply semantic assertion ${marker}`);
}

requireText(linuxJob, 'bash scripts/tools/package-release-cli.sh', 'Linux packaged CLI job');
requireText(linuxJob, 'actions/setup-node@v4', 'Linux JSON validator Node setup');
requireText(linuxJob, 'node-version: 22', 'Linux JSON validator Node version');
requireText(windowsJob, '--features wasm-cpu-runtime,webgpu-search', 'Windows publish features');
requireText(windowsJob, '$identity.source_commit -ne $env:GITHUB_SHA', 'Windows source identity');
requireText(windowsJob, '$identity.engine_build_id -ne $env:GITHUB_SHA', 'Windows engine identity');
requireText(
  windowsJob,
  '--expected-source-commit $env:GITHUB_SHA',
  'Windows terminal-supply expected source identity'
);
for (const smoke of ['rules', 'rules-export-srs-x', 'solver', 'pc-tiling', 'failed-queue', 'build-probability', 'pc-srs-x', 'terminal-supply-p0']) {
  requireText(windowsJob, `-Name '${smoke}'`, `Windows ${smoke} smoke`);
}
for (const marker of [
  "$embedded.id -ne 'srs-x'",
  "$embedded.source_rule -ne 'srs-x'",
  '$embedded.entries.Count -ne 80',
  '$halfTurnCount -ne 24',
  "action = 'export'; profile = 'srs-x'"
]) {
  requireText(windowsJob, marker, `Windows SRS-X export assertion ${marker}`);
}
for (const semanticMarker of [
  "actual_backend = 'wasm-cpu-build-probability'",
  'probability_calculated = $true',
  "rule_profile = 'srs-x'",
  "effective_kick_model = 'srs-x'",
  "coverage_probability = 'not-calculated'",
  'probability_calculated = $false',
  'probability_complete = $false',
  'supply_probability_complete = $false',
  'resource_probability_complete = $false'
]) {
  requireText(windowsJob, semanticMarker, `Windows semantic assertion ${semanticMarker}`);
}
requireText(
  windowsJob,
  '-ExpectedResourceReport @{ truncated = $false; truncation_reason = $null; probability_complete = $false }',
  'Windows tiling non-calculation resource assertion'
);
const windowsTerminalSmoke = section(
  windowsJob,
  "\n          Invoke-ClearraJsonSmoke -Name 'terminal-supply-p0'",
  '\n      - name: Build standalone SvelteKit'
);
requireTerminalSupplySmoke(windowsTerminalSmoke, 'Windows');
requireText(
  windowsJob,
  'validate-release-cli-smokes.mjs',
  'Windows terminal-supply executable validator script'
);
requireText(windowsJob, TERMINAL_SUPPLY_MODE, 'Windows terminal-supply validator mode');
for (const marker of [
  "@('--format', 'json', '--include-solution-data', 'pc-scenario'",
  "'--field', '0x1c0701c07', '--visible-height', '4', '--queue', 'STOILJZ'",
  "'--max-pieces', '7', '--exact-pieces', '7'",
  "'--backend', 'cpu', '--workers', '1'",
  'unique_solution_count = 18',
  "normalized_solution_set_hash = 'cts1:8a7fc484d9b49994'"
]) {
  requireText(windowsTerminalSmoke, marker, `Windows terminal-supply semantic assertion ${marker}`);
}
for (const marker of [
  '$artifactDir = Join-Path $PWD.Path',
  '$cli = Join-Path $artifactDir "Clearra-CLI-v$version-windows-x86_64.exe"',
  'Copy-Item -LiteralPath $builtCli -Destination $cli -Force',
  '$jsonLines = & $cli @CommandArguments'
]) {
  requireText(windowsJob, marker, `Windows staged release asset execution ${marker}`);
}

if (/^    if:/m.test(linuxJob) || /^    if:/m.test(windowsJob)) {
  throw new Error('release CLI smoke jobs must run for both tag and workflow_dispatch events');
}
requireText(workflow, "tags: ['v*']", 'tag release trigger');
requireText(workflow, 'workflow_dispatch:', 'canonical acceptance trigger');
requireText(
  workflow,
  'node --test scripts/release/validate-release-metadata.test.mjs',
  'release metadata regression test invocation'
);
requireText(
  publishJob,
  'needs: [metadata, release-acceptance, linux-cli, windows-products, discord-bot]',
  'publication dependency on both CLI smoke jobs'
);

console.log('Release CLI smoke contract passed.');

function section(source, startMarker, endMarker) {
  const start = source.indexOf(startMarker);
  const end = source.indexOf(endMarker, start + startMarker.length);
  if (start < 0 || end < 0) {
    throw new Error(`release workflow section is missing: ${startMarker.trim()}`);
  }
  return source.slice(start, end);
}

function requireText(source, marker, description) {
  if (!source.includes(marker)) {
    throw new Error(`${description} is missing marker: ${marker}`);
  }
}

function requireTerminalSupplySmoke(source, platform) {
  for (const marker of [
    '--format',
    'json',
    '--include-solution-data',
    'pc-scenario',
    '--field',
    '0x1c0701c07',
    '--visible-height',
    '--queue',
    'STOILJZ',
    '--max-pieces',
    '--exact-pieces',
    '--count-policy',
    'count-unique',
    '--backend',
    'cpu',
    '--workers'
  ]) {
    requireText(source, marker, `${platform} terminal-supply fixture ${marker}`);
  }
  if (source.includes('--no-hold')) {
    throw new Error(`${platform} terminal-supply fixture must preserve enabled empty hold`);
  }
}

async function readStandardInput() {
  process.stdin.setEncoding('utf8');
  let raw = '';
  for await (const chunk of process.stdin) raw += chunk;
  return raw;
}

function validateTerminalSupplyJson(raw, expectedSourceCommit) {
  if (raw.trim().length === 0) {
    throw new Error('terminal-supply release asset returned empty JSON');
  }
  const response = JSON.parse(raw);
  requireEqual(response?.schema_version, 2, 'schema_version');
  requireEqual(response?.kind, 'pc-scenario', 'kind');
  const expectedIdentity = {
    source_commit: expectedSourceCommit,
    engine_build_id: expectedSourceCommit,
    contract_schema_version: 'clearra.search.contract.v2',
    supply_semantics_id: 'clearra.supply.projected-terminal-lookahead.v1',
    artifact_schema_version: 'clearra.solution-data.v1'
  };
  for (const [field, expected] of Object.entries(expectedIdentity)) {
    requireEqual(response?.runtime_identity?.[field], expected, `runtime_identity.${field}`);
  }

  const expectedSummary = {
    actual_backend: 'wasm-cpu',
    unique_solution_count: TERMINAL_SUPPLY_EXPECTED_COUNT,
    normalized_unique_solution_count: TERMINAL_SUPPLY_EXPECTED_COUNT,
    solution_count_calculated: true,
    solution_set_materialized: true,
    solution_keys_materialized_count: TERMINAL_SUPPLY_EXPECTED_COUNT,
    solution_keys_complete: true,
    count_complete: true,
    supply_window_resolution: 'projected-terminal-lookahead',
    projects_unplaced_lookahead: true,
    projects_standard_bag_lookahead: false,
    source_sequence_length: 7,
    total_possible_pattern_count: '1',
    normalized_solution_set_hash: TERMINAL_SUPPLY_EXPECTED_HASH,
    actual_normalized_solution_set_hash: TERMINAL_SUPPLY_EXPECTED_HASH
  };
  for (const [field, expected] of Object.entries(expectedSummary)) {
    requireEqual(response?.summary?.[field], expected, `summary.${field}`);
  }

  requireEqual(response?.contract?.command?.kind, 'pc-scenario', 'contract.command.kind');
  requireEqual(response?.contract?.command?.input_mode, 'inline', 'contract.command.input_mode');
  requireEqual(response?.contract?.pc?.search?.backend_selected, 'wasm-cpu', 'contract.pc.search.backend_selected');
  requireEqual(
    response?.contract?.pc?.execution_report?.replay?.trace_steps,
    7,
    'contract.pc.execution_report.replay.trace_steps'
  );
  requireEqual(response?.contract?.solution_data?.requested, true, 'contract.solution_data.requested');
  requireEqual(response?.contract?.solution_data?.status, 'complete', 'contract.solution_data.status');
  requireEqual(response?.contract?.solution_data?.reason, null, 'contract.solution_data.reason');
  requireEqual(
    response?.contract?.artifacts?.schema_version,
    'clearra.solution-data.v1',
    'contract.artifacts.schema_version'
  );

  const solutionKeys = response?.contract?.artifacts?.solution_keys;
  if (!Array.isArray(solutionKeys)) {
    throw new Error('contract.artifacts.solution_keys must be an array');
  }
  requireEqual(solutionKeys.length, TERMINAL_SUPPLY_EXPECTED_COUNT, 'solution_keys.length');
  for (let index = 0; index < solutionKeys.length; index += 1) {
    const key = solutionKeys[index];
    if (typeof key !== 'string') {
      throw new Error(`solution_keys[${index}] must be a string`);
    }
    if (index > 0 && solutionKeys[index - 1] >= key) {
      throw new Error('solution_keys must be strictly sorted and unique');
    }
    assertCanonicalTerminalSolutionKey(key);
  }

  const recomputedHash = normalizedSetHash(solutionKeys);
  requireEqual(recomputedHash, TERMINAL_SUPPLY_EXPECTED_HASH, 'independently recomputed solution hash');
  return Object.freeze({
    solutionCount: solutionKeys.length,
    normalizedSetHash: recomputedHash
  });
}

function assertCanonicalTerminalSolutionKey(key) {
  const match = /^ctk1\|initial=([0-9a-f]{16})\|placements=(.*)$/.exec(key);
  if (!match || BigInt(`0x${match[1]}`) !== TERMINAL_SUPPLY_INITIAL_MASK) {
    throw new Error(`non-canonical terminal-supply solution key: ${key}`);
  }

  const encodedPlacements = match[2].split(',');
  requireEqual(encodedPlacements.length, 7, 'canonical placement count');
  const masksByPiece = new Map(CANONICAL_PIECE_ORDER.map((piece) => [piece, []]));
  for (const encoded of encodedPlacements) {
    const placement = /^([IOTSZJL]):([0-9a-f]{16})$/.exec(encoded);
    if (!placement) throw new Error(`non-canonical placement: ${encoded}`);
    masksByPiece.get(placement[1]).push(BigInt(`0x${placement[2]}`));
  }

  let occupied = TERMINAL_SUPPLY_INITIAL_MASK;
  const canonicalPlacements = [];
  for (const piece of CANONICAL_PIECE_ORDER) {
    const masks = masksByPiece.get(piece).sort(compareBigInt);
    requireEqual(masks.length, 1, `canonical ${piece} placement count`);
    for (const mask of masks) {
      requireEqual(popcount(mask), 4, `canonical ${piece} cell count`);
      if ((occupied & mask) !== 0n) {
        throw new Error(`canonical ${piece} placement overlaps occupied cells`);
      }
      occupied |= mask;
      canonicalPlacements.push(`${piece}:${hex64(mask)}`);
    }
  }
  requireEqual(occupied, TERMINAL_SUPPLY_FULL_FIELD, 'terminal-supply completed field mask');
  requireEqual(
    key,
    `ctk1|initial=${hex64(TERMINAL_SUPPLY_INITIAL_MASK)}|placements=${canonicalPlacements.join(',')}`,
    'canonical solution key encoding'
  );
}

function normalizedSetHash(keys) {
  let hash = FNV_OFFSET;
  for (const key of [...keys].sort()) {
    for (const byte of Buffer.from(`${key}\0`, 'utf8')) {
      hash ^= BigInt(byte);
      hash = (hash * FNV_PRIME) & U64_MASK;
    }
  }
  return `cts1:${hash.toString(16).padStart(16, '0')}`;
}

function popcount(value) {
  let count = 0;
  for (let remaining = value; remaining !== 0n; remaining &= remaining - 1n) count += 1;
  return count;
}

function compareBigInt(left, right) {
  return left < right ? -1 : left > right ? 1 : 0;
}

function hex64(value) {
  return value.toString(16).padStart(16, '0');
}

function requireEqual(actual, expected, label) {
  if (actual !== expected) {
    throw new Error(`${label} mismatch: expected ${String(expected)}, received ${String(actual)}`);
  }
}
