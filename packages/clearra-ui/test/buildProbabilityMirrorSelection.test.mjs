import assert from 'node:assert/strict';
import test from 'node:test';
import {
  buildProbabilityCommandArguments, buildProbabilityRequestForDesktop,
  buildProbabilityValidationCodes, createDefaultBuildProbabilityRequest
} from '../src/lib/workspace/buildProbabilityModel.ts';
import { buildMandatorySource } from '../src/lib/workspace/mandatorySolutionSelection.ts';

const request = {
  ...createDefaultBuildProbabilityRequest(), height: 4, existingMask: 0n,
  targetMask: 0xfn, queue: 'I', holdEnabled: false, workers: 1
};

test('default Build mirror is selected by the existing field, not the asymmetric target', () => {
  for (const [existingMask, targetMask, include] of [[0n, 0xfn, true], [0n, 0x78n, true], [0x10n, 0xfn, false]]) {
    const draft = { ...request, existingMask, targetMask };
    assert.deepEqual(buildProbabilityValidationCodes(draft), []);
    const args = buildProbabilityCommandArguments(draft);
    assert.equal(args.includes('--include-mirror'), include);
    assert.equal(args.includes('--no-mirror'), !include);
    assert.equal(args[args.indexOf('--target-mask') + 1], `0x${targetMask.toString(16).padStart(16, '0')}`);
    assert.ok(args.includes('--no-hold'));
    assert.deepEqual(buildProbabilityRequestForDesktop(draft, 'en').arguments, args);
  }
});

test('Build minimum retains the complete source identity and the selected mirrored drawing', () => {
  const selected = {
    ...request, resultMode: 'minimum-solutions',
    pinnedSolutionKeys: ['ctk1|initial=0000000000000000|placements=I:00000000000003c0'],
    pinnedSolutionDocument: 'ctk3_selected_drawing', pinnedSourceSetHash: 'cts1:0123456789abcdef'
  };
  assert.deepEqual(buildProbabilityValidationCodes(selected), []);
  assert.equal(buildMandatorySource(request), buildMandatorySource(selected));
  assert.notEqual(buildMandatorySource(selected), buildMandatorySource({ ...selected, targetMask: 0x78n }));
  const args = buildProbabilityCommandArguments(selected);
  assert.deepEqual(args.slice(0, 3), ['clearra', 'build', 'pinned-minimals']);
  assert.equal(args[args.indexOf('--required-document') + 1], selected.pinnedSolutionDocument);
  assert.equal(args[args.indexOf('--expected-source-set-hash') + 1], selected.pinnedSourceSetHash);
});
