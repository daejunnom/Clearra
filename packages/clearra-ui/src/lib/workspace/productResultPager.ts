import type {
  ClearraPcBestSavePayload,
  ClearraPcSaveGroupPayload,
  ClearraPcSaveGroupsPayload,
  ClearraPcSavePieceMultisetPayload,
  ClearraPcSaveRunMetadataPayload,
  ClearraProductPageWorkerPayload,
  ClearraProductResultPayload,
  ClearraSolutionSetArtifactPayload
} from '../wasm/wasmCommandClient';

export const PRODUCT_MEMBER_PAGE_SIZE = 100;

export type ProductNextPageLoader = (
  signal?: AbortSignal
) => Promise<ClearraProductPageWorkerPayload>;

export type ProductMemberPageLoader = (
  outerPageNumber: number,
  memberPageNumber: number,
  signal?: AbortSignal
) => Promise<ClearraProductPageWorkerPayload>;

export type ProductPageRelease = () => void | Promise<void>;

export function productResultIdentity(payload: ClearraProductResultPayload | null | undefined) {
  if (!payload) return '';
  if (payload.content.payload_kind === 'build-v2') {
    const build = payload.content.payload;
    return [
      build.capability_id,
      build.result_contract,
      build.input_identity_sha256,
      build.evaluation_identity_sha256 ?? '',
      build.page_source_identity_sha256 ?? ''
    ].join(':');
  }
  if (payload.content.payload_kind === 'build-coverage-portfolio-v2') {
    const portfolio = payload.content.payload;
    return [
      payload.contract,
      payload.result_kind,
      portfolio.normalized_solution_set_hash,
      portfolio.page_source_identity_sha256 ?? ''
    ].join(':');
  }
  if (payload.content.payload_kind === 'build-setup-family-v1') {
    const family = payload.content.payload;
    return [
      payload.contract,
      payload.result_kind,
      family.input_identity_sha256,
      family.evaluation_identity_sha256
    ].join(':');
  }
  if (payload.content.payload_kind === 'coverage-portfolio') {
    const page = payload.content.payload;
    return [
      payload.contract,
      payload.result_kind,
      page.set_identity_sha256,
      page.candidate_map_sha256
    ].join(':');
  }
  if (payload.content.payload_kind === 'score-pattern-winner-family') {
    const family = payload.content.payload;
    return [
      payload.contract,
      payload.result_kind,
      family.winner_contract,
      family.winner_count
    ].join(':');
  }
  if (payload.content.payload_kind === 'pc-path-family') {
    const family = payload.content.payload;
    return [
      payload.contract,
      payload.result_kind,
      family.problem_id,
      family.witness_count
    ].join(':');
  }
  if (payload.content.payload_kind === 'pc-save-groups') {
    const family = payload.content.payload;
    return [
      payload.contract,
      payload.result_kind,
      family.metadata.problem_id,
      family.metadata.pattern_universe_id,
      family.group_count
    ].join(':');
  }
  if (payload.content.payload_kind === 'pc-best-save') {
    const family = payload.content.payload;
    return [
      payload.contract,
      payload.result_kind,
      family.metadata.problem_id,
      family.metadata.pattern_universe_id,
      family.winner_count
    ].join(':');
  }
  if (payload.content.payload_kind === 'setup-ranked-family') {
    const family = payload.content.payload;
    return [
      payload.contract,
      payload.result_kind,
      family.query_identity_sha256,
      family.supply_identity_sha256,
      family.universe_identity_sha256,
      family.product_build
    ].join(':');
  }
  if (payload.content.payload_kind === 'setup-score-ranking') {
    const ranking = payload.content.payload;
    return [
      payload.contract,
      payload.result_kind,
      ranking.input_identity_sha256,
      ranking.evaluation_identity_sha256
    ].join(':');
  }
  if (payload.content.payload_kind === 'spin-structure-family') {
    const family = payload.content.payload;
    return [
      payload.contract,
      payload.result_kind,
      family.query_identity_sha256,
      family.supply_identity_sha256,
      family.universe_identity_sha256,
      family.product_build
    ].join(':');
  }
  if (payload.content.payload_kind === 'parity-report-page') {
    const page = payload.content.payload;
    return [payload.contract, payload.result_kind, page.document_format, page.total_pages].join(':');
  }
  if (payload.content.payload_kind === 'field-document') {
    return [payload.contract, payload.result_kind, payload.content.payload.canonical_sha256].join(':');
  }
  if (payload.content.payload_kind === 'field-document-set') {
    return [
      payload.contract,
      payload.result_kind,
      ...payload.content.payload.documents.map((document) => document.canonical_sha256)
    ].join(':');
  }
  return [payload.contract, payload.result_kind, payload.content.payload.sha256].join(':');
}

export function isCanonicalDecimal(value: string): boolean {
  return /^(0|[1-9][0-9]*)$/.test(value);
}

export function isCanonicalProbability(value: string): boolean {
  if (!/^(?:0|1|0\.[0-9]+|[1-9][0-9]*(?:\.[0-9]+)?e-?[0-9]+)$/u.test(value)) {
    return false;
  }
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed >= 0 && parsed <= 1;
}

export function validateSolutionSetArtifactPayload(
  artifact: ClearraSolutionSetArtifactPayload
): string | null {
  if (
    artifact.contract !== 'solution-set-artifact.v2' ||
    artifact.completeness !== 'complete' ||
    !validArtifactIdentity(artifact.source_result_kind) ||
    !validArtifactIdentity(artifact.source_solution_set_contract) ||
    !validArtifactIdentity(artifact.selection_id) ||
    !validArtifactIdentity(artifact.normalized_key_algorithm) ||
    !validArtifactIdentity(artifact.normalized_set_hash_algorithm) ||
    !validArtifactIdentity(artifact.normalized_set_hash) ||
    (artifact.page_source_identity_sha256 !== null &&
      !/^[0-9a-f]{64}$/u.test(artifact.page_source_identity_sha256)) ||
    !Number.isSafeInteger(artifact.solution_count) ||
    artifact.solution_count <= 0 ||
    artifact.formats.length !== 2 ||
    artifact.formats[0]?.format !== 'ctk3' ||
    artifact.formats[1]?.format !== 'fumen' ||
    !artifact.formats.some((format) => format.state === 'available') ||
    artifact.formats.some((format) => !validSolutionSetArtifactFormat(format))
  ) {
    return 'invalid solution-set artifact payload';
  }
  return null;
}

function validSolutionSetArtifactFormat(
  format: ClearraSolutionSetArtifactPayload['formats'][number]
): boolean {
  if (format.state === 'unavailable') {
    return (
      format.unavailable_reason !== null &&
      [
        'empty-solution-set',
        'unsupported-solution-key',
        'page-limit-exceeded',
        'encoding-failed',
        'transport-byte-limit-exceeded'
      ].includes(format.unavailable_reason) &&
      format.media_type === null &&
      format.filename === null &&
      format.byte_length === null &&
      format.sha256 === null &&
      format.page_count === null &&
      format.document === null
    );
  }
  if (
    format.unavailable_reason !== null ||
    format.media_type === null ||
    format.filename === null ||
    format.byte_length === null ||
    format.sha256 === null ||
    format.page_count === null ||
    format.document === null ||
    !Number.isSafeInteger(format.byte_length) ||
    format.byte_length <= 0 ||
    format.byte_length > 8 * 1024 * 1024 ||
    new TextEncoder().encode(format.document).byteLength !== format.byte_length ||
    !/^[0-9a-f]{64}$/u.test(format.sha256) ||
    !Number.isSafeInteger(format.page_count) ||
    format.page_count <= 0
  ) {
    return false;
  }
  return format.format === 'ctk3'
    ? format.media_type === 'application/vnd.clearra.ctk3' &&
        format.filename.endsWith('.ctk3') &&
        (format.document.startsWith('ctk3_') || format.document.startsWith('ctk3b_'))
    : format.format === 'fumen' &&
        format.media_type === 'text/plain;charset=utf-8' &&
        format.filename.endsWith('.fumen') &&
        format.document.startsWith('v115@');
}

function validArtifactIdentity(value: string): boolean {
  return value.length > 0 && value.length <= 512 && !/[\u0000-\u0020\u007f]/u.test(value);
}

export function validateProductResultPayload(
  payload: ClearraProductResultPayload
): string | null {
  if (payload.content.payload_kind === 'build-v2') {
    return validateBuildV2(payload.contract, payload.result_kind, payload.content.payload);
  }
  if (payload.content.payload_kind === 'build-coverage-portfolio-v2') {
    const portfolio = payload.content.payload;
    const complete = portfolio.completeness;
    return payload.contract === 'build.cover' &&
      payload.result_kind === 'build-coverage-portfolio.v2' &&
      portfolio.contract === 'build-coverage-portfolio.v2' &&
      ['min-cover', 'max-probability-minimum'].includes(portfolio.objective) &&
      [
        portfolio.source_candidate_count,
        portfolio.selected_candidate_count,
        portfolio.pattern_count,
        portfolio.required_pattern_count
      ].every(isCanonicalDecimal) &&
      isCanonicalProbability(portfolio.union_probability) &&
      Boolean(portfolio.probability_basis) &&
      Boolean(portfolio.canonical_first_candidate_id) &&
      /^[0-9a-f]{64}$/u.test(portfolio.normalized_solution_set_hash) &&
      portfolio.page_source_available === true &&
      portfolio.page_source_identity_sha256 !== null &&
      /^[0-9a-f]{64}$/u.test(portfolio.page_source_identity_sha256) &&
      complete.source_universe_complete === true &&
      complete.coverage_rows_complete === true &&
      complete.probability_weights_complete === true &&
      complete.exact_minimum_proven === true &&
      complete.query_bound === true
      ? null
      : 'invalid Build coverage portfolio payload';
  }
  if (payload.content.payload_kind === 'build-setup-family-v1') {
    const family = payload.content.payload;
    const complete = family.completeness;
    return payload.contract === 'build.setup' &&
      payload.result_kind === 'build-target-family.v2' &&
      family.contract === 'build-target-family.v2' &&
      /^[0-9a-f]{64}$/u.test(family.input_identity_sha256) &&
      /^[0-9a-f]{64}$/u.test(family.evaluation_identity_sha256) &&
      ['all', 'unique'].includes(family.objective) &&
      [
        family.source_candidate_count,
        family.reachable_candidate_count,
        family.pattern_count,
        family.covered_pattern_count
      ].every(isCanonicalDecimal) &&
      isCanonicalProbability(family.union_probability) &&
      family.source_candidate_count === family.candidates.length.toString() &&
      canonicalBuildCandidateRows(family.candidates) &&
      complete.input_identity_bound === true &&
      complete.producer_filter_bound === true &&
      complete.buildability_replay_complete === true &&
      complete.coverage_rows_complete === true &&
      complete.probability_weights_complete === true
      ? null
      : 'invalid Build setup family payload';
  }
  if (payload.content.payload_kind === 'setup-ranked-family') {
    return validateSetupRankedFamily(
      payload.contract,
      payload.result_kind,
      payload.content.payload
    );
  }
  if (payload.content.payload_kind === 'setup-score-ranking') {
    return validateSetupScoreRanking(
      payload.contract,
      payload.result_kind,
      payload.content.payload
    );
  }
  if (payload.content.payload_kind === 'spin-structure-family') {
    return validateSpinStructureFamily(
      payload.contract,
      payload.result_kind,
      payload.content.payload
    );
  }
  if (payload.content.payload_kind === 'coverage-portfolio') {
    const page = payload.content.payload;
    const expectedPair =
      (payload.contract === 'pc.minimals' && payload.result_kind === 'pc-minimum-cover.v2') ||
      (payload.contract === 'pc.score-minimals' &&
        payload.result_kind === 'pc-score-portfolio.v2') ||
      (payload.contract === 'spin-structure.cover' &&
        payload.result_kind === 'spin-structure-coverage.v1');
    if (
      !expectedPair ||
      !/^[0-9a-f]{64}$/.test(page.set_identity_sha256) ||
      !/^[0-9a-f]{64}$/.test(page.candidate_map_sha256) ||
      !isCanonicalDecimal(page.alternative_index) ||
      !isCanonicalDecimal(page.optimal_cardinality) ||
      !isCanonicalDecimal(page.known_alternative_count) ||
      !isCanonicalDecimal(page.member_page_number) ||
      !isCanonicalDecimal(page.total_member_pages) ||
      page.members.length > PRODUCT_MEMBER_PAGE_SIZE ||
      page.members.some(
        (member) =>
          !isCanonicalDecimal(member.candidate_id) || !member.normalized_solution_key
      )
    ) {
      return 'invalid coverage portfolio payload';
    }
    return null;
  }
  if (payload.content.payload_kind === 'score-pattern-winner-family') {
    const family = payload.content.payload;
    const expectedPair =
      (payload.contract === 'pc.score' && payload.result_kind === 'pc-score-summary.v2') ||
      (payload.contract === 'pc.score-finder' &&
        payload.result_kind === 'pc-fixed-score-witness.v2');
    if (
      !expectedPair ||
      family.ordering !== 'pattern-id-ascending-then-candidate-id-ascending' ||
      family.equality !== 'score-only-attack-informational' ||
      family.page_size !== PRODUCT_MEMBER_PAGE_SIZE.toString() ||
      !isCanonicalDecimal(family.winner_count) ||
      family.winner_count !== family.winners.length.toString() ||
      family.winners.some(
        (winner) =>
          !isCanonicalDecimal(winner.pattern_id) ||
          !isCanonicalDecimal(winner.candidate_id) ||
          !isCanonicalDecimal(winner.score) ||
          !isCanonicalDecimal(winner.informational_attack) ||
          !winner.normalized_solution_key
      )
    ) {
      return 'invalid score winner family payload';
    }
    return null;
  }
  if (payload.content.payload_kind === 'pc-path-family') {
    return validatePcPathFamily(
      payload.contract,
      payload.result_kind,
      payload.content.payload
    );
  }
  if (payload.content.payload_kind === 'pc-save-groups') {
    const family = payload.content.payload;
    if (
      payload.contract !== 'pc.saves' ||
      payload.result_kind !== 'pc-save-groups.v2' ||
      !validatePcSaveGroups(family)
    ) {
      return 'invalid pc save groups payload';
    }
    return null;
  }
  if (payload.content.payload_kind === 'pc-best-save') {
    const family = payload.content.payload;
    if (
      payload.contract !== 'pc.best-save' ||
      payload.result_kind !== 'pc-best-save.v2' ||
      !validatePcBestSave(family)
    ) {
      return 'invalid pc best-save payload';
    }
    return null;
  }
  if (payload.content.payload_kind === 'parity-report-page') {
    const page = payload.content.payload;
    return Number.isInteger(page.page_number) &&
      page.page_number >= 1 &&
      Number.isInteger(page.total_pages) &&
      page.page_number <= page.total_pages &&
      page.feasibility_claim === false &&
      page.pruning_authority === 'none' &&
      page.four_color_counts.length === 4
      ? null
      : 'invalid parity report payload';
  }
  if (payload.content.payload_kind === 'field-document') {
    return validateFieldDocument(payload.content.payload);
  }
  if (payload.content.payload_kind === 'field-document-set') {
    const set = payload.content.payload;
    return set.documents.length > 0 &&
      set.documents.length <= 4096 &&
      set.documents.every((document) => validateFieldDocument(document) === null)
      ? null
      : 'invalid field document set payload';
  }
  const artifact = payload.content.payload;
  return artifact.render_exact === true &&
    /^[0-9a-f]{64}$/.test(artifact.sha256) &&
    Number.isSafeInteger(artifact.byte_length) &&
    artifact.byte_length >= 0 &&
    artifact.byte_length <= artifact.product_max_bytes &&
    artifact.byte_length <= artifact.transport_max_bytes &&
    ((artifact.artifact_format === 'png' && artifact.media_type === 'image/png') ||
      (artifact.artifact_format === 'gif' && artifact.media_type === 'image/gif'))
    ? null
    : 'invalid render artifact payload';
}

function validatePcPathFamily(
  outerContract: string,
  outerResultKind: string,
  family: Extract<
    ClearraProductResultPayload,
    { content: { payload_kind: 'pc-path-family' } }
  >['content']['payload']
): string | null {
  let previousOrderKey: readonly [string, string, string] | null = null;
  const witnessesValid = family.witnesses.every((witness) => {
    const orderKey = [
      witness.candidate_id,
      witness.pattern_id,
      witness.normalized_trace_key
    ] as const;
    const ordered = previousOrderKey === null || comparePcPathOrder(previousOrderKey, orderKey) < 0;
    previousOrderKey = orderKey;
    return (
      ordered &&
      [
        witness.candidate_id,
        witness.producer_candidate_id,
        witness.pattern_id,
        witness.consumed_piece_count
      ].every(isCanonicalDecimal) &&
      witness.trace_identity.length > 0 &&
      witness.normalized_trace_key.length > 0 &&
      (witness.terminal_hold_piece === null ||
        /^[IOTSZJL]$/u.test(witness.terminal_hold_piece)) &&
      witness.steps.length > 0 &&
      witness.steps.every(
        (step, stepIndex) =>
          step.step_index === stepIndex.toString() &&
          step.operation_id.length > 0 &&
          /^[IOTSZJL]$/u.test(step.active_piece) &&
          [step.input_cursor, step.output_cursor, step.cleared_lines].every(
            isCanonicalDecimal
          ) &&
          (step.input_hold_piece === null || /^[IOTSZJL]$/u.test(step.input_hold_piece)) &&
          (step.output_hold_piece === null || /^[IOTSZJL]$/u.test(step.output_hold_piece)) &&
          step.hold_decision.length > 0 &&
          step.rotation.length > 0 &&
          isCanonicalSignedDecimal(step.x) &&
          isCanonicalSignedDecimal(step.y) &&
          [
            step.placement_mask,
            step.board_before_mask,
            step.board_after_placement_mask,
            step.board_after_line_clear_mask,
            step.cleared_row_mask
          ].every((mask) => /^0x[0-9a-f]{16}$/u.test(mask)) &&
          step.line_clear_identity.length > 0
      )
    );
  });
  return outerContract === 'pc.path' &&
    outerResultKind === 'pc-path-family.v2' &&
    family.witness_contract === 'pc-path-witness.v2' &&
    family.ordering ===
      'candidate-id-ascending-then-pattern-id-ascending-then-trace-key-ascending' &&
    family.problem_id.length > 0 &&
    isCanonicalDecimal(family.materialized_pattern_count) &&
    isCanonicalDecimal(family.witness_count) &&
    family.witness_count === family.witnesses.length.toString() &&
    family.complete === true &&
    witnessesValid
    ? null
    : 'invalid pc.path replay family payload';
}

function isCanonicalSignedDecimal(value: string): boolean {
  return /^(?:0|[1-9][0-9]*|-[1-9][0-9]*)$/u.test(value);
}

function comparePcPathOrder(
  left: readonly [string, string, string],
  right: readonly [string, string, string]
): number {
  const candidateOrder = compareCanonicalDecimals(left[0], right[0]);
  if (candidateOrder !== 0) return candidateOrder;
  const patternOrder = compareCanonicalDecimals(left[1], right[1]);
  if (patternOrder !== 0) return patternOrder;
  return left[2] < right[2] ? -1 : left[2] > right[2] ? 1 : 0;
}

function validateSetupRankedFamily(
  outerContract: string,
  outerResultKind: string,
  family: Extract<
    ClearraProductResultPayload,
    { content: { payload_kind: 'setup-ranked-family' } }
  >['content']['payload']
): string | null {
  const expected = {
    'setup.joint': [
      'setup-joint-ranking.v2',
      'joint-probability-descending',
      'longer'
    ],
    'setup.build': [
      'setup-build-ranking.v2',
      'build-probability-descending',
      'longer'
    ],
    'setup.pc': [
      'setup-pc-ranking.v2',
      'conditional-pc-probability-descending',
      'shorter'
    ]
  } as const;
  const pair = expected[outerContract as keyof typeof expected];
  const candidateIds = new Set<string>();
  const valid =
    pair !== undefined &&
    outerResultKind === pair[0] &&
    family.schema_id === pair[0] &&
    family.ordering === pair[1] &&
    family.resolved_length_preference === pair[2] &&
    [
      family.query_identity_sha256,
      family.supply_identity_sha256,
      family.universe_identity_sha256
    ].every((identity) => /^[0-9a-f]{64}$/u.test(identity)) &&
    family.rule_profile.length > 0 &&
    family.product_build.length > 0 &&
    isCanonicalDecimal(family.candidate_count) &&
    family.candidate_count === family.candidates.length.toString() &&
    family.candidates.every((candidate) => {
      if (
        !candidate.candidate_id.startsWith('setup-candidate.v1:') ||
        candidate.condition_id.length === 0 ||
        candidate.setup_id.length === 0 ||
        candidateIds.has(candidate.candidate_id)
      ) {
        return false;
      }
      candidateIds.add(candidate.candidate_id);
      return true;
    });
  return valid ? null : 'invalid setup ranked family payload';
}

function validateSetupScoreRanking(
  outerContract: string,
  outerResultKind: string,
  ranking: Extract<
    ClearraProductResultPayload,
    { content: { payload_kind: 'setup-score-ranking' } }
  >['content']['payload']
): string | null {
  const candidateCount = isCanonicalDecimal(ranking.candidate_count)
    ? BigInt(ranking.candidate_count)
    : null;
  const sourcePageCount = isCanonicalDecimal(ranking.source_page_count)
    ? BigInt(ranking.source_page_count)
    : null;
  const setupPatternCount = isCanonicalDecimal(ranking.setup_pattern_count)
    ? BigInt(ranking.setup_pattern_count)
    : null;
  const averageScore = Number(ranking.average_priority_score);
  const candidateIds = new Set<string>();
  let previousScore: number | null = null;
  let previousCandidateId: string | null = null;
  const candidatesValid = ranking.candidates.every((candidate, index) => {
    const score = Number(candidate.unconditional_expected_score);
    const coveredCount = isCanonicalDecimal(candidate.setup_covered_pattern_count)
      ? BigInt(candidate.setup_covered_pattern_count)
      : null;
    const candidateValid =
      candidate.rank === (index + 1).toString() &&
      candidate.candidate_id.length > 0 &&
      candidate.candidate_id.trim() === candidate.candidate_id &&
      !candidateIds.has(candidate.candidate_id) &&
      /^0x[0-9a-f]+$/iu.test(candidate.completed_board_mask) &&
      coveredCount !== null &&
      setupPatternCount !== null &&
      coveredCount <= setupPatternCount &&
      finiteInRange(candidate.setup_covered_probability, 0, 1) &&
      finiteInRange(candidate.continuation_probability, 0, 1) &&
      Number.isFinite(score) &&
      score >= 0 &&
      (previousScore === null ||
        previousScore > score ||
        (previousScore === score &&
          previousCandidateId !== null &&
          previousCandidateId < candidate.candidate_id));
    if (!candidateValid) return false;
    candidateIds.add(candidate.candidate_id);
    previousScore = score;
    previousCandidateId = candidate.candidate_id;
    return true;
  });
  const valid =
    outerContract === 'setup.score' &&
    outerResultKind === 'setup-score-ranking.v1' &&
    ranking.schema_id === 'setup-score-ranking.v1' &&
    /^[0-9a-f]{64}$/u.test(ranking.input_identity_sha256) &&
    /^[0-9a-f]{64}$/u.test(ranking.evaluation_identity_sha256) &&
    ['ctk3', 'fumen'].includes(ranking.document_format) &&
    ranking.rule_profile.length > 0 &&
    ['tetrio', 'guideline', 'jstris-ultra'].includes(ranking.score_profile) &&
    isCanonicalDecimal(ranking.initial_b2b) &&
    ranking.ordering ===
      'unconditional-expected-score-descending-then-canonical-candidate-id' &&
    ranking.complete === true &&
    candidateCount !== null &&
    candidateCount > 0n &&
    candidateCount === BigInt(ranking.candidates.length) &&
    sourcePageCount !== null &&
    sourcePageCount >= candidateCount &&
    setupPatternCount !== null &&
    setupPatternCount > 0n &&
    Number.isFinite(averageScore) &&
    averageScore >= 0 &&
    candidatesValid;
  return valid ? null : 'invalid setup score ranking payload';
}

function validateSpinStructureFamily(
  outerContract: string,
  outerResultKind: string,
  family: Extract<
    ClearraProductResultPayload,
    { content: { payload_kind: 'spin-structure-family' } }
  >['content']['payload']
): string | null {
  const regularCount = isCanonicalDecimal(family.regular_count)
    ? BigInt(family.regular_count)
    : null;
  const miniCount = isCanonicalDecimal(family.mini_count) ? BigInt(family.mini_count) : null;
  const candidateCount = isCanonicalDecimal(family.candidate_count)
    ? BigInt(family.candidate_count)
    : null;
  const declaredMinimum =
    family.minimum_placements === null
      ? null
      : isCanonicalDecimal(family.minimum_placements)
        ? BigInt(family.minimum_placements)
        : undefined;
  const candidateIds = new Set<string>();
  let observedMini = false;
  let actualMinimum: bigint | null = null;
  let observedRegular = 0n;
  let observedMiniCount = 0n;
  const candidatesValid = family.candidates.every((candidate) => {
    if (
      !candidate.candidate_id.startsWith('spin-structure-candidate.v1:') ||
      candidateIds.has(candidate.candidate_id) ||
      !isCanonicalDecimal(candidate.placement_count) ||
      !['regular', 'mini'].includes(candidate.partition) ||
      (candidate.partition === 'regular' && observedMini)
    ) {
      return false;
    }
    candidateIds.add(candidate.candidate_id);
    if (candidate.partition === 'regular') {
      observedRegular += 1n;
    } else {
      observedMini = true;
      observedMiniCount += 1n;
    }
    const placementCount = BigInt(candidate.placement_count);
    actualMinimum =
      actualMinimum === null || placementCount < actualMinimum ? placementCount : actualMinimum;
    return true;
  });
  const searchPair =
    outerContract === 'spin-structure.search' &&
    outerResultKind === 'spin-structure-family.v2' &&
    family.schema_id === 'spin-structure-family.v2' &&
    family.guaranteed_final_piece === null &&
    family.guarantee_basis === null &&
    family.dependency_report_included === null &&
    family.dependency_relation === null &&
    family.dependency_edge_count === null;
  const guaranteedPair =
    outerContract === 'spin-structure.guaranteed' &&
    outerResultKind === 'spin-structure-guaranteed.v1' &&
    family.schema_id === 'spin-structure-guaranteed.v1' &&
    ['I', 'O', 'T', 'S', 'Z', 'J', 'L'].includes(family.guaranteed_final_piece ?? '') &&
    family.guarantee_basis ===
      'every-unique-non-target-piece-order-exact-replay-final-piece-last' &&
    (family.dependency_report_included === false
      ? family.dependency_relation === null && family.dependency_edge_count === null
      : family.dependency_report_included === true &&
        family.dependency_relation === 'non-target-universal-precedence' &&
        family.dependency_edge_count === '0');
  const valid =
    (searchPair || guaranteedPair) &&
    [
      family.query_identity_sha256,
      family.supply_identity_sha256,
      family.universe_identity_sha256
    ].every((identity) => /^[0-9a-f]{64}$/u.test(identity)) &&
    family.rule_profile.length > 0 &&
    family.spin_profile.length > 0 &&
    family.product_build.length > 0 &&
    family.ordering === 'regular-then-mini-canonical-operation-key' &&
    family.complete === true &&
    regularCount !== null &&
    miniCount !== null &&
    candidateCount !== null &&
    regularCount + miniCount === candidateCount &&
    candidateCount === BigInt(family.candidates.length) &&
    observedRegular === regularCount &&
    observedMiniCount === miniCount &&
    declaredMinimum !== undefined &&
    declaredMinimum === actualMinimum &&
    candidatesValid;
  return valid ? null : 'invalid spin structure family payload';
}

function finiteInRange(value: string, minimum: number, maximum: number): boolean {
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed >= minimum && parsed <= maximum;
}

function validateBuildV2(
  outerContract: string,
  outerResultKind: string,
  build: Extract<
    ClearraProductResultPayload,
    { content: { payload_kind: 'build-v2' } }
  >['content']['payload']
): string | null {
  const pairs: Record<string, readonly [string, string, readonly string[]]> = {
    'build.congruent': [
      'candidate-family',
      'build-congruence-family.v1',
      ['all', 'unique']
    ],
    'build.evaluate.cover': ['candidate-family', 'build-supplied-coverage.v1', ['all']],
    'build.evaluate.b2b-cover': [
      'candidate-family',
      'build-supplied-b2b-coverage.v1',
      ['all']
    ],
    'build.setup-cover-percent': [
      'probability',
      'build-setup-cover-probability.v1',
      ['all', 'unique']
    ],
    'build.evaluate.cover-percent': [
      'probability',
      'build-supplied-probability.v1',
      ['unique']
    ],
    'build.congruent-cover': [
      'portfolio',
      'build-congruence-coverage.v1',
      ['min-cover', 'max-probability-minimum']
    ],
    'build.setup-cover': [
      'portfolio',
      'build-setup-cover.v1',
      ['min-cover', 'max-probability-minimum']
    ],
    'build.evaluate.minimals': [
      'portfolio',
      'build-supplied-minimum-cover.v1',
      ['min-cover']
    ],
    'build.setup-cover-score': [
      'score-portfolio',
      'build-setup-cover-score.v1',
      ['max-score-cover']
    ],
    'build.evaluate.score': [
      'score-portfolio',
      'build-supplied-score.v1',
      ['max-score-cover']
    ]
  };
  const pair = pairs[build.capability_id];
  if (
    !pair ||
    pair[0] !== build.kind ||
    pair[1] !== build.result_contract ||
    !pair[2].includes(build.objective) ||
    outerContract !== build.capability_id ||
    outerResultKind !== build.result_contract ||
    !/^[0-9a-f]{64}$/u.test(build.input_identity_sha256) ||
    (build.evaluation_identity_sha256 !== null &&
      !/^[0-9a-f]{64}$/u.test(build.evaluation_identity_sha256)) ||
    ![
      build.source_candidate_count,
      build.reachable_candidate_count,
      build.pattern_count
    ].every(isCanonicalDecimal) ||
    [
      build.selected_candidate_count,
      build.covered_pattern_count,
      build.required_pattern_count,
      build.initial_b2b
    ].some((value) => value !== null && !isCanonicalDecimal(value)) ||
    compareCanonicalDecimals(build.reachable_candidate_count, build.source_candidate_count) > 0 ||
    (build.replay_basis !== null && build.replay_basis.length === 0) ||
    !build.completeness.input_identity_bound ||
    !build.completeness.producer_filter_bound ||
    !build.completeness.buildability_replay_complete ||
    !build.completeness.coverage_rows_complete ||
    !build.completeness.probability_weights_complete
  ) {
    return 'invalid Build v2 payload';
  }
  if (build.kind === 'candidate-family') {
    const b2b = build.capability_id === 'build.evaluate.b2b-cover';
    const expectedB2b = build.capability_id === 'build.congruent' ? null : b2b;
    if (
      build.evaluation_identity_sha256 === null ||
      build.replay_basis !== null ||
      build.covered_pattern_count === null ||
      compareCanonicalDecimals(build.covered_pattern_count, build.pattern_count) > 0 ||
      build.union_probability === null ||
      !isCanonicalProbability(build.union_probability) ||
      build.source_candidate_count !== build.candidates.length.toString() ||
      !canonicalBuildCandidateRows(build.candidates) ||
      build.candidates.some(
        (candidate) =>
          compareCanonicalDecimals(candidate.covered_pattern_count, build.pattern_count) > 0
      ) ||
      build.reachable_candidate_count !==
        build.candidates.filter((candidate) => candidate.covered_pattern_count !== '0').length.toString() ||
      build.selected_candidate_count !== null ||
      build.required_pattern_count !== null ||
      build.canonical_candidate_keys.length !== 0 ||
      build.winners.length !== 0 ||
      build.page_source_available ||
      build.page_source_identity_sha256 !== null ||
      build.completeness.exact_minimum_proven ||
      build.completeness.score_evidence_complete ||
      build.score_profile !== null ||
      build.initial_b2b !== null ||
      build.score_accuracy !== null ||
      build.profile_specific_exact !== null ||
      build.score_equality_basis !== null ||
      build.informational_attack_basis !== null ||
      build.b2b_preservation_required !== expectedB2b
    ) {
      return 'invalid Build v2 candidate family';
    }
    return null;
  }
  if (build.kind === 'probability') {
    const supplied = build.capability_id === 'build.evaluate.cover-percent';
    return build.evaluation_identity_sha256 !== null &&
      (supplied
        ? build.replay_basis !== null && build.replay_basis.length > 0
        : build.replay_basis === null) &&
      build.covered_pattern_count !== null &&
      compareCanonicalDecimals(build.covered_pattern_count, build.pattern_count) <= 0 &&
      build.union_probability !== null &&
      isCanonicalProbability(build.union_probability) &&
      build.selected_candidate_count === null &&
      build.required_pattern_count === null &&
      build.candidates.length === 0 &&
      build.canonical_candidate_keys.length === 0 &&
      build.winners.length === 0 &&
      !build.page_source_available &&
      build.page_source_identity_sha256 === null &&
      !build.completeness.exact_minimum_proven &&
      !build.completeness.score_evidence_complete &&
      build.score_profile === null &&
      build.initial_b2b === null &&
      build.score_accuracy === null &&
      build.profile_specific_exact === null &&
      build.score_equality_basis === null &&
      build.informational_attack_basis === null &&
      build.b2b_preservation_required === null
      ? null
      : 'invalid Build v2 probability';
  }
  const canonicalKeys = build.canonical_candidate_keys;
  const suppliedMinimum = build.capability_id === 'build.evaluate.minimals';
  if (
    build.evaluation_identity_sha256 !== null ||
    (suppliedMinimum
      ? build.replay_basis === null || build.replay_basis.length === 0
      : build.replay_basis !== null) ||
    build.selected_candidate_count === null ||
    build.required_pattern_count === null ||
    build.selected_candidate_count === '0' ||
    compareCanonicalDecimals(build.selected_candidate_count, build.reachable_candidate_count) > 0 ||
    compareCanonicalDecimals(build.required_pattern_count, build.pattern_count) > 0 ||
    build.selected_candidate_count !== canonicalKeys.length.toString() ||
    canonicalKeys.length === 0 ||
    !strictlySortedNonempty(canonicalKeys) ||
    build.candidates.length !== 0 ||
    !build.page_source_available ||
    build.page_source_identity_sha256 === null ||
    !/^[0-9a-f]{64}$/u.test(build.page_source_identity_sha256) ||
    !build.completeness.exact_minimum_proven ||
    build.covered_pattern_count !== null ||
    build.b2b_preservation_required !== null
  ) {
    return 'invalid Build v2 portfolio';
  }
  if (build.kind === 'portfolio') {
    return build.union_probability !== null &&
      isCanonicalProbability(build.union_probability) &&
      build.winners.length === 0 &&
      build.score_profile === null &&
      build.initial_b2b === null &&
      build.score_accuracy === null &&
      build.profile_specific_exact === null &&
      build.score_equality_basis === null &&
      build.informational_attack_basis === null &&
      !build.completeness.score_evidence_complete
      ? null
      : 'invalid Build v2 minimum portfolio';
  }
  return build.objective === 'max-score-cover' &&
    build.replay_basis === null &&
    build.union_probability === null &&
    build.score_profile !== null &&
    ['tetrio', 'guideline', 'jstris-ultra'].includes(build.score_profile) &&
    build.initial_b2b !== null &&
    compareCanonicalDecimals(build.initial_b2b, '65535') <= 0 &&
    build.score_accuracy === 'basic-approximation' &&
    build.profile_specific_exact === false &&
    build.score_equality_basis === 'score-only' &&
    build.informational_attack_basis === 'canonical-equal-score-trace' &&
    build.completeness.score_evidence_complete &&
    build.required_pattern_count === build.winners.length.toString() &&
    build.winners.every(
      (winner, index) =>
        isCanonicalDecimal(winner.pattern_id) &&
        isCanonicalDecimal(winner.score) &&
        isCanonicalDecimal(winner.informational_attack) &&
        canonicalKeys.includes(winner.candidate_key) &&
        compareCanonicalDecimals(winner.pattern_id, build.pattern_count) < 0 &&
        (index === 0 ||
          compareCanonicalDecimals(build.winners[index - 1]!.pattern_id, winner.pattern_id) < 0)
    )
    ? null
    : 'invalid Build v2 score portfolio';
}

function canonicalBuildCandidateRows(
  rows: readonly { candidate_key: string; covered_pattern_count: string }[]
): boolean {
  return (
    rows.every((row) => Boolean(row.candidate_key) && isCanonicalDecimal(row.covered_pattern_count)) &&
    strictlySortedNonempty(rows.map((row) => row.candidate_key))
  );
}

function strictlySortedNonempty(values: readonly string[]): boolean {
  return values.every(
    (value, index) => Boolean(value) && (index === 0 || values[index - 1]! < value)
  );
}

function validatePcSaveGroups(family: ClearraPcSaveGroupsPayload): boolean {
  return (
    family.schema_id === 'clearra-save-v1' &&
    family.page_size === PRODUCT_MEMBER_PAGE_SIZE.toString() &&
    isCanonicalDecimal(family.group_count) &&
    family.group_count === family.groups.length.toString() &&
    validatePcSaveMetadata(family.metadata, ['canonical-pc-saves', 'compatibility-saves']) &&
    family.groups.every(validatePcSaveGroup)
  );
}

function validatePcBestSave(family: ClearraPcBestSavePayload): boolean {
  const forbiddenKeys = [
    'portfolio',
    'portfolio_id',
    'tie_cursor',
    'tie_metadata',
    'set_identity_sha256',
    'page_handle_available'
  ];
  if (forbiddenKeys.some((key) => Object.prototype.hasOwnProperty.call(family, key))) {
    return false;
  }
  if (
    family.schema_id !== 'clearra-save-v1' ||
    family.probability_basis !== 'whole-universe-unconditional' ||
    family.ordering !==
      'weighted-total-descending-then-balanced-jl-descending-then-unconditional-probability-descending-then-canonical-candidate-id-ascending' ||
    family.equality !== 'weighted-total-balanced-jl-and-exact-unconditional-probability' ||
    family.page_size !== PRODUCT_MEMBER_PAGE_SIZE.toString() ||
    !isCanonicalDecimal(family.winner_count) ||
    family.winner_count !== family.winners.length.toString() ||
    !validatePcSaveMetadata(family.metadata, [
      'canonical-pc-best-save',
      'compatibility-best-save'
    ]) ||
    family.winners.some(
      (winner) =>
        !isCanonicalDecimal(winner.weighted_total) ||
        !isCanonicalDecimal(winner.balanced_jl_count) ||
        !isCanonicalProbability(winner.exact_group_probability) ||
        winner.exact_group_probability !== winner.group.unconditional_probability ||
        !validatePcSaveGroup(winner.group)
    )
  ) {
    return false;
  }
  const first = family.winners[0];
  if (
    first &&
    family.winners.some(
      (winner) =>
        winner.weighted_total !== first.weighted_total ||
        winner.balanced_jl_count !== first.balanced_jl_count ||
        winner.exact_group_probability !== first.exact_group_probability
    )
  ) {
    return false;
  }
  return family.winners.every(
    (winner, index) =>
      index === 0 ||
      compareCanonicalDecimals(
        family.winners[index - 1]!.group.canonical_candidate_id,
        winner.group.canonical_candidate_id
      ) <= 0
  );
}

function validatePcSaveMetadata(
  metadata: ClearraPcSaveRunMetadataPayload,
  origins: readonly string[]
): boolean {
  const complete = metadata.completeness;
  return (
    origins.includes(metadata.origin) &&
    (metadata.problem_preset === 'opening-pc' || metadata.problem_preset === 'scenario-pc') &&
    Boolean(metadata.problem_id) &&
    Boolean(metadata.piece_source_id) &&
    Boolean(metadata.pattern_universe_id) &&
    Boolean(metadata.pattern_weight_model_id) &&
    isCanonicalDecimal(metadata.materialized_pattern_count) &&
    isCanonicalDecimal(metadata.pc_success_pattern_count) &&
    isCanonicalProbability(metadata.pc_probability) &&
    complete.source_universe_complete === true &&
    complete.fixed_bag_boundary_proven === true &&
    complete.execution_batch_complete === true &&
    complete.pattern_weights_complete === true &&
    complete.count_complete === true &&
    complete.probability_complete === true &&
    complete.complete === true
  );
}

function validatePcSaveGroup(group: ClearraPcSaveGroupPayload): boolean {
  if (
    group.identity_contract !== 'terminal-hold-plus-active-bag-remainder-multiset.v1' ||
    !validatePcSaveMultiset(group.identity) ||
    !isCanonicalDecimal(group.successful_pattern_count) ||
    group.successful_pattern_count !== group.witnesses.length.toString() ||
    !isCanonicalProbability(group.unconditional_probability) ||
    !isCanonicalProbability(group.conditional_probability_given_pc) ||
    !isCanonicalDecimal(group.canonical_candidate_id) ||
    group.witnesses.length === 0
  ) {
    return false;
  }
  const candidateIds: string[] = [];
  for (const witness of group.witnesses) {
    if (
      !isCanonicalDecimal(witness.pattern_index) ||
      !isCanonicalDecimal(witness.candidate_id) ||
      !isCanonicalDecimal(witness.source_cursor) ||
      !witness.trace_identity ||
      (witness.terminal_hold !== null && !/^[IOTSZJL]$/u.test(witness.terminal_hold)) ||
      !validatePcSaveMultiset(witness.active_bag_remainder)
    ) {
      return false;
    }
    candidateIds.push(witness.candidate_id);
  }
  candidateIds.sort(compareCanonicalDecimals);
  return group.canonical_candidate_id === candidateIds[0];
}

function validatePcSaveMultiset(multiset: ClearraPcSavePieceMultisetPayload): boolean {
  const counts = [
    multiset.t,
    multiset.i,
    multiset.o,
    multiset.j,
    multiset.l,
    multiset.s,
    multiset.z
  ];
  return (
    counts.every((count) => Number.isInteger(count) && count >= 0 && count <= 0xff) &&
    Number.isInteger(multiset.total_count) &&
    multiset.total_count >= 0 &&
    multiset.total_count <= 0xff &&
    counts.reduce((sum, count) => sum + count, 0) === multiset.total_count &&
    multiset.canonical_id ===
      `T${multiset.t}I${multiset.i}O${multiset.o}J${multiset.j}L${multiset.l}S${multiset.s}Z${multiset.z}`
  );
}

function compareCanonicalDecimals(left: string, right: string): number {
  return left.length === right.length
    ? left.localeCompare(right)
    : left.length - right.length;
}

function validateFieldDocument(document: {
  format: 'ctk3' | 'fumen';
  document: string;
  page_count: number;
  canonical_sha256: string;
  filename: string;
}): string | null {
  if (
    !/^[0-9a-f]{64}$/.test(document.canonical_sha256) ||
    !Number.isInteger(document.page_count) ||
    document.page_count < 1 ||
    document.page_count > 4096 ||
    !document.filename ||
    (document.format === 'ctk3'
      ? !/^ctk3(?:b_|_|@)/.test(document.document)
      : !/^(?:v115|[Ddm]115)@/.test(document.document))
  ) {
    return 'invalid field document payload';
  }
  return null;
}
