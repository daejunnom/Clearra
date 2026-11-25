import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import {
  buildWorkspaceCommand,
  buildWorkspaceCommandArguments,
  createDefaultWorkspaceRequest,
  normalizeWorkspaceRequest,
  workspaceRequestForDesktop
} from '../../../packages/clearra-ui/src/lib/workspace/solverWorkspaceModel.ts';
import {
  buildSetupFinderCommand,
  buildSetupFinderCommandArguments,
  buildSetupPathDetailCommandArguments,
  createDefaultSetupFinderRequest,
  setupFinderRequestForDesktop
} from '../../../packages/clearra-ui/src/lib/workspace/setupFinderModel.ts';
import { serializeCliCommandArguments }
  from '../../../packages/clearra-ui/src/lib/workspace/cliCommandModel.ts';

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

test('PC GUI defaults keep tablebase off through execution and both transports', async () => {
  const defaults = createDefaultWorkspaceRequest();
  assert.equal(defaults.tablebaseEnabled, false);
  for (const scoreMode of ['off', 'tiling', 'minimum-cover', 'failed-queue',
    'summary', 'score-finder', 'score-minimals', 'path']) {
    const request = { ...defaults, scoreMode, workers: 7, queue: 'IJLOSTZ' };
    assert.equal(normalizeWorkspaceRequest(request).tablebaseEnabled, false, scoreMode);
    const args = buildWorkspaceCommandArguments(request);
    assert.equal(args.includes('--tablebase'), false, scoreMode);
    assert.equal(args.includes('--no-tablebase'), !['tiling', 'score-finder'].includes(scoreMode), scoreMode);
    assert.equal(args[args.indexOf('--workers') + 1], '7', 'tablebase defaults must not reduce compute workers');
    assert.equal(buildWorkspaceCommand(request), serializeCliCommandArguments(args), scoreMode);
    assert.deepEqual(workspaceRequestForDesktop(request, 'ko').arguments, args, scoreMode);
  }
  // Explicit canonical callers retain the engine option. Geometry-only and
  // score-finder projections keep their existing unsupported-option policy.
  for (const scoreMode of ['off', 'path', 'minimum-cover', 'summary']) {
    const request = { ...defaults, scoreMode, tablebaseEnabled: true };
    assert.ok(buildWorkspaceCommandArguments(request).includes('--tablebase'), scoreMode);
    assert.ok(workspaceRequestForDesktop(request, 'ko').arguments.includes('--tablebase'), scoreMode);
  }
  for (const scoreMode of ['tiling', 'score-finder']) {
    const request = { ...defaults, scoreMode, tablebaseEnabled: true };
    assert.equal(normalizeWorkspaceRequest(request).tablebaseEnabled, false, scoreMode);
    assert.equal(buildWorkspaceCommandArguments(request).includes('--tablebase'), false, scoreMode);
  }

  const controls = await read('../../../packages/clearra-ui/src/lib/workspace/SearchControls.svelte');
  assert.doesNotMatch(controls, /class="tablebase-control"|label\('tablebase'\)/u);
  assert.match(controls, /targetIdentityChanged[\s\S]*\{ \.\.\.next, tablebaseEnabled: false \}/u);
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

test('Setup GUI defaults are off while explicit tablebase callers retain exact qualification', async () => {
  const defaults = createDefaultSetupFinderRequest();
  assert.equal(defaults.tablebaseEnabled, false);
  for (const tablebaseEnabled of [false, true]) {
    const request = { ...defaults, tablebaseEnabled, useAllLogicalProcessors: true };
    const expectedOption = tablebaseEnabled ? '--tablebase' : '--no-tablebase';
    const args = buildSetupFinderCommandArguments(request, 7);
    assert.ok(args.includes(expectedOption));
    assert.equal(args.includes(tablebaseEnabled ? '--no-tablebase' : '--tablebase'), false);
    assert.equal(args[args.indexOf('--auto-workers') + 1], '7');
    assert.ok(args.includes('--use-all-cpu-threads'));
    assert.equal(buildSetupFinderCommand(request, 7), serializeCliCommandArguments(args));
    assert.deepEqual(setupFinderRequestForDesktop(request, 'ko', 7).arguments, args);
    const detail = { setupId: 'setup-1', conditionId: 'condition-1' };
    const detailArgs = buildSetupPathDetailCommandArguments(request, detail, 1);
    assert.ok(detailArgs.includes(expectedOption), 'path detail keeps the source request option');
    assert.deepEqual(setupFinderRequestForDesktop(request, 'ko', 1, detail).arguments, detailArgs);
  }
  const controls = await read('../../../packages/clearra-ui/src/lib/workspace/SetupFinderControls.svelte');
  assert.doesNotMatch(controls, /tablebaseControlAvailable|class="tablebase-control"|label\('tablebase'\)/u);
  assert.match(controls, /next\.rule === request\.rule[\s\S]*\{ \.\.\.next, tablebaseEnabled: false \}/u);

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
