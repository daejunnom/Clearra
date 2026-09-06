// SRP rationale: this contract test has one behavior-level change reason: verifying the complete fail-closed product-result validation, canonical identity, and paging boundary exposed by the workspace pager.
import assert from 'node:assert/strict';

import type {
  ClearraBuildV2ProductPayload,
  ClearraCoveragePortfolioRuntimePage,
  ClearraProductPageWorkerPayload,
  ClearraProductResultPayload,
  ClearraSolutionSetArtifactFormatPayload,
  ClearraSolutionSetArtifactPayload
} from '../src/lib/wasm/wasmCommandClient';
import {
  CoveragePortfolioPagerController,
  PRODUCT_MEMBER_PAGE_SIZE,
  isCanonicalDecimal,
  isCanonicalProbability,
  loadCoveragePortfolioExactPage,
  productResultIdentity,
  validateCoveragePortfolioRuntimePage,
  validateProductResultPayload,
  validateSolutionSetArtifactPayload
} from '../src/lib/workspace/productResultPager';

const coverage = coveragePayload(PRODUCT_MEMBER_PAGE_SIZE);
assert.equal(validateProductResultPayload(coverage), null);
const buildScoreCoverage = coveragePayload(1);
buildScoreCoverage.contract = 'build.highest-score-minimum-set';
buildScoreCoverage.result_kind = 'build-probability-score-minimum.v1';
assert.equal(validateProductResultPayload(buildScoreCoverage), null,
  'Build score minimum uses the owner-bound runtime portfolio page after initial loading');
const invalidBuildScorePair = structuredClone(buildScoreCoverage);
invalidBuildScorePair.result_kind = 'pc-score-portfolio.v2';
assert.equal(validateProductResultPayload(invalidBuildScorePair), 'invalid coverage portfolio payload');
assert.equal(coverage.content.payload.members.length, 100);
assert.match(productResultIdentity(coverage), /a{64}:b{64}$/u);
const forgedPageHandle = structuredClone(coverage) as unknown as {
  content: { payload: { page_handle_available: unknown } };
};
forgedPageHandle.content.payload.page_handle_available = 'true';
assert.equal(
  validateProductResultPayload(forgedPageHandle as unknown as ClearraProductResultPayload),
  'invalid coverage portfolio payload',
  'coverage portfolios reject non-boolean page handle authority'
);

const oversized = coveragePayload(PRODUCT_MEMBER_PAGE_SIZE + 1);
assert.equal(validateProductResultPayload(oversized), 'invalid coverage portfolio payload');
const leadingZero = coveragePayload(1);
leadingZero.content.payload.members[0]!.candidate_id = '01';
assert.equal(validateProductResultPayload(leadingZero), 'invalid coverage portfolio payload');
const completeEmpty = emptyCoveragePayload();
assert.equal(
  validateProductResultPayload(completeEmpty),
  null,
  'a complete zero-cardinality portfolio is a successful empty result'
);
const incompleteEmpty = structuredClone(completeEmpty);
incompleteEmpty.content.payload.enumeration_complete = false;
incompleteEmpty.content.payload.total_alternative_count = null;
assert.equal(
  validateProductResultPayload(incompleteEmpty),
  'invalid coverage portfolio payload',
  'an empty page cannot claim success without sealed enumeration authority'
);
assert.equal(isCanonicalDecimal('184467440737095516160'), true);
assert.equal(isCanonicalDecimal('00'), false);
assert.equal(isCanonicalProbability('0.14285714285714285'), true);
assert.equal(isCanonicalProbability('1.2'), false);

await verifyCoveragePortfolioPagerNavigation();
await verifyCoveragePortfolioPagerDemandLoadsOnlyTheVisibleAlternative();
await verifyCoveragePortfolioPagerPrunesAdjacentPrefetchOnBacktrack();
await verifyCoveragePortfolioPagerResetIgnoresStaleGet();
await verifyCoveragePortfolioPagerUsesCanonicalLargeIndices();
await verifyCoveragePortfolioReplayRequiresBoundedProgress();
await verifySharedExactPageLoaderPreservesCancellationAndGeneration();
await verifyCancelledPortfolioPrefetchDoesNotForgeSealedEnumeration();
verifyCoveragePortfolioPageValidator();

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
  'build.evaluate.score',
  'build.highest-score-minimum-set'
] as const;
for (const capability of buildV2Capabilities) {
  const build = buildV2Payload(capability);
  assert.equal(validateProductResultPayload(build), null, capability);
  assert.match(productResultIdentity(build), new RegExp(`^${capability}:`, 'u'));
}

const buildCover = buildCoveragePortfolioPayload();
assert.equal(validateProductResultPayload(buildCover), null);
const emptyBuildCover = buildCoveragePortfolioPayload();
Object.assign(emptyBuildCover.content.payload, {
  source_candidate_count: '0', selected_candidate_count: '0', required_pattern_count: '0',
  union_probability: '0', canonical_first_candidate_id: ''
});
assert.equal(validateProductResultPayload(emptyBuildCover), null, 'exact empty Build cover remains a successful pageable product');
const incompleteEmptyBuildCover = structuredClone(emptyBuildCover);
incompleteEmptyBuildCover.content.payload.completeness.exact_minimum_proven = false;
assert.equal(validateProductResultPayload(incompleteEmptyBuildCover), 'invalid Build coverage portfolio payload');
assert.match(productResultIdentity(buildCover), /cts1:ea2c4fa12ddc1b01:e{64}$/u);
for (const invalidHash of ['d'.repeat(64), 'cts1:abc', 'ea2c4fa12ddc1b01']) {
  const malformed = structuredClone(buildCover);
  malformed.content.payload.normalized_solution_set_hash = invalidHash;
  assert.equal(validateProductResultPayload(malformed), 'invalid Build coverage portfolio payload');
}

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
    payload_kind: 'pc-score-field-summary',
    payload: {
      field_contract: 'pc-score-solution-field-average.v1',
      ordering: 'normalized-solution-field-order',
      solution_field_average_basis: 'whole-materialized-pattern-universe-failed-pc-zero',
      score_evaluation_basis: 'all-traces',
      score_evaluation_scope: 'full',
      overall_score_basis: 'all-materialized-patterns-failed-pc-zero',
      piece_source_id: '101',
      pattern_universe_id: '202',
      pattern_weight_model_id: '303',
      materialized_pattern_count: '2',
      solution_field_count: '2',
      scored_pattern_count: '2',
      failed_pc_pattern_count: '0',
      covered_probability: '1',
      overall_score: '75',
      score_covered_pattern_conditional_average_score: '75',
      complete: true,
      fields: [
        {
          normalized_field_key:
            'ctk1|initial=0000000000000000|placements=I:000000000000000f',
          average_score: '50',
          covered_pattern_count: '1',
          pattern_count: '2',
          score_complete: true
        },
        {
          normalized_field_key:
            'ctk1|initial=0000000000000000|placements=O:0000000000000033',
          average_score: '25',
          covered_pattern_count: '1',
          pattern_count: '2',
          score_complete: true
        }
      ]
    }
  }
};
assert.equal(validateProductResultPayload(score), null);
assert.deepEqual(
  score.content.payload.fields.map((field) => [field.normalized_field_key, field.average_score]),
  [
    ['ctk1|initial=0000000000000000|placements=I:000000000000000f', '50'],
    ['ctk1|initial=0000000000000000|placements=O:0000000000000033', '25']
  ],
  'ordinary score keeps every normalized field and its whole-universe average'
);
assert.ok(
  Number(score.content.payload.overall_score) >
    Math.max(...score.content.payload.fields.map((field) => Number(field.average_score))),
  'the whole-result score is the per-pattern optimum across all fields, not one field average'
);
assert.match(productResultIdentity(score), /^pc\.score:pc-score-summary\.v2:202:303:2:75$/u);
const forgedFieldUniverse = structuredClone(score);
forgedFieldUniverse.content.payload.fields[1]!.pattern_count = '1';
assert.equal(
  validateProductResultPayload(forgedFieldUniverse),
  'invalid PC score field summary payload'
);
const forgedFieldOrder = structuredClone(score);
forgedFieldOrder.content.payload.fields.reverse();
assert.equal(
  validateProductResultPayload(forgedFieldOrder),
  'invalid PC score field summary payload',
  'PC score fields must follow the declared strict normalized-field order'
);
const buildFieldAverage = structuredClone(score) as unknown as {
  contract: string;
  result_kind: string;
  content: {
    payload_kind: 'pc-score-field-summary';
    payload: typeof score.content.payload & { field_contract: string };
  };
};
buildFieldAverage.contract = 'build.field-average-score';
buildFieldAverage.result_kind = 'build-field-average-score.v1';
buildFieldAverage.content.payload.field_contract = 'build-solution-field-average.v1';
assert.equal(
  validateProductResultPayload(buildFieldAverage as unknown as ClearraProductResultPayload),
  null,
  'Build field average reuses the score-row shape under an explicit Build contract'
);
const forgedBuildScoreAsPc = structuredClone(buildFieldAverage);
forgedBuildScoreAsPc.contract = 'pc.score';
forgedBuildScoreAsPc.result_kind = 'pc-score-summary.v2';
assert.equal(
  validateProductResultPayload(forgedBuildScoreAsPc as unknown as ClearraProductResultPayload),
  'invalid PC score field summary payload',
  'Build field-average content cannot be cross-relabeled as PC score'
);

const scoreFinder: ClearraProductResultPayload = {
  contract: 'pc.score-finder',
  result_kind: 'pc-fixed-score-witness.v2',
  content: {
    payload_kind: 'score-pattern-winner-family',
    payload: {
      winner_contract: 'pc-score-pattern-winner.v1',
      ordering: 'pattern-id-ascending-then-candidate-id-ascending',
      equality: 'score-only-attack-informational',
      informational_attack_basis: 'canonical-equal-score-trace',
      page_size: '100',
      winner_count: '2',
      canonical_selection: 'smallest-canonical-candidate-id',
      canonical_winner: {
        pattern_id: '0',
        candidate_id: '1',
        normalized_solution_key: 'first',
        score: '100',
        informational_attack: '99'
      },
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
const buildFixedScore = structuredClone(scoreFinder) as unknown as {
  contract: string;
  result_kind: string;
  content: { payload: { winner_contract: string } };
};
buildFixedScore.contract = 'build.fixed-queue-maximum-score';
buildFixedScore.result_kind = 'build-fixed-score-witness.v1';
buildFixedScore.content.payload.winner_contract = 'build-score-pattern-winner.v1';
assert.equal(
  validateProductResultPayload(buildFixedScore as unknown as ClearraProductResultPayload),
  null,
  'Build fixed-queue score uses the same score-only family under its own nominal contract'
);
const forgedBuildFixedAsPc = structuredClone(buildFixedScore);
forgedBuildFixedAsPc.contract = 'pc.score-finder';
forgedBuildFixedAsPc.result_kind = 'pc-fixed-score-witness.v2';
assert.equal(
  validateProductResultPayload(forgedBuildFixedAsPc as unknown as ClearraProductResultPayload),
  'invalid score winner family payload',
  'Build fixed-score content cannot be cross-relabeled as PC score-finder'
);
const forgedScoreFinderPair = structuredClone(scoreFinder) as unknown as { result_kind: string };
forgedScoreFinderPair.result_kind = 'pc-score-summary.v2';
assert.equal(
  validateProductResultPayload(forgedScoreFinderPair as unknown as ClearraProductResultPayload),
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

const buildPathFamily = structuredClone(pathFamily) as unknown as {
  contract: string;
  result_kind: string;
  content: {
    payload_kind: string;
    payload: typeof pathFamily.content.payload & {
      witness_contract: string;
      target_terminal_board_mask: string;
      mirrored_terminal_board_mask?: string | null;
    };
  };
};
buildPathFamily.contract = 'build.complete-replay-paths';
buildPathFamily.result_kind = 'build-path-family.v1';
buildPathFamily.content.payload_kind = 'build-path-family';
(buildPathFamily.content.payload as { witness_contract: string }).witness_contract =
  'build-path-witness.v1';
buildPathFamily.content.payload.target_terminal_board_mask = '0x0000000000000001';
for (const witness of buildPathFamily.content.payload.witnesses) {
  witness.steps.at(-1)!.board_after_line_clear_mask = '0x0000000000000001';
}
buildPathFamily.content.payload.canonical_witness = structuredClone(
  buildPathFamily.content.payload.witnesses[0]!
);
assert.equal(
  validateProductResultPayload(buildPathFamily as unknown as ClearraProductResultPayload),
  null,
  'Build replay explicitly accepts the requested non-empty terminal'
);
const forgedBuildTerminal = structuredClone(buildPathFamily);
forgedBuildTerminal.content.payload.target_terminal_board_mask = '0x0000000000000002';
assert.equal(
  validateProductResultPayload(forgedBuildTerminal as unknown as ClearraProductResultPayload),
  'invalid Build replay family payload',
  'Build replay rejects a target relabel that does not match every terminal witness'
);
const mirroredBuildFamily = structuredClone(buildPathFamily);
mirroredBuildFamily.content.payload.mirrored_terminal_board_mask = '0x0000000000000200';
for (const witness of mirroredBuildFamily.content.payload.witnesses) {
  witness.steps.at(-1)!.board_after_line_clear_mask = '0x0000000000000200';
}
mirroredBuildFamily.content.payload.canonical_witness = structuredClone(
  mirroredBuildFamily.content.payload.witnesses[0]!
);
assert.equal(
  validateProductResultPayload(mirroredBuildFamily as unknown as ClearraProductResultPayload),
  null,
  'Build replay preserves an explicitly authorized horizontal-mirror target'
);
mirroredBuildFamily.content.payload.mirrored_terminal_board_mask = '0x0000000000000100';
assert.equal(
  validateProductResultPayload(mirroredBuildFamily as unknown as ClearraProductResultPayload),
  'invalid Build replay family payload',
  'an arbitrary alternative terminal cannot claim horizontal-mirror authority'
);
const forgedBuildAsPc = structuredClone(buildPathFamily);
forgedBuildAsPc.contract = 'pc.path';
forgedBuildAsPc.result_kind = 'pc-path-family.v2';
assert.equal(
  validateProductResultPayload(forgedBuildAsPc as unknown as ClearraProductResultPayload),
  'invalid Build replay family payload',
  'Build path content cannot be cross-relabeled as a PC path result'
);

const wrongOrdering = structuredClone(scoreFinder);
if (wrongOrdering.content.payload_kind === 'score-pattern-winner-family') {
  (wrongOrdering.content.payload as { ordering: string }).ordering =
    'score-descending-then-candidate-id-ascending';
}
assert.equal(validateProductResultPayload(wrongOrdering), 'invalid score winner family payload');
const forgedScoreFinderOrder = structuredClone(scoreFinder);
forgedScoreFinderOrder.content.payload.winners.reverse();
assert.equal(
  validateProductResultPayload(forgedScoreFinderOrder),
  'invalid score winner family payload',
  'score winners must follow the declared strict pattern/candidate order'
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
    case 'build.highest-score-minimum-set':
      payload.kind = 'score-portfolio';
      payload.result_contract =
        capability === 'build.setup-cover-score'
          ? 'build-setup-cover-score.v1'
          : capability === 'build.evaluate.score'
            ? 'build-supplied-score.v1'
            : 'build-probability-score-minimum.v1';
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
        normalized_solution_set_hash: 'cts1:ea2c4fa12ddc1b01',
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

function emptyCoveragePayload(): Extract<
  ClearraProductResultPayload,
  { content: { payload_kind: 'coverage-portfolio' } }
> {
  const payload = coveragePayload(1);
  payload.content.payload.optimal_cardinality = '0';
  payload.content.payload.known_alternative_count = '1';
  payload.content.payload.total_alternative_count = '1';
  payload.content.payload.enumeration_complete = true;
  payload.content.payload.member_page_number = '1';
  payload.content.payload.total_member_pages = '1';
  payload.content.payload.members = [];
  return payload;
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
        canonical_selection: 'smallest-canonical-candidate-id',
        canonical_witness: witness('1', '0', 'trace-a'),
        witnesses: [witness('1', '0', 'trace-a'), witness('2', '1', 'trace-b')]
      }
    }
  };
}

async function verifyCoveragePortfolioPagerNavigation(): Promise<void> {
  const nextResponses = [
    deferred<ClearraProductPageWorkerPayload>(),
    deferred<ClearraProductPageWorkerPayload>(),
    deferred<ClearraProductPageWorkerPayload>()
  ];
  const pageThreeGet = deferred<ClearraProductPageWorkerPayload>();
  const getCalls: Array<readonly [string, string]> = [];
  let loadNextCallCount = 0;
  const controller = new CoveragePortfolioPagerController({
    loadNextPage: () => {
      const response = nextResponses[loadNextCallCount];
      loadNextCallCount += 1;
      if (!response) throw new Error('unexpected loadNextPage call');
      return response.promise;
    },
    loadMemberPage: (alternativeNumber, memberPageNumber) => {
      getCalls.push([alternativeNumber, memberPageNumber]);
      if (alternativeNumber === '1' && memberPageNumber === '1') {
        return Promise.resolve(coveragePageResponse(runtimeCoveragePage(1)));
      }
      if (alternativeNumber === '3' && memberPageNumber === '1') {
        return pageThreeGet.promise;
      }
      if (alternativeNumber === '4' && memberPageNumber === '1') {
        return Promise.resolve(
          coveragePageResponse(runtimeCoveragePage(4, { complete: true }))
        );
      }
      throw new Error(`unexpected member page request ${alternativeNumber}:${memberPageNumber}`);
    },
    onChange: (snapshot) => assertCoveragePagerWindow(snapshot)
  });

  controller.reset('portfolio-old', runtimeCoveragePage(1), { autoPrefetch: true });
  assert.equal(loadNextCallCount, 1, 'reset starts exactly one page-2 prefetch');
  nextResponses[0]!.resolve(coveragePageResponse(runtimeCoveragePage(2)));
  await settlePromises();
  assert.equal(controller.snapshot().prefetchedPage?.alternative_index, '2');

  assert.equal((await controller.next())?.alternative_index, '2');
  assert.equal(loadNextCallCount, 2, 'showing page 2 starts exactly one page-3 prefetch');
  nextResponses[1]!.resolve(coveragePageResponse(runtimeCoveragePage(3)));
  await settlePromises();
  assert.equal((await controller.next())?.alternative_index, '3');
  assert.equal(loadNextCallCount, 3, 'showing page 3 starts exactly one page-4 prefetch');
  assert.equal(controller.snapshot().prefetchInFlight, true);
  assert.deepEqual(
    controller.snapshot().pages.map((page) => page.alternative_index),
    ['2', '3'],
    'the outer cache remains bounded while page 4 is in flight'
  );

  assert.equal((await controller.previous())?.alternative_index, '2');
  assert.equal((await controller.previous())?.alternative_index, '1');
  assert.deepEqual(getCalls, [['1', '1']], 'only evicted page 1 is reloaded on 3 -> 2 -> 1');

  nextResponses[2]!.resolve(coveragePageResponse(runtimeCoveragePage(4, { complete: true })));
  await settlePromises();
  const discardedPrefetchSnapshot = controller.snapshot();
  assert.equal(discardedPrefetchSnapshot.currentPage?.alternative_index, '1');
  assert.deepEqual(
    discardedPrefetchSnapshot.pages.map((page) => page.alternative_index),
    ['1', '2']
  );
  assert.equal(
    discardedPrefetchSnapshot.prefetchedPage,
    null,
    'page 4 is not retained while only current page 1 and next page 2 are in-window'
  );
  assert.equal(discardedPrefetchSnapshot.highestMaterializedAlternativeIndex, '4');
  assert.equal(discardedPrefetchSnapshot.enumerationSealed, true);
  assert.equal(discardedPrefetchSnapshot.prefetchInFlight, false);

  assert.equal((await controller.next())?.alternative_index, '2');
  assert.equal(loadNextCallCount, 3, 'backtracking never advances the enumerator');

  const forwardToThree = controller.next();
  const duplicateForward = controller.next();
  assert.equal(await duplicateForward, null, 'a double-click joins no second navigation');
  assert.equal(controller.snapshot().navigating, true, 'the first reload remains single-flight');
  assert.deepEqual(
    getCalls,
    [
      ['1', '1'],
      ['3', '1']
    ],
    'the evicted page 3 is requested exactly once'
  );
  pageThreeGet.resolve(coveragePageResponse(runtimeCoveragePage(3)));
  assert.equal((await forwardToThree)?.alternative_index, '3');
  assert.equal(controller.snapshot().navigating, false);
  assert.equal(loadNextCallCount, 3);

  assert.equal(
    (await controller.next())?.alternative_index,
    '4',
    'the discarded high-water page is recovered through exact replay'
  );
  const finalSnapshot = controller.snapshot();
  assert.equal(finalSnapshot.currentPage?.alternative_index, '4');
  assert.equal(finalSnapshot.prefetchedPage, null);
  assert.equal(finalSnapshot.prefetchInFlight, false);
  assert.equal(finalSnapshot.enumerationSealed, true);
  assert.deepEqual(getCalls, [
    ['1', '1'],
    ['3', '1'],
    ['4', '1']
  ]);
  assert.equal(loadNextCallCount, 3, 'exact replay never duplicates the completed next request');
  controller.dispose();
}

async function verifyCoveragePortfolioPagerPrunesAdjacentPrefetchOnBacktrack(): Promise<void> {
  const getCalls: Array<readonly [string, string]> = [];
  let loadNextCallCount = 0;
  const controller = new CoveragePortfolioPagerController({
    loadNextPage: () => {
      loadNextCallCount += 1;
      return Promise.resolve(coveragePageResponse(runtimeCoveragePage(3, { complete: true })));
    },
    loadMemberPage: (alternativeIndex, memberPageNumber) => {
      getCalls.push([alternativeIndex, memberPageNumber]);
      return Promise.resolve(
        coveragePageResponse(
          runtimeCoveragePage(alternativeIndex, {
            complete: alternativeIndex === '3'
          })
        )
      );
    },
    onChange: (snapshot) => assertCoveragePagerWindow(snapshot)
  });

  controller.reset('portfolio-prune', runtimeCoveragePage(2), { autoPrefetch: true });
  await settlePromises();
  assert.equal(controller.snapshot().prefetchedPage?.alternative_index, '3');
  assert.equal((await controller.previous())?.alternative_index, '1');
  assert.equal(controller.snapshot().prefetchedPage, null);
  assert.equal(controller.snapshot().highestMaterializedAlternativeIndex, '3');
  assert.equal((await controller.next())?.alternative_index, '2');
  assert.equal((await controller.next())?.alternative_index, '3');
  assert.equal(loadNextCallCount, 1);
  assert.deepEqual(getCalls, [
    ['1', '1'],
    ['3', '1']
  ]);
  controller.dispose();
}

async function verifyCoveragePortfolioPagerDemandLoadsOnlyTheVisibleAlternative(): Promise<void> {
  let loadNextCallCount = 0;
  const controller = new CoveragePortfolioPagerController({
    loadNextPage: () => {
      loadNextCallCount += 1;
      return Promise.resolve(
        coveragePageResponse(
          runtimeCoveragePage(loadNextCallCount + 1, {
            complete: loadNextCallCount === 2
          })
        )
      );
    },
    loadMemberPage: null
  });

  controller.reset('portfolio-demand-only', runtimeCoveragePage(1));
  await settlePromises();
  assert.equal(loadNextCallCount, 0, 'opening a result does not compute the hidden next page');
  assert.equal(controller.snapshot().prefetchedPage, null);

  assert.equal((await controller.next())?.alternative_index, '2');
  await settlePromises();
  assert.equal(
    loadNextCallCount,
    1,
    'showing page 2 does not compute page 3 until the user asks for it'
  );
  assert.equal(controller.snapshot().prefetchedPage, null);

  assert.equal((await controller.next())?.alternative_index, '3');
  assert.equal(loadNextCallCount, 2, 'each explicit next action advances exactly one page');
  controller.dispose();
}

async function verifyCoveragePortfolioPagerResetIgnoresStaleGet(): Promise<void> {
  const staleGet = deferred<ClearraProductPageWorkerPayload>();
  const getSignals: AbortSignal[] = [];
  let getCallCount = 0;
  const controller = new CoveragePortfolioPagerController({
    loadNextPage: null,
    loadMemberPage: (_alternativeNumber, _memberPageNumber, signal) => {
      getCallCount += 1;
      if (signal) getSignals.push(signal);
      return staleGet.promise;
    }
  });
  controller.reset('portfolio-old', runtimeCoveragePage(2), {
    autoPrefetch: false
  });
  const pendingPrevious = controller.previous();
  assert.equal(getCallCount, 1);
  assert.equal(controller.snapshot().navigating, true);

  const replacement = runtimeCoveragePage(1, {
    setIdentitySha256: 'e'.repeat(64),
    candidateMapSha256: 'f'.repeat(64)
  });
  controller.reset('portfolio-new', replacement, { autoPrefetch: false });
  assert.equal(getSignals[0]?.aborted, true, 'reset aborts the in-flight get signal');
  staleGet.resolve(coveragePageResponse(runtimeCoveragePage(1)));
  assert.equal(await pendingPrevious, null);
  await settlePromises();

  const snapshot = controller.snapshot();
  assert.equal(snapshot.identity, 'portfolio-new');
  assert.equal(snapshot.currentPage?.set_identity_sha256, 'e'.repeat(64));
  assert.equal(snapshot.currentPage?.alternative_index, '1');
  assert.equal(snapshot.error, '');
  assert.equal(snapshot.navigating, false);
  assert.equal(getCallCount, 1, 'the stale completion does not retry or mutate the new pager');
  controller.dispose();
}

async function verifyCoveragePortfolioPagerUsesCanonicalLargeIndices(): Promise<void> {
  const largeAlternativeIndex = '900719925474099312345678901234567890';
  const nextAlternativeIndex = (BigInt(largeAlternativeIndex) + 1n).toString();
  const previousAlternativeIndex = (BigInt(largeAlternativeIndex) - 1n).toString();
  const memberRequests: Array<readonly [string, string]> = [];
  let nextCallCount = 0;
  const controller = new CoveragePortfolioPagerController({
    loadNextPage: () => {
      nextCallCount += 1;
      return Promise.resolve(
        coveragePageResponse(runtimeCoveragePage(nextAlternativeIndex, { complete: true }))
      );
    },
    loadMemberPage: (alternativeIndex, memberPageNumber) => {
      memberRequests.push([alternativeIndex, memberPageNumber]);
      return Promise.resolve(coveragePageResponse(runtimeCoveragePage(previousAlternativeIndex)));
    },
    onChange: (snapshot) => assertCoveragePagerWindow(snapshot)
  });

  controller.reset('portfolio-large', runtimeCoveragePage(largeAlternativeIndex), {
    autoPrefetch: true
  });
  await settlePromises();
  assert.equal(nextCallCount, 1);
  assert.equal(controller.snapshot().prefetchedPage?.alternative_index, nextAlternativeIndex);
  assert.equal((await controller.next())?.alternative_index, nextAlternativeIndex);
  assert.equal(nextCallCount, 1, 'a complete large-index page does not request a successor');
  assert.equal((await controller.previous())?.alternative_index, largeAlternativeIndex);
  assert.equal((await controller.previous())?.alternative_index, previousAlternativeIndex);
  assert.deepEqual(
    memberRequests,
    [[previousAlternativeIndex, '1']],
    'large alternative identity reaches the loader as an exact decimal string'
  );
  controller.dispose();
}

async function verifyCoveragePortfolioReplayRequiresBoundedProgress(): Promise<void> {
  let replayCalls = 0;
  const resumable = new CoveragePortfolioPagerController({
    loadNextPage: null,
    loadMemberPage: (alternativeIndex, memberPageNumber, _signal, maximumWorkSteps) => {
      replayCalls += 1;
      assert.equal(alternativeIndex, '1');
      assert.equal(memberPageNumber, '1');
      assert.equal(maximumWorkSteps, 10_000);
      if (replayCalls === 1) {
        return Promise.resolve({
          schema_version: 1,
          runtime: 'clearra-wasm',
          product_page_kind: 'coverage-portfolio',
          state: 'work-budget-exhausted',
          known_alternative_count: '2',
          enumeration_complete: false,
          work_steps: 1,
          replay_cursor_alternative_index: '1'
        });
      }
      return Promise.resolve(coveragePageResponse(runtimeCoveragePage(1)));
    }
  });
  resumable.reset('portfolio-resumable', runtimeCoveragePage(2), { autoPrefetch: false });
  assert.equal((await resumable.previous())?.alternative_index, '1');
  assert.equal(replayCalls, 2, 'an incomplete slice yields before the exact retry');
  resumable.dispose();

  const stalled = new CoveragePortfolioPagerController({
    loadNextPage: null,
    loadMemberPage: () =>
      Promise.resolve({
        schema_version: 1,
        runtime: 'clearra-wasm',
        product_page_kind: 'coverage-portfolio',
        state: 'work-budget-exhausted',
        known_alternative_count: '2',
        enumeration_complete: false,
        work_steps: 0,
        replay_cursor_alternative_index: '1'
      })
  });
  stalled.reset('portfolio-zero-progress', runtimeCoveragePage(2), { autoPrefetch: false });
  assert.equal(await stalled.previous(), null);
  assert.match(stalled.snapshot().error, /no bounded progress/u);
  assert.equal(stalled.snapshot().currentPage?.alternative_index, '2');
  stalled.dispose();
}

async function verifySharedExactPageLoaderPreservesCancellationAndGeneration(): Promise<void> {
  const expected = runtimeCoveragePage(1);
  let calls = 0;
  const loaded = await loadCoveragePortfolioExactPage({
    alternativeIndex: '1',
    memberPageNumber: '1',
    expectation: {
      setIdentitySha256: expected.set_identity_sha256,
      candidateMapSha256: expected.candidate_map_sha256,
      alternativeIndex: '1',
      memberPageNumber: '1'
    },
    loadMemberPage: () => {
      calls += 1;
      return Promise.resolve(
        calls === 1
          ? {
              schema_version: 1,
              runtime: 'clearra-wasm',
              product_page_kind: 'coverage-portfolio',
              state: 'work-budget-exhausted',
              known_alternative_count: '2',
              enumeration_complete: false,
              work_steps: 1,
              replay_cursor_alternative_index: '1'
            }
          : coveragePageResponse(expected)
      );
    }
  });
  assert.equal(loaded?.alternative_index, '1');
  assert.equal(calls, 2, 'every exact-page consumer resumes the shared bounded replay contract');

  const cancelled = await loadCoveragePortfolioExactPage({
    alternativeIndex: '1',
    memberPageNumber: '1',
    expectation: {
      setIdentitySha256: expected.set_identity_sha256,
      alternativeIndex: '1',
      memberPageNumber: '1'
    },
    loadMemberPage: () => Promise.resolve({
      schema_version: 1,
      runtime: 'clearra-desktop',
      product_page_kind: 'coverage-portfolio',
      state: 'cancelled',
      known_alternative_count: '1',
      enumeration_complete: false,
      work_steps: 0,
      replay_cursor_alternative_index: null
    })
  });
  assert.equal(cancelled, null, 'a cancelled replay never publishes a page');

  let current = true;
  const deferredPage = deferred<ClearraProductPageWorkerPayload>();
  const staleLoad = loadCoveragePortfolioExactPage({
    alternativeIndex: '1',
    memberPageNumber: '1',
    expectation: {
      setIdentitySha256: expected.set_identity_sha256,
      alternativeIndex: '1',
      memberPageNumber: '1'
    },
    isCurrent: () => current,
    loadMemberPage: () => deferredPage.promise
  });
  current = false;
  deferredPage.resolve(coveragePageResponse(expected));
  assert.equal(await staleLoad, null, 'a replaced generation never publishes its late page');
}

async function verifyCancelledPortfolioPrefetchDoesNotForgeSealedEnumeration(): Promise<void> {
  const controller = new CoveragePortfolioPagerController({
    loadNextPage: () =>
      Promise.resolve({
        schema_version: 1,
        runtime: 'clearra-desktop',
        product_page_kind: 'coverage-portfolio',
        state: 'cancelled',
        known_alternative_count: '1',
        enumeration_complete: false,
        work_steps: 0,
        replay_cursor_alternative_index: null
      }),
    loadMemberPage: null
  });
  controller.reset('portfolio-cancelled-prefetch', runtimeCoveragePage(1), {
    autoPrefetch: true
  });
  await settlePromises();
  assert.equal(controller.snapshot().enumerationSealed, false);
  assert.equal(controller.snapshot().prefetchedPage, null);
  assert.equal(controller.snapshot().error, '');
  controller.dispose();
}

function verifyCoveragePortfolioPageValidator(): void {
  const valid = runtimeCoveragePage(1);
  const expectation = {
    setIdentitySha256: valid.set_identity_sha256,
    candidateMapSha256: valid.candidate_map_sha256,
    alternativeIndex: '1',
    memberPageNumber: '1'
  };
  assert.equal(validateCoveragePortfolioRuntimePage(valid, expectation), null);

  const mutations: Array<(page: ClearraCoveragePortfolioRuntimePage) => void> = [
    (page) => {
      page.page_contract = 'forged-page-contract';
    },
    (page) => {
      page.member_page_contract = 'forged-member-contract';
    },
    (page) => {
      page.set_identity_sha256 = '0'.repeat(64);
    },
    (page) => {
      page.candidate_map_sha256 = '0'.repeat(64);
    },
    (page) => {
      page.alternative_index = '2';
    },
    (page) => {
      page.optimal_cardinality = '2';
    },
    (page) => {
      page.known_alternative_count = '0';
    },
    (page) => {
      page.total_alternative_count = '1';
    },
    (page) => {
      page.enumeration_complete = true;
    },
    (page) => {
      page.members[0]!.candidate_id = '0';
    }
  ];
  for (const mutate of mutations) {
    const forged = structuredClone(valid);
    mutate(forged);
    assert.notEqual(
      validateCoveragePortfolioRuntimePage(forged, expectation),
      null,
      'every navigation path rejects forged page identity, count, or member metadata'
    );
  }
}

function assertCoveragePagerWindow(
  snapshot: ReturnType<CoveragePortfolioPagerController['snapshot']>
): void {
  const currentPage = snapshot.currentPage;
  if (!currentPage) {
    assert.equal(snapshot.pages.length, 0);
    assert.equal(snapshot.prefetchedPage, null);
    return;
  }
  const currentIndex = BigInt(currentPage.alternative_index);
  const retainedPages = [
    ...snapshot.pages,
    ...(snapshot.prefetchedPage ? [snapshot.prefetchedPage] : [])
  ];
  assert.ok(retainedPages.length <= 3, 'the pager retains at most previous/current/next');
  assert.equal(
    new Set(retainedPages.map((page) => page.alternative_index)).size,
    retainedPages.length,
    'the pager does not retain duplicate page states'
  );
  for (const page of retainedPages) {
    const distance = BigInt(page.alternative_index) - currentIndex;
    assert.ok(
      distance >= -1n && distance <= 1n,
      `retained page ${page.alternative_index} is outside current ${currentPage.alternative_index}`
    );
  }
  if (snapshot.prefetchedPage) {
    assert.equal(
      BigInt(snapshot.prefetchedPage.alternative_index),
      currentIndex + 1n,
      'a prefetched body is retained only when it is the immediate next page'
    );
  }
}

function runtimeCoveragePage(
  alternativeNumber: number | string,
  {
    complete = false,
    setIdentitySha256 = 'a'.repeat(64),
    candidateMapSha256 = 'b'.repeat(64)
  }: {
    complete?: boolean;
    setIdentitySha256?: string;
    candidateMapSha256?: string;
  } = {}
): ClearraCoveragePortfolioRuntimePage {
  const alternativeIndex = alternativeNumber.toString();
  return {
    page_contract: 'portfolio-alternative-page.v1',
    member_page_contract: 'portfolio-member-page.v1',
    set_identity_sha256: setIdentitySha256,
    candidate_map_sha256: candidateMapSha256,
    alternative_index: alternativeIndex,
    optimal_cardinality: '1',
    known_alternative_count: alternativeIndex,
    total_alternative_count: complete ? alternativeIndex : null,
    enumeration_complete: complete,
    member_page_number: '1',
    total_member_pages: '1',
    members: [
      {
        candidate_id: alternativeIndex,
        normalized_solution_key: `alternative-${alternativeIndex}`
      }
    ]
  };
}

function coveragePageResponse(
  page: ClearraCoveragePortfolioRuntimePage
): ClearraProductPageWorkerPayload {
  return {
    schema_version: 1,
    runtime: 'clearra-wasm',
    product_page_kind: 'coverage-portfolio',
    state: 'page',
    page
  };
}

function deferred<T>(): {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason: unknown) => void;
} {
  let resolvePromise!: (value: T) => void;
  let rejectPromise!: (reason: unknown) => void;
  const promise = new Promise<T>((resolve, reject) => {
    resolvePromise = resolve;
    rejectPromise = reject;
  });
  return { promise, resolve: resolvePromise, reject: rejectPromise };
}

async function settlePromises(): Promise<void> {
  for (let index = 0; index < 6; index += 1) await Promise.resolve();
}

console.log(JSON.stringify({
  member_page_boundary: PRODUCT_MEMBER_PAGE_SIZE,
  decimal_identity: 'string-exact',
  score_tie_equality: 'attack-independent',
  retired_product_surface: 'absent'
}));
