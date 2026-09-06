import assert from 'node:assert/strict';

import {
  buildProbabilityCommand,
  buildProbabilityRequestForDesktop,
  createDefaultBuildProbabilityRequest
} from '../../../packages/clearra-ui/src/lib/workspace/buildProbabilityModel.ts';
import {
  buildForwardSearchCommand,
  createDefaultForwardSearchRequest,
  forwardSearchRequestForDesktop
} from '../../../packages/clearra-ui/src/lib/workspace/forwardSearchModel.ts';
import {
  buildSetupFinderCommand,
  buildSetupPathDetailCommand,
  createDefaultSetupFinderRequest,
  setupFinderRequestForDesktop
} from '../../../packages/clearra-ui/src/lib/workspace/setupFinderModel.ts';
import {
  buildWorkspaceCommand,
  createDefaultWorkspaceRequest,
  workspaceRequestForDesktop
} from '../../../packages/clearra-ui/src/lib/workspace/solverWorkspaceModel.ts';
import {
  automaticWorkerAuthority,
  createHostCapabilitySnapshot
} from '../../../packages/clearra-ui/src/lib/wasm/hostCapabilitySnapshot.ts';

function optionValue(arguments_: readonly string[], option: string): string | undefined {
  const index = arguments_.indexOf(option);
  return index === -1 ? undefined : arguments_[index + 1];
}

const eightLogicalProcessors = createHostCapabilitySnapshot({
  snapshotId: 'browser-worker-budget-eight',
  source: 'host-provided',
  reportedLogicalProcessors: 8,
  reportedDeviceMemoryGiB: 4
});
assert.deepEqual(automaticWorkerAuthority(eightLogicalProcessors), {
  snapshotId: 'browser-worker-budget-eight',
  reportedLogicalProcessors: 8,
  workersRequested: 7,
  workersEffective: 7,
  reason: 'reserved-main-thread'
});
assert.deepEqual(automaticWorkerAuthority(eightLogicalProcessors, true), {
  snapshotId: 'browser-worker-budget-eight',
  reportedLogicalProcessors: 8,
  workersRequested: 8,
  workersEffective: 8,
  reason: 'all-logical-processors'
});

const setup = createDefaultSetupFinderRequest();
assert.match(buildSetupFinderCommand(setup, 11), /--auto-workers 11(?:\s|$)/);
assert.doesNotMatch(buildSetupFinderCommand(setup, 11), /--use-all-cpu-threads/);
assert.doesNotMatch(buildSetupFinderCommand(setup), /--auto-workers/);
const fullSetup = { ...setup, useAllLogicalProcessors: true };
assert.match(buildSetupFinderCommand(fullSetup, 12), /--auto-workers 12(?:\s|$)/);
assert.match(buildSetupFinderCommand(fullSetup, 12), /--use-all-cpu-threads(?:\s|$)/);
assert.match(
  buildSetupPathDetailCommand(
    fullSetup,
    { setupId: 'setup-1', conditionId: 'condition-1' },
    1
  ),
  /--auto-workers 1(?:\s|$)/
);
assert.doesNotMatch(
  buildSetupPathDetailCommand(
    fullSetup,
    { setupId: 'setup-1', conditionId: 'condition-1' },
    1
  ),
  /--use-all-cpu-threads/
);

const forward = createDefaultForwardSearchRequest('damage');
assert.match(
  buildForwardSearchCommand(forward, 11),
  /--auto-workers 11(?:\s|$)/
);
assert.doesNotMatch(buildForwardSearchCommand(forward, 11), /--use-all-cpu-threads/);
const fullForward = { ...forward, useAllLogicalProcessors: true };
assert.match(buildForwardSearchCommand(fullForward, 12), /--auto-workers 12(?:\s|$)/);
assert.match(buildForwardSearchCommand(fullForward, 12), /--use-all-cpu-threads(?:\s|$)/);

const pc = { ...createDefaultWorkspaceRequest(), useAllLogicalProcessors: true, workers: 12 };
assert.match(buildWorkspaceCommand(pc), /--workers 12(?:\s|$)/);
assert.match(buildWorkspaceCommand(pc), /--use-all-cpu-threads(?:\s|$)/);
assert.doesNotMatch(buildWorkspaceCommand(pc), /--cpu-warmup(?:\s|$)/);
const pcDesktop = workspaceRequestForDesktop(pc, 'en');
assert.equal(optionValue(pcDesktop.arguments, '--workers'), String(pc.workers));
assert.equal(pcDesktop.arguments.includes('--use-all-cpu-threads'), true);
assert.equal(pcDesktop.arguments.includes('--cpu-warmup'), false);
const defaultPcDesktop = workspaceRequestForDesktop(createDefaultWorkspaceRequest(), 'en');
assert.equal(
  optionValue(defaultPcDesktop.arguments, '--workers'),
  String(createDefaultWorkspaceRequest().workers)
);
assert.equal(
  defaultPcDesktop.arguments.includes('--use-all-cpu-threads'),
  false
);
assert.equal(defaultPcDesktop.arguments.includes('--cpu-warmup'), false);
for (const scoreMode of ['minimum-cover', 'summary', 'score-finder', 'score-minimals'] as const) {
  const product = { ...pc, scoreMode, queue: 'I' };
  assert.doesNotMatch(buildWorkspaceCommand(product), /--cpu-warmup(?:\s|$)/, scoreMode);
  assert.equal(workspaceRequestForDesktop(product, 'en').arguments.includes('--cpu-warmup'), false, scoreMode);
}

const build = {
  ...createDefaultBuildProbabilityRequest(),
  useAllLogicalProcessors: true,
  workers: 12
};
assert.match(buildProbabilityCommand(build), /--workers 12(?:\s|$)/);
assert.match(buildProbabilityCommand(build), /--use-all-cpu-threads(?:\s|$)/);
const buildDesktop = buildProbabilityRequestForDesktop(build, 'en');
assert.equal(optionValue(buildDesktop.arguments, '--workers'), String(build.workers));
assert.equal(buildDesktop.arguments.includes('--use-all-cpu-threads'), true);
assert.equal(
  optionValue(
    buildProbabilityRequestForDesktop(createDefaultBuildProbabilityRequest(), 'en').arguments,
    '--workers'
  ),
  String(createDefaultBuildProbabilityRequest().workers)
);

const fullSetupDesktop = setupFinderRequestForDesktop(fullSetup, 'en', 12);
assert.equal(optionValue(fullSetupDesktop.arguments, '--auto-workers'), '12');
assert.equal(
  fullSetupDesktop.arguments.includes('--use-all-cpu-threads'),
  true
);
assert.equal(
  optionValue(setupFinderRequestForDesktop(setup, 'en', 11).arguments, '--auto-workers'),
  '11'
);
const setupPathDesktop = setupFinderRequestForDesktop(
  fullSetup,
  'en',
  1,
  { setupId: 'setup-1', conditionId: 'condition-1' }
);
assert.equal(
  optionValue(setupPathDesktop.arguments, '--auto-workers'),
  '1'
);
assert.equal(
  setupPathDesktop.arguments.includes('--use-all-cpu-threads'),
  false
);
const fullForwardDesktop = forwardSearchRequestForDesktop(fullForward, 'en', 12);
assert.equal(optionValue(fullForwardDesktop.arguments, '--auto-workers'), '12');
assert.equal(
  fullForwardDesktop.arguments.includes('--use-all-cpu-threads'),
  true
);
assert.equal(
  optionValue(forwardSearchRequestForDesktop(forward, 'en', 11).arguments, '--auto-workers'),
  '11'
);
