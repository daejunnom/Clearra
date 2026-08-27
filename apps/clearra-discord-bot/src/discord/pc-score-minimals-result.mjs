const SCORE_MINIMALS_RESULT_CONTRACT = "pc-score-portfolio.v2";
const SCORE_ONLY_EQUALITY = "score-only";
const INFORMATIONAL_ATTACK = "informational-only";
const CANONICAL_SELECTION = "smallest-canonical-candidate-id";

const FORBIDDEN_RESULT_KEY = /(?:^|_)(?:tie|ties|alternative|alternatives|cursor|page|pages)(?:_|$)/u;
const CANDIDATE_ID_KEY = /candidate(?:_ids?|[A-Z].*Ids?)$/u;
const SOLUTION_KEY_KEY = /solution(?:_keys?|[A-Z].*Keys?)$/u;

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
    !canonicalDecimal(summary.score_minimals_canonical_candidate_id) ||
    !safeCanonicalSolutionKey(summary.score_minimals_canonical_solution_key)
  ) return false;

  return hasOnlyOnePublicCandidate(structured) && !containsTieMetadata(structured);
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

export function discordPcScoreMinimalsSummaryLines(structured, locale = "en") {
  const source = structured?.kind === "score-minimals"
    ? { ...structured, kind: SCORE_MINIMALS_RESULT_CONTRACT }
    : structured;
  const projection = discordPcScoreMinimalsResultProjection(source);
  if (projection === null) return null;
  const labels = String(locale).toLowerCase().startsWith("ko")
    ? {
        candidate: "정규 후보 ID",
        solution: "정규 해법 키",
        equality: "점수 동등성",
        attack: "공격력 역할",
      }
    : {
        candidate: "Canonical candidate ID",
        solution: "Canonical solution key",
        equality: "Score equality",
        attack: "Attack role",
      };
  return Object.freeze([
    `${labels.candidate}: ${projection.canonicalCandidateId}`,
    `${labels.solution}: ${projection.canonicalSolutionKey}`,
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

function hasOnlyOnePublicCandidate(value, path = []) {
  if (Array.isArray(value)) {
    return value.every((entry, index) => hasOnlyOnePublicCandidate(entry, [...path, index]));
  }
  if (!isPlainObject(value)) return true;
  for (const [key, child] of Object.entries(value)) {
    const childPath = [...path, key];
    if (CANDIDATE_ID_KEY.test(key)) {
      if (childPath.join(".") !== "summary.score_minimals_canonical_candidate_id") {
        return false;
      }
    }
    if (SOLUTION_KEY_KEY.test(key)) {
      if (childPath.join(".") !== "summary.score_minimals_canonical_solution_key") {
        return false;
      }
    }
    if (!hasOnlyOnePublicCandidate(child, childPath)) return false;
  }
  return true;
}

function containsTieMetadata(value) {
  if (Array.isArray(value)) return value.some(containsTieMetadata);
  if (!isPlainObject(value)) return false;
  return Object.entries(value).some(([key, child]) =>
    FORBIDDEN_RESULT_KEY.test(key) || containsTieMetadata(child)
  );
}

function canonicalDecimal(value) {
  return typeof value === "string" && /^(?:0|[1-9][0-9]*)$/u.test(value);
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
