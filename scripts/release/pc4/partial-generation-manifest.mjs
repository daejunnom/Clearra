// SRP: validates one already-resolved PC4 generation candidate. It performs no
// discovery, signing, network I/O, promotion, fallback, or product activation.

export const PC4_PARTIAL_GENERATION_MANIFEST_SCHEMA =
  "clearra.pc4.partial-generation-manifest.v1";

export const PC4_RULE_PROFILES = Object.freeze([
  "srs",
  "srs-plus",
  "srs-x",
  "jstris-180",
  "no-kick",
]);

export const PC4_NOT_QUALIFIED_REASONS = Object.freeze([
  "missing-profile-artifacts",
  "missing-profile-specific-index",
  "missing-format-specification",
  "missing-provenance",
  "missing-known-answers",
  "unknown-graph-encoding",
]);

const PROFILE_SET = new Set(PC4_RULE_PROFILES);
const REASON_SET = new Set(PC4_NOT_QUALIFIED_REASONS);
const ARTIFACT_ROLES = Object.freeze([
  "field-hash-index",
  "graph-offsets",
  "graph",
]);
const ARTIFACT_ROLE_SET = new Set(ARTIFACT_ROLES);
const USE_CASES = new Set(["pc-search", "setup-search"]);
const REPOSITORY = /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/u;
const REVISION = /^[0-9a-f]{40}$/u;
const IDENTITY = /^[A-Za-z0-9][A-Za-z0-9._:+/-]{0,255}$/u;
const ARTIFACT_PATH = /^[A-Za-z0-9][A-Za-z0-9._/-]{0,511}$/u;

export function validatePc4PartialGenerationManifest(value) {
  requireExactKeys(value, [
    "schema",
    "repository",
    "resolved_revision",
    "generation_id",
    "manifest_content_identity",
    "profiles",
  ], "PC4 partial generation manifest");
  if (value.schema !== PC4_PARTIAL_GENERATION_MANIFEST_SCHEMA) {
    throw new Error("PC4 partial generation manifest schema is unsupported");
  }
  const repository = requirePattern(value.repository, REPOSITORY, "repository");
  const resolvedRevision = requirePattern(
    value.resolved_revision,
    REVISION,
    "resolved revision",
  );
  const generationId = requireIdentity(value.generation_id, "generation ID");
  const manifestContentIdentity = requireIdentity(
    value.manifest_content_identity,
    "manifest content identity",
  );
  if (!Array.isArray(value.profiles) || value.profiles.length !== PC4_RULE_PROFILES.length) {
    throw new Error("PC4 partial generation manifest must contain exactly five profile slots");
  }

  const profileNames = new Set();
  const qualificationOwners = new Map();
  const artifactPathOwners = new Map();
  const artifactContentOwners = new Map();
  let qualifiedCount = 0;
  const profiles = value.profiles.map((profile) => {
    const normalized = validateProfileSlot(profile, {
      qualificationOwners,
      artifactPathOwners,
      artifactContentOwners,
    });
    if (profileNames.has(normalized.profile)) {
      throw new Error(`PC4 profile slot is duplicated: ${normalized.profile}`);
    }
    profileNames.add(normalized.profile);
    if (normalized.status === "qualified") qualifiedCount += 1;
    return normalized;
  });
  if (profileNames.size !== PROFILE_SET.size ||
      PC4_RULE_PROFILES.some((profile) => !profileNames.has(profile))) {
    throw new Error("PC4 partial generation manifest profile set is incomplete");
  }
  if (qualifiedCount === 0) {
    throw new Error("PC4 generation cannot activate without a qualified profile");
  }
  profiles.sort((left, right) =>
    PC4_RULE_PROFILES.indexOf(left.profile) - PC4_RULE_PROFILES.indexOf(right.profile));

  return deepFreeze({
    schema: PC4_PARTIAL_GENERATION_MANIFEST_SCHEMA,
    repository,
    resolved_revision: resolvedRevision,
    generation_id: generationId,
    manifest_content_identity: manifestContentIdentity,
    profiles,
  });
}

function validateProfileSlot(value, owners) {
  requireObject(value, "PC4 profile slot");
  const profile = requirePattern(value.profile, IDENTITY, "profile");
  if (!PROFILE_SET.has(profile)) throw new Error(`unknown PC4 profile: ${profile}`);
  if (value.status === "not_qualified") {
    requireExactKeys(value, ["profile", "status", "reason"], `PC4 ${profile} slot`);
    if (!REASON_SET.has(value.reason)) {
      throw new Error(`PC4 ${profile} not-qualified reason is unsupported`);
    }
    return { profile, status: "not_qualified", reason: value.reason };
  }
  if (value.status !== "qualified") {
    throw new Error(`PC4 ${profile} slot status is unsupported`);
  }
  requireExactKeys(value, [
    "profile",
    "status",
    "profile_binding_identity",
    "rule_identity",
    "graph_format_identity",
    "provenance_identity",
    "known_answers_identity",
    "artifacts",
    "targets",
  ], `PC4 ${profile} slot`);

  const identities = {
    profile_binding_identity: requireIdentity(
      value.profile_binding_identity,
      `${profile} profile binding identity`,
    ),
    rule_identity: requireIdentity(value.rule_identity, `${profile} rule identity`),
    graph_format_identity: requireIdentity(
      value.graph_format_identity,
      `${profile} graph format identity`,
    ),
    provenance_identity: requireIdentity(
      value.provenance_identity,
      `${profile} provenance identity`,
    ),
    known_answers_identity: requireIdentity(
      value.known_answers_identity,
      `${profile} known-answer identity`,
    ),
  };
  for (const [kind, identity] of Object.entries(identities)) {
    claimExclusiveIdentity(owners.qualificationOwners, identity, profile, kind);
  }

  if (!Array.isArray(value.artifacts) || value.artifacts.length !== ARTIFACT_ROLES.length) {
    throw new Error(`PC4 ${profile} must bind exactly three graph/index artifacts`);
  }
  const roles = new Set();
  const artifacts = value.artifacts.map((artifact) => {
    const normalized = validateArtifact(artifact, profile);
    if (roles.has(normalized.role)) {
      throw new Error(`PC4 ${profile} artifact role is duplicated: ${normalized.role}`);
    }
    roles.add(normalized.role);
    claimExclusiveIdentity(
      owners.artifactPathOwners,
      normalized.path,
      profile,
      "artifact path",
    );
    claimExclusiveIdentity(
      owners.artifactContentOwners,
      normalized.content_identity,
      profile,
      "artifact content identity",
    );
    claimExclusiveIdentity(
      owners.qualificationOwners,
      normalized.profile_binding_identity,
      profile,
      "artifact profile binding identity",
    );
    return normalized;
  });
  if (ARTIFACT_ROLES.some((role) => !roles.has(role))) {
    throw new Error(`PC4 ${profile} graph/index artifact set is incomplete`);
  }
  artifacts.sort((left, right) =>
    ARTIFACT_ROLES.indexOf(left.role) - ARTIFACT_ROLES.indexOf(right.role));

  if (!Array.isArray(value.targets) || value.targets.length === 0 || value.targets.length > 8) {
    throw new Error(`PC4 ${profile} must qualify at least one bounded target`);
  }
  const targetKeys = new Set();
  const targets = value.targets.map((target) => {
    const normalized = validateTarget(target, profile, owners.qualificationOwners);
    const key = `${normalized.use_case}:${normalized.lines}`;
    if (targetKeys.has(key)) {
      throw new Error(`PC4 ${profile} target is duplicated: ${key}`);
    }
    targetKeys.add(key);
    return normalized;
  });
  targets.sort((left, right) =>
    left.use_case.localeCompare(right.use_case) || left.lines - right.lines);

  return {
    profile,
    status: "qualified",
    ...identities,
    artifacts,
    targets,
  };
}

function validateArtifact(value, profile) {
  requireExactKeys(value, [
    "role",
    "path",
    "byte_length",
    "content_identity",
    "profile_binding_identity",
  ], `PC4 ${profile} artifact`);
  if (!ARTIFACT_ROLE_SET.has(value.role)) {
    throw new Error(`PC4 ${profile} artifact role is unsupported`);
  }
  const path = requirePattern(value.path, ARTIFACT_PATH, `${profile} artifact path`);
  if (path.includes("//") || path.includes("/../") || path.endsWith("/..")) {
    throw new Error(`PC4 ${profile} artifact path is not canonical`);
  }
  if (!Number.isSafeInteger(value.byte_length) || value.byte_length <= 0) {
    throw new Error(`PC4 ${profile} artifact byte length is invalid`);
  }
  return {
    role: value.role,
    path,
    byte_length: value.byte_length,
    content_identity: requireIdentity(
      value.content_identity,
      `${profile} artifact content identity`,
    ),
    profile_binding_identity: requireIdentity(
      value.profile_binding_identity,
      `${profile} artifact profile binding identity`,
    ),
  };
}

function validateTarget(value, profile, qualificationOwners) {
  requireExactKeys(value, [
    "use_case",
    "lines",
    "terminal_identity",
    "outgoing_edge_completeness_identity",
    "known_answers_identity",
    "offline_exact_parity_identity",
  ], `PC4 ${profile} target`);
  if (!USE_CASES.has(value.use_case)) {
    throw new Error(`PC4 ${profile} target use case is unsupported`);
  }
  if (!Number.isInteger(value.lines) || value.lines < 1 || value.lines > 4) {
    throw new Error(`PC4 ${profile} target lines are outside 1..=4`);
  }
  const result = {
    use_case: value.use_case,
    lines: value.lines,
    terminal_identity: requireIdentity(
      value.terminal_identity,
      `${profile} terminal identity`,
    ),
    outgoing_edge_completeness_identity: requireIdentity(
      value.outgoing_edge_completeness_identity,
      `${profile} outgoing-edge completeness identity`,
    ),
    known_answers_identity: requireIdentity(
      value.known_answers_identity,
      `${profile} target known-answer identity`,
    ),
    offline_exact_parity_identity: requireIdentity(
      value.offline_exact_parity_identity,
      `${profile} offline exact parity identity`,
    ),
  };
  for (const [kind, identity] of Object.entries(result)) {
    if (kind === "use_case" || kind === "lines") continue;
    claimExclusiveIdentity(qualificationOwners, identity, profile, `target ${kind}`);
  }
  return result;
}

function claimExclusiveIdentity(owners, identity, profile, kind) {
  const owner = owners.get(identity);
  if (owner !== undefined && owner !== profile) {
    throw new Error(
      `PC4 ${kind} cannot be borrowed across profiles: ${owner} -> ${profile}`,
    );
  }
  owners.set(identity, profile);
}

function requireObject(value, label) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object`);
  }
  return value;
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

function requirePattern(value, pattern, label) {
  if (typeof value !== "string" || !pattern.test(value)) {
    throw new Error(`${label} is invalid`);
  }
  return value;
}

function requireIdentity(value, label) {
  return requirePattern(value, IDENTITY, label);
}

function deepFreeze(value) {
  Object.freeze(value);
  for (const child of Object.values(value)) {
    if (child !== null && typeof child === "object" && !Object.isFrozen(child)) {
      deepFreeze(child);
    }
  }
  return value;
}
