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
assert.equal(workspaceRequestForDesktop(pc, 'en').workers, 0);
assert.equal(workspaceRequestForDesktop(pc, 'en').use_all_logical_processors, true);
assert.equal(workspaceRequestForDesktop(createDefaultWorkspaceRequest(), 'en').workers, 0);
assert.equal(
  workspaceRequestForDesktop(createDefaultWorkspaceRequest(), 'en').use_all_logical_processors,
  false
);

const build = {
  ...createDefaultBuildProbabilityRequest(),
  useAllLogicalProcessors: true,
  workers: 12
};
assert.match(buildProbabilityCommand(build), /--workers 12(?:\s|$)/);
assert.match(buildProbabilityCommand(build), /--use-all-cpu-threads(?:\s|$)/);
assert.equal(buildProbabilityRequestForDesktop(build, 'en').workers, 0);
assert.equal(buildProbabilityRequestForDesktop(build, 'en').use_all_logical_processors, true);
assert.equal(
  buildProbabilityRequestForDesktop(createDefaultBuildProbabilityRequest(), 'en').workers,
  0
);

assert.equal(setupFinderRequestForDesktop(fullSetup, 'en', 12).workers, 0);
assert.equal(
  setupFinderRequestForDesktop(fullSetup, 'en', 12).use_all_logical_processors,
  true
);
assert.equal(setupFinderRequestForDesktop(setup, 'en', 11).workers, 0);
assert.equal(
  setupFinderRequestForDesktop(
    fullSetup,
    'en',
    1,
    { setupId: 'setup-1', conditionId: 'condition-1' }
  ).workers,
  1
);
assert.equal(
  setupFinderRequestForDesktop(
    fullSetup,
    'en',
    1,
    { setupId: 'setup-1', conditionId: 'condition-1' }
  ).use_all_logical_processors,
  false
);
assert.equal(forwardSearchRequestForDesktop(fullForward, 'en', 12).workers, 0);
assert.equal(
  forwardSearchRequestForDesktop(fullForward, 'en', 12).use_all_logical_processors,
  true
);
assert.equal(forwardSearchRequestForDesktop(forward, 'en', 11).workers, 0);
