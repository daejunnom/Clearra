// SRP: compile and seal one monotonic PC4 rollout transition. No persistence,
// clock access, environment access, deployment, or rollback execution lives here.

import {
  PC4_MAX_ROLLBACK_GENERATIONS,
  PC4_PRODUCTION_CHANNEL,
  PC4_ROLLOUT_ENVELOPE_SCHEMA,
  PC4_ROLLOUT_SIGNATURE_DOMAIN,
  PC4_ROLLOUT_STATEMENT_SCHEMA,
  PC4_SIGNATURE_ALGORITHM,
  keyIdForPublicKey,
  requireCanonicalUnsignedInteger,
  requireSha256Identity,
  sealStatement,
  verifyStatementEnvelope,
} from "./activation-envelope-contract.mjs";
import { verifyProductionHostGenerationEnvelope } from "./seal-production-host-generation.mjs";
import { requireExactKeys } from "../canonical-release-evidence.mjs";

export function compileProductionRolloutPointer({
  selectedGenerationEnvelopeText,
  previousRolloutEnvelopeText = null,
  privateKey,
  publicKeyring,
  issuedAtUnixSeconds,
  expiresAtUnixSeconds,
  rollbackLimit = 3,
}) {
  if (!Number.isSafeInteger(rollbackLimit) || rollbackLimit < 1 ||
      rollbackLimit > PC4_MAX_ROLLBACK_GENERATIONS) {
    throw new Error("PC4 rollback limit is invalid");
  }
  const issued = requireCanonicalUnsignedInteger(String(issuedAtUnixSeconds),
    "PC4 rollout issued time");
  const expires = requireCanonicalUnsignedInteger(String(expiresAtUnixSeconds),
    "PC4 rollout expiry time");
  if (issued >= expires) throw new Error("PC4 rollout validity window is invalid");
  const selected = verifyProductionHostGenerationEnvelope(
    selectedGenerationEnvelopeText, publicKeyring);
  const previous = previousRolloutEnvelopeText === null
    ? null
    : verifyProductionRolloutPointer(previousRolloutEnvelopeText, publicKeyring);
  const retained = uniqueRetained([
    retainedEntry(selected),
    ...(previous?.statement.retained_generations ?? []),
  ]).slice(0, rollbackLimit + 1);
  if (previous && !retained.some((entry) =>
    entry.generation_identity === previous.statement.selected_generation_identity)) {
    throw new Error("PC4 rollout would discard its immediate rollback generation");
  }
  const sequence = previous
    ? requireCanonicalUnsignedInteger(previous.statement.rollout_sequence,
      "PC4 previous rollout sequence") + 1
    : 1;
  const statement = {
    schema: PC4_ROLLOUT_STATEMENT_SCHEMA,
    algorithm: PC4_SIGNATURE_ALGORITHM,
    key_id: keyIdForPublicKey(privateKey),
    channel: PC4_PRODUCTION_CHANNEL,
    rollout_sequence: String(sequence),
    selected_generation_identity: selected.generationIdentity,
    selected_authority_identity: selected.authorityIdentity,
    previous_pointer_identity: previous?.pointerIdentity ?? null,
    retained_generations: retained,
    rollback_limit: String(rollbackLimit),
    issued_at_unix_seconds: String(issued),
    expires_at_unix_seconds: String(expires),
  };
  return sealStatement({
    envelopeSchema: PC4_ROLLOUT_ENVELOPE_SCHEMA,
    statement,
    statementSchema: PC4_ROLLOUT_STATEMENT_SCHEMA,
    domain: PC4_ROLLOUT_SIGNATURE_DOMAIN,
    privateKey,
    publicKeyring,
  });
}

export function verifyProductionRolloutPointer(envelopeText, publicKeyring) {
  const verified = verifyStatementEnvelope({
    envelopeText,
    envelopeSchema: PC4_ROLLOUT_ENVELOPE_SCHEMA,
    statementSchema: PC4_ROLLOUT_STATEMENT_SCHEMA,
    domain: PC4_ROLLOUT_SIGNATURE_DOMAIN,
    publicKeyring,
  });
  const statement = verified.statement;
  requireExactKeys(statement, [
    "schema", "algorithm", "key_id", "channel", "rollout_sequence",
    "selected_generation_identity", "selected_authority_identity",
    "previous_pointer_identity", "retained_generations", "rollback_limit",
    "issued_at_unix_seconds", "expires_at_unix_seconds",
  ], "PC4 production rollout statement");
  if (statement.channel !== PC4_PRODUCTION_CHANNEL) {
    throw new Error("PC4 rollout channel is invalid");
  }
  const sequence = requireCanonicalUnsignedInteger(statement.rollout_sequence,
    "PC4 rollout sequence");
  const rollbackLimit = requireCanonicalUnsignedInteger(statement.rollback_limit,
    "PC4 rollback limit");
  if (sequence < 1 || rollbackLimit < 1 || rollbackLimit > PC4_MAX_ROLLBACK_GENERATIONS ||
      !Array.isArray(statement.retained_generations) ||
      statement.retained_generations.length < 1 ||
      statement.retained_generations.length > rollbackLimit + 1) {
    throw new Error("PC4 rollout bounds are invalid");
  }
  requireSha256Identity(statement.selected_generation_identity,
    "PC4 selected generation identity");
  requireSha256Identity(statement.selected_authority_identity,
    "PC4 selected authority identity");
  if ((sequence === 1 && statement.previous_pointer_identity !== null) ||
      (sequence > 1 && !requireSha256Identity(statement.previous_pointer_identity,
        "PC4 previous pointer identity"))) {
    throw new Error("PC4 previous pointer identity is invalid");
  }
  const seen = new Set();
  statement.retained_generations.forEach((entry) => {
    requireExactKeys(entry, ["generation_identity", "authority_identity"],
      "PC4 retained generation");
    requireSha256Identity(entry.generation_identity, "PC4 retained generation identity");
    requireSha256Identity(entry.authority_identity, "PC4 retained authority identity");
    const key = `${entry.generation_identity}:${entry.authority_identity}`;
    if (seen.has(key)) throw new Error("PC4 retained generation is duplicated");
    seen.add(key);
  });
  const selected = statement.retained_generations[0];
  if (selected.generation_identity !== statement.selected_generation_identity ||
      selected.authority_identity !== statement.selected_authority_identity) {
    throw new Error("PC4 selected generation is not first in retention order");
  }
  const issued = requireCanonicalUnsignedInteger(statement.issued_at_unix_seconds,
    "PC4 rollout issued time");
  const expires = requireCanonicalUnsignedInteger(statement.expires_at_unix_seconds,
    "PC4 rollout expiry time");
  if (issued >= expires) throw new Error("PC4 rollout validity window is invalid");
  return { ...verified, pointerIdentity: verified.statementIdentity };
}

function retainedEntry(verifiedGeneration) {
  return {
    generation_identity: verifiedGeneration.generationIdentity,
    authority_identity: verifiedGeneration.authorityIdentity,
  };
}

function uniqueRetained(entries) {
  const seen = new Set();
  return entries.filter((entry) => {
    const key = `${entry.generation_identity}:${entry.authority_identity}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}
