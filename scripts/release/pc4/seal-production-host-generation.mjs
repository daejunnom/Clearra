// SRP: seal already-compiled canonical PC4 host-generation bytes with an
// in-memory key. This module never reads files, environment variables, or network.

import {
  PC4_GENERATION_ENVELOPE_SCHEMA,
  PC4_GENERATION_SIGNATURE_DOMAIN,
  PC4_GENERATION_STATEMENT_SCHEMA,
  PC4_PRODUCTION_CHANNEL,
  PC4_SIGNATURE_ALGORITHM,
  assertCanonicalText,
  keyIdForPublicKey,
  requireSha256Identity,
  sealStatement,
  sha256Identity,
  verifyStatementEnvelope,
} from "./activation-envelope-contract.mjs";
import { requireExactKeys } from "../canonical-release-evidence.mjs";

const HOST_GENERATION_SCHEMA = "clearra.pc4.host-generation.v1";

export function sealProductionHostGeneration({
  hostGenerationText,
  privateKey,
  publicKeyring,
  compatibilityIdentity,
  channel = PC4_PRODUCTION_CHANNEL,
}) {
  const generation = assertCanonicalText(hostGenerationText, "PC4 production host generation", 65_536);
  validateGenerationRoot(generation);
  validateAuthorityProfiles(generation.profiles);
  requireSha256Identity(compatibilityIdentity, "PC4 product compatibility identity");
  if (channel !== PC4_PRODUCTION_CHANNEL) throw new Error("PC4 activation channel is invalid");
  const statement = {
    schema: PC4_GENERATION_STATEMENT_SCHEMA,
    algorithm: PC4_SIGNATURE_ALGORITHM,
    key_id: keyIdForPublicKey(privateKey),
    channel,
    compatibility_identity: compatibilityIdentity,
    generation_id: generation.admission.generation_id,
    generation_identity: sha256Identity(Buffer.from(hostGenerationText, "utf8")),
    generation_json: hostGenerationText,
    repository: generation.repository,
    revision: generation.revision,
  };
  return sealStatement({
    envelopeSchema: PC4_GENERATION_ENVELOPE_SCHEMA,
    statement,
    statementSchema: PC4_GENERATION_STATEMENT_SCHEMA,
    domain: PC4_GENERATION_SIGNATURE_DOMAIN,
    privateKey,
    publicKeyring,
  });
}

export function verifyProductionHostGenerationEnvelope(envelopeText, publicKeyring) {
  const verified = verifyStatementEnvelope({
    envelopeText,
    envelopeSchema: PC4_GENERATION_ENVELOPE_SCHEMA,
    statementSchema: PC4_GENERATION_STATEMENT_SCHEMA,
    domain: PC4_GENERATION_SIGNATURE_DOMAIN,
    publicKeyring,
  });
  requireExactKeys(verified.statement, [
    "schema", "algorithm", "key_id", "channel", "compatibility_identity",
    "generation_id", "generation_identity", "generation_json", "repository", "revision",
  ], "PC4 production generation statement");
  if (verified.statement.channel !== PC4_PRODUCTION_CHANNEL) {
    throw new Error("PC4 activation channel is invalid");
  }
  requireSha256Identity(verified.statement.compatibility_identity,
    "PC4 product compatibility identity");
  const generation = assertCanonicalText(verified.statement.generation_json,
    "PC4 production host generation", 65_536);
  validateGenerationRoot(generation);
  const generationIdentity = sha256Identity(Buffer.from(verified.statement.generation_json, "utf8"));
  if (verified.statement.generation_identity !== generationIdentity ||
      verified.statement.repository !== generation.repository ||
      verified.statement.revision !== generation.revision ||
      verified.statement.generation_id !== generation.admission.generation_id) {
    throw new Error("PC4 signed generation statement does not bind its generation");
  }
  validateAuthorityProfiles(generation.profiles);
  return {
    ...verified,
    generation,
    generationText: verified.statement.generation_json,
    generationIdentity,
    authorityIdentity: verified.statementIdentity,
  };
}

function validateGenerationRoot(generation) {
  requireExactKeys(generation, [
    "schema", "repository", "revision", "admission", "profiles", "transferred_bytes",
  ], "PC4 production host generation");
  requireExactKeys(generation.admission, ["generation_id", "manifest_content_identity"],
    "PC4 production generation admission");
  if (generation.schema !== HOST_GENERATION_SCHEMA || generation.transferred_bytes !== 0 ||
      typeof generation.repository !== "string" || typeof generation.revision !== "string" ||
      typeof generation.admission.generation_id !== "string" ||
      !Array.isArray(generation.profiles) || generation.profiles.length !== 5) {
    throw new Error("PC4 production host generation contract is invalid");
  }
  requireSha256Identity(generation.admission.manifest_content_identity,
    "PC4 generation admission identity");
}

function validateAuthorityProfiles(profiles) {
  const expected = ["srs", "srs-plus", "srs-x", "jstris-180", "no-kick"];
  let ready = 0;
  profiles.forEach((slot, index) => {
    if (slot.profile !== expected[index]) throw new Error("PC4 profile slots are not canonical");
    if (slot.status === "unavailable") {
      requireExactKeys(slot, ["profile", "upstream_complete", "status", "reason"],
        "PC4 unavailable profile");
      if (slot.upstream_complete !== false || typeof slot.reason !== "string" || !slot.reason) {
        throw new Error("PC4 unavailable profile is invalid");
      }
      return;
    }
    if (slot.status !== "ready" || slot.upstream_complete !== true) {
      throw new Error("PC4 ready profile is invalid");
    }
    requireExactKeys(slot, [
      "profile", "upstream_complete", "status", "reader_contract", "field_count",
      "target_width", "target_lines", "pc_search_target_lines",
      "setup_search_target_lines", "target_qualification_receipts", "terminal_id",
      "artifacts", "evidence", "admission",
    ], "PC4 ready profile");
    if (typeof slot.reader_contract !== "string" || !slot.reader_contract ||
        !Number.isSafeInteger(slot.field_count) || slot.field_count < 2 ||
        !Number.isSafeInteger(slot.terminal_id) || slot.terminal_id < 1 ||
        ![3, 4].includes(slot.target_width) || JSON.stringify(slot.target_lines) !== "[4]" ||
        !Array.isArray(slot.pc_search_target_lines) ||
        !Array.isArray(slot.setup_search_target_lines)) {
      throw new Error("PC4 ready profile fields are invalid");
    }
    const pc = new Set(slot.pc_search_target_lines);
    const setup = new Set(slot.setup_search_target_lines);
    if (pc.size !== 1 || !pc.has(4)) {
      throw new Error("PC4 ready profile lacks its exact PC target");
    }
    const coveredPc = new Set();
    const coveredSetup = new Set();
    if (!Array.isArray(slot.target_qualification_receipts)) {
      throw new Error("PC4 target qualification receipts are invalid");
    }
    for (const receipt of slot.target_qualification_receipts) {
      if (receipt.profile !== slot.profile || receipt.target_lines !== 4) {
        throw new Error("PC4 target qualification receipt binding is invalid");
      }
      if (receipt.use_case === "pc-search" &&
          receipt.schema === "clearra.pc4.exact-target-qualification.v1") {
        if (coveredPc.has(4)) throw new Error("PC4 target receipt is duplicated");
        coveredPc.add(4);
      } else if (receipt.use_case === "setup-search" &&
                 receipt.schema === "clearra.pc4.exact-setup-target-qualification.v1") {
        if (coveredSetup.has(4)) throw new Error("PC4 setup receipt is duplicated");
        coveredSetup.add(4);
      } else {
        throw new Error("PC4 target receipt schema does not match its use case");
      }
    }
    if (JSON.stringify([...pc]) !== JSON.stringify([...coveredPc]) ||
        JSON.stringify([...setup]) !== JSON.stringify([...coveredSetup])) {
      throw new Error("PC4 target receipt coverage is incomplete");
    }
    ready += 1;
  });
  if (ready === 0) throw new Error("PC4 activation requires at least one qualified profile");
}
