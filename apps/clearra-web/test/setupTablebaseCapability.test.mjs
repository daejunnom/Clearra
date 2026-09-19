import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const read = relative => readFile(new URL(relative, import.meta.url), 'utf8');

test('host qualification keeps PC and Setup target authority separate', async () => {
  const qualification = await read('../../../scripts/release/pc4/qualify-upstream-generation.mjs');
  assert.doesNotMatch(qualification, /pc_search_target_lines:\s*\[4\]/u);
  assert.match(qualification, /dependencies\.targetQualificationReceipts\s*\?\?\s*\[\]/u);
  assert.match(
    qualification,
    /pc_search_target_lines:\s*pcReceipts\.map\(receipt\s*=>\s*receipt\.target_lines\)/u
  );
  assert.match(
    qualification,
    /receipt\.schema === PC4_SETUP_TARGET_QUALIFICATION_RECEIPT_SCHEMA[\s\S]*receipt\.use_case === 'setup-search'/u
  );
  assert.match(
    qualification,
    /setup_search_target_lines:\s*setupReceipts\.map\(receipt\s*=>\s*receipt\.target_lines\)/u
  );

  const worker = await read('../src/workers/clearraWorker.ts');
  assert.match(worker, /pcSearchTargetLines:\s*pc_search_target_lines\s*\?\?\s*\[\]/u);
  assert.match(worker, /setupSearchTargetLines:\s*setup_search_target_lines\s*\?\?\s*\[\]/u);
});

test('PC surface requires the selected profile and exact target receipt', async () => {
  const controls = await read('../../../packages/clearra-ui/src/lib/workspace/SearchControls.svelte');
  assert.match(controls, /profile\.pcSearchTargetLines\?\.includes\(lines\)/u);
  assert.doesNotMatch(controls, /disabled=\{!pcTablebaseAvailable/u);
  assert.doesNotMatch(controls, /if \(!pcTablebaseAvailable && request\.tablebaseEnabled\)/u);
  assert.match(
    controls,
    /targetIdentityChanged[\s\S]*\{ \.\.\.next, tablebaseEnabled: false \}/u
  );
  assert.match(
    controls,
    /tablebaseStatus === 'loading'[\s\S]*\? 'loading'/u
  );
  assert.match(
    controls,
    /slot\.status === 'ready' && slot\.pcSearchTargetLines\?\.includes\(request\.lines\)/u
  );
  assert.doesNotMatch(controls, /request\.lines\s*!==\s*4/u);

  const workspace = await read('../../../packages/clearra-ui/src/lib/workspace/SolverWorkspace.svelte');
  assert.match(workspace, /profile\.pcSearchTargetLines\?\.includes\(request\.lines\)/u);
  assert.match(
    workspace,
    /targetIdentityChanged[\s\S]*tablebaseEnabled: false/u
  );
  assert.match(workspace, /tablebaseBlocked[\s\S]*!pcTablebaseAvailable/u);
  assert.match(workspace, /runDisabled=\{validationCodes\.length > 0 \|\| tablebaseBlocked\}/u);
  assert.match(workspace, /tablebaseEnabled:\s*bounded === request\.lines/u);
  assert.match(workspace, /tablebaseEnabled:\s*lines === request\.lines/u);

  const messages = await read('../../../packages/clearra-ui/src/lib/workspace/workspaceI18n.ts');
  assert.match(messages, /Reader readiness alone is not qualification/u);
  assert.match(messages, /reader 준비만으로는 사용할 수 없습니다/u);
  const japanese = await read('../../../packages/clearra-ui/src/lib/i18n/japaneseWorkspaceCatalog.ts');
  assert.match(japanese, /readerの準備完了だけでは利用できず/u);
});

test('Setup surface stays fail-closed until an exact SetupSearch target is qualified', async () => {
  const controls = await read('../../../packages/clearra-ui/src/lib/workspace/SetupFinderControls.svelte');
  assert.match(controls, /setupSearchTargetLines\?\.includes\(4\)/u);
  assert.doesNotMatch(controls, /disabled=\{!setupTablebaseAvailable\}/u);
  assert.doesNotMatch(controls, /if \(!setupTablebaseAvailable && request\.tablebaseEnabled\)/u);
  assert.match(controls, /tablebaseStatus === 'loading'[\s\S]*\? 'loading'/u);
  assert.match(controls, /setupTablebaseHelp/u);
  assert.doesNotMatch(controls, /label\('tablebaseHelp'\)/u);

  const workspace = await read('../../../packages/clearra-ui/src/lib/workspace/SetupFinderWorkspace.svelte');
  assert.match(workspace, /profile\.setupSearchTargetLines\?\.includes\(4\)/u);
  assert.match(workspace, /tablebaseBlocked[\s\S]*!setupTablebaseAvailable/u);
  assert.match(workspace, /runDisabled=\{validationCodes\.length > 0 \|\| tablebaseBlocked\}/u);

  const cli = await read('../../../crates/clearra-cli/src/tablebase_online_execution.rs');
  assert.match(cli, /AppCommand::Setup\(command\)\s*=>\s*\(command\.query\(\)\.rule\(\), false\)/u);
  const host = await read('../../../crates/clearra-app/src/pc4_online_host_execution.rs');
  assert.match(host, /Pc4TerminalUseCase::SetupSearch/u);
  assert.match(host, /setup_pc_acceleration_not_qualified/u);
  assert.match(host, /\.into_completed_reducer_input\(&guard\)/u);
  assert.doesNotMatch(
    host,
    /completed_reducer_input\(\)[\s\S]{0,160}\.clone\(\)/u
  );
  const wasm = await read('../../../crates/clearra-wasm/src/wasm_command_runtime.rs');
  assert.match(
    wasm,
    /AppCommand::Setup\(command\)\s*=>\s*command\.query\(\)\.tablebase_requested\(\)/u
  );

  const help = await read('../../../crates/clearra-cli/src/args/cli_parser.rs');
  assert.match(help, /4-line SetupSearch target has full-solution and differential qualification/u);
  assert.doesNotMatch(help, /only rejects precomputed exact-dead PC4 completion states/u);
});
