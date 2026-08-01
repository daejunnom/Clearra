import assert from 'node:assert/strict';
import test from 'node:test';

import {
  buildWorkspaceCommand,
  createDefaultWorkspaceRequest,
  workspaceRequestForDesktop
} from '../src/lib/workspace/solverWorkspaceModel.ts';

test('failed queue mode uses the reverse coverage command without scoring', () => {
  const request = {
    ...createDefaultWorkspaceRequest(),
    queue: 'P5',
    lines: 2,
    scoreMode: 'failed-queue',
    solutionProbabilities: true,
    initialB2B: 4
  };

  const command = buildWorkspaceCommand(request);
  assert.match(command, /^clearra failed-queue /);
  assert.match(command, /--patterns P5/);
  assert.match(command, /--count all/);
  assert.doesNotMatch(command, /--score(?:\s|$)/);
  assert.doesNotMatch(command, /--initial-b2b/);
  assert.doesNotMatch(command, /--solution-probabilities/);

  const desktop = workspaceRequestForDesktop(request, 'en');
  assert.equal(desktop.score_mode, 'failed-queue');
  assert.equal(desktop.count_policy, 'all');
  assert.equal(desktop.initial_b2b, 0);
  assert.equal(desktop.solution_probabilities, false);
});
