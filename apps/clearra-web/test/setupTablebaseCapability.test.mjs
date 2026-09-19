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
    /pc_search_target_lines:\s*receipts\.map\(receipt\s*=>\s*receipt\.target_lines\)/u
  );
  assert.match(
    qualification,
    /receipt\.use_case\s*!==\s*'pc-search'[\s\S]*receipt\.target_lines\s*!==\s*4/u
  );
  assert.match(qualification, /setup_search_target_lines:\s*\[\]/u);

  const worker = await read('../src/workers/clearraWorker.ts');
  assert.match(worker, /pcSearchTargetLines:\s*pc_search_target_lines\s*\?\?\s*\[\]/u);
  assert.match(worker, /setupSearchTargetLines:\s*setup_search_target_lines\s*\?\?\s*\[\]/u);
});

test('PC surface requires the selected profile and exact target receipt', async () => {
  const controls = await read('../../../packages/clearra-ui/src/lib/workspace/SearchControls.svelte');
  assert.match(controls, /profile\.pcSearchTargetLines\?\.includes\(lines\)/u);
  assert.match(controls, /disabled=\{!pcTablebaseAvailable\s*\|\|/u);
  assert.match(
    controls,
    /if \(!pcTablebaseAvailable && request\.tablebaseEnabled\)[\s\S]*tablebaseEnabled: false/u
  );
  assert.match(
    controls,
    /targetIdentityChanged[\s\S]*\{ \.\.\.next, tablebaseEnabled: false \}/u
  );
  assert.match(
    controls,
    /pcTablebaseAvailable\s*\?\s*tablebaseStatus\s*:\s*'unavailable'/u
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
    /targetIdentityChanged[\s\S]*targetQualified[\s\S]*tablebaseEnabled: false/u
  );
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
  assert.match(controls, /disabled=\{!setupTablebaseAvailable\}/u);
  assert.match(
    controls,
    /if \(!setupTablebaseAvailable && request\.tablebaseEnabled\)[\s\S]*tablebaseEnabled: false/u
  );
  assert.match(controls, /setupTablebaseHelp/u);
  assert.doesNotMatch(controls, /label\('tablebaseHelp'\)/u);

  const cli = await read('../../../crates/clearra-cli/src/tablebase_online_execution.rs');
  assert.match(cli, /AppCommand::Setup\(_\)\s*=>\s*return Err\("setup_pc_acceleration_not_qualified"\)/u);

  const help = await read('../../../crates/clearra-cli/src/args/cli_parser.rs');
  assert.match(help, /4-line SetupSearch target has full-solution and differential qualification/u);
  assert.doesNotMatch(help, /only rejects precomputed exact-dead PC4 completion states/u);
});
