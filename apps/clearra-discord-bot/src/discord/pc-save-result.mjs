const SAVE_GROUPS_KIND = "pc-save-groups.v2";
const BEST_SAVE_KIND = "pc-best-save.v2";

export function validDiscordPcSaveResult(structured, publicKind) {
  if (!plainObject(structured) || !plainObject(structured.summary)) return false;
  const summary = structured.summary;
  if (containsForbiddenTieMetadata(structured)) return false;
  if (publicKind === "saves") {
    return structured.kind === SAVE_GROUPS_KIND &&
      summary.save_contract === SAVE_GROUPS_KIND &&
      unitProbability(summary.save_pc_probability) &&
      Array.isArray(summary.save_groups) &&
      summary.save_groups.length > 0 &&
      summary.save_groups.every((group) => validSaveGroup(group, summary.save_pc_probability));
  }
  return publicKind === "best-save" &&
    structured.kind === BEST_SAVE_KIND &&
    summary.best_save_contract === BEST_SAVE_KIND &&
    summary.best_save_schema === "clearra-save-v1" &&
    summary.best_save_probability_basis === "whole-universe-unconditional" &&
    unitProbability(summary.best_save_pc_probability) &&
    Array.isArray(summary.best_save_winners) &&
    summary.best_save_winners.length > 0 &&
    summary.best_save_winners.every((winner) =>
      validBestSaveWinner(winner, summary.best_save_pc_probability));
}

// Discord deliberately presents one exact tie representative. The engine and
// CLI retain the unmodified ordinary winner list.
export function selectDiscordBestSaveWinner(summary) {
  if (
    !plainObject(summary) ||
    !unitProbability(summary.best_save_pc_probability) ||
    !Array.isArray(summary.best_save_winners) ||
    summary.best_save_winners.length === 0 ||
    !summary.best_save_winners.every((winner) =>
      validBestSaveWinner(winner, summary.best_save_pc_probability))
  ) return null;
  return [...summary.best_save_winners].sort((left, right) => {
    const leftCandidateId = BigInt(left.group.canonical_candidate_id);
    const rightCandidateId = BigInt(right.group.canonical_candidate_id);
    const candidateOrder = leftCandidateId < rightCandidateId
      ? -1
      : leftCandidateId > rightCandidateId ? 1 : 0;
    return candidateOrder || left.group.identity.localeCompare(right.group.identity, "en");
  })[0] ?? null;
}

function validBestSaveWinner(winner, pcProbability) {
  return plainObject(winner) &&
    nonNegativeSafeInteger(winner.weighted_total) &&
    nonNegativeSafeInteger(winner.balanced_jl_count) &&
    unitProbability(winner.exact_group_probability) &&
    validSaveGroup(winner.group, pcProbability) &&
    winner.exact_group_probability === winner.group.unconditional_probability;
}

function validSaveGroup(group, pcProbability) {
  if (
    !plainObject(group) ||
    typeof group.identity !== "string" ||
    group.identity.length === 0 ||
    !nonNegativeSafeInteger(group.successful_pattern_count) ||
    !unitProbability(group.unconditional_probability) ||
    !unitProbability(group.conditional_probability_given_pc) ||
    !canonicalPositiveDecimalU64(group.canonical_candidate_id) ||
    !Array.isArray(group.witnesses) ||
    group.witnesses.length === 0 ||
    !group.witnesses.every((witness) => plainObject(witness) &&
      canonicalPositiveDecimalU64(witness.candidate_id))
  ) {
    return false;
  }
  if (group.successful_pattern_count !== group.witnesses.length) return false;
  if (group.unconditional_probability > pcProbability) return false;
  const smallestWitnessCandidateId = group.witnesses.reduce((smallest, witness) => {
    const candidateId = BigInt(witness.candidate_id);
    return smallest === null || candidateId < smallest ? candidateId : smallest;
  }, null);
  return smallestWitnessCandidateId !== null &&
    BigInt(group.canonical_candidate_id) === smallestWitnessCandidateId;
}

function containsForbiddenTieMetadata(value) {
  if (Array.isArray(value)) return value.some(containsForbiddenTieMetadata);
  if (!plainObject(value)) return false;
  return Object.entries(value).some(([key, nested]) => {
    const normalizedKey = key.toLowerCase();
    const forbiddenKey = normalizedKey.includes("portfolio") ||
      normalizedKey.includes("alternative") ||
      /(?:^|_)ties?(?:_|$)/u.test(normalizedKey) ||
      normalizedKey === "cursor";
    return forbiddenKey || containsForbiddenTieMetadata(nested);
  });
}

function unitProbability(value) {
  return typeof value === "number" && Number.isFinite(value) && value >= 0 && value <= 1;
}

function nonNegativeSafeInteger(value) {
  return Number.isSafeInteger(value) && value >= 0;
}

function canonicalPositiveDecimalU64(value) {
  if (typeof value !== "string" || !/^[1-9]\d*$/u.test(value)) return false;
  return BigInt(value) <= 18_446_744_073_709_551_615n;
}

function plainObject(value) {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}
