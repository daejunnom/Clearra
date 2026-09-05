const SCORE_MINIMALS_RESULT_CONTRACT = "pc-score-portfolio.v2";
const SCORE_ONLY_EQUALITY = "score-only";
const INFORMATIONAL_ATTACK = "informational-only";
const CANONICAL_SELECTION = "smallest-canonical-candidate-id";
const PORTFOLIO_SET_CONTRACT = "portfolio-alternative-set.v1";
const PORTFOLIO_PAGE_CONTRACT = "portfolio-alternative-page.v1";
const PORTFOLIO_MEMBER_PAGE_CONTRACT = "portfolio-member-page.v1";
const PORTFOLIO_MEMBER_PAGE_SIZE = 100n;

const FORBIDDEN_RESULT_KEY = /(?:^|_)(?:tie|ties|alternative|alternatives|cursor|page|pages)(?:_|$)/u;
const CANDIDATE_ID_KEY = /candidate(?:_ids?|[A-Z].*Ids?)$/u;
const SOLUTION_KEY_KEY = /solution(?:_keys?|[A-Z].*Keys?)$/u;
const SHA256 = /^[0-9a-f]{64}$/u;
const ALLOWED_CANONICAL_PAGE_KEYS = new Set([
  "summary.alternative_index",
  "summary.known_alternative_count",
  "summary.total_alternative_count",
  "summary.page_contract",
  "summary.member_page_contract",
  "summary.member_page_number",
  "summary.total_member_pages",
  "summary.page_handle_available",
]);

export function validDiscordPcScoreMinimalsResult(structured) {
  if (
    !isPlainObject(structured) ||
    structured.kind !== SCORE_MINIMALS_RESULT_CONTRACT ||
    structured.contract?.command?.kind !== SCORE_MINIMALS_RESULT_CONTRACT ||
    !isPlainObject(structured.summary) ||
    !completeResourceReport(structured.resource_report)
  ) return false;

  const summary = structured.summary;
  if (
    summary.score_minimals_contract !== SCORE_MINIMALS_RESULT_CONTRACT ||
    summary.score_minimals_score_equality !== SCORE_ONLY_EQUALITY ||
    summary.score_minimals_attack_role !== INFORMATIONAL_ATTACK ||
    summary.score_minimals_canonical_selection !== CANONICAL_SELECTION ||
    !canonicalPositiveDecimalU64(summary.score_minimals_canonical_candidate_id) ||
    !safeCanonicalSolutionKey(summary.score_minimals_canonical_solution_key) ||
    !validCanonicalCoveragePortfolio(summary)
  ) return false;

  return validCanonicalArtifacts(structured) &&
    hasOnlyCanonicalPortfolioCandidates(structured) &&
    !containsUnexpectedTieMetadata(structured);
}

export function discordPcScoreMinimalsResultProjection(structured) {
  if (!validDiscordPcScoreMinimalsResult(structured)) return null;
  return Object.freeze({
    canonicalCandidateId: structured.summary.score_minimals_canonical_candidate_id,
    canonicalSolutionKey: structured.summary.score_minimals_canonical_solution_key,
    scoreEquality: SCORE_ONLY_EQUALITY,
    attackRole: INFORMATIONAL_ATTACK,
  });
}

/**
 * Retains the complete CLI evidence envelope for downstream contract checks,
 * while reducing Discord's renderable solution artifact to its one governed
 * canonical witness. Numeric candidate identity, never attack, owns this
 * selection.
 */
export function projectDiscordPcScoreMinimalsCanonicalResult(structured) {
  const selection = discordPcScoreMinimalsResultProjection(structured);
  if (selection === null) return null;

  const projected = clonePlain(structured);
  const artifacts = projected.contract?.artifacts;
  if (isPlainObject(artifacts)) {
    const sourceKeys = artifacts.solution_keys;
    const canonicalIndex = sourceKeys.indexOf(selection.canonicalSolutionKey);
    artifacts.solution_keys = [selection.canonicalSolutionKey];
    if (Array.isArray(artifacts.solution_classes)) {
      artifacts.solution_classes = [artifacts.solution_classes[canonicalIndex]];
    }
    if (Array.isArray(artifacts.solution_probabilities)) {
      artifacts.solution_probabilities = artifacts.solution_probabilities.filter(
        (entry) => entry.solution_key === selection.canonicalSolutionKey,
      );
    }
  }
  return deepFreeze(stripAttackObservations(projected));
}

export function discordPcScoreMinimalsSummaryLines(structured, locale = "en") {
  const source = structured?.kind === "score-minimals"
    ? { ...structured, kind: SCORE_MINIMALS_RESULT_CONTRACT }
    : structured;
  const projection = discordPcScoreMinimalsResultProjection(source);
  if (projection === null) return null;
  const labels = String(locale).toLowerCase().startsWith("ko")
    ? {
        selected: "선택된 결과",
        equality: "점수 동등성",
        attack: "공격력 역할",
      }
    : {
        selected: "Selected result",
        equality: "Score equality",
        attack: "Attack role",
      };
  return Object.freeze([
    `${labels.selected}: 1`,
    `${labels.equality}: ${projection.scoreEquality}`,
    `${labels.attack}: ${projection.attackRole}`,
  ]);
}

function completeResourceReport(report) {
  return isPlainObject(report) &&
    report.probability_complete === true &&
    report.count_complete === true &&
    report.truncated === false &&
    report.truncation_reason === null &&
    report.count_truncated_reason === null &&
    report.renormalized === false;
}

function validCanonicalCoveragePortfolio(summary) {
  if (
    summary.capability_id !== "pc.score-minimals" ||
    summary.result_contract !== SCORE_MINIMALS_RESULT_CONTRACT ||
    summary.payload_kind !== "coverage-portfolio" ||
    summary.set_contract !== PORTFOLIO_SET_CONTRACT ||
    summary.page_contract !== PORTFOLIO_PAGE_CONTRACT ||
    summary.member_page_contract !== PORTFOLIO_MEMBER_PAGE_CONTRACT ||
    !SHA256.test(summary.set_identity_sha256) ||
    !SHA256.test(summary.candidate_map_sha256) ||
    summary.alternative_index !== "1" ||
    summary.member_page_number !== "1" ||
    !positiveCanonicalDecimal(summary.optimal_cardinality) ||
    !positiveCanonicalDecimal(summary.known_alternative_count) ||
    !(summary.total_alternative_count === null ||
      positiveCanonicalDecimal(summary.total_alternative_count)) ||
    !positiveCanonicalDecimal(summary.total_member_pages) ||
    typeof summary.enumeration_complete !== "boolean" ||
    summary.page_handle_available !== true ||
    !Array.isArray(summary.members) ||
    summary.members.length === 0
  ) return false;

  const cardinality = BigInt(summary.optimal_cardinality);
  const knownAlternatives = BigInt(summary.known_alternative_count);
  const totalAlternatives = summary.total_alternative_count === null
    ? null
    : BigInt(summary.total_alternative_count);
  const totalMemberPages = BigInt(summary.total_member_pages);
  if (
    BigInt(summary.members.length) !==
      (cardinality < PORTFOLIO_MEMBER_PAGE_SIZE ? cardinality : PORTFOLIO_MEMBER_PAGE_SIZE) ||
    totalMemberPages !==
      ((cardinality + PORTFOLIO_MEMBER_PAGE_SIZE - 1n) / PORTFOLIO_MEMBER_PAGE_SIZE) ||
    (summary.enumeration_complete
      ? totalAlternatives !== knownAlternatives
      : totalAlternatives !== null)
  ) return false;

  let previousCandidateId = 0n;
  for (const member of summary.members) {
    if (
      !isPlainObject(member) ||
      !canonicalPositiveDecimalU64(member.candidate_id) ||
      !safeCanonicalSolutionKey(member.normalized_solution_key)
    ) return false;
    const candidateId = BigInt(member.candidate_id);
    if (candidateId <= previousCandidateId) return false;
    previousCandidateId = candidateId;
  }
  const suppliedCandidateId = BigInt(summary.score_minimals_canonical_candidate_id);
  const suppliedWitness = summary.members.find((member) =>
    member.candidate_id === summary.score_minimals_canonical_candidate_id
  );
  return suppliedWitness !== undefined &&
    suppliedWitness.normalized_solution_key === summary.score_minimals_canonical_solution_key &&
    summary.members.every((member) => BigInt(member.candidate_id) >= suppliedCandidateId);
}

function hasOnlyCanonicalPortfolioCandidates(value, path = []) {
  if (Array.isArray(value)) {
    return value.every((entry, index) =>
      hasOnlyCanonicalPortfolioCandidates(entry, [...path, index]));
  }
  if (!isPlainObject(value)) return true;
  for (const [key, child] of Object.entries(value)) {
    const childPath = [...path, key];
    const childPathText = childPath.join(".");
    if (CANDIDATE_ID_KEY.test(key)) {
      const canonicalField = childPathText ===
        "summary.score_minimals_canonical_candidate_id";
      const canonicalMember = /^summary\.members\.[0-9]+\.candidate_id$/u.test(childPathText);
      if (!canonicalField && !canonicalMember) {
        return false;
      }
    }
    if (SOLUTION_KEY_KEY.test(key)) {
      const canonicalField = childPathText ===
        "summary.score_minimals_canonical_solution_key";
      const canonicalMember =
        /^summary\.members\.[0-9]+\.normalized_solution_key$/u.test(childPathText);
      const solutionArtifact = childPathText === "contract.artifacts.solution_keys" ||
        /^contract\.artifacts\.solution_probabilities\.[0-9]+\.solution_key$/u
          .test(childPathText);
      if (!canonicalField && !canonicalMember && !solutionArtifact) {
        return false;
      }
    }
    if (!hasOnlyCanonicalPortfolioCandidates(child, childPath)) return false;
  }
  return true;
}

function containsUnexpectedTieMetadata(value, path = []) {
  if (Array.isArray(value)) {
    return value.some((entry, index) =>
      containsUnexpectedTieMetadata(entry, [...path, index]));
  }
  if (!isPlainObject(value)) return false;
  return Object.entries(value).some(([key, child]) => {
    const childPath = [...path, key];
    return (FORBIDDEN_RESULT_KEY.test(key) &&
      !ALLOWED_CANONICAL_PAGE_KEYS.has(childPath.join("."))) ||
      containsUnexpectedTieMetadata(child, childPath);
  });
}

function validCanonicalArtifacts(structured) {
  const artifacts = structured.contract?.artifacts;
  if (artifacts === undefined) return true;
  if (
    !isPlainObject(artifacts) ||
    artifacts.schema_version !== "clearra.solution-data.v1" ||
    !Array.isArray(artifacts.solution_keys) ||
    artifacts.solution_keys.length === 0 ||
    artifacts.solution_keys.some((key) => !safeCanonicalSolutionKey(key)) ||
    new Set(artifacts.solution_keys).size !== artifacts.solution_keys.length
  ) return false;

  const canonicalKey = structured.summary.score_minimals_canonical_solution_key;
  if (!artifacts.solution_keys.includes(canonicalKey)) return false;
  if (
    artifacts.solution_classes !== undefined &&
    (!Array.isArray(artifacts.solution_classes) ||
      artifacts.solution_classes.length !== artifacts.solution_keys.length)
  ) return false;
  if (artifacts.solution_probabilities !== undefined) {
    if (!Array.isArray(artifacts.solution_probabilities)) return false;
    const sourceKeys = new Set(artifacts.solution_keys);
    for (const probability of artifacts.solution_probabilities) {
      if (
        !isPlainObject(probability) ||
        !safeCanonicalSolutionKey(probability.solution_key) ||
        !sourceKeys.has(probability.solution_key)
      ) return false;
    }
  }
  return true;
}

function stripAttackObservations(value, path = []) {
  if (Array.isArray(value)) {
    return value.map((entry, index) => stripAttackObservations(entry, [...path, index]));
  }
  if (!isPlainObject(value)) return value;
  return Object.fromEntries(Object.entries(value).flatMap(([key, child]) => {
    const childPath = [...path, key];
    const policyField = childPath.join(".") === "summary.score_minimals_attack_role";
    if (key.toLowerCase().includes("attack") && !policyField) return [];
    return [[key, stripAttackObservations(child, childPath)]];
  }));
}

function canonicalDecimal(value) {
  return typeof value === "string" && /^(?:0|[1-9][0-9]*)$/u.test(value);
}

function positiveCanonicalDecimal(value) {
  return canonicalDecimal(value) && value !== "0";
}

function canonicalPositiveDecimalU64(value) {
  return positiveCanonicalDecimal(value) &&
    BigInt(value) <= 18_446_744_073_709_551_615n;
}

function safeCanonicalSolutionKey(value) {
  return typeof value === "string" &&
    value.length > 0 &&
    value.length <= 4_096 &&
    value.trim() === value &&
    !/[\u0000-\u001f\u007f]/u.test(value) &&
    !/@(?:everyone|here)/iu.test(value);
}

function isPlainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function clonePlain(value) {
  if (Array.isArray(value)) return value.map(clonePlain);
  if (isPlainObject(value)) {
    return Object.fromEntries(
      Object.entries(value).map(([key, nested]) => [key, clonePlain(nested)]),
    );
  }
  return value;
}

function deepFreeze(value) {
  if (!value || typeof value !== "object" || Object.isFrozen(value)) return value;
  for (const nested of Object.values(value)) deepFreeze(nested);
  return Object.freeze(value);
}
