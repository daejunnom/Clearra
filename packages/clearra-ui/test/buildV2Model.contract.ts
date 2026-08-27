import assert from 'node:assert/strict';

import {
  BUILD_V2_CAPABILITIES,
  buildV2AllowedObjectives,
  buildV2Command,
  buildV2DefaultObjective,
  buildV2RequestForDesktop,
  buildV2SourceKind,
  buildV2ValidationCodes,
  createDefaultBuildV2Request,
  type BuildV2Capability,
  type BuildV2Request
} from '../src/lib/workspace/buildV2Model';

for (const capability of BUILD_V2_CAPABILITIES) {
  const request = validRequest(capability);
  assert.deepEqual(buildV2ValidationCodes(request), [], capability);
  const command = buildV2Command(request);
  assert.match(command, /^clearra build /u);
  assert.match(command, / --backend cpu --no-backend-fallback /u);
  assert.doesNotMatch(command, /max-memory/u);
  assert.match(command, / --queue I /u);
  assert.match(command, new RegExp(commandPath(capability), 'u'));

  const desktop = buildV2RequestForDesktop(request, 'ko');
  assert.equal(desktop.app_request_model, 'clearra-app/AppRequest');
  assert.equal(desktop.command, 'build-v2');
  assert.equal(desktop.capability_id, capability);
  assert.equal(desktop.backend, 'cpu');
  assert.equal(desktop.allow_backend_fallback, false);
  assert.equal('memory_budget_mb' in desktop, false);
  assert.equal('max_memory_mib' in desktop, false);

  if (buildV2SourceKind(capability) === 'target-document') {
    assert.match(command, / --target-format ctk3 --target-document ctk3_test /u);
    assert.equal(desktop.target_document, 'ctk3_test');
    assert.equal('solution_document' in desktop, false);
  } else if (buildV2SourceKind(capability) === 'solution-document') {
    assert.match(command, / --solution-format ctk3 --solution-document ctk3_test /u);
    assert.equal(desktop.solution_document, 'ctk3_test');
    assert.equal('target_document' in desktop, false);
  } else {
    assert.match(
      command,
      / --base-mask 0x0000000000000000 --target-mask 0x000000000000000f --height 4 /u
    );
    assert.equal(desktop.target_mask, '0x000000000000000f');
  }
}

const switched = validRequest('build.setup-cover-score');
switched.objective = 'all';
assert.deepEqual(buildV2ValidationCodes(switched), ['objective_invalid']);
assert.match(buildV2Command(switched), / --objective max-score-cover /u);

const invalidB2b = validRequest('build.evaluate.score');
invalidB2b.initialB2B = 65_536;
assert.deepEqual(buildV2ValidationCodes(invalidB2b), ['initial_b2b_invalid']);

const invalidNominalTarget = validRequest('build.setup');
invalidNominalTarget.targetDocument = 'v115@wrong-nominal-format';
assert.deepEqual(buildV2ValidationCodes(invalidNominalTarget), ['target_document_invalid']);

const score = buildV2RequestForDesktop(validRequest('build.evaluate.score'), 'en');
assert.equal(score.score_profile, 'tetrio');
assert.equal(score.initial_b2b, 0);
const nonScore = buildV2RequestForDesktop(validRequest('build.evaluate.cover'), 'en');
assert.equal('score_profile' in nonScore, false);
assert.equal('initial_b2b' in nonScore, false);

function validRequest(capability: BuildV2Capability): BuildV2Request {
  return {
    ...createDefaultBuildV2Request(),
    capability,
    objective: buildV2DefaultObjective(capability),
    targetDocument: 'ctk3_test',
    solutionDocument: 'ctk3_test',
    queue: 'I',
    workers: 1
  };
}

function commandPath(capability: BuildV2Capability): string {
  return capability.startsWith('build.evaluate.')
    ? `build evaluate ${capability.slice('build.evaluate.'.length)}`
    : `build ${capability.slice('build.'.length)}`;
}

assert.equal(BUILD_V2_CAPABILITIES.length, 12);
assert.deepEqual(buildV2AllowedObjectives('build.cover'), [
  'min-cover',
  'max-probability-minimum'
]);
console.log(JSON.stringify({ build_v2_capabilities: 12, finite_memory_option: 'rejected' }));
