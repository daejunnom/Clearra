import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { COMPONENT_QUALIFICATION_COMMANDS } from './fast-fix-qualification-evidence.mjs';

const workflowNames = [
  'candidate-preflight.yml', 'release-cli.yml',
  'pages-rollback.yml', 'fast-fix-qualification.yml',
];
const workflows = new Map(await Promise.all(workflowNames.map(async (name) => [
  name,
  (await readFile(new URL(`../../.github/workflows/${name}`, import.meta.url), 'utf8'))
    .replaceAll('\r\n', '\n'),
])));

function assertManagedCargoSteps(source) {
  assert.doesNotMatch(source, /CARGO_TARGET_DIR[^\n]*GITHUB_ENV/u,
    'a target path in GITHUB_ENV does not keep its owner process alive');
  assert.doesNotMatch(source, /(?:RUNNER_TEMP|runner\.temp)[^\n]*(?:cargo-target|product-e2e-native|product-e2e-cli-target)/u,
    'CI has no build-root exception');
  assert.doesNotMatch(source, /(?:~\/AppData\/Local|~\/\.cache)\/Clearra\/build/u,
    'Actions cache must not restore managed generations or stale owner leases');
  assert.doesNotMatch(source, /release-acceptance-native-v3-|release-acceptance-wasm-v4-|product-v2-|product-linux-bookworm-rust-1\.96-v3-/u,
    'old cache keys can restore archived target paths even after path declarations change');
  for (const step of source.split(/^      - /mu)) {
    const rawCargo = /^\s+(?:&\s+)?cargo\s+(?:build|test|install|check|run|clippy|rustc)\b/mu.exec(step);
    const owner = /Ensure-ClearraBuildArtifactCache -RepositoryRoot \$env:GITHUB_WORKSPACE -Purpose (?:experiment|product)/u.exec(step);
    if (rawCargo) {
      assert.ok(owner && owner.index < rawCargo.index,
        'raw Cargo requires a live owner in the same step before the first compiler call');
    }
    if (!owner) continue;
    assert.match(step, /shell: (?:pwsh|powershell)/u);
    assert.match(step.slice(owner.index), /^Ensure-ClearraBuildArtifactCache[^\n]*\n          try \{/u,
      'the direct owner must enter protected execution before running its payload');
    assert.match(step, /^            Complete-ClearraBuildTransaction\n          \} finally \{\n            Exit-ClearraBuildArtifactCacheUsage\n          \}(?:\n|$)/mu,
      'success completes the transaction and every normal failure releases the owner lease');
    assert.equal((step.match(/^\s*Complete-ClearraBuildTransaction\s*$/gmu) ?? []).length, 1,
      'completion must happen only once after all payload checks');
  }
}

for (const [name, source] of workflows) {
  test(`${name} never bypasses the managed build root or owner lifetime`, () => {
    assertManagedCargoSteps(source);
  });
}

test('workflow source gate rejects an unowned Cargo mutation and stale-target handoff', () => {
  const source = workflows.get('candidate-preflight.yml');
  assert.throws(() => assertManagedCargoSteps(source.replaceAll(
    'Ensure-ClearraBuildArtifactCache -RepositoryRoot $env:GITHUB_WORKSPACE -Purpose experiment',
    '# removed owner',
  )));
  assert.throws(() => assertManagedCargoSteps(`${source}\n          "CARGO_TARGET_DIR=$targetDir" >> $env:GITHUB_ENV\n`));
  assert.throws(() => assertManagedCargoSteps(`${source}\n          $targetDir = Join-Path $env:RUNNER_TEMP 'cargo-target'\n`));
});

test('workflow source gate rejects missing completion or leaked direct-owner leases', () => {
  for (const name of ['candidate-preflight.yml', 'release-cli.yml']) {
    const source = workflows.get(name);
    assert.throws(() => assertManagedCargoSteps(source.replaceAll(
      '            Complete-ClearraBuildTransaction\n', '',
    )));
    assert.throws(() => assertManagedCargoSteps(source.replaceAll(
      '            Exit-ClearraBuildArtifactCacheUsage\n', '',
    )));
    assert.throws(() => assertManagedCargoSteps(source.replaceAll(
      '          try {\n', '          try {\n            Complete-ClearraBuildTransaction\n',
    )));
  }
});

test('ProductE2E verifies and executes the same Rust release CLI that Windows ships', () => {
  const source = workflows.get('release-cli.yml');
  const producer = source.split('  windows-cli:\n')[1].split(/^  [a-z][a-z0-9-]*:\n/mu)[0];
  const consumer = source.split('  release-acceptance-rust-product:\n')[1]
    .split(/^  [a-z][a-z0-9-]*:\n/mu)[0];
  const operations = [
    'Ensure-ClearraBuildArtifactCache -RepositoryRoot $env:GITHUB_WORKSPACE -Purpose product',
    'cargo build --locked --release --package clearra-cli --bin clearra --no-default-features --features online-pc4-tablebase,wasm-cpu-runtime',
    'Copy-Item -LiteralPath $builtCli -Destination $cli',
    "Invoke-ClearraJsonSmoke -Name 'terminal-supply-p0'",
    'node scripts/release/product-e2e-cli-artifact.mjs seal',
    '--binary $cli',
    'Complete-ClearraBuildTransaction',
  ].map((marker) => producer.indexOf(marker));
  assert.ok(operations.every((position) => position >= 0));
  assert.deepEqual([...operations].sort((left, right) => left - right), operations);
  assert.doesNotMatch(producer, /native-c-core|prepare-native-build-identity/u);
  assert.doesNotMatch(consumer, /cargo build|native-c-core|prepare-native-build-identity/u);
  assert.doesNotMatch(source, /^  release-acceptance-product-cli:/mu);
  assert.match(consumer, /needs: \[metadata, ctk3, windows-cli\]/u);
  const artifactNames = (job) => job.match(/^\s*name: (?:clearra-windows-cli-v|product-e2e-cli-receipt-).+$/gmu)
    ?.map((name) => name.trim());
  assert.equal(artifactNames(producer)?.length, 2);
  assert.deepEqual(artifactNames(consumer), artifactNames(producer));
  assert.match(producer, /path: \$\{\{ steps\.windows_cli_build\.outputs\.artifact_path \}\}/u);
  assert.match(producer, /path: \$\{\{ steps\.windows_cli_build\.outputs\.receipt_path \}\}/u);
  assert.match(consumer, /node scripts\/release\/product-e2e-cli-artifact\.mjs verify/u);
  assert.match(consumer, /CLEARRA_VERIFIED_PRODUCT_E2E_CLI: \$\{\{ steps\.product_cli\.outputs\.binary_path \}\}/u);
});

test('Windows release binaries are staged inside and exported by their product owner', () => {
  const source = workflows.get('release-cli.yml');
  for (const id of ['windows_cli_build', 'windows_gui_build']) {
    const step = source.split(`        id: ${id}\n`)[1].split(/^      - /mu)[0];
    assert.match(step, /Ensure-ClearraBuildArtifactCache -RepositoryRoot \$env:GITHUB_WORKSPACE -Purpose product/u);
    assert.match(step, /Join-Path \$env:CLEARRA_BUILD_TRANSACTION_ROOT 'release-artifacts'/u);
    assert.match(step, /artifact_path=[^\n]+GITHUB_OUTPUT/u);
    assert.ok(source.includes(`path: \${{ steps.${id}.outputs.artifact_path }}`));
  }
});

test('a policy-adopted Pages snapshot uses its own managed launcher after authoritative preflight', () => {
  const source = workflows.get('pages-rollback.yml');
  assert.match(source, /snapshot-source\/scripts\/tools\/invoke-clearra-build\.ps1" -SourceRoot "\$GITHUB_WORKSPACE\/snapshot-source" -Purpose product -Command npm/u);
  assert.ok(source.indexOf('authority-source/scripts/tools/validate-managed-frontend-source.mjs --source-root snapshot-source') <
    source.indexOf('snapshot-source/scripts/tools/invoke-clearra-build.ps1'));
  assert.doesNotMatch(source, /run: cargo|CARGO_TARGET_DIR=/u);
});

test('CLI qualification executes and records exactly the managed product command', () => {
  const source = workflows.get('fast-fix-qualification.yml');
  const command = COMPONENT_QUALIFICATION_COMMANDS.get('cli');
  assert.equal(source.split(command).length - 1, 2,
    'the fixed environment evidence and executed command must both be present');
  assert.match(source, /--command "\$CLI_COMPONENT_COMMAND"/u);
  assert.doesNotMatch(source, /run: cargo test/u);
});
