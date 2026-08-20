import { spawnSync } from "node:child_process";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

const PROJECT_PATTERN = /^[a-z][a-z0-9-]{4,28}[a-z0-9]$/;
const PROJECT_NUMBER_PATTERN = /^[1-9][0-9]{5,19}$/;
const ACCOUNT_PATTERN = /^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+$/;
const SECRET_ID_PATTERN = /^[A-Za-z0-9_-]{1,255}$/;
const LOCATION_ID_PATTERN = /^[a-z](?:[a-z0-9-]{0,61}[a-z0-9])?$/;
const RUNTIME_SERVICE_ACCOUNT_ID = "clearra-current-job";
const BUILD_SERVICE_ACCOUNT_ID = "clearra-build";
const ARTIFACT_REPOSITORY = "clearra";
const ARTIFACT_REGION = "asia-northeast1";
const JOB_SECRET = "clearra-job-token";
const DISCORD_SECRET = "discord-bot-token";
const POLICY_TROUBLESHOOTER_SERVICE = "policytroubleshooter.googleapis.com";
const PROJECT_PAB_SEARCH_PERMISSION = "resourcemanager.projects.searchPolicyBindings";
const SECRET_ACCESS_PERMISSION = "secretmanager.versions.access";
const SECRET_SET_IAM_POLICY_PERMISSION = "secretmanager.secrets.setIamPolicy";
const SECRET_ACCESSOR_ROLE = "roles/secretmanager.secretAccessor";
const SECRET_MANAGER_ENDPOINT_ENV = "CLOUDSDK_API_ENDPOINT_OVERRIDES_SECRETMANAGER";
const GLOBAL_SECRET_MANAGER_ENDPOINT = "https://secretmanager.googleapis.com/";
const OWNER_ROLE = "roles/owner";
const SERVICE_ACCOUNT_USER_ROLE = "roles/iam.serviceAccountUser";
const MAX_OUTPUT_BYTES = 1024 * 1024;
const OBSERVATION_DELAYS_MS = Object.freeze([0, 1_000, 2_000, 4_000, 8_000, 16_000, 20_000]);

const CLOUD_BUILD_CALLER_ROLES = Object.freeze(new Set([
  OWNER_ROLE,
  "roles/cloudbuild.builds.editor",
]));
const CLOUD_RUN_ADMIN_ROLES = Object.freeze(new Set([
  OWNER_ROLE,
  "roles/run.admin",
]));
const ARTIFACT_READER_ROLES = Object.freeze(new Set([
  OWNER_ROLE,
  "roles/artifactregistry.admin",
  "roles/artifactregistry.reader",
  "roles/artifactregistry.writer",
]));
const SERVICE_ACCOUNT_CREATOR_ROLES = Object.freeze(new Set([
  OWNER_ROLE,
  "roles/iam.serviceAccountAdmin",
  "roles/iam.serviceAccountCreator",
]));
const SECRET_METADATA_VIEWER_ROLES = Object.freeze(new Set([
  OWNER_ROLE,
  "roles/iam.securityReviewer",
  "roles/secretmanager.admin",
  "roles/secretmanager.viewer",
]));
const SERVICE_ACCOUNT_POLICY_VIEWER_ROLES = Object.freeze(new Set([
  OWNER_ROLE,
  "roles/iam.securityReviewer",
  "roles/iam.serviceAccountAdmin",
  "roles/iam.serviceAccountViewer",
]));
const POLICY_TROUBLESHOOTER_CALLER_ROLES = Object.freeze(new Set([
  OWNER_ROLE,
  "roles/serviceusage.serviceUsageConsumer",
]));
const SECURITY_REVIEWER_ROLES = Object.freeze(new Set([
  OWNER_ROLE,
  "roles/iam.securityReviewer",
]));
const DENY_REVIEWER_ROLES = Object.freeze(new Set([
  OWNER_ROLE,
  "roles/iam.denyReviewer",
]));
const GLOBAL_SECRET_MANAGER_EXECUTION = Object.freeze({
  environment: Object.freeze({
    [SECRET_MANAGER_ENDPOINT_ENV]: GLOBAL_SECRET_MANAGER_ENDPOINT,
  }),
});

export async function prepareCloudRuntimeServiceAccount(options, dependencies = {}) {
  const projectId = canonicalProjectId(options?.projectId);
  const run = dependencies.runGcloud ?? runGcloud;
  const wait = dependencies.wait ?? waitFor;
  const callerMember = canonicalCallerMember(
    options?.callerMember ?? requiredText(
      run,
      ["config", "get-value", "account", "--quiet"],
      "active gcloud account",
    ),
  );
  const runtimeEmail = `${RUNTIME_SERVICE_ACCOUNT_ID}@${projectId}.iam.gserviceaccount.com`;
  const runtimeMember = `serviceAccount:${runtimeEmail}`;
  const buildEmail = `${BUILD_SERVICE_ACCOUNT_ID}@${projectId}.iam.gserviceaccount.com`;
  const projectNumber = requiredProjectNumber(run, projectId);
  assertParentlessProject(run, projectId);
  assertNoProjectPabBindings(run, projectId, projectNumber);

  const projectPolicy = requiredJson(
    run,
    ["projects", "get-iam-policy", projectId, "--format=json"],
    "project IAM policy",
  );
  const callerProjectRoles = unconditionalRoles(projectPolicy, callerMember);
  requireAnyRole(
    callerProjectRoles,
    CLOUD_BUILD_CALLER_ROLES,
    "deployment caller cannot submit the exact-source Cloud Build",
  );
  requireAnyRole(
    callerProjectRoles,
    CLOUD_RUN_ADMIN_ROLES,
    "deployment caller cannot update the public Cloud Run service",
  );
  requireAnyRole(
    callerProjectRoles,
    SECRET_METADATA_VIEWER_ROLES,
    "deployment caller lacks project Secret Manager Viewer metadata authority",
  );
  requireAnyRole(
    callerProjectRoles,
    SERVICE_ACCOUNT_POLICY_VIEWER_ROLES,
    "deployment caller lacks service-account getIamPolicy read authority",
  );
  requireAnyRole(
    callerProjectRoles,
    POLICY_TROUBLESHOOTER_CALLER_ROLES,
    "deployment caller lacks Policy Troubleshooter service-usage authority",
  );
  requireAnyRole(
    callerProjectRoles,
    SECURITY_REVIEWER_ROLES,
    "deployment caller lacks Security Reviewer policy visibility",
  );
  requireAnyRole(
    callerProjectRoles,
    DENY_REVIEWER_ROLES,
    "deployment caller lacks Deny Reviewer policy visibility",
  );

  const troubleshooterServices = requiredJson(
    run,
    [
      "services",
      "list",
      "--enabled",
      `--project=${projectId}`,
      `--filter=config.name=${POLICY_TROUBLESHOOTER_SERVICE}`,
      "--format=json(config.name,state)",
    ],
    "Policy Troubleshooter API state",
  );
  if (
    !Array.isArray(troubleshooterServices) ||
    troubleshooterServices.length !== 1 ||
    troubleshooterServices[0]?.config?.name !== POLICY_TROUBLESHOOTER_SERVICE ||
    troubleshooterServices[0]?.state !== "ENABLED"
  ) {
    throw new Error("Policy Troubleshooter API must be enabled as an explicit prerequisite");
  }

  const buildAccount = requiredServiceAccount(run, projectId, buildEmail, "build service account");
  const buildPolicy = requiredJson(
    run,
    ["iam", "service-accounts", "get-iam-policy", buildEmail, `--project=${projectId}`, "--format=json"],
    "build service account IAM policy",
  );
  requireActAs(callerProjectRoles, buildPolicy, callerMember, "build service account");

  const repositoryPolicy = requiredJson(
    run,
    [
      "artifacts",
      "repositories",
      "get-iam-policy",
      ARTIFACT_REPOSITORY,
      `--project=${projectId}`,
      `--location=${ARTIFACT_REGION}`,
      "--format=json",
    ],
    "Artifact Registry repository IAM policy",
  );
  const callerRepositoryRoles = unconditionalRoles(repositoryPolicy, callerMember);
  if (
    !hasAnyRole(callerProjectRoles, ARTIFACT_READER_ROLES) &&
    !hasAnyRole(callerRepositoryRoles, ARTIFACT_READER_ROLES)
  ) {
    throw new Error("deployment caller cannot read the release image repository");
  }

  const initialSecretInventory = requiredSecretInventory(run, projectId, projectNumber);
  const secrets = initialSecretInventory.secrets;
  const jobSecret = secrets.find(isGlobalJobSecret);
  if (
    !jobSecret ||
    !secrets.some(({ location, name }) => location === null && name === DISCORD_SECRET)
  ) {
    throw new Error("required managed Secret metadata is unavailable");
  }
  requiredEffectivePermission(
    run,
    projectId,
    buildEmail,
    jobSecret,
    SECRET_ACCESS_PERMISSION,
  );
  const jobSecretState = requiredJson(
    run,
    [
      "secrets",
      "versions",
      "describe",
      "latest",
      `--secret=${JOB_SECRET}`,
      `--project=${projectId}`,
      "--format=json",
    ],
    "job bearer Secret version metadata",
    globalSecretManagerExecution(),
  );
  if (jobSecretState?.state !== "ENABLED") {
    throw new Error("job bearer Secret latest version is not enabled");
  }

  let runtimeObservation = observeServiceAccount(run, projectId, runtimeEmail);
  const runtimeWasMissing = runtimeObservation.missing;
  if (runtimeWasMissing) {
    requireAnyRole(
      callerProjectRoles,
      SERVICE_ACCOUNT_CREATOR_ROLES,
      "deployment caller cannot create the dedicated runtime service account",
    );
    if (!hasActAsProjectRole(callerProjectRoles)) {
      throw new Error("deployment caller must have project-level actAs before runtime account creation");
    }
  } else {
    assertEnabledServiceAccount(runtimeObservation.value, runtimeEmail, "runtime service account");
  }

  const initialJobPolicy = requiredSecretPolicy(run, projectId, jobSecret);
  const initialRuntimeJobRoles = allRoles(initialJobPolicy, runtimeMember);
  if (
    initialRuntimeJobRoles.length > 0 &&
    !hasExactUnconditionalRoles(initialJobPolicy, runtimeMember, [SECRET_ACCESSOR_ROLE])
  ) {
    throw new Error("runtime service account has non-canonical job Secret authority");
  }
  if (initialRuntimeJobRoles.length === 0) {
    const callerSecretPolicyAccess = requiredEffectivePermission(
      run,
      projectId,
      principalEmail(callerMember),
      jobSecret,
      SECRET_SET_IAM_POLICY_PERMISSION,
    );
    if (callerSecretPolicyAccess !== "GRANTED") {
      throw new Error("deployment caller cannot bind the exact job bearer Secret");
    }
  }

  if (runtimeWasMissing) {
    const creation = run([
      "iam",
      "service-accounts",
      "create",
      RUNTIME_SERVICE_ACCOUNT_ID,
      `--project=${projectId}`,
      "--display-name=Clearra current job runtime",
      "--description=Dedicated least-privilege identity for clearra-current-job",
      "--quiet",
    ]);
    runtimeObservation = await observeServiceAccountEventually(
      run,
      wait,
      projectId,
      runtimeEmail,
    );
    if (!isSuccessful(creation) && runtimeObservation.missing) {
      throw new Error("runtime service account creation failed");
    }
    if (runtimeObservation.missing) {
      throw new Error("created runtime service account is not observable");
    }
    assertEnabledServiceAccount(runtimeObservation.value, runtimeEmail, "runtime service account");
  }

  const projectPolicyAfterCreate = requiredJson(
    run,
    ["projects", "get-iam-policy", projectId, "--format=json"],
    "post-bootstrap project IAM policy",
  );
  if (allRoles(projectPolicyAfterCreate, runtimeMember).length !== 0) {
    throw new Error("runtime service account must have zero project-level roles");
  }
  const callerProjectRolesAfterCreate = unconditionalRoles(projectPolicyAfterCreate, callerMember);
  const runtimePolicy = requiredJson(
    run,
    ["iam", "service-accounts", "get-iam-policy", runtimeEmail, `--project=${projectId}`, "--format=json"],
    "runtime service account IAM policy",
  );
  requireActAs(
    callerProjectRolesAfterCreate,
    runtimePolicy,
    callerMember,
    "runtime service account",
  );

  const preBindingSecretInventory = requiredSecretInventory(run, projectId, projectNumber);
  assertSecretInventoryUnchanged(initialSecretInventory, preBindingSecretInventory);
  const preBindingAuthority = verifyPreBindingSecretAuthority(
    run,
    projectId,
    preBindingSecretInventory.secrets,
    runtimeMember,
    runtimeEmail,
  );
  if (!preBindingAuthority.jobAccessorPresent && initialRuntimeJobRoles.length !== 0) {
    throw new Error("job Secret authority changed before binding preparation");
  }
  if (!preBindingAuthority.jobAccessorPresent && initialRuntimeJobRoles.length === 0) {
    const callerSecretPolicyAccess = requiredEffectivePermission(
      run,
      projectId,
      principalEmail(callerMember),
      jobSecret,
      SECRET_SET_IAM_POLICY_PERMISSION,
    );
    if (callerSecretPolicyAccess !== "GRANTED") {
      throw new Error("deployment caller cannot bind the exact job bearer Secret");
    }
  }

  let accessorAdded = false;
  if (!preBindingAuthority.jobAccessorPresent) {
    const addition = run(
      [
        "secrets",
        "add-iam-policy-binding",
        JOB_SECRET,
        `--project=${projectId}`,
        `--member=${runtimeMember}`,
        `--role=${SECRET_ACCESSOR_ROLE}`,
        "--condition=None",
        "--quiet",
        "--format=json",
      ],
      globalSecretManagerExecution(),
    );
    const observedPolicy = requiredSecretPolicy(run, projectId, jobSecret);
    if (
      !isSuccessful(addition) &&
      !hasExactUnconditionalRoles(observedPolicy, runtimeMember, [SECRET_ACCESSOR_ROLE])
    ) {
      throw new Error("job bearer Secret accessor binding failed");
    }
    accessorAdded = true;
  }

  const postBindingSecretInventory = requiredSecretInventory(run, projectId, projectNumber);
  assertSecretInventoryUnchanged(initialSecretInventory, postBindingSecretInventory);
  verifyExclusiveSecretAuthority(
    run,
    projectId,
    postBindingSecretInventory.secrets,
    runtimeMember,
    runtimeEmail,
  );
  const sealedSecretInventory = requiredSecretInventory(run, projectId, projectNumber);
  assertSecretInventoryUnchanged(initialSecretInventory, sealedSecretInventory);
  if (allRoles(
    requiredJson(
      run,
      ["projects", "get-iam-policy", projectId, "--format=json"],
      "final project IAM policy",
    ),
    runtimeMember,
  ).length !== 0) {
    throw new Error("runtime service account gained a project-level role during bootstrap");
  }
  assertParentlessProject(run, projectId);
  assertNoProjectPabBindings(run, projectId, projectNumber);

  return Object.freeze({
    projectId,
    callerMember,
    runtimeServiceAccount: runtimeEmail,
    buildServiceAccount: buildAccount.email,
    created: runtimeWasMissing,
    accessorAdded,
  });
}

function verifyPreBindingSecretAuthority(run, projectId, secrets, runtimeMember, runtimeEmail) {
  let jobAccessorPresent = false;
  for (const secret of secrets) {
    const policy = requiredSecretPolicy(run, projectId, secret);
    const roles = allRoles(policy, runtimeMember);
    const effectiveAccess = requiredEffectivePermission(
      run,
      projectId,
      runtimeEmail,
      secret,
      SECRET_ACCESS_PERMISSION,
    );
    if (isGlobalJobSecret(secret)) {
      if (roles.length === 0) {
        if (effectiveAccess !== "NOT_GRANTED") {
          throw new Error(
            "runtime service account has inherited effective job Secret access before binding",
          );
        }
        continue;
      }
      if (!hasExactUnconditionalRoles(policy, runtimeMember, [SECRET_ACCESSOR_ROLE])) {
        throw new Error("runtime service account has non-canonical job Secret authority");
      }
      if (effectiveAccess !== "GRANTED") {
        throw new Error("runtime service account lacks effective job Secret access");
      }
      jobAccessorPresent = true;
      continue;
    }
    assertNoDirectNonJobSecretAuthority(secret, roles);
    if (effectiveAccess !== "NOT_GRANTED") {
      throw new Error("runtime service account has effective non-job Secret access");
    }
  }
  return Object.freeze({ jobAccessorPresent });
}

function verifyExclusiveSecretAuthority(run, projectId, secrets, runtimeMember, runtimeEmail) {
  for (const secret of secrets) {
    const policy = requiredSecretPolicy(run, projectId, secret);
    const roles = allRoles(policy, runtimeMember);
    if (isGlobalJobSecret(secret)) {
      if (!hasExactUnconditionalRoles(policy, runtimeMember, [SECRET_ACCESSOR_ROLE])) {
        throw new Error("runtime service account lacks the exact job Secret accessor");
      }
      const effectiveAccess = requiredEffectivePermission(
        run,
        projectId,
        runtimeEmail,
        secret,
        SECRET_ACCESS_PERMISSION,
      );
      if (effectiveAccess !== "GRANTED") {
        throw new Error("runtime service account lacks effective job Secret access");
      }
      continue;
    }
    assertNoDirectNonJobSecretAuthority(secret, roles);
    const effectiveAccess = requiredEffectivePermission(
      run,
      projectId,
      runtimeEmail,
      secret,
      SECRET_ACCESS_PERMISSION,
    );
    if (effectiveAccess !== "NOT_GRANTED") {
      throw new Error("runtime service account has effective non-job Secret access");
    }
  }
}

function assertNoDirectNonJobSecretAuthority(secret, roles) {
  if (roles.length === 0) return;
  if (isGlobalDiscordSecret(secret)) {
    throw new Error("runtime service account must not access the Discord token Secret");
  }
  throw new Error("runtime service account must not access any non-job Secret");
}

function isGlobalJobSecret(secret) {
  return secret.location === null && secret.name === JOB_SECRET;
}

function isGlobalDiscordSecret(secret) {
  return secret.location === null && secret.name === DISCORD_SECRET;
}

function requiredSecretInventory(run, projectId, projectNumber) {
  const locations = requiredRegionalSecretLocations(run, projectId, projectNumber);
  const secrets = [
    ...requiredSecretCatalog(run, projectId, projectNumber, null),
  ];
  for (const location of locations) {
    secrets.push(...requiredSecretCatalog(run, projectId, projectNumber, location.location));
  }
  secrets.sort((left, right) => left.resourceName.localeCompare(right.resourceName));
  if (new Set(secrets.map(({ resourceName }) => resourceName)).size !== secrets.length) {
    throw new Error("Secret metadata inventory contains duplicate resources");
  }
  return Object.freeze({
    locations: Object.freeze([...locations]),
    secrets: Object.freeze(secrets),
  });
}

function requiredRegionalSecretLocations(run, projectId, projectNumber) {
  const value = requiredJson(
    run,
    ["secrets", "locations", "list", `--project=${projectId}`, "--format=json(name)"],
    "regional Secret location catalog",
    globalSecretManagerExecution(),
  );
  if (!Array.isArray(value) || value.length === 0) {
    throw new Error("regional Secret location catalog is invalid or empty");
  }
  const prefixes = Object.freeze([
    `projects/${projectId}/locations/`,
    `projects/${projectNumber}/locations/`,
  ]);
  const locations = value.map((entry) => {
    const resource = typeof entry?.name === "string" ? entry.name : "";
    const prefix = prefixes.find((candidate) => resource.startsWith(candidate));
    if (!prefix || resource.length === prefix.length) {
      throw new Error("regional Secret location catalog contains an invalid resource");
    }
    const location = resource.slice(prefix.length);
    if (!LOCATION_ID_PATTERN.test(location) || location === "global") {
      throw new Error("regional Secret location catalog contains an invalid resource");
    }
    return Object.freeze({ location, resourceName: resource });
  });
  if (
    new Set(locations.map(({ location }) => location)).size !== locations.length ||
    new Set(locations.map(({ resourceName }) => resourceName)).size !== locations.length
  ) {
    throw new Error("regional Secret location catalog contains duplicate resources");
  }
  return locations.sort((left, right) => left.location.localeCompare(right.location));
}

function requiredSecretCatalog(run, projectId, projectNumber, location) {
  const regional = location !== null;
  const arguments_ = ["secrets", "list", `--project=${projectId}`];
  if (regional) arguments_.push(`--location=${location}`);
  arguments_.push("--format=json(name)");
  const value = requiredJson(
    run,
    arguments_,
    regional ? `regional Secret metadata catalog (${location})` : "global Secret metadata catalog",
    secretManagerExecution(location),
  );
  if (!Array.isArray(value)) {
    throw new Error("Secret metadata catalog is invalid");
  }
  const prefixes = regional
    ? Object.freeze([
      `projects/${projectId}/locations/${location}/secrets/`,
      `projects/${projectNumber}/locations/${location}/secrets/`,
    ])
    : Object.freeze([
      `projects/${projectId}/secrets/`,
      `projects/${projectNumber}/secrets/`,
    ]);
  const resources = value.map((entry) => {
    const resource = typeof entry?.name === "string" ? entry.name : "";
    const prefix = prefixes.find((candidate) => resource.startsWith(candidate));
    if (!prefix || resource.length === prefix.length) {
      throw new Error("Secret metadata catalog contains an invalid resource");
    }
    const name = resource.slice(prefix.length);
    if (!SECRET_ID_PATTERN.test(name)) {
      throw new Error("Secret metadata catalog contains an invalid resource");
    }
    return Object.freeze({ location, name, resourceName: resource });
  });
  if (new Set(resources.map(({ resourceName }) => resourceName)).size !== resources.length) {
    throw new Error("Secret metadata catalog contains duplicate resources");
  }
  return resources;
}

function assertSecretInventoryUnchanged(expected, actual) {
  const expectedIdentity = JSON.stringify({
    locations: expected.locations.map(({ resourceName }) => resourceName),
    secrets: expected.secrets.map(({ resourceName }) => resourceName),
  });
  const actualIdentity = JSON.stringify({
    locations: actual.locations.map(({ resourceName }) => resourceName),
    secrets: actual.secrets.map(({ resourceName }) => resourceName),
  });
  if (actualIdentity !== expectedIdentity) {
    throw new Error("Secret location or metadata catalog drifted during bootstrap");
  }
}

function requiredSecretPolicy(run, projectId, secret) {
  const arguments_ = ["secrets", "get-iam-policy", secret.name, `--project=${projectId}`];
  if (secret.location !== null) arguments_.push(`--location=${secret.location}`);
  arguments_.push("--format=json");
  return requiredJson(
    run,
    arguments_,
    "Secret IAM policy",
    secretManagerExecution(secret.location),
  );
}

function globalSecretManagerExecution() {
  return GLOBAL_SECRET_MANAGER_EXECUTION;
}

function secretManagerExecution(location) {
  if (location === null) return globalSecretManagerExecution();
  if (!LOCATION_ID_PATTERN.test(location) || location === "global") {
    throw new Error("regional Secret location is invalid");
  }
  return Object.freeze({
    environment: Object.freeze({
      [SECRET_MANAGER_ENDPOINT_ENV]: `https://secretmanager.${location}.rep.googleapis.com/`,
    }),
  });
}

function requiredProjectNumber(run, projectId) {
  const value = requiredJson(
    run,
    ["projects", "describe", projectId, "--format=json(projectId,projectNumber)"],
    "project metadata",
  );
  const projectNumber = String(value?.projectNumber ?? "");
  if (value?.projectId !== projectId || !PROJECT_NUMBER_PATTERN.test(projectNumber)) {
    throw new Error("project metadata is invalid");
  }
  return projectNumber;
}

function assertParentlessProject(run, projectId) {
  const value = requiredJson(
    run,
    ["projects", "get-ancestors", projectId, "--format=json(id,type)"],
    "project ancestry",
  );
  if (
    !Array.isArray(value) ||
    value.length !== 1 ||
    value[0]?.type !== "project" ||
    value[0]?.id !== projectId
  ) {
    throw new Error("Cloud runtime IAM bootstrap requires an exact parentless project");
  }
}

function assertNoProjectPabBindings(run, projectId, projectNumber) {
  const value = requiredJson(
    run,
    [
      "iam",
      "policy-bindings",
      "search-target-policy-bindings",
      `--project=${projectId}`,
      "--location=global",
      `--target=//cloudresourcemanager.googleapis.com/projects/${projectNumber}`,
      "--filter=policyKind=PRINCIPAL_ACCESS_BOUNDARY",
      "--format=json",
    ],
    `project PAB binding search (${PROJECT_PAB_SEARCH_PERMISSION})`,
  );
  if (!Array.isArray(value) || value.length !== 0) {
    throw new Error("Cloud runtime IAM bootstrap requires zero project PAB bindings");
  }
}

function requiredEffectivePermission(
  run,
  projectId,
  principal,
  secret,
  permission,
) {
  const fullResourceName = `//secretmanager.googleapis.com/${secret.resourceName}`;
  const result = run([
    "policy-intelligence",
    "troubleshoot-policy",
    "iam",
    fullResourceName,
    `--principal-email=${principal}`,
    `--permission=${permission}`,
    `--project=${projectId}`,
    "--format=json",
  ]);
  if (!isSuccessful(result)) {
    throw new Error(
      "Policy Troubleshooter is unavailable or incomplete; enable its API and full policy visibility explicitly",
    );
  }
  const value = parseJson(result.stdout, "Policy Troubleshooter response");
  assertCompleteTroubleshooterResponse(
    value,
    principal,
    fullResourceName,
    permission,
  );
  if (value.overallAccessState === "CAN_ACCESS") {
    return "GRANTED";
  }
  if (value.overallAccessState === "CANNOT_ACCESS") {
    return "NOT_GRANTED";
  }
  throw new Error("Policy Troubleshooter returned an unknown effective access state");
}

function assertCompleteTroubleshooterResponse(
  value,
  principal,
  fullResourceName,
  permission,
) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Policy Troubleshooter response is invalid");
  }
  if (
    value.accessTuple?.principal !== principal ||
    value.accessTuple?.fullResourceName !== fullResourceName ||
    value.accessTuple?.permission !== permission
  ) {
    throw new Error("Policy Troubleshooter response identity is invalid");
  }
  if (
    value.allowPolicyExplanation === null ||
    typeof value.allowPolicyExplanation !== "object" ||
    Array.isArray(value.allowPolicyExplanation) ||
    value.denyPolicyExplanation === null ||
    typeof value.denyPolicyExplanation !== "object" ||
    Array.isArray(value.denyPolicyExplanation)
  ) {
    throw new Error("Policy Troubleshooter response is incomplete");
  }
  const overallAccessState = value.overallAccessState;
  if (![
    "CAN_ACCESS",
    "CANNOT_ACCESS",
  ].includes(overallAccessState)) {
    throw new Error("Policy Troubleshooter returned an unknown effective access state");
  }
  if (![
    "ALLOW_ACCESS_STATE_GRANTED",
    "ALLOW_ACCESS_STATE_NOT_GRANTED",
  ].includes(value.allowPolicyExplanation.allowAccessState)) {
    throw new Error("Policy Troubleshooter allow-policy explanation is incomplete");
  }
  if (![
    "DENY_ACCESS_STATE_DENIED",
    "DENY_ACCESS_STATE_NOT_DENIED",
  ].includes(value.denyPolicyExplanation.denyAccessState)) {
    throw new Error("Policy Troubleshooter deny-policy explanation is incomplete");
  }
  rejectUnknownTroubleshooterState(value);
}

function rejectUnknownTroubleshooterState(value) {
  if (typeof value === "string") {
    if (/(?:UNKNOWN|UNSPECIFIED)/u.test(value)) {
      throw new Error("Policy Troubleshooter response contains unknown policy state");
    }
    return;
  }
  if (Array.isArray(value)) {
    for (const entry of value) rejectUnknownTroubleshooterState(entry);
    return;
  }
  if (value === null || typeof value !== "object") return;
  for (const [key, entry] of Object.entries(value)) {
    if ((key === "errors" || key === "error") && entry !== null) {
      if (!Array.isArray(entry) || entry.length > 0) {
        throw new Error("Policy Troubleshooter response contains evaluation errors");
      }
    }
    rejectUnknownTroubleshooterState(entry);
  }
}

async function observeServiceAccountEventually(run, wait, projectId, email) {
  let observation = { missing: true, value: null };
  for (const delayMs of OBSERVATION_DELAYS_MS) {
    if (delayMs > 0) await wait(delayMs);
    observation = observeServiceAccount(run, projectId, email);
    if (!observation.missing) return observation;
  }
  return observation;
}

function requiredServiceAccount(run, projectId, email, label) {
  const observation = observeServiceAccount(run, projectId, email);
  if (observation.missing) throw new Error(`${label} is missing`);
  assertEnabledServiceAccount(observation.value, email, label);
  return observation.value;
}

function observeServiceAccount(run, projectId, email) {
  const result = run([
    "iam",
    "service-accounts",
    "describe",
    email,
    `--project=${projectId}`,
    "--format=json(email,disabled)",
  ]);
  if (isSuccessful(result)) {
    return { missing: false, value: parseJson(result.stdout, "service account metadata") };
  }
  if (/\bNOT_FOUND\b/.test(String(result?.stderr ?? ""))) {
    return { missing: true, value: null };
  }
  throw new Error("service account metadata lookup failed");
}

function assertEnabledServiceAccount(value, expectedEmail, label) {
  if (value?.email !== expectedEmail || value?.disabled === true) {
    throw new Error(`${label} is invalid or disabled`);
  }
}

function requireActAs(projectRoles, serviceAccountPolicy, callerMember, label) {
  if (
    !hasActAsProjectRole(projectRoles) &&
    !unconditionalRoles(serviceAccountPolicy, callerMember).includes(SERVICE_ACCOUNT_USER_ROLE)
  ) {
    throw new Error(`deployment caller cannot act as the ${label}`);
  }
}

function hasActAsProjectRole(roles) {
  return roles.includes(OWNER_ROLE) || roles.includes(SERVICE_ACCOUNT_USER_ROLE);
}

function requireAnyRole(actualRoles, allowedRoles, message) {
  if (!hasAnyRole(actualRoles, allowedRoles)) throw new Error(message);
}

function hasAnyRole(actualRoles, allowedRoles) {
  return actualRoles.some((role) => allowedRoles.has(role));
}

function unconditionalRoles(policy, member) {
  return policyRoles(policy, member, false);
}

function allRoles(policy, member) {
  return policyRoles(policy, member, true);
}

function policyRoles(policy, member, includeConditional) {
  if (policy === null || typeof policy !== "object" || Array.isArray(policy)) {
    throw new Error("IAM policy is invalid");
  }
  const roles = [];
  for (const binding of policy.bindings ?? []) {
    if (
      binding === null ||
      typeof binding !== "object" ||
      typeof binding.role !== "string" ||
      !Array.isArray(binding.members)
    ) {
      throw new Error("IAM policy binding is invalid");
    }
    if (
      binding.members.includes(member) &&
      (includeConditional || binding.condition === undefined || binding.condition === null)
    ) {
      roles.push(binding.role);
    }
  }
  return [...new Set(roles)].sort();
}

function exactRoleSet(actual, expected) {
  const actualSet = [...new Set(actual)].sort();
  const expectedSet = [...new Set(expected)].sort();
  return actualSet.length === expectedSet.length &&
    actualSet.every((role, index) => role === expectedSet[index]);
}

function hasExactUnconditionalRoles(policy, member, expected) {
  return exactRoleSet(allRoles(policy, member), expected) &&
    exactRoleSet(unconditionalRoles(policy, member), expected);
}

function canonicalProjectId(value) {
  const projectId = typeof value === "string" ? value.trim() : "";
  if (!PROJECT_PATTERN.test(projectId)) {
    throw new Error("project ID is invalid");
  }
  return projectId;
}

function canonicalCallerMember(value) {
  const raw = typeof value === "string" ? value.trim() : "";
  if (raw.startsWith("user:") || raw.startsWith("serviceAccount:")) {
    const [kind, account] = raw.split(":", 2);
    if (!ACCOUNT_PATTERN.test(account)) throw new Error("deployment caller is invalid");
    return `${kind}:${account}`;
  }
  if (!ACCOUNT_PATTERN.test(raw)) throw new Error("active gcloud account is invalid");
  return raw.endsWith(".gserviceaccount.com") ? `serviceAccount:${raw}` : `user:${raw}`;
}

function principalEmail(member) {
  const separator = member.indexOf(":");
  if (separator < 0 || separator === member.length - 1) {
    throw new Error("deployment caller is invalid");
  }
  return member.slice(separator + 1);
}

function requiredText(run, arguments_, label) {
  const result = run(arguments_);
  if (!isSuccessful(result)) throw new Error(`${label} lookup failed`);
  const value = String(result.stdout ?? "").trim();
  if (!value || value === "(unset)") throw new Error(`${label} is unavailable`);
  return value;
}

function requiredJson(run, arguments_, label, execution = undefined) {
  const result = run(arguments_, execution);
  if (!isSuccessful(result)) throw new Error(`${label} lookup failed`);
  return parseJson(result.stdout, label);
}

function parseJson(value, label) {
  try {
    return JSON.parse(String(value ?? ""));
  } catch {
    throw new Error(`${label} returned invalid JSON`);
  }
}

function isSuccessful(result) {
  return result?.status === 0 && !result?.error;
}

function runGcloud(arguments_, execution = undefined) {
  const invocation = gcloudProcessInvocation(arguments_);
  const result = spawnSync(invocation.command, invocation.arguments, {
    encoding: "utf8",
    env: gcloudProcessEnvironment(execution),
    maxBuffer: MAX_OUTPUT_BYTES,
    shell: false,
    stdio: ["ignore", "pipe", "pipe"],
  });
  return {
    status: result.status,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
    error: result.error ?? null,
  };
}

export function gcloudProcessEnvironment(execution, ambientEnvironment = process.env) {
  if (
    execution !== undefined &&
    (
      execution === null ||
      typeof execution !== "object" ||
      Array.isArray(execution) ||
      execution.environment === null ||
      typeof execution.environment !== "object" ||
      Array.isArray(execution.environment)
    )
  ) {
    throw new Error("gcloud execution environment is invalid");
  }
  const overrides = execution?.environment ?? {};
  const overrideKeys = Object.keys(overrides);
  if (
    overrideKeys.some((key) => key !== SECRET_MANAGER_ENDPOINT_ENV) ||
    overrideKeys.length > 1
  ) {
    throw new Error("gcloud execution environment contains an unsupported override");
  }
  const environment = { ...ambientEnvironment };
  for (const key of Object.keys(environment)) {
    if (key.toUpperCase() === SECRET_MANAGER_ENDPOINT_ENV) delete environment[key];
  }
  if (overrideKeys.length === 1) {
    const endpoint = overrides[SECRET_MANAGER_ENDPOINT_ENV];
    const regionalEndpoint = typeof endpoint === "string"
      ? /^https:\/\/secretmanager\.([a-z](?:[a-z0-9-]{0,61}[a-z0-9])?)\.rep\.googleapis\.com\/$/u.exec(endpoint)
      : null;
    if (
      endpoint !== GLOBAL_SECRET_MANAGER_ENDPOINT &&
      (!regionalEndpoint || regionalEndpoint[1] === "global")
    ) {
      throw new Error("Secret Manager endpoint override is invalid");
    }
    environment[SECRET_MANAGER_ENDPOINT_ENV] = endpoint;
  }
  return environment;
}

export function gcloudProcessInvocation(
  arguments_,
  platform = process.platform,
  environment = process.env,
) {
  if (!Array.isArray(arguments_) || arguments_.some((value) => typeof value !== "string")) {
    throw new Error("gcloud arguments are invalid");
  }
  if (platform === "win32") {
    return Object.freeze({
      command: environment.ComSpec || "cmd.exe",
      arguments: Object.freeze(["/d", "/s", "/c", "gcloud.cmd", ...arguments_]),
    });
  }
  return Object.freeze({
    command: "gcloud",
    arguments: Object.freeze([...arguments_]),
  });
}

function waitFor(delayMs) {
  return new Promise((resolvePromise) => setTimeout(resolvePromise, delayMs));
}

async function main() {
  const { values } = parseArgs({
    options: {
      project: { type: "string" },
    },
    strict: true,
  });
  try {
    const result = await prepareCloudRuntimeServiceAccount({ projectId: values.project });
    console.log(
      `cloud_runtime_service_account=ready created=${result.created} accessor_added=${result.accessorAdded}`,
    );
  } catch {
    console.error("cloud_runtime_service_account=failed");
    process.exitCode = 2;
  }
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  await main();
}
