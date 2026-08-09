import assert from 'node:assert/strict';
import test from 'node:test';

import {
  DEFAULT_BUILD_PROBABILITY_FINESSE,
  DEFAULT_BUILD_PROBABILITY_PATTERN_KNOWLEDGE,
  buildProbabilityFinesseCommandArguments,
  buildProbabilityFinesseDesktopFields,
  buildProbabilityFinesseView,
  formatFinesseInputCount,
  representativeWitnessExportForSolution
} from '../src/lib/workspace/buildProbabilityFinesse.ts';
import { workspaceMessage } from '../src/lib/workspace/workspaceI18n.ts';

test('build probability keeps finesse disabled and both policies selected by default', () => {
  assert.equal(DEFAULT_BUILD_PROBABILITY_FINESSE, 'off');
  assert.equal(DEFAULT_BUILD_PROBABILITY_PATTERN_KNOWLEDGE, 'both');
  assert.deepEqual(
    buildProbabilityFinesseCommandArguments(
      DEFAULT_BUILD_PROBABILITY_FINESSE,
      DEFAULT_BUILD_PROBABILITY_PATTERN_KNOWLEDGE
    ),
    []
  );
});

test('build probability forwards finesse policy to CLI and desktop requests', () => {
  assert.deepEqual(buildProbabilityFinesseCommandArguments('inputs', 'visible-7'), [
    '--finesse',
    'inputs',
    '--pattern-knowledge',
    'visible-7'
  ]);
  assert.deepEqual(buildProbabilityFinesseDesktopFields('inputs', 'visible-7'), {
    finesse: 'inputs',
    pattern_knowledge: 'visible-7'
  });
});

test('finesse report projection groups per-solution averages by policy', () => {
  const view = buildProbabilityFinesseView({
    metric: 'inputs',
    mode: 'search',
    pattern_knowledge: 'both',
    complete: false,
    exact_total_inputs: 3,
    representative_witness: {
      policy: 'oracle',
      solution_key: 'solution-a',
      pattern_ids: [3],
      queue: ['T', 'I'],
      total_inputs: 3,
      input_sequence: ['hold', 'rotate-clockwise', 'hard-drop'],
      placements: [{ piece: 'T', rotation: 1, x: 2, y: 0 }]
    },
    policy_results: [
      {
        policy: 'oracle',
        overall_average_inputs: '8.5',
        complete: true,
        solution_averages: [
          { solution_key: 'solution-a', average_inputs: '8', complete: true }
        ]
      },
      {
        policy: 'visible-7',
        overall_average_inputs: '9.25',
        complete: false,
        solution_averages: [
          { solution_key: 'solution-a', average_inputs: '9', complete: true },
          { solution_key: 'solution-b', average_inputs: '10', complete: false }
        ]
      }
    ]
  });

  assert.equal(view?.exactTotalInputs, '3');
  assert.deepEqual(view?.representativeWitness, {
    policy: 'oracle',
    solution_key: 'solution-a',
    pattern_ids: [3],
    queue: ['T', 'I'],
    total_inputs: 3,
    input_sequence: ['hold', 'rotate-clockwise', 'hard-drop'],
    placements: [{ piece: 'T', rotation: 1, x: 2, y: 0 }]
  });
  assert.equal(view?.policyResults[0].complete, false);
  assert.deepEqual(view?.solutionByKey['solution-a'], [
    { policy: 'oracle', average_inputs: '8', complete: false },
    { policy: 'visible-7', average_inputs: '9', complete: false }
  ]);
  assert.deepEqual(view?.solutionByKey['solution-b'], [
    { policy: 'visible-7', average_inputs: '10', complete: false }
  ]);
});

test('pattern finesse keeps one representative witness while the exact total stays absent', () => {
  const report = {
    metric: 'inputs',
    mode: 'search',
    pattern_knowledge: 'oracle',
    complete: false,
    exact_total_inputs: null,
    representative_witness: {
      policy: 'oracle',
      solution_key: 'solution-a',
      pattern_ids: [2, 7],
      queue: ['O'],
      total_inputs: 1,
      input_sequence: ['hard-drop'],
      placements: [{ piece: 'O', rotation: 0, x: 4, y: 0 }]
    },
    policy_results: [{
      policy: 'oracle',
      overall_average_inputs: '1.5',
      complete: false,
      solution_averages: [
        { solution_key: 'solution-a', average_inputs: '1.5', complete: false }
      ]
    }]
  };

  const view = buildProbabilityFinesseView(report);
  assert.equal(view?.exactTotalInputs, null);
  assert.deepEqual(view?.representativeWitness, report.representative_witness);
  assert.deepEqual(
    representativeWitnessExportForSolution(
      view?.representativeWitness,
      'solution-a',
      view?.solutionByKey['solution-a']
    ),
    {
      solutionKey: 'solution-a',
      totalInputs: 1,
      annotationInputs: '1.5',
      inputSequence: ['hard-drop'],
      placements: [{ piece: 'O', rotation: 0, x: 4, y: 0 }]
    }
  );
});

test('build probability ignores fixed-document score reports', () => {
  assert.equal(
    buildProbabilityFinesseView({
      metric: 'inputs',
      mode: 'score',
      pattern_knowledge: 'oracle',
      complete: true,
      exact_total_inputs: '7',
      policy_results: []
    }),
    null
  );
});

test('finesse input formatting hides engine sentinels from the GUI', () => {
  assert.equal(formatFinesseInputCount('not-calculated', 'en'), '—');
  assert.equal(formatFinesseInputCount('unavailable', 'ko'), '—');
  assert.equal(formatFinesseInputCount(null, 'en'), '—');
  assert.equal(formatFinesseInputCount('-1', 'ko'), '—');
  assert.equal(formatFinesseInputCount('12.5000', 'en'), '12.5');
  assert.equal(formatFinesseInputCount('private-internal-value', 'en'), '—');
});

test('average inputs are labeled consistently beside probability in both languages', () => {
  assert.equal(workspaceMessage('en', 'finesseOverallAverageInputs'), 'Average inputs');
  assert.equal(workspaceMessage('ko', 'finesseOverallAverageInputs'), '평균 입력');
  assert.equal(workspaceMessage('en', 'finesseSolutionAverageInputs'), 'Average inputs');
  assert.equal(workspaceMessage('ko', 'finesseSolutionAverageInputs'), '평균 입력');
});

test('the GUI drops a malformed representative witness instead of rendering internal text', () => {
  const view = buildProbabilityFinesseView({
    metric: 'inputs',
    mode: 'search',
    pattern_knowledge: 'oracle',
    complete: true,
    exact_total_inputs: '1',
    representative_witness: {
      policy: 'oracle',
      solution_key: 'solution-a',
      pattern_ids: [0],
      queue: ['private-server'],
      total_inputs: 1,
      input_sequence: ['hard-drop'],
      placements: [{ piece: 'I', rotation: 0, x: 0, y: 0 }]
    },
    policy_results: []
  });

  assert.equal(view?.representativeWitness, null);
});

test('the GUI accepts only bounded geometry placements matching hard-drop locks', () => {
  const report = {
    metric: 'inputs',
    mode: 'search',
    pattern_knowledge: 'oracle',
    complete: true,
    exact_total_inputs: '1',
    representative_witness: {
      policy: 'oracle',
      solution_key: 'solution-a',
      pattern_ids: [0],
      queue: ['I'],
      total_inputs: 1,
      input_sequence: ['hard-drop'],
      placements: [{ piece: 'I', rotation: 0, x: 0, y: 0 }]
    },
    policy_results: []
  };

  assert.equal(buildProbabilityFinesseView(report)?.representativeWitness?.placements.length, 1);
  assert.equal(buildProbabilityFinesseView({
    ...report,
    representative_witness: {
      ...report.representative_witness,
      placements: [{ piece: 'I', rotation: 4, x: 0, y: 0 }]
    }
  })?.representativeWitness, null);
  assert.equal(buildProbabilityFinesseView({
    ...report,
    representative_witness: {
      ...report.representative_witness,
      placements: []
    }
  })?.representativeWitness, null);
});

test('only the selected representative solution card receives the witness export', () => {
  const witness = {
    policy: 'oracle',
    solution_key: 'solution-a',
    pattern_ids: [0],
    queue: ['I'],
    total_inputs: 1,
    input_sequence: ['hard-drop'],
    placements: [{ piece: 'I', rotation: 0, x: 0, y: 0 }]
  };
  assert.deepEqual(representativeWitnessExportForSolution(witness, 'solution-a'), {
    solutionKey: 'solution-a',
    totalInputs: 1,
    inputSequence: ['hard-drop'],
    placements: [{ piece: 'I', rotation: 0, x: 0, y: 0 }]
  });
  assert.equal(representativeWitnessExportForSolution(witness, 'solution-b'), null);
  assert.equal(representativeWitnessExportForSolution(null, 'solution-a'), null);
});
