// SRP: compile already-admitted, immutable PC4 qualification evidence into the
// exact host-generation bytes consumed by products. This module performs no
// network I/O, upstream selection, persistence, signing, promotion, fallback,
// or product activation.

import {
  canonicalJson,
  canonicalSha256,
} from "../canonical-release-evidence.mjs";
import {
  PC4_RULE_PROFILES,
  validatePc4PartialGenerationManifest,
} from "./partial-generation-manifest.mjs";

export const PC4_PRODUCTION_HOST_GENERATION_SCHEMA =
  "clearra.pc4.host-generation.v1";

const PC_TARGET_RECEIPT_SCHEMA =
  "clearra.pc4.exact-target-qualification.v1";
const SETUP_TARGET_RECEIPT_SCHEMA =
  "clearra.pc4.exact-setup-target-qualification.v1";
const PC_TERMINAL_SEMANTICS =
  "clearra.pc4.full-bottom-rows-after-clear.v1";
const SETUP_TERMINAL_SEMANTICS =
  "clearra.pc4.setup-complete-bottom-rows-after-clear.v1";
const SHA256_IDENTITY = /^sha256:[0-9a-f]{64}$/u;
const FORBIDDEN_ARTIFACT_ROLE = /(?:^|[._/-])(?:vstar|v-star|value|policy|krylov)(?:[._/-]|$)/iu;
const FULL_FOUR_ROW_FIELD = 2 ** 40 - 1;
const MAX_HOST_GENERATION_BYTES = 65_536;
const ARTIFACT_KEYS = Object.freeze([
  ["fields", "field-hash-index"],
  ["offsets", "graph-offsets"],
  ["graph", "graph"],
]);

/**
 * Returns canonical UTF-8 JSON text with one final LF. Inputs are evidence
 * objects only: the caller must separately persist/sign/promote the result.
 */
export function compilePc4ProductionHostGeneration(input) {
  requireExactKeys(input, [
    "partialGeneration",
    "readerGeneration",
    "targetQualificationReceipts",
  ], "PC4 production generation compiler input");

  const partial = validatePc4PartialGenerationManifest(input.partialGeneration);
  const readers = validateReaderGeneration(input.readerGeneration, partial);
  const receipts = validateTargetReceipts(
    input.targetQualificationReceipts,
    partial,
    readers,
  );

  const profiles = partial.profiles.map((slot) => {
    if (slot.status === "not_qualified") {
      return {
        profile: slot.profile,
        upstream_complete: false,
        status: "unavailable",
        reason: slot.reason,
      };
    }
    return compileQualifiedProfile(slot, readers.get(slot.profile), receipts.get(slot.profile));
  });

  const generation = {
    schema: PC4_PRODUCTION_HOST_GENERATION_SCHEMA,
    repository: partial.repository,
    revision: partial.resolved_revision,
    admission: {
      generation_id: partial.generation_id,
      manifest_content_identity: partial.manifest_content_identity,
    },
    profiles,
    // Transport counters are observations, not generation identity. Production
    // bytes always reset the diagnostic so repeated compilation is identical.
    transferred_bytes: 0,
  };
  const output = `${canonicalJson(generation)}\n`;
  if (new TextEncoder().encode(output).byteLength > MAX_HOST_GENERATION_BYTES) {
    throw new Error("PC4 production host generation exceeds the product parser bound");
  }
  return output;
}

function validateReaderGeneration(value, partial) {
  requireObject(value, "PC4 reader-qualified generation");
  if (value.schema !== PC4_PRODUCTION_HOST_GENERATION_SCHEMA ||
      value.repository !== partial.repository ||
      value.revision !== partial.resolved_revision ||
      !Number.isSafeInteger(value.transferred_bytes) || value.transferred_bytes < 0 ||
      !Array.isArray(value.profiles) || value.profiles.length !== PC4_RULE_PROFILES.length) {
    throw new Error("PC4 reader-qualified generation does not match the admitted generation");
  }
  const profiles = new Map();
  for (const slot of value.profiles) {
    requireObject(slot, "PC4 reader profile slot");
    if (!PC4_RULE_PROFILES.includes(slot.profile) || profiles.has(slot.profile)) {
      throw new Error("PC4 reader-qualified generation profile set is invalid");
    }
    profiles.set(slot.profile, slot);
  }
  if (PC4_RULE_PROFILES.some((profile) => !profiles.has(profile))) {
    throw new Error("PC4 reader-qualified generation profile set is incomplete");
  }
  return profiles;
}

function compileQualifiedProfile(slot, reader, receipts) {
  if (reader?.status !== "ready" || reader.upstream_complete !== true) {
    throw new Error(`PC4 ${slot.profile} lacks an exact reader qualification`);
  }
  requireExactKeys(reader, [
    "profile",
    "upstream_complete",
    "status",
    "reader_contract",
    "field_count",
    "target_width",
    "target_lines",
    "pc_search_target_lines",
    "setup_search_target_lines",
    "target_qualification_receipts",
    "terminal_id",
    "artifacts",
    "evidence",
  ], `PC4 ${slot.profile} reader qualification`);
  if (typeof reader.reader_contract !== "string" || reader.reader_contract.length === 0 ||
      reader.reader_contract.length > 256 ||
      !Number.isSafeInteger(reader.field_count) || reader.field_count < 2 ||
      reader.field_count > 2 ** 24 ||
      ![3, 4].includes(reader.target_width) ||
      canonicalJson(reader.target_lines) !== "[4]" ||
      reader.terminal_id !== reader.field_count - 1 ||
      canonicalJson(reader.pc_search_target_lines) !== "[]" ||
      canonicalJson(reader.setup_search_target_lines) !== "[]" ||
      canonicalJson(reader.target_qualification_receipts) !== "[]") {
    throw new Error(`PC4 ${slot.profile} reader qualification is not a clean 4L base`);
  }

  const artifacts = validateAndBindArtifacts(slot, reader);
  const evidence = validateReaderEvidence(reader.evidence, reader.field_count, artifacts.graph);
  const profileReceipts = receipts ?? [];
  if (profileReceipts.length !== slot.targets.length) {
    throw new Error(`PC4 ${slot.profile} is missing an exact target qualification receipt`);
  }
  for (const receipt of profileReceipts) {
    if (receipt.reader_contract !== reader.reader_contract ||
        receipt.terminal_id !== reader.terminal_id) {
      throw new Error(`PC4 ${slot.profile} target receipt does not bind the reader`);
    }
  }

  const pcReceipts = profileReceipts.filter(({ use_case: useCase }) => useCase === "pc-search");
  const setupReceipts = profileReceipts.filter(({ use_case: useCase }) => useCase === "setup-search");
  return {
    profile: slot.profile,
    upstream_complete: true,
    status: "ready",
    reader_contract: reader.reader_contract,
    field_count: reader.field_count,
    target_width: reader.target_width,
    target_lines: [4],
    pc_search_target_lines: pcReceipts.map(({ target_lines: lines }) => lines),
    setup_search_target_lines: setupReceipts.map(({ target_lines: lines }) => lines),
    target_qualification_receipts: [...pcReceipts, ...setupReceipts],
    terminal_id: reader.terminal_id,
    artifacts,
    evidence,
    admission: {
      profile_binding_identity: slot.profile_binding_identity,
      rule_identity: slot.rule_identity,
      graph_format_identity: slot.graph_format_identity,
      provenance_identity: slot.provenance_identity,
      known_answers_identity: slot.known_answers_identity,
      // The partial manifest's terminal identity is profile-specific admission
      // evidence. The target receipt separately binds the executable terminal
      // ID/hash/semantics; keeping both under the same target key avoids
      // pretending the shared terminal-semantics label is a profile receipt.
      targets: slot.targets,
    },
  };
}

function validateAndBindArtifacts(slot, reader) {
  requireExactKeys(reader.artifacts, ["fields", "offsets", "graph"],
    `PC4 ${slot.profile} reader artifacts`);
  const admitted = new Map(slot.artifacts.map((artifact) => [artifact.role, artifact]));
  const result = {};
  for (const [key, role] of ARTIFACT_KEYS) {
    const actual = reader.artifacts[key];
    const expected = admitted.get(role);
    requireExactKeys(actual, ["path", "byte_length", "content_identity"],
      `PC4 ${slot.profile} ${key} artifact`);
    if (actual.path !== expected?.path || actual.byte_length !== expected?.byte_length ||
        actual.content_identity !== expected?.content_identity ||
        !SHA256_IDENTITY.test(actual.content_identity) ||
        FORBIDDEN_ARTIFACT_ROLE.test(actual.path)) {
      throw new Error(`PC4 ${slot.profile} reader artifact differs from admission: ${key}`);
    }
    result[key] = {
      path: actual.path,
      byte_length: actual.byte_length,
      content_identity: actual.content_identity,
    };
  }
  if (result.fields.byte_length !== 16 + reader.field_count * 8 ||
      result.offsets.byte_length !== 16 + (reader.field_count + 1) * 4) {
    throw new Error(`PC4 ${slot.profile} reader index lengths are inconsistent`);
  }
  return result;
}

function validateReaderEvidence(value, fieldCount, graph) {
  if (!Array.isArray(value) || value.length < 2 || value.length > 16) {
    throw new Error("PC4 reader evidence is outside the bounded sample set");
  }
  const evidence = value.map((entry) => {
    requireExactKeys(entry, ["id", "hash", "start", "end"], "PC4 reader evidence entry");
    for (const key of ["id", "hash", "start", "end"]) {
      if (!Number.isSafeInteger(entry[key]) || entry[key] < 0) {
        throw new Error("PC4 reader evidence contains an invalid integer");
      }
    }
    if (entry.id >= fieldCount || entry.hash > FULL_FOUR_ROW_FIELD ||
        entry.end <= entry.start || entry.end > graph.byte_length) {
      throw new Error("PC4 reader evidence is outside the graph bounds");
    }
    return { id: entry.id, hash: entry.hash, start: entry.start, end: entry.end };
  });
  for (let index = 1; index < evidence.length; index += 1) {
    if (evidence[index - 1].id >= evidence[index].id ||
        evidence[index - 1].end > evidence[index].start) {
      throw new Error("PC4 reader evidence is not strictly ordered");
    }
  }
  const first = evidence[0];
  const last = evidence.at(-1);
  if (first.id !== 0 || first.hash !== 0 || first.start !== 0 ||
      last.id !== fieldCount - 1 || last.hash !== FULL_FOUR_ROW_FIELD ||
      last.end !== graph.byte_length) {
    throw new Error("PC4 reader evidence does not bind root and terminal records");
  }
  return evidence;
}

function validateTargetReceipts(values, partial, readers) {
  if (!Array.isArray(values) || values.length === 0 || values.length > 16) {
    throw new Error("PC4 production generation needs bounded target receipts");
  }
  const qualified = new Map(partial.profiles
    .filter(({ status }) => status === "qualified")
    .map((slot) => [slot.profile, slot]));
  const byProfile = new Map();
  const keys = new Set();
  const identities = new Set();

  for (const value of values) {
    const receipt = validateTargetReceipt(value, partial, qualified, readers);
    const key = `${receipt.profile}:${receipt.use_case}:${receipt.target_lines}`;
    if (keys.has(key) || identities.has(receipt.receipt_identity)) {
      throw new Error("PC4 production target receipt is duplicated");
    }
    keys.add(key);
    identities.add(receipt.receipt_identity);
    const list = byProfile.get(receipt.profile) ?? [];
    list.push(receipt);
    byProfile.set(receipt.profile, list);
  }

  for (const slot of qualified.values()) {
    const expected = new Set(slot.targets.map(({ use_case: useCase, lines }) =>
      `${slot.profile}:${useCase}:${lines}`));
    const actual = byProfile.get(slot.profile) ?? [];
    if (actual.length !== expected.size ||
        actual.some((receipt) => !expected.has(
          `${receipt.profile}:${receipt.use_case}:${receipt.target_lines}`))) {
      throw new Error(`PC4 ${slot.profile} target receipts do not cover admission exactly`);
    }
    actual.sort((left, right) =>
      useCaseOrder(left.use_case) - useCaseOrder(right.use_case) ||
      left.target_lines - right.target_lines);
  }
  return byProfile;
}

function validateTargetReceipt(value, partial, qualified, readers) {
  requireObject(value, "PC4 production target receipt");
  const receiptIdentity = requireSha256(value.receipt_identity,
    "PC4 target receipt identity");
  const { receipt_identity: _identity, ...core } = value;
  if (`sha256:${canonicalSha256(core)}` !== receiptIdentity) {
    throw new Error("PC4 target receipt identity differs from canonical content");
  }
  const slot = qualified.get(value.profile);
  const reader = readers.get(value.profile);
  if (!slot || reader?.status !== "ready" || value.repository !== partial.repository ||
      value.revision !== partial.resolved_revision || value.target_lines !== 4 ||
      value.terminal_id !== reader.terminal_id ||
      value.terminal_hash !== FULL_FOUR_ROW_FIELD ||
      value.reader_contract !== reader.reader_contract) {
    throw new Error("PC4 target receipt is stale or generation-mismatched");
  }
  const target = slot.targets.find(({ use_case: useCase, lines }) =>
    useCase === value.use_case && lines === value.target_lines);
  if (!target ||
      target.outgoing_edge_completeness_identity !== value.outgoing_edge_completeness_identity ||
      target.known_answers_identity !== value.known_answer_identity ||
      target.offline_exact_parity_identity !== value.offline_exact_parity_identity) {
    throw new Error("PC4 target receipt differs from the admitted target");
  }
  for (const key of [
    "outgoing_edge_completeness_identity",
    "known_answer_identity",
    "offline_exact_parity_identity",
  ]) {
    requireSha256(value[key], `PC4 target receipt ${key}`);
  }

  const isPc = value.schema === PC_TARGET_RECEIPT_SCHEMA &&
    value.use_case === "pc-search" &&
    value.terminal_semantics_identity === PC_TERMINAL_SEMANTICS &&
    !Object.hasOwn(value, "setup_differential_identities");
  const isSetup = value.schema === SETUP_TARGET_RECEIPT_SCHEMA &&
    value.use_case === "setup-search" &&
    value.terminal_semantics_identity === SETUP_TERMINAL_SEMANTICS;
  if (!isPc && !isSetup) {
    throw new Error("PC4 target receipt schema and use case do not agree");
  }
  if (isSetup) validateSetupDifferential(value.setup_differential_identities);

  const outgoing = value.evidence?.outgoing_statement;
  requireObject(outgoing, "PC4 target receipt outgoing statement");
  if (outgoing.receipt_identity !== value.outgoing_edge_completeness_identity ||
      canonicalJson(outgoing.artifacts) !== canonicalJson(publicArtifacts(slot))) {
    throw new Error("PC4 target receipt does not bind the admitted artifacts");
  }
  return JSON.parse(canonicalJson(value));
}

function validateSetupDifferential(value) {
  requireExactKeys(value, [
    "ranked_joint_identity",
    "ranked_build_probability_identity",
    "ranked_conditional_pc_identity",
    "exact_path_detail_identity",
  ], "PC4 Setup target differential identities");
  for (const [key, identity] of Object.entries(value)) {
    requireSha256(identity, `PC4 Setup target ${key}`);
  }
}

function publicArtifacts(slot) {
  const artifacts = new Map(slot.artifacts.map((artifact) => [artifact.role, artifact]));
  return Object.fromEntries(ARTIFACT_KEYS.map(([key, role]) => {
    const artifact = artifacts.get(role);
    return [key, {
      path: artifact.path,
      byte_length: artifact.byte_length,
      content_identity: artifact.content_identity,
    }];
  }));
}

function useCaseOrder(value) {
  return value === "pc-search" ? 0 : 1;
}

function requireObject(value, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
}

function requireExactKeys(value, keys, label) {
  requireObject(value, label);
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  if (actual.length !== expected.length ||
      actual.some((key, index) => key !== expected[index])) {
    throw new Error(`${label} fields are not exact`);
  }
}

function requireSha256(value, label) {
  if (typeof value !== "string" || !SHA256_IDENTITY.test(value) ||
      value === `sha256:${"0".repeat(64)}`) {
    throw new Error(`${label} is not an exact SHA-256 identity`);
  }
  return value;
}
