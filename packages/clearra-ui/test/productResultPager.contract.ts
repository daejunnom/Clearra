import assert from 'node:assert/strict';

import type {
  ClearraBuildV2ProductPayload,
  ClearraProductResultPayload,
  ClearraSolutionSetArtifactFormatPayload,
  ClearraSolutionSetArtifactPayload
} from '../src/lib/wasm/wasmCommandClient';
import {
  PRODUCT_MEMBER_PAGE_SIZE,
  isCanonicalDecimal,
  isCanonicalProbability,
  productResultIdentity,
  validateProductResultPayload,
  validateSolutionSetArtifactPayload
} from '../src/lib/workspace/productResultPager';

const coverage = coveragePayload(PRODUCT_MEMBER_PAGE_SIZE);
assert.equal(validateProductResultPayload(coverage), null);
assert.equal(coverage.content.payload.members.length, 100);
assert.match(productResultIdentity(coverage), /a{64}:b{64}$/u);

const oversized = coveragePayload(PRODUCT_MEMBER_PAGE_SIZE + 1);
assert.equal(validateProductResultPayload(oversized), 'invalid coverage portfolio payload');
const leadingZero = coveragePayload(1);
leadingZero.content.payload.members[0]!.candidate_id = '01';
assert.equal(validateProductResultPayload(leadingZero), 'invalid coverage portfolio payload');
assert.equal(isCanonicalDecimal('184467440737095516160'), true);
assert.equal(isCanonicalDecimal('00'), false);
assert.equal(isCanonicalProbability('0.14285714285714285'), true);
assert.equal(isCanonicalProbability('1.2'), false);

const buildV2Capabilities = [
  'build.congruent',
  'build.evaluate.cover',
  'build.evaluate.b2b-cover',
  'build.setup-cover-percent',
  'build.evaluate.cover-percent',
  'build.congruent-cover',
  'build.setup-cover',
  'build.evaluate.minimals',
  'build.setup-cover-score',
  'build.evaluate.score'
] as const;
for (const capability of buildV2Capabilities) {
  const build = buildV2Payload(capability);
  assert.equal(validateProductResultPayload(build), null, capability);
  assert.match(productResultIdentity(build), new RegExp(`^${capability}:`, 'u'));
}

const buildCover = buildCoveragePortfolioPayload();
assert.equal(validateProductResultPayload(buildCover), null);
assert.match(productResultIdentity(buildCover), /d{64}:e{64}$/u);

const buildSetup = buildSetupFamilyPayload();
assert.equal(validateProductResultPayload(buildSetup), null);
assert.match(productResultIdentity(buildSetup), /a{64}:b{64}$/u);

const setupRanked = setupRankedFamilyPayload();
assert.equal(validateProductResultPayload(setupRanked), null);
assert.match(productResultIdentity(setupRanked), /^setup\.joint:setup-joint-ranking\.v2:/u);
const forgedSetupPreference = structuredClone(setupRanked);
forgedSetupPreference.content.payload.resolved_length_preference = 'shorter';
assert.equal(
  validateProductResultPayload(forgedSetupPreference),
  'invalid setup ranked family payload'
);

const setupScore = setupScoreRankingPayload();
assert.equal(validateProductResultPayload(setupScore), null);
assert.match(productResultIdentity(setupScore), /^setup\.score:setup-score-ranking\.v1:/u);
const forgedSetupScoreOrder = structuredClone(setupScore);
forgedSetupScoreOrder.content.payload.candidates.reverse();
assert.equal(
  validateProductResultPayload(forgedSetupScoreOrder),
  'invalid setup score ranking payload'
);

const spinStructure = spinStructureFamilyPayload();
assert.equal(validateProductResultPayload(spinStructure), null);
assert.match(
  productResultIdentity(spinStructure),
  /^spin-structure\.search:spin-structure-family\.v2:/u
);
const forgedSpinPartition = structuredClone(spinStructure);
forgedSpinPartition.content.payload.candidates.reverse();
assert.equal(
  validateProductResultPayload(forgedSpinPartition),
  'invalid spin structure family payload'
);
const guaranteedSpin = structuredClone(spinStructure) as unknown as {
  contract: string;
  result_kind: string;
  content: { payload: Record<string, unknown> };
};
guaranteedSpin.contract = 'spin-structure.guaranteed';
guaranteedSpin.result_kind = 'spin-structure-guaranteed.v1';
guaranteedSpin.content.payload.schema_id = 'spin-structure-guaranteed.v1';
guaranteedSpin.content.payload.guaranteed_final_piece = 'T';
guaranteedSpin.content.payload.guarantee_basis =
  'every-unique-non-target-piece-order-exact-replay-final-piece-last';
guaranteedSpin.content.payload.dependency_report_included = true;
guaranteedSpin.content.payload.dependency_relation = 'non-target-universal-precedence';
guaranteedSpin.content.payload.dependency_edge_count = '0';
assert.equal(
  validateProductResultPayload(guaranteedSpin as unknown as ClearraProductResultPayload),
  null
);
const forgedGuaranteedDependency = structuredClone(guaranteedSpin);
forgedGuaranteedDependency.content.payload.dependency_edge_count = '1';
assert.equal(
  validateProductResultPayload(
    forgedGuaranteedDependency as unknown as ClearraProductResultPayload
  ),
  'invalid spin structure family payload'
);

const spinCover = coveragePayload(2) as unknown as {
  contract: string;
  result_kind: string;
};
spinCover.contract = 'spin-structure.cover';
spinCover.result_kind = 'spin-structure-coverage.v1';
assert.equal(
  validateProductResultPayload(spinCover as unknown as ClearraProductResultPayload),
  null
);

const forgedBuildObjective = buildV2Payload('build.evaluate.minimals');
forgedBuildObjective.content.payload.objective = 'unique';
assert.equal(validateProductResultPayload(forgedBuildObjective), 'invalid Build v2 payload');

const forgedBuildReachability = buildV2Payload('build.congruent');
forgedBuildReachability.content.payload.reachable_candidate_count = '2';
assert.equal(
  validateProductResultPayload(forgedBuildReachability),
  'invalid Build v2 candidate family'
);

const scoreBuild = buildV2Payload('build.evaluate.score');
scoreBuild.content.payload.winners[0]!.informational_attack = '999999';
scoreBuild.content.payload.winners[1]!.informational_attack = '0';
assert.equal(
  validateProductResultPayload(scoreBuild),
  null,
  'Build score equality and winner validity do not use informational attack'
);
const forgedBuildScoreEquality = structuredClone(scoreBuild);
forgedBuildScoreEquality.content.payload.score_equality_basis = null;
assert.equal(
  validateProductResultPayload(forgedBuildScoreEquality),
  'invalid Build v2 score portfolio'
);

const artifact = solutionSetArtifactPayload();
assert.equal(validateSolutionSetArtifactPayload(artifact), null);
const forgedArtifactLength = structuredClone(artifact);
forgedArtifactLength.formats[0]!.byte_length = 1;
assert.equal(
  validateSolutionSetArtifactPayload(forgedArtifactLength),
  'invalid solution-set artifact payload'
);
const noAvailableArtifact = structuredClone(artifact);
noAvailableArtifact.formats[0] = unavailableArtifactFormat('ctk3');
assert.equal(
  validateSolutionSetArtifactPayload(noAvailableArtifact),
  'invalid solution-set artifact payload'
);

const scoreMinimals = scoreMinimalsPayload(3);
assert.equal(validateProductResultPayload(scoreMinimals), null);
assert.deepEqual(
  scoreMinimals.content.payload.members.map((member) => member.candidate_id),
  ['101', '205', '309'],
  'the GUI keeps product-owned score candidate IDs on the canonical tie page'
);
assert.equal(scoreMinimals.content.payload.known_alternative_count, '2');
assert.equal(scoreMinimals.content.payload.enumeration_complete, false);
const forgedScoreMinimalsPair = structuredClone(scoreMinimals) as unknown as {
  result_kind: string;
};
forgedScoreMinimalsPair.result_kind = 'pc-minimum-cover.v2';
assert.equal(
  validateProductResultPayload(
    forgedScoreMinimalsPair as unknown as ClearraProductResultPayload
  ),
  'invalid coverage portfolio payload'
);

const score: ClearraProductResultPayload = {
  contract: 'pc.score',
  result_kind: 'pc-score-summary.v2',
  content: {
    payload_kind: 'score-pattern-winner-family',
    payload: {
      winner_contract: 'pc-score-pattern-winner.v1',
      ordering: 'pattern-id-ascending-then-candidate-id-ascending',
      equality: 'score-only-attack-informational',
      informational_attack_basis: 'canonical-equal-score-trace',
      page_size: '100',
      winner_count: '2',
      winners: [
        {
          pattern_id: '0',
          candidate_id: '1',
          normalized_solution_key: 'first',
          score: '100',
          informational_attack: '99'
        },
        {
          pattern_id: '0',
          candidate_id: '2',
          normalized_solution_key: 'second',
          score: '100',
          informational_attack: '1'
        }
      ]
    }
  }
};
assert.equal(validateProductResultPayload(score), null);
assert.deepEqual(
  score.content.payload.winners.map((winner) => [winner.candidate_id, winner.score]),
  [['1', '100'], ['2', '100']],
  'different attack values cannot remove equal-score winners'
);

const scoreFinder = structuredClone(score);
scoreFinder.contract = 'pc.score-finder';
scoreFinder.result_kind = 'pc-fixed-score-witness.v2';
assert.equal(validateProductResultPayload(scoreFinder), null);
assert.deepEqual(
  scoreFinder.content.payload.winners.map((winner) => [
    winner.candidate_id,
    winner.score,
    winner.informational_attack
  ]),
  [
    ['1', '100', '99'],
    ['2', '100', '1']
  ],
  'score-finder is a normal score-only family and does not use attack to remove winners'
);
const forgedScoreFinderPair = structuredClone(scoreFinder);
forgedScoreFinderPair.result_kind = 'pc-score-summary.v2';
assert.equal(
  validateProductResultPayload(forgedScoreFinderPair),
  'invalid score winner family payload'
);

const pathFamily = pcPathFamilyPayload();
assert.equal(validateProductResultPayload(pathFamily), null);
assert.match(productResultIdentity(pathFamily), /^pc\.path:pc-path-family\.v2:/u);
assert.equal(pathFamily.content.payload.witness_count, '2');
assert.equal(
  Object.prototype.hasOwnProperty.call(pathFamily.content.payload, 'tie_metadata'),
  false,
  'pc.path stays an ordinary complete family without portfolio tie metadata'
);
const forgedPathOrder = structuredClone(pathFamily);
forgedPathOrder.content.payload.witnesses.reverse();
assert.equal(
  validateProductResultPayload(forgedPathOrder),
  'invalid pc.path replay family payload'
);
const forgedPathPair = structuredClone(pathFamily) as unknown as { contract: string };
forgedPathPair.contract = 'pc.minimals';
assert.equal(
  validateProductResultPayload(forgedPathPair as unknown as ClearraProductResultPayload),
  'invalid pc.path replay family payload'
);

const wrongOrdering = structuredClone(score);
if (wrongOrdering.content.payload_kind === 'score-pattern-winner-family') {
  (wrongOrdering.content.payload as { ordering: string }).ordering =
    'score-descending-then-candidate-id-ascending';
}
assert.equal(validateProductResultPayload(wrongOrdering), 'invalid score winner family payload');

const saveGroups = saveGroupsPayload(PRODUCT_MEMBER_PAGE_SIZE + 1);
assert.equal(validateProductResultPayload(saveGroups), null);
assert.equal(saveGroups.content.payload.groups.slice(0, PRODUCT_MEMBER_PAGE_SIZE).length, 100);
assert.equal(saveGroups.content.payload.groups.slice(PRODUCT_MEMBER_PAGE_SIZE).length, 1);
assert.equal(
  saveGroups.content.payload.groups[0]!.unconditional_probability ===
    saveGroups.content.payload.groups[0]!.conditional_probability_given_pc,
  false,
  'whole-universe and conditional-on-PC probabilities remain distinct fields'
);
assert.match(productResultIdentity(saveGroups), /:101$/u);

const forgedCanonicalWinner = structuredClone(saveGroups);
forgedCanonicalWinner.content.payload.groups[0]!.canonical_candidate_id = '999';
assert.equal(
  validateProductResultPayload(forgedCanonicalWinner),
  'invalid pc save groups payload'
);

const bestSave = bestSavePayload(PRODUCT_MEMBER_PAGE_SIZE + 1);
assert.equal(validateProductResultPayload(bestSave), null);
assert.equal(bestSave.content.payload.winners.slice(0, PRODUCT_MEMBER_PAGE_SIZE).length, 100);
assert.equal(bestSave.content.payload.winners.slice(PRODUCT_MEMBER_PAGE_SIZE).length, 1);
assert.deepEqual(
  bestSave.content.payload.winners.map((winner) => winner.group.canonical_candidate_id),
  Array.from({ length: 101 }, (_, index) => (index + 1).toString()),
  'the GUI payload retains the complete canonical-ID-ordered tied family'
);

const forgedTieCursor = structuredClone(bestSave) as unknown as {
  content: { payload: Record<string, unknown> };
};
forgedTieCursor.content.payload.tie_cursor = '2';
assert.equal(
  validateProductResultPayload(forgedTieCursor as unknown as ClearraProductResultPayload),
  'invalid pc best-save payload'
);

const forgedWinnerProbability = structuredClone(bestSave);
forgedWinnerProbability.content.payload.winners[0]!.exact_group_probability = '0.5';
assert.equal(
  validateProductResultPayload(forgedWinnerProbability),
  'invalid pc best-save payload'
);

const parity: ClearraProductResultPayload = {
  contract: 'parity-report.v1',
  result_kind: 'parity',
  content: {
    payload_kind: 'parity-report-page',
    payload: {
      document_format: 'ctk3',
      page_number: 1,
      total_pages: 2,
      coordinate_basis: 'bottom-left-zero-based',
      width: 2,
      height: 1,
      occupied_cell_count: 1,
      checker_black_count: 1,
      checker_white_count: 0,
      checker_delta: 1,
      four_color_counts: [1, 0, 0, 0],
      even_column_count: 1,
      odd_column_count: 0,
      column_parity_delta: 1,
      occupied_area_mod_four: 1,
      pending_garbage_occupied_cell_count: 0,
      feasibility_claim: false,
      pruning_authority: 'none',
      page_handle_available: true
    }
  }
};
assert.equal(validateProductResultPayload(parity), null);
const forgedParity = structuredClone(parity) as unknown as {
  content: { payload: { feasibility_claim: boolean } };
};
forgedParity.content.payload.feasibility_claim = true;
assert.equal(
  validateProductResultPayload(forgedParity as unknown as ClearraProductResultPayload),
  'invalid parity report payload'
);

type BuildV2Capability = (typeof buildV2Capabilities)[number];
type BuildV2ProductResult = Extract<
  ClearraProductResultPayload,
  { content: { payload_kind: 'build-v2' } }
>;

function buildV2Payload(capability: BuildV2Capability): BuildV2ProductResult {
  const payload: ClearraBuildV2ProductPayload = {
    kind: 'candidate-family',
    capability_id: capability,
    result_contract: 'build-congruence-family.v1',
    input_identity_sha256: 'a'.repeat(64),
    evaluation_identity_sha256: 'b'.repeat(64),
    replay_basis: null,
    objective: 'all',
    score_profile: null,
    initial_b2b: null,
    score_accuracy: null,
    profile_specific_exact: null,
    score_equality_basis: null,
    informational_attack_basis: null,
    source_candidate_count: '2',
    reachable_candidate_count: '1',
    selected_candidate_count: null,
    pattern_count: '2',
    covered_pattern_count: '1',
    required_pattern_count: null,
    union_probability: '0.5',
    b2b_preservation_required: null,
    candidates: [
      { candidate_key: 'candidate-a', covered_pattern_count: '1' },
      { candidate_key: 'candidate-b', covered_pattern_count: '0' }
    ],
    canonical_candidate_keys: [],
    winners: [],
    completeness: {
      input_identity_bound: true,
      producer_filter_bound: true,
      buildability_replay_complete: true,
      coverage_rows_complete: true,
      probability_weights_complete: true,
      exact_minimum_proven: false,
      score_evidence_complete: false
    },
    page_source_available: false,
    page_source_identity_sha256: null
  };

  switch (capability) {
    case 'build.congruent':
      payload.objective = 'unique';
      break;
    case 'build.evaluate.cover':
      payload.result_contract = 'build-supplied-coverage.v1';
      payload.b2b_preservation_required = false;
      break;
    case 'build.evaluate.b2b-cover':
      payload.result_contract = 'build-supplied-b2b-coverage.v1';
      payload.b2b_preservation_required = true;
      break;
    case 'build.setup-cover-percent':
    case 'build.evaluate.cover-percent':
      payload.kind = 'probability';
      payload.result_contract =
        capability === 'build.setup-cover-percent'
          ? 'build-setup-cover-probability.v1'
          : 'build-supplied-probability.v1';
      payload.objective = 'unique';
      payload.replay_basis =
        capability === 'build.evaluate.cover-percent'
          ? 'normalized-colored-solution-replay.v1'
          : null;
      payload.b2b_preservation_required = null;
      payload.candidates = [];
      break;
    case 'build.congruent-cover':
    case 'build.setup-cover':
    case 'build.evaluate.minimals':
      payload.kind = 'portfolio';
      payload.result_contract =
        capability === 'build.congruent-cover'
          ? 'build-congruence-coverage.v1'
          : capability === 'build.setup-cover'
            ? 'build-setup-cover.v1'
            : 'build-supplied-minimum-cover.v1';
      payload.objective =
        capability === 'build.congruent-cover' ? 'max-probability-minimum' : 'min-cover';
      payload.evaluation_identity_sha256 = null;
      payload.replay_basis =
        capability === 'build.evaluate.minimals'
          ? 'normalized-colored-solution-replay.v1'
          : null;
      payload.reachable_candidate_count = '2';
      payload.selected_candidate_count = '1';
      payload.covered_pattern_count = null;
      payload.required_pattern_count = '2';
      payload.union_probability = '1';
      payload.candidates = [];
      payload.canonical_candidate_keys = ['candidate-a'];
      payload.completeness.exact_minimum_proven = true;
      payload.page_source_available = true;
      payload.page_source_identity_sha256 = 'c'.repeat(64);
      break;
    case 'build.setup-cover-score':
    case 'build.evaluate.score':
      payload.kind = 'score-portfolio';
      payload.result_contract =
        capability === 'build.setup-cover-score'
          ? 'build-setup-cover-score.v1'
          : 'build-supplied-score.v1';
      payload.objective = 'max-score-cover';
      payload.evaluation_identity_sha256 = null;
      payload.reachable_candidate_count = '2';
      payload.selected_candidate_count = '1';
      payload.covered_pattern_count = null;
      payload.required_pattern_count = '2';
      payload.union_probability = null;
      payload.candidates = [];
      payload.canonical_candidate_keys = ['candidate-a'];
      payload.winners = [
        {
          pattern_id: '0',
          candidate_key: 'candidate-a',
          score: '100',
          informational_attack: '9'
        },
        {
          pattern_id: '1',
          candidate_key: 'candidate-a',
          score: '100',
          informational_attack: '1'
        }
      ];
      payload.score_profile = 'tetrio';
      payload.initial_b2b = '0';
      payload.score_accuracy = 'basic-approximation';
      payload.profile_specific_exact = false;
      payload.score_equality_basis = 'score-only';
      payload.informational_attack_basis = 'canonical-equal-score-trace';
      payload.completeness.exact_minimum_proven = true;
      payload.completeness.score_evidence_complete = true;
      payload.page_source_available = true;
      payload.page_source_identity_sha256 = 'c'.repeat(64);
      break;
  }

  return {
    contract: capability,
    result_kind: payload.result_contract,
    content: { payload_kind: 'build-v2', payload }
  };
}

function buildCoveragePortfolioPayload(): Extract<
  ClearraProductResultPayload,
  { content: { payload_kind: 'build-coverage-portfolio-v2' } }
> {
  return {
    contract: 'build.cover',
    result_kind: 'build-coverage-portfolio.v2',
    content: {
      payload_kind: 'build-coverage-portfolio-v2',
      payload: {
        contract: 'build-coverage-portfolio.v2',
        objective: 'min-cover',
        probability_basis: 'whole-pattern-universe-unconditional',
        source_candidate_count: '2',
        selected_candidate_count: '1',
        pattern_count: '2',
        required_pattern_count: '2',
        union_probability: '1',
        normalized_solution_set_hash: 'd'.repeat(64),
        canonical_first_candidate_id: 'candidate-a',
        completeness: {
          source_universe_complete: true,
          coverage_rows_complete: true,
          probability_weights_complete: true,
          exact_minimum_proven: true,
          query_bound: true
        },
        page_source_available: true,
        page_source_identity_sha256: 'e'.repeat(64)
      }
    }
  };
}

function buildSetupFamilyPayload(): Extract<
  ClearraProductResultPayload,
  { content: { payload_kind: 'build-setup-family-v1' } }
> {
  return {
    contract: 'build.setup',
    result_kind: 'build-target-family.v2',
    content: {
      payload_kind: 'build-setup-family-v1',
      payload: {
        contract: 'build-target-family.v2',
        input_identity_sha256: 'a'.repeat(64),
        evaluation_identity_sha256: 'b'.repeat(64),
        objective: 'unique',
        source_candidate_count: '2',
        reachable_candidate_count: '1',
        pattern_count: '2',
        covered_pattern_count: '1',
        union_probability: '0.5',
        completeness: {
          input_identity_bound: true,
          producer_filter_bound: true,
          buildability_replay_complete: true,
          coverage_rows_complete: true,
          probability_weights_complete: true
        },
        candidates: [
          { candidate_key: 'candidate-a', covered_pattern_count: '1' },
          { candidate_key: 'candidate-b', covered_pattern_count: '0' }
        ]
      }
    }
  };
}

function solutionSetArtifactPayload(): ClearraSolutionSetArtifactPayload {
  return {
    contract: 'solution-set-artifact.v2',
    source_result_kind: 'build-supplied-minimum-cover.v1',
    source_solution_set_contract: 'portfolio-alternative-set.v1',
    selection_kind: 'portfolio-alternative',
    selection_id: 'alternative:1',
    page_source_identity_sha256: 'e'.repeat(64),
    normalized_key_algorithm: 'normalized-colored-solution-key-v1',
    normalized_set_hash_algorithm: 'normalized-solution-set-hash-v1',
    normalized_set_hash: 'normalized-set:test',
    solution_count: 1,
    completeness: 'complete',
    formats: [
      {
        format: 'ctk3',
        state: 'available',
        unavailable_reason: null,
        media_type: 'application/vnd.clearra.ctk3',
        filename: 'clearra-solutions.ctk3',
        byte_length: 9,
        sha256: '0'.repeat(64),
        page_count: 1,
        document: 'ctk3_test'
      },
      unavailableArtifactFormat('fumen')
    ]
  };
}

function setupRankedFamilyPayload(): Extract<
  ClearraProductResultPayload,
  { content: { payload_kind: 'setup-ranked-family' } }
> {
  return {
    contract: 'setup.joint',
    result_kind: 'setup-joint-ranking.v2',
    content: {
      payload_kind: 'setup-ranked-family',
      payload: {
        schema_id: 'setup-joint-ranking.v2',
        query_identity_sha256: '1'.repeat(64),
        rule_profile: 'srs',
        supply_identity_sha256: '2'.repeat(64),
        universe_identity_sha256: '3'.repeat(64),
        product_build: 'v0.8.0',
        ordering: 'joint-probability-descending',
        resolved_length_preference: 'longer',
        candidate_count: '2',
        candidates: [
          {
            candidate_id: 'setup-candidate.v1:1',
            condition_id: 'condition-1',
            setup_id: 'setup-1'
          },
          {
            candidate_id: 'setup-candidate.v1:2',
            condition_id: 'condition-2',
            setup_id: 'setup-2'
          }
        ]
      }
    }
  };
}

function setupScoreRankingPayload(): Extract<
  ClearraProductResultPayload,
  { content: { payload_kind: 'setup-score-ranking' } }
> {
  return {
    contract: 'setup.score',
    result_kind: 'setup-score-ranking.v1',
    content: {
      payload_kind: 'setup-score-ranking',
      payload: {
        schema_id: 'setup-score-ranking.v1',
        input_identity_sha256: '4'.repeat(64),
        evaluation_identity_sha256: '5'.repeat(64),
        document_format: 'ctk3',
        rule_profile: 'srs',
        score_profile: 'guideline',
        initial_b2b: '0',
        ordering: 'unconditional-expected-score-descending-then-canonical-candidate-id',
        source_page_count: '2',
        candidate_count: '2',
        setup_pattern_count: '2',
        average_priority_score: '10',
        complete: true,
        candidates: [
          {
            rank: '1',
            candidate_id: 'candidate-a',
            completed_board_mask: '0x1',
            setup_covered_pattern_count: '2',
            setup_covered_probability: '1',
            continuation_probability: '0.5',
            unconditional_expected_score: '10'
          },
          {
            rank: '2',
            candidate_id: 'candidate-b',
            completed_board_mask: '0x2',
            setup_covered_pattern_count: '1',
            setup_covered_probability: '0.5',
            continuation_probability: '0.5',
            unconditional_expected_score: '10'
          }
        ]
      }
    }
  };
}

function spinStructureFamilyPayload(): Extract<
  ClearraProductResultPayload,
  { content: { payload_kind: 'spin-structure-family' } }
> {
  return {
    contract: 'spin-structure.search',
    result_kind: 'spin-structure-family.v2',
    content: {
      payload_kind: 'spin-structure-family',
      payload: {
        schema_id: 'spin-structure-family.v2',
        query_identity_sha256: '6'.repeat(64),
        rule_profile: 'srs',
        spin_profile: 't-spins',
        supply_identity_sha256: '7'.repeat(64),
        universe_identity_sha256: '8'.repeat(64),
        product_build: 'v0.8.0',
        ordering: 'regular-then-mini-canonical-operation-key',
        minimum_placements: '1',
        guaranteed_final_piece: null,
        guarantee_basis: null,
        dependency_report_included: null,
        dependency_relation: null,
        dependency_edge_count: null,
        regular_count: '1',
        mini_count: '1',
        candidate_count: '2',
        complete: true,
        candidates: [
          {
            candidate_id: 'spin-structure-candidate.v1:regular',
            partition: 'regular',
            placement_count: '2'
          },
          {
            candidate_id: 'spin-structure-candidate.v1:mini',
            partition: 'mini',
            placement_count: '1'
          }
        ]
      }
    }
  };
}

function unavailableArtifactFormat(
  format: 'ctk3' | 'fumen'
): ClearraSolutionSetArtifactFormatPayload {
  return {
    format,
    state: 'unavailable',
    unavailable_reason: 'page-limit-exceeded',
    media_type: null,
    filename: null,
    byte_length: null,
    sha256: null,
    page_count: null,
    document: null
  };
}

function coveragePayload(memberCount: number): Extract<
  ClearraProductResultPayload,
  { content: { payload_kind: 'coverage-portfolio' } }
> {
  return {
    contract: 'pc.minimals',
    result_kind: 'pc-minimum-cover.v2',
    content: {
      payload_kind: 'coverage-portfolio',
      payload: {
        set_contract: 'portfolio-alternative-set.v1',
        page_contract: 'portfolio-alternative-page.v1',
        member_page_contract: 'portfolio-member-page.v1',
        set_identity_sha256: 'a'.repeat(64),
        candidate_map_sha256: 'b'.repeat(64),
        alternative_index: '1',
        optimal_cardinality: memberCount.toString(),
        known_alternative_count: '1',
        total_alternative_count: null,
        enumeration_complete: false,
        member_page_number: '1',
        total_member_pages: Math.max(1, Math.ceil(memberCount / 100)).toString(),
        members: Array.from({ length: memberCount }, (_, index) => ({
          candidate_id: (index + 1).toString(),
          normalized_solution_key: `candidate-${index + 1}`
        })),
        page_handle_available: true
      }
    }
  };
}

function scoreMinimalsPayload(memberCount: number): Extract<
  ClearraProductResultPayload,
  { contract: 'pc.score-minimals' }
> {
  const members = ['101', '205', '309'].slice(0, memberCount).map((candidateId, index) => ({
    candidate_id: candidateId,
    normalized_solution_key: `score-candidate-${index + 1}`
  }));
  return {
    contract: 'pc.score-minimals',
    result_kind: 'pc-score-portfolio.v2',
    content: {
      payload_kind: 'coverage-portfolio',
      payload: {
        set_contract: 'portfolio-alternative-set.v1',
        page_contract: 'portfolio-alternative-page.v1',
        member_page_contract: 'portfolio-member-page.v1',
        set_identity_sha256: 'c'.repeat(64),
        candidate_map_sha256: 'd'.repeat(64),
        alternative_index: '1',
        optimal_cardinality: members.length.toString(),
        known_alternative_count: '2',
        total_alternative_count: null,
        enumeration_complete: false,
        member_page_number: '1',
        total_member_pages: '1',
        members,
        page_handle_available: true
      }
    }
  };
}

function pcPathFamilyPayload(): Extract<
  ClearraProductResultPayload,
  { content: { payload_kind: 'pc-path-family' } }
> {
  const witness = (candidateId: string, patternId: string, trace: string) => ({
    candidate_id: candidateId,
    producer_candidate_id: candidateId,
    pattern_id: patternId,
    trace_identity: `trace-${trace}`,
    normalized_trace_key: trace,
    consumed_piece_count: '1',
    terminal_hold_piece: null,
    steps: [
      {
        step_index: '0',
        operation_id: `operation-${trace}`,
        active_piece: 'I',
        input_cursor: '0',
        output_cursor: '1',
        input_hold_piece: null,
        output_hold_piece: null,
        hold_decision: 'no-hold',
        rotation: 'spawn',
        x: '3',
        y: '0',
        placement_mask: '0x000000000000000f',
        board_before_mask: '0x00000000000003f0',
        board_after_placement_mask: '0x00000000000003ff',
        board_after_line_clear_mask: '0x0000000000000000',
        cleared_row_mask: '0x0000000000000001',
        cleared_lines: '1',
        line_clear_identity: `line-clear-${trace}`
      }
    ]
  });
  return {
    contract: 'pc.path',
    result_kind: 'pc-path-family.v2',
    content: {
      payload_kind: 'pc-path-family',
      payload: {
        witness_contract: 'pc-path-witness.v2',
        ordering:
          'candidate-id-ascending-then-pattern-id-ascending-then-trace-key-ascending',
        problem_id: 'pc-path-problem-v2',
        materialized_pattern_count: '2',
        witness_count: '2',
        complete: true,
        witnesses: [witness('1', '0', 'trace-a'), witness('2', '1', 'trace-b')]
      }
    }
  };
}

function saveGroupsPayload(groupCount: number): Extract<
  ClearraProductResultPayload,
  { content: { payload_kind: 'pc-save-groups' } }
> {
  return {
    contract: 'pc.saves',
    result_kind: 'pc-save-groups.v2',
    content: {
      payload_kind: 'pc-save-groups',
      payload: {
        schema_id: 'clearra-save-v1',
        page_size: '100',
        group_count: groupCount.toString(),
        metadata: saveMetadata('canonical-pc-saves', groupCount),
        groups: Array.from({ length: groupCount }, (_, index) => saveGroup(index))
      }
    }
  };
}

function bestSavePayload(winnerCount: number): Extract<
  ClearraProductResultPayload,
  { content: { payload_kind: 'pc-best-save' } }
> {
  return {
    contract: 'pc.best-save',
    result_kind: 'pc-best-save.v2',
    content: {
      payload_kind: 'pc-best-save',
      payload: {
        schema_id: 'clearra-save-v1',
        probability_basis: 'whole-universe-unconditional',
        ordering:
          'weighted-total-descending-then-balanced-jl-descending-then-unconditional-probability-descending-then-canonical-candidate-id-ascending',
        equality: 'weighted-total-balanced-jl-and-exact-unconditional-probability',
        page_size: '100',
        winner_count: winnerCount.toString(),
        metadata: saveMetadata('canonical-pc-best-save', winnerCount),
        winners: Array.from({ length: winnerCount }, (_, index) => ({
          weighted_total: '7',
          balanced_jl_count: '1',
          exact_group_probability: '0.25',
          group: saveGroup(index)
        }))
      }
    }
  };
}

function saveMetadata(origin: string, patternCount: number) {
  return {
    origin,
    problem_preset: 'scenario-pc' as const,
    problem_id: 'pc-save-test-problem',
    piece_source_id: 'standard-7-bag',
    pattern_universe_id: 'test-universe',
    pattern_weight_model_id: 'uniform',
    materialized_pattern_count: patternCount.toString(),
    pc_success_pattern_count: patternCount.toString(),
    pc_probability: '0.5',
    completeness: {
      source_universe_complete: true,
      fixed_bag_boundary_proven: true,
      execution_batch_complete: true,
      pattern_weights_complete: true,
      count_complete: true,
      probability_complete: true,
      complete: true
    }
  };
}

function saveGroup(index: number) {
  const candidateId = (index + 1).toString();
  return {
    identity_contract: 'terminal-hold-plus-active-bag-remainder-multiset.v1' as const,
    identity: {
      canonical_id: `T${index}I0O0J0L0S0Z0`,
      t: index,
      i: 0,
      o: 0,
      j: 0,
      l: 0,
      s: 0,
      z: 0,
      total_count: index
    },
    successful_pattern_count: '1',
    unconditional_probability: '0.25',
    conditional_probability_given_pc: '0.5',
    canonical_candidate_id: candidateId,
    witnesses: [
      {
        pattern_index: index.toString(),
        candidate_id: candidateId,
        trace_identity: `trace-${candidateId}`,
        source_cursor: '1',
        terminal_hold: null,
        active_bag_remainder: {
          canonical_id: 'T0I0O0J0L0S0Z0',
          t: 0,
          i: 0,
          o: 0,
          j: 0,
          l: 0,
          s: 0,
          z: 0,
          total_count: 0
        }
      }
    ]
  };
}

console.log(JSON.stringify({
  member_page_boundary: PRODUCT_MEMBER_PAGE_SIZE,
  decimal_identity: 'string-exact',
  score_tie_equality: 'attack-independent',
  pc_save_family_paging: 'finite-100',
  pc_best_save_ties: 'ordinary-complete-list'
}));
