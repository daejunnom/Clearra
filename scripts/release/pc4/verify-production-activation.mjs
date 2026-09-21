// SRP: verify signed release authority, monotonic rollout state, and exact
// reader binding. It returns an in-memory activation candidate and performs no I/O.

import { canonicalText } from "./activation-envelope-contract.mjs";
import { verifyProductionHostGenerationEnvelope } from "./seal-production-host-generation.mjs";
import { verifyProductionRolloutPointer } from "./compile-production-rollout-pointer.mjs";
import { requireExactKeys } from "../canonical-release-evidence.mjs";

const PROFILES = ["srs", "srs-plus", "srs-x", "jstris-180", "no-kick"];

export function verifyAndLinkProductionActivation({
  readerGenerationText,
  signedGenerationEnvelopeText,
  signedRolloutEnvelopeText,
  publicKeyring,
  replayState = null,
  nowUnixSeconds,
  bootstrapMinSequence = 1,
  expectedCompatibilityIdentity,
}) {
  const authority = verifyProductionHostGenerationEnvelope(
    signedGenerationEnvelopeText, publicKeyring);
  const pointer = verifyProductionRolloutPointer(signedRolloutEnvelopeText, publicKeyring);
  if (pointer.statement.selected_generation_identity !== authority.generationIdentity ||
      pointer.statement.selected_authority_identity !== authority.authorityIdentity ||
      authority.statement.compatibility_identity !== expectedCompatibilityIdentity) {
    throw new Error("PC4 rollout does not bind the requested product generation");
  }
  const replay = previewReplay(pointer, replayState, nowUnixSeconds, bootstrapMinSequence);
  const reader = validateReaderGeneration(readerGenerationText);
  if (reader.repository !== authority.generation.repository ||
      reader.revision !== authority.generation.revision) {
    throw new Error("PC4 reader and authority generation identities differ");
  }
  const profiles = [];
  const effective = authority.generation.profiles.map((slot, index) => {
    const readerSlot = reader.profiles[index];
    if (slot.status === "unavailable") {
      profiles.push({ profile: slot.profile, status: "unavailable", reason: slot.reason });
      return slot;
    }
    if (readerSlot.status === "unavailable") {
      const reason = `reader-unavailable:${readerSlot.reason}`;
      profiles.push({ profile: slot.profile, status: "unavailable", reason });
      return { profile: slot.profile, upstream_complete: false, status: "unavailable", reason };
    }
    for (const field of [
      "profile", "upstream_complete", "status", "reader_contract", "field_count",
      "target_width", "target_lines", "terminal_id", "artifacts", "evidence",
    ]) {
      if (JSON.stringify(readerSlot[field]) !== JSON.stringify(slot[field])) {
        throw new Error(`PC4 reader profile binding differs: ${slot.profile}/${field}`);
      }
    }
    profiles.push({
      profile: slot.profile,
      status: "ready",
      pc_search_target_lines: [...slot.pc_search_target_lines],
      setup_search_target_lines: [...slot.setup_search_target_lines],
    });
    return slot;
  });
  if (!profiles.some(({ status }) => status === "ready")) {
    throw new Error("PC4 reader has no effective qualified profile");
  }
  return {
    hostGenerationText: canonicalText({ ...authority.generation, profiles: effective }),
    generationIdentity: authority.generationIdentity,
    authorityIdentity: authority.authorityIdentity,
    pointerIdentity: pointer.pointerIdentity,
    profiles,
    replayDecision: replay.decision,
    nextReplayState: replay.state,
  };
}

function validateReaderGeneration(text) {
  const reader = JSON.parse(text);
  if (canonicalText(reader) !== text) throw new Error("PC4 reader generation is not canonical");
  requireExactKeys(reader, ["schema", "repository", "revision", "profiles", "transferred_bytes"],
    "PC4 reader generation");
  if (reader.schema !== "clearra.pc4.host-generation.v1" ||
      !Array.isArray(reader.profiles) || reader.profiles.length !== PROFILES.length) {
    throw new Error("PC4 reader generation contract is invalid");
  }
  reader.profiles.forEach((slot, index) => {
    if (slot.profile !== PROFILES[index]) throw new Error("PC4 reader profile order is invalid");
    if (slot.status === "ready") {
      if (JSON.stringify(slot.pc_search_target_lines) !== "[]" ||
          JSON.stringify(slot.setup_search_target_lines) !== "[]" ||
          JSON.stringify(slot.target_qualification_receipts) !== "[]") {
        throw new Error("PC4 reader generation attempted to mint target authority");
      }
    } else if (slot.status !== "unavailable") {
      throw new Error("PC4 reader profile status is invalid");
    }
  });
  return reader;
}

function previewReplay(pointer, replayState, nowUnixSeconds, bootstrapMinSequence) {
  const sequence = Number(pointer.statement.rollout_sequence);
  const now = Number(nowUnixSeconds);
  if (!Number.isSafeInteger(now) || now < Number(pointer.statement.issued_at_unix_seconds) ||
      now >= Number(pointer.statement.expires_at_unix_seconds)) {
    throw new Error("PC4 rollout pointer is outside its validity window");
  }
  if (sequence < bootstrapMinSequence) throw new Error("PC4 rollout is below the bootstrap floor");
  const state = {
    highest_sequence: sequence,
    pointer_identity: pointer.pointerIdentity,
    selected_generation_identity: pointer.statement.selected_generation_identity,
  };
  if (replayState === null) return { decision: "initial", state };
  if (sequence < replayState.highest_sequence) throw new Error("PC4 rollout replay rejected");
  if (sequence === replayState.highest_sequence) {
    if (pointer.pointerIdentity === replayState.pointer_identity &&
        pointer.statement.selected_generation_identity === replayState.selected_generation_identity) {
      return { decision: "idempotent", state: replayState };
    }
    throw new Error("PC4 rollout equivocation rejected");
  }
  if (sequence !== replayState.highest_sequence + 1 ||
      pointer.statement.previous_pointer_identity !== replayState.pointer_identity) {
    throw new Error("PC4 rollout chain has a gap");
  }
  if (!pointer.statement.retained_generations.some((entry) =>
    entry.generation_identity === replayState.selected_generation_identity)) {
    throw new Error("PC4 rollout discarded its immediate rollback generation");
  }
  return { decision: "advanced", state };
}
