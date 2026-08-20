export const RUNTIME_IDENTITY_SCHEMA = "clearra.runtime.identity.v2";
export const CONTRACT_SCHEMA_VERSION = "clearra.search.contract.v2";
export const LEGACY_CONTRACT_SCHEMA_VERSION =
  "clearra.search.contract.legacy-v1";
export const SUPPLY_SEMANTICS_ID =
  "clearra.supply.projected-terminal-lookahead.v1";
export const LEGACY_SUPPLY_SEMANTICS_ID = "clearra.supply.legacy-v1";
export const ARTIFACT_SCHEMA_VERSION = "clearra.solution-data.v1";
export const LEGACY_ARTIFACT_SCHEMA_VERSION =
  "clearra.solution-data.legacy-v1";

// Compatibility aliases are source-only. The v2 wire field is
// `contractSchemaVersion`, never the former `contractRevision` key.
export const SEARCH_CONTRACT_REVISION = CONTRACT_SCHEMA_VERSION;
export const LEGACY_SEARCH_CONTRACT_REVISION = LEGACY_CONTRACT_SCHEMA_VERSION;

const COMMIT_PATTERN = /^[0-9a-f]{40}$/;

export function runtimeIdentityFromEnvironment(environment, options = {}) {
  const required = options.required ?? environment.NODE_ENV === "production";
  const sourceCommit = environment.CLEARRA_SOURCE_COMMIT?.trim().toLowerCase();
  const engineBuildId = environment.CLEARRA_ENGINE_BUILD_ID?.trim().toLowerCase();
  const declaredContractSchemaVersion =
    environment.CLEARRA_SEARCH_CONTRACT_REVISION?.trim();
  const declaredSupplySemanticsId =
    environment.CLEARRA_SUPPLY_SEMANTICS_ID?.trim();
  const declaredArtifactSchemaVersion =
    environment.CLEARRA_ARTIFACT_SCHEMA_VERSION?.trim();
  if (required && !declaredContractSchemaVersion) {
    throw new Error(
      "Clearra production runtime must declare its contract schema version.",
    );
  }
  if (required && !declaredSupplySemanticsId) {
    throw new Error(
      "Clearra production runtime must declare its supply semantics ID.",
    );
  }
  if (required && !declaredArtifactSchemaVersion) {
    throw new Error(
      "Clearra production runtime must declare its artifact schema version.",
    );
  }
  const contractSchemaVersion =
    declaredContractSchemaVersion || CONTRACT_SCHEMA_VERSION;
  const supplySemanticsId = declaredSupplySemanticsId || SUPPLY_SEMANTICS_ID;
  const artifactSchemaVersion =
    declaredArtifactSchemaVersion || ARTIFACT_SCHEMA_VERSION;

  if (!sourceCommit && !engineBuildId && !required) return null;
  return normalizeRuntimeIdentity({
    schema: RUNTIME_IDENTITY_SCHEMA,
    sourceCommit,
    engineBuildId,
    contractSchemaVersion,
    supplySemanticsId,
    artifactSchemaVersion,
  });
}

export function normalizeRuntimeIdentity(value) {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Clearra runtime identity must be an object.");
  }
  if (value.schema !== RUNTIME_IDENTITY_SCHEMA) {
    throw new Error("Clearra runtime identity schema is not supported.");
  }
  const sourceCommit = canonicalCommit(value.sourceCommit, "source commit");
  const engineBuildId = canonicalCommit(value.engineBuildId, "engine build ID");
  if (
    value.contractSchemaVersion !== CONTRACT_SCHEMA_VERSION &&
    value.contractSchemaVersion !== LEGACY_CONTRACT_SCHEMA_VERSION
  ) {
    throw new Error("Clearra contract schema version is not supported.");
  }
  const expectedStableIdentity = value.contractSchemaVersion === CONTRACT_SCHEMA_VERSION
    ? {
      supplySemanticsId: SUPPLY_SEMANTICS_ID,
      artifactSchemaVersion: ARTIFACT_SCHEMA_VERSION,
    }
    : {
      supplySemanticsId: LEGACY_SUPPLY_SEMANTICS_ID,
      artifactSchemaVersion: LEGACY_ARTIFACT_SCHEMA_VERSION,
    };
  if (value.supplySemanticsId !== expectedStableIdentity.supplySemanticsId) {
    throw new Error("Clearra supply semantics ID is not supported.");
  }
  if (value.artifactSchemaVersion !== expectedStableIdentity.artifactSchemaVersion) {
    throw new Error("Clearra artifact schema version is not supported.");
  }
  return Object.freeze({
    schema: RUNTIME_IDENTITY_SCHEMA,
    sourceCommit,
    engineBuildId,
    contractSchemaVersion: value.contractSchemaVersion,
    supplySemanticsId: value.supplySemanticsId,
    artifactSchemaVersion: value.artifactSchemaVersion,
  });
}

export function runtimeIdentityMatches(left, right) {
  try {
    const a = normalizeRuntimeIdentity(left);
    const b = normalizeRuntimeIdentity(right);
    return a.schema === b.schema &&
      a.sourceCommit === b.sourceCommit &&
      a.engineBuildId === b.engineBuildId &&
      a.contractSchemaVersion === b.contractSchemaVersion &&
      a.supplySemanticsId === b.supplySemanticsId &&
      a.artifactSchemaVersion === b.artifactSchemaVersion;
  } catch {
    return false;
  }
}

export function currentRuntimeIdentityForCommit(sourceCommit) {
  const commit = canonicalCommit(sourceCommit, "source commit");
  return normalizeRuntimeIdentity({
    schema: RUNTIME_IDENTITY_SCHEMA,
    sourceCommit: commit,
    engineBuildId: commit,
    contractSchemaVersion: CONTRACT_SCHEMA_VERSION,
    supplySemanticsId: SUPPLY_SEMANTICS_ID,
    artifactSchemaVersion: ARTIFACT_SCHEMA_VERSION,
  });
}

export function productBuildIdentityFromRuntime(value) {
  const identity = normalizeRuntimeIdentity(value);
  return Object.freeze({
    source_commit: identity.sourceCommit,
    engine_build_id: identity.engineBuildId,
    contract_schema_version: identity.contractSchemaVersion,
    supply_semantics_id: identity.supplySemanticsId,
    artifact_schema_version: identity.artifactSchemaVersion,
  });
}

export function productBuildIdentityMatchesRuntime(productIdentity, runtimeIdentity) {
  if (!productIdentity || typeof productIdentity !== "object" ||
      Array.isArray(productIdentity)) return false;
  try {
    const expected = productBuildIdentityFromRuntime(runtimeIdentity);
    return productIdentity.source_commit === expected.source_commit &&
      productIdentity.engine_build_id === expected.engine_build_id &&
      productIdentity.contract_schema_version === expected.contract_schema_version &&
      productIdentity.supply_semantics_id === expected.supply_semantics_id &&
      productIdentity.artifact_schema_version === expected.artifact_schema_version;
  } catch {
    return false;
  }
}

function canonicalCommit(value, label) {
  const normalized = typeof value === "string" ? value.trim().toLowerCase() : "";
  if (!COMMIT_PATTERN.test(normalized)) {
    throw new Error(`Clearra runtime ${label} must be a full Git commit SHA.`);
  }
  return normalized;
}
