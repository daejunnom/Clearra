import assert from 'node:assert/strict';
import test from 'node:test';

import {
  buildWorkspaceCommand,
  buildWorkspaceCommandArguments,
  createDefaultWorkspaceRequest,
  workspaceRequestForDesktop
} from '../src/lib/workspace/solverWorkspaceModel.ts';

test('failed queue mode uses the typed PC reverse coverage command without scoring', () => {
  const request = {
    ...createDefaultWorkspaceRequest(),
    queue: 'P5',
    lines: 2,
    scoreMode: 'failed-queue',
    solutionProbabilities: true,
    initialB2B: 4
  };

  const command = buildWorkspaceCommand(request);
  const arguments_ = buildWorkspaceCommandArguments(request);
  assert.deepEqual(arguments_.slice(0, 3), ['clearra', 'pc', 'failed-queue']);
  assert.match(command, /^clearra pc failed-queue /);
  assert.match(command, /--patterns P5/);
  assert.match(command, /--count all/);
  assert.doesNotMatch(command, /--score(?:\s|$)/);
  assert.doesNotMatch(command, /--initial-b2b/);
  assert.doesNotMatch(command, /--solution-probabilities/);

  const desktop = workspaceRequestForDesktop(request, 'en');
  assert.equal(desktop.app_request_model, 'clearra-cli/CommandRequest');
  assert.equal(desktop.command, 'cli');
  assert.deepEqual(desktop.arguments, arguments_);
});
