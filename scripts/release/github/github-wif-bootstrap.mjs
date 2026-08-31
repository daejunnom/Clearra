import { spawnSync } from "node:child_process";
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { parseArgs } from "node:util";

export const GITHUB_WIF_BOOTSTRAP_CONTRACT = "clearra.github-wif-bootstrap.v1";
const REDACTED_FAILURE_DETAIL = "redacted failure detail";
const MAX_FAILURE_DETAIL_CHARACTERS = 512;

const PROJECT_ID = "clearra-cloud";
const PROJECT_NUMBER = "50060711800";
const GITHUB_REPOSITORY = "daejunnom/Clearra";
const GITHUB_REPOSITORY_ID = "1309293231";
const GITHUB_REPOSITORY_OWNER_ID = "271715321";
const GITHUB_REF = "refs/heads/main";
const GITHUB_IMMUTABLE_REPOSITORY =
  `daejunnom@${GITHUB_REPOSITORY_OWNER_ID}/Clearra@${GITHUB_REPOSITORY_ID}`;
const GITHUB_SUBJECT_PREFIX = `repo:${GITHUB_IMMUTABLE_REPOSITORY}`;
const GITHUB_MAIN_SUBJECT = `${GITHUB_SUBJECT_PREFIX}:ref:${GITHUB_REF}`;
const GITHUB_PATH_CONFIRMATION_SUBJECT =
  `${GITHUB_SUBJECT_PREFIX}:environment:discord-path-confirmation`;
const GITHUB_RUNTIME_ROLLBACK_SUBJECT =
  `${GITHUB_SUBJECT_PREFIX}:environment:discord-runtime-rollback`;
const GITHUB_COMMAND_SYNC_SUBJECT =
  `${GITHUB_SUBJECT_PREFIX}:environment:discord-global-command-sync`;
const LEGACY_GITHUB_SUBJECT_PREFIX = `repo:${GITHUB_REPOSITORY}`;
const LEGACY_GITHUB_MAIN_SUBJECT = `${LEGACY_GITHUB_SUBJECT_PREFIX}:ref:${GITHUB_REF}`;
const LEGACY_GITHUB_PATH_CONFIRMATION_SUBJECT =
  `${LEGACY_GITHUB_SUBJECT_PREFIX}:environment:discord-path-confirmation`;
const LEGACY_GITHUB_RUNTIME_ROLLBACK_SUBJECT =
  `${LEGACY_GITHUB_SUBJECT_PREFIX}:environment:discord-runtime-rollback`;
const LEGACY_GITHUB_COMMAND_SYNC_SUBJECT =
  `${LEGACY_GITHUB_SUBJECT_PREFIX}:environment:discord-global-command-sync`;
const GITHUB_WORKFLOW_REF =
  "daejunnom/Clearra/.github/workflows/discord-deploy.yml@refs/heads/main";
const GITHUB_ROLLBACK_WORKFLOW_REF =
  "daejunnom/Clearra/.github/workflows/discord-deploy-recovery.yml@refs/heads/main";
const REGION = "asia-northeast1";
const POOL_ID = "clearra-github";
const PROVIDER_ID = "clearra-main";
const POOL_NAME = `projects/${PROJECT_NUMBER}/locations/global/workloadIdentityPools/${POOL_ID}`;
const PROVIDER_NAME = `${POOL_NAME}/providers/${PROVIDER_ID}`;
const ROLLBACK_POOL_ID = "clearra-github-rollback";
const ROLLBACK_PROVIDER_ID = "clearra-runtime-rollback";
const ROLLBACK_POOL_NAME =
  `projects/${PROJECT_NUMBER}/locations/global/workloadIdentityPools/${ROLLBACK_POOL_ID}`;
const ROLLBACK_PROVIDER_NAME = `${ROLLBACK_POOL_NAME}/providers/${ROLLBACK_PROVIDER_ID}`;
const BUILDER_ID = "clearra-github-builder";
const BUILDER_EMAIL = `${BUILDER_ID}@${PROJECT_ID}.iam.gserviceaccount.com`;
const DEPLOYER_ID = "clearra-github-deployer";
const DEPLOYER_EMAIL = `${DEPLOYER_ID}@${PROJECT_ID}.iam.gserviceaccount.com`;
const ROLLBACK_ID = "clearra-github-rollback";
const ROLLBACK_EMAIL = `${ROLLBACK_ID}@${PROJECT_ID}.iam.gserviceaccount.com`;
const COMMAND_SYNC_EMAIL = `clearra-command-sync@${PROJECT_ID}.iam.gserviceaccount.com`;
const BUILD_EMAIL = `clearra-build@${PROJECT_ID}.iam.gserviceaccount.com`;
const RUNTIME_EMAIL = `clearra-current-job@${PROJECT_ID}.iam.gserviceaccount.com`;
const INTERACTION_EMAIL = `clearra-interaction@${PROJECT_ID}.iam.gserviceaccount.com`;
const TELEMETRY_EMAIL = `clearra-telemetry-relay@${PROJECT_ID}.iam.gserviceaccount.com`;
const JOB_RUNNER_EMAIL = `clearra-job-runner@${PROJECT_ID}.iam.gserviceaccount.com`;
const DEFAULT_COMPUTE_EMAIL = `${PROJECT_NUMBER}-compute@developer.gserviceaccount.com`;
const PRIMARY_HUMAN_MEMBER = "user:daejun0311@gmail.com";
const LEGACY_BUILD_USER_MEMBER = "user:stemxstudioproject@gmail.com";
const BUILD_MEMBER = `serviceAccount:${BUILD_EMAIL}`;
const RUNTIME_MEMBER = `serviceAccount:${RUNTIME_EMAIL}`;
const INTERACTION_MEMBER = `serviceAccount:${INTERACTION_EMAIL}`;
const TELEMETRY_MEMBER = `serviceAccount:${TELEMETRY_EMAIL}`;
const BUILDER_MEMBER = `serviceAccount:${BUILDER_EMAIL}`;
const DEPLOYER_MEMBER = `serviceAccount:${DEPLOYER_EMAIL}`;
const ROLLBACK_MEMBER = `serviceAccount:${ROLLBACK_EMAIL}`;
const COMMAND_SYNC_MEMBER = `serviceAccount:${COMMAND_SYNC_EMAIL}`;
const githubSubjectMember = (poolName, subject) =>
  `principal://iam.googleapis.com/${poolName}/subject/${subject}`;
const BUILDER_WIF_MEMBERS = Object.freeze([
  githubSubjectMember(POOL_NAME, GITHUB_MAIN_SUBJECT),
]);
const DEPLOYER_WIF_MEMBERS = Object.freeze([
  githubSubjectMember(POOL_NAME, GITHUB_PATH_CONFIRMATION_SUBJECT),
]);
const ROLLBACK_WIF_MEMBERS = Object.freeze([
  githubSubjectMember(ROLLBACK_POOL_NAME, GITHUB_RUNTIME_ROLLBACK_SUBJECT),
]);
const COMMAND_SYNC_WIF_MEMBERS = Object.freeze([
  githubSubjectMember(POOL_NAME, GITHUB_COMMAND_SYNC_SUBJECT),
  githubSubjectMember(ROLLBACK_POOL_NAME, GITHUB_RUNTIME_ROLLBACK_SUBJECT),
]);
const BUILDER_REMOVABLE_LEGACY_WIF_MEMBERS = Object.freeze([
  githubSubjectMember(POOL_NAME, LEGACY_GITHUB_MAIN_SUBJECT),
]);
const DEPLOYER_REMOVABLE_LEGACY_WIF_MEMBERS = Object.freeze([
  githubSubjectMember(POOL_NAME, LEGACY_GITHUB_PATH_CONFIRMATION_SUBJECT),
]);
const ROLLBACK_REMOVABLE_LEGACY_WIF_MEMBERS = Object.freeze([
  githubSubjectMember(ROLLBACK_POOL_NAME, LEGACY_GITHUB_RUNTIME_ROLLBACK_SUBJECT),
]);
const COMMAND_SYNC_REMOVABLE_LEGACY_WIF_MEMBERS = Object.freeze([
  githubSubjectMember(POOL_NAME, LEGACY_GITHUB_COMMAND_SYNC_SUBJECT),
  githubSubjectMember(ROLLBACK_POOL_NAME, LEGACY_GITHUB_RUNTIME_ROLLBACK_SUBJECT),
]);
const ALL_WIF_MEMBERS = Object.freeze([
  ...new Set([
    ...BUILDER_WIF_MEMBERS,
    ...DEPLOYER_WIF_MEMBERS,
    ...ROLLBACK_WIF_MEMBERS,
    ...COMMAND_SYNC_WIF_MEMBERS,
  ]),
]);
const ARTIFACT_REPOSITORY = "clearra";
const SOURCE_BUCKET = `gs://${PROJECT_ID}_cloudbuild`;
const DISCORD_SECRET = "discord-bot-token";
const JOB_SECRET = "clearra-job-token";
const TELEMETRY_EVENT_SECRET = "clearra-telemetry-event-key";
const TELEMETRY_TRANSPORT_SECRET = "clearra-telemetry-transport-key";
const SECRET_ACCESSOR_ROLE = "roles/secretmanager.secretAccessor";
const SECRET_MANAGER_ENDPOINT_ENV = "CLOUDSDK_API_ENDPOINT_OVERRIDES_SECRETMANAGER";
const GLOBAL_SECRET_MANAGER_ENDPOINT = "https://secretmanager.googleapis.com/";
const MAX_OUTPUT_BYTES = 4 * 1024 * 1024;
const MAX_RECONCILE_MILLISECONDS = 5 * 60 * 1000;
const RECONCILE_BACKOFF_MILLISECONDS = Object.freeze([
  1_000,
  2_000,
  4_000,
  8_000,
  16_000,
  30_000,
]);

const POOL_DISPLAY_NAME = "Clearra GitHub Actions";
const POOL_DESCRIPTION = "Exact GitHub OIDC pool for daejunnom/Clearra";
const PROVIDER_DISPLAY_NAME = "Clearra main GitHub OIDC";
const PROVIDER_DESCRIPTION = "Exact workflow/main provider for daejunnom/Clearra";
const ROLLBACK_POOL_DISPLAY_NAME = "Clearra GitHub rollback";
const ROLLBACK_POOL_DESCRIPTION = "Exact recovery-workflow OIDC pool for daejunnom/Clearra";
const ROLLBACK_PROVIDER_DISPLAY_NAME = "Clearra rollback GitHub OIDC";
const ROLLBACK_PROVIDER_DESCRIPTION =
  "Exact recovery-workflow/main provider for Clearra runtime rollback";
const BUILDER_DISPLAY_NAME = "Clearra GitHub builder";
const BUILDER_DESCRIPTION = "Keyless least-privilege exact-source image build identity";
const DEPLOYER_DISPLAY_NAME = "Clearra GitHub deployer";
const DEPLOYER_DESCRIPTION = "Keyless least-privilege protected Cloud Run deployment identity";
const ROLLBACK_DISPLAY_NAME = "Clearra GitHub rollback";
const ROLLBACK_DESCRIPTION = "Keyless protected existing-service traffic recovery identity";
const DEPLOYER_RUN_ROLE_ID = "clearraGithubRuntimeDeployer";
const DEPLOYER_RUN_ROLE_NAME = `projects/${PROJECT_ID}/roles/${DEPLOYER_RUN_ROLE_ID}`;
const DEPLOYER_RUN_ROLE_TITLE = "Clearra GitHub runtime deployer";
const DEPLOYER_RUN_ROLE_DESCRIPTION =
  "Exact Cloud Run service update, traffic rollback, and ephemeral smoke-job authority";
const DEPLOYER_RUN_ROLE_PERMISSIONS = Object.freeze([
  "run.executions.get",
  "run.jobs.create",
  "run.jobs.delete",
  "run.jobs.get",
  "run.jobs.run",
  "run.jobs.update",
  "run.operations.get",
  "run.revisions.get",
  "run.services.get",
  "run.services.update",
]);
const ROLLBACK_RUN_ROLE_ID = "clearraGithubRuntimeRollback";
const ROLLBACK_RUN_ROLE_NAME = `projects/${PROJECT_ID}/roles/${ROLLBACK_RUN_ROLE_ID}`;
const ROLLBACK_RUN_ROLE_TITLE = "Clearra GitHub runtime rollback";
const ROLLBACK_RUN_ROLE_DESCRIPTION =
  "Exact existing-service Cloud Run traffic rollback and readback authority";
const ROLLBACK_RUN_ROLE_PERMISSIONS = Object.freeze([
  "run.operations.get",
  "run.revisions.get",
  "run.revisions.list",
  "run.services.get",
  "run.services.update",
]);
const OFFICIAL_PERMISSION_REFERENCES = Object.freeze({
  artifactRegistryCloudRun:
    "https://cloud.google.com/artifact-registry/docs/integrate-cloud-run#permissions_required_to_deploy",
  cloudRunDeployment:
    "https://cloud.google.com/run/docs/reference/iam/roles#deployment-permissions",
  cloudRunPermissions:
    "https://cloud.google.com/run/docs/reference/iam/permissions",
  cloudRunRevisionDelete:
    "https://cloud.google.com/run/docs/reference/rest/v2/projects.locations.services.revisions/delete",
  customRoles:
    "https://cloud.google.com/iam/docs/creating-custom-roles#gcloud",
});
const OIDC_ISSUER = "https://token.actions.githubusercontent.com";
const ATTRIBUTE_CONDITION =
  "assertion.repository == 'daejunnom/Clearra' && assertion.repository_id == '1309293231' && assertion.repository_owner_id == '271715321' && assertion.ref == 'refs/heads/main' && assertion.workflow_ref == 'daejunnom/Clearra/.github/workflows/discord-deploy.yml@refs/heads/main'";
const ROLLBACK_ATTRIBUTE_CONDITION =
  "assertion.repository == 'daejunnom/Clearra' && assertion.repository_id == '1309293231' && assertion.repository_owner_id == '271715321' && assertion.ref == 'refs/heads/main' && assertion.workflow_ref == 'daejunnom/Clearra/.github/workflows/discord-deploy-recovery.yml@refs/heads/main'";
const ATTRIBUTE_MAPPING = Object.freeze({
  "attribute.ref": "assertion.ref",
  "attribute.repository": "assertion.repository",
  "attribute.repository_id": "assertion.repository_id",
  "attribute.repository_owner_id": "assertion.repository_owner_id",
  "attribute.workflow_ref": "assertion.workflow_ref",
  "google.subject": "assertion.sub",
});

const REQUIRED_EXISTING_SERVICES = Object.freeze([
  "artifactregistry.googleapis.com",
  "cloudbuild.googleapis.com",
  "iam.googleapis.com",
  "logging.googleapis.com",
  "policytroubleshooter.googleapis.com",
  "run.googleapis.com",
  "secretmanager.googleapis.com",
  "serviceusage.googleapis.com",
]);
const MANAGED_WIF_SERVICES = Object.freeze([
  "iamcredentials.googleapis.com",
  "sts.googleapis.com",
]);

const BUILDER_PROJECT_ROLES = Object.freeze([
  "roles/cloudbuild.builds.editor",
  "roles/logging.viewer",
  "roles/serviceusage.serviceUsageConsumer",
]);
const DEPLOYER_PROJECT_ROLES = Object.freeze([
  DEPLOYER_RUN_ROLE_NAME,
  "roles/logging.viewer",
  "roles/serviceusage.serviceUsageConsumer",
]);
const ROLLBACK_PROJECT_ROLES = Object.freeze([
  ROLLBACK_RUN_ROLE_NAME,
  "roles/serviceusage.serviceUsageConsumer",
]);
const DEPLOYER_REMOVABLE_LEGACY_PROJECT_ROLES = Object.freeze([
  "roles/run.admin",
]);
const COMMAND_SYNC_PROJECT_ROLES = Object.freeze([
  "roles/run.viewer",
]);
const COMMAND_SYNC_REMOVABLE_LEGACY_PROJECT_ROLES = Object.freeze([
  "roles/logging.logWriter",
]);
const BUILD_PROJECT_ROLES = Object.freeze([
  "roles/logging.logWriter",
]);
const BUILD_REMOVABLE_LEGACY_PROJECT_ROLES = Object.freeze([
  "roles/storage.objectViewer",
]);
const BUILDER_SOURCE_BUCKET_ROLES = Object.freeze([
  "roles/storage.bucketViewer",
  "roles/storage.objectCreator",
  "roles/storage.objectViewer",
]);
const BUILD_SOURCE_BUCKET_ROLES = Object.freeze([
  "roles/storage.objectViewer",
]);
const COMMAND_SYNC_REMOVABLE_LEGACY_BUCKET_ROLES = Object.freeze([
  "roles/storage.objectViewer",
]);
const BUILD_REQUIRED_POLICY_TUPLES = Object.freeze([
  Object.freeze([PRIMARY_HUMAN_MEMBER, "roles/iam.serviceAccountAdmin"]),
  Object.freeze([PRIMARY_HUMAN_MEMBER, "roles/owner"]),
  Object.freeze([LEGACY_BUILD_USER_MEMBER, "roles/iam.serviceAccountUser"]),
]);
const COMMAND_SYNC_REQUIRED_POLICY_TUPLES = Object.freeze([
  Object.freeze([PRIMARY_HUMAN_MEMBER, "roles/iam.serviceAccountAdmin"]),
  Object.freeze([PRIMARY_HUMAN_MEMBER, "roles/owner"]),
]);
const TRUSTED_HUMAN_SERVICE_ACCOUNT_POLICY_TUPLES = Object.freeze({
  [BUILD_EMAIL]: BUILD_REQUIRED_POLICY_TUPLES,
  [COMMAND_SYNC_EMAIL]: COMMAND_SYNC_REQUIRED_POLICY_TUPLES,
  [DEFAULT_COMPUTE_EMAIL]: COMMAND_SYNC_REQUIRED_POLICY_TUPLES,
  [INTERACTION_EMAIL]: BUILD_REQUIRED_POLICY_TUPLES,
  [JOB_RUNNER_EMAIL]: COMMAND_SYNC_REQUIRED_POLICY_TUPLES,
  [RUNTIME_EMAIL]: Object.freeze([]),
  [TELEMETRY_EMAIL]: BUILD_REQUIRED_POLICY_TUPLES,
});
const REQUIRED_EXISTING_SERVICE_ACCOUNTS = Object.freeze(
  Object.keys(TRUSTED_HUMAN_SERVICE_ACCOUNT_POLICY_TUPLES).sort(),
);
const REPOSITORY_VARIABLES = Object.freeze({
  GCP_BUILD_SERVICE_ACCOUNT: BUILDER_EMAIL,
  GCP_COMMAND_SYNC_SERVICE_ACCOUNT: COMMAND_SYNC_EMAIL,
  GCP_DEPLOY_SERVICE_ACCOUNT: DEPLOYER_EMAIL,
  GCP_PROJECT_ID: PROJECT_ID,
  GCP_PROJECT_NUMBER: PROJECT_NUMBER,
  GCP_REGION: REGION,
  GCP_ROLLBACK_WORKLOAD_IDENTITY_PROVIDER: ROLLBACK_PROVIDER_NAME,
  GCP_ROLLBACK_SERVICE_ACCOUNT: ROLLBACK_EMAIL,
  GCP_WORKLOAD_IDENTITY_PROVIDER: PROVIDER_NAME,
});

export async function createGitHubWifBootstrapPlan(options = {}, dependencies = {}) {
  return createGitHubWifBootstrapPlanInternal(options, dependencies, true);
}

export function githubWifBootstrapFailureDiagnostic(error) {
  const detail = error instanceof Error ? error.message.trim() : "";
  const sensitive =
    detail.length === 0 ||
    detail.length > MAX_FAILURE_DETAIL_CHARACTERS ||
    /[\u0000-\u001f\u007f]/u.test(detail) ||
    /-----BEGIN [A-Z ]+PRIVATE KEY-----/u.test(detail) ||
    /(?:password|private[_ -]?key|access[_ -]?token|bearer)\s*[:=]\s*\S+/iu.test(detail) ||
    /(?:AIza|ya29\.|gh[pousr]_)[A-Za-z0-9._-]+/u.test(detail) ||
    /\b[A-Za-z0-9+/]{80,}={0,2}\b/u.test(detail);
  return Object.freeze({
    contract: GITHUB_WIF_BOOTSTRAP_CONTRACT,
    status: "failed",
    detail: sensitive ? REDACTED_FAILURE_DETAIL : detail,
  });
}

async function createGitHubWifBootstrapPlanInternal(
  options,
  dependencies,
  includeSecretBoundaryAudit,
) {
  assertClosedOptions(options);
  const run = dependencies.runGcloud ?? runGcloud;
  const plannedMutations = [];
  const observations = {};

  assertNoPersistentApiEndpointOverrides(run);
  assertExactProject(run);
  const enabledServices = requiredEnabledServices(run);
  for (const service of REQUIRED_EXISTING_SERVICES) {
    if (!enabledServices.has(service)) {
      throw new Error(`required deployment API is not enabled: ${service}`);
    }
  }
  const missingWifServices = MANAGED_WIF_SERVICES.filter((service) => !enabledServices.has(service));
  if (missingWifServices.length > 0) {
    plannedMutations.push(mutation(
      "enable-wif-services",
      "enable the keyless federation services",
      [
        "services", "enable", ...missingWifServices,
        `--project=${PROJECT_ID}`,
        "--quiet",
      ],
    ));
  }
  observations.services = missingWifServices.length === 0 ? "ready" : "wif-services-missing";

  const poolCatalog = requiredJson(
    run,
    [
      "iam", "workload-identity-pools", "list",
      `--project=${PROJECT_ID}`,
      "--location=global",
      "--show-deleted",
      "--format=json",
    ],
    "Workload Identity Pool catalog",
  );
  if (!Array.isArray(poolCatalog)) throw new Error("Workload Identity Pool catalog is invalid");
  const matchingPools = poolCatalog.filter((entry) => entry?.name === POOL_NAME);
  if (matchingPools.length > 1) throw new Error("Workload Identity Pool catalog is ambiguous");
  const pool = matchingPools[0];
  if (pool) {
    assertExactPool(pool, {
      name: POOL_NAME,
      displayName: POOL_DISPLAY_NAME,
      description: POOL_DESCRIPTION,
      label: "primary Workload Identity Pool",
    });
    observations.pool = "ready";
  } else {
    observations.pool = "missing";
    plannedMutations.push(mutation(
      "create-pool",
      "create the dedicated global GitHub pool",
      [
        "iam", "workload-identity-pools", "create", POOL_ID,
        `--project=${PROJECT_ID}`,
        "--location=global",
        `--display-name=${POOL_DISPLAY_NAME}`,
        `--description=${POOL_DESCRIPTION}`,
        "--quiet",
      ],
    ));
  }

  const matchingRollbackPools = poolCatalog.filter((entry) => entry?.name === ROLLBACK_POOL_NAME);
  if (matchingRollbackPools.length > 1) {
    throw new Error("rollback Workload Identity Pool catalog is ambiguous");
  }
  const rollbackPool = matchingRollbackPools[0];
  if (rollbackPool) {
    assertExactPool(rollbackPool, {
      name: ROLLBACK_POOL_NAME,
      displayName: ROLLBACK_POOL_DISPLAY_NAME,
      description: ROLLBACK_POOL_DESCRIPTION,
      label: "rollback Workload Identity Pool",
    });
    observations.rollbackPool = "ready";
  } else {
    observations.rollbackPool = "missing";
    plannedMutations.push(createRollbackPoolMutation());
  }

  if (pool) {
    const providerCatalog = requiredJson(
      run,
      [
        "iam", "workload-identity-pools", "providers", "list",
        `--project=${PROJECT_ID}`,
        "--location=global",
        `--workload-identity-pool=${POOL_ID}`,
        "--show-deleted",
        "--format=json",
      ],
      "Workload Identity Provider catalog",
    );
    if (!Array.isArray(providerCatalog)) {
      throw new Error("Workload Identity Provider catalog is invalid");
    }
    const unexpectedProviders = providerCatalog.filter((entry) => entry?.name !== PROVIDER_NAME);
    if (unexpectedProviders.length > 0) {
      throw new Error("dedicated Workload Identity Pool contains an unexpected provider");
    }
    const providers = providerCatalog.filter((entry) => entry?.name === PROVIDER_NAME);
    if (providers.length > 1) throw new Error("Workload Identity Provider catalog is ambiguous");
    if (providers.length === 1) {
      assertExactProvider(providers[0], {
        name: PROVIDER_NAME,
        displayName: PROVIDER_DISPLAY_NAME,
        description: PROVIDER_DESCRIPTION,
        attributeCondition: ATTRIBUTE_CONDITION,
        label: "primary Workload Identity Provider",
      });
      observations.provider = "ready";
    } else {
      observations.provider = "missing";
      plannedMutations.push(createProviderMutation());
    }
  } else {
    observations.provider = "missing-with-pool";
    plannedMutations.push(createProviderMutation());
  }

  if (rollbackPool) {
    const rollbackProviderCatalog = requiredJson(
      run,
      [
        "iam", "workload-identity-pools", "providers", "list",
        `--project=${PROJECT_ID}`,
        "--location=global",
        `--workload-identity-pool=${ROLLBACK_POOL_ID}`,
        "--show-deleted",
        "--format=json",
      ],
      "rollback Workload Identity Provider catalog",
    );
    if (!Array.isArray(rollbackProviderCatalog)) {
      throw new Error("rollback Workload Identity Provider catalog is invalid");
    }
    const unexpectedRollbackProviders = rollbackProviderCatalog.filter(
      (entry) => entry?.name !== ROLLBACK_PROVIDER_NAME,
    );
    if (unexpectedRollbackProviders.length > 0) {
      throw new Error("dedicated rollback Workload Identity Pool contains an unexpected provider");
    }
    const rollbackProviders = rollbackProviderCatalog.filter(
      (entry) => entry?.name === ROLLBACK_PROVIDER_NAME,
    );
    if (rollbackProviders.length > 1) {
      throw new Error("rollback Workload Identity Provider catalog is ambiguous");
    }
    if (rollbackProviders.length === 1) {
      assertExactProvider(rollbackProviders[0], {
        name: ROLLBACK_PROVIDER_NAME,
        displayName: ROLLBACK_PROVIDER_DISPLAY_NAME,
        description: ROLLBACK_PROVIDER_DESCRIPTION,
        attributeCondition: ROLLBACK_ATTRIBUTE_CONDITION,
        label: "rollback Workload Identity Provider",
      });
      observations.rollbackProvider = "ready";
    } else {
      observations.rollbackProvider = "missing";
      plannedMutations.push(createRollbackProviderMutation());
    }
  } else {
    observations.rollbackProvider = "missing-with-pool";
    plannedMutations.push(createRollbackProviderMutation());
  }

  const serviceAccounts = requiredJson(
    run,
    ["iam", "service-accounts", "list", `--project=${PROJECT_ID}`, "--format=json"],
    "service-account catalog",
  );
  const serviceAccountCatalog = assertExactServiceAccountCatalog(serviceAccounts);
  for (const email of REQUIRED_EXISTING_SERVICE_ACCOUNTS) {
    assertRequiredServiceAccount(serviceAccountCatalog, email);
  }
  assertNoUserManagedKeys(run, BUILD_EMAIL, "build service account");
  assertNoUserManagedKeys(run, RUNTIME_EMAIL, "runtime service account");
  const builder = serviceAccountCatalog.get(BUILDER_EMAIL);
  if (builder) {
    assertExactBuilder(builder);
    assertNoUserManagedKeys(run, BUILDER_EMAIL, "builder service account");
    observations.builder = "ready";
  } else {
    observations.builder = "missing";
    plannedMutations.push(mutation(
      "create-builder",
      "create the keyless GitHub exact-source builder service account",
      [
        "iam", "service-accounts", "create", BUILDER_ID,
        `--project=${PROJECT_ID}`,
        `--display-name=${BUILDER_DISPLAY_NAME}`,
        `--description=${BUILDER_DESCRIPTION}`,
        "--quiet",
      ],
    ));
  }
  const deployer = serviceAccountCatalog.get(DEPLOYER_EMAIL);
  if (deployer) {
    assertExactDeployer(deployer);
    assertNoUserManagedKeys(run, DEPLOYER_EMAIL, "deployer service account");
    observations.deployer = "ready";
  } else {
    observations.deployer = "missing";
    plannedMutations.push(mutation(
      "create-deployer",
      "create the keyless GitHub deployer service account",
      [
        "iam", "service-accounts", "create", DEPLOYER_ID,
        `--project=${PROJECT_ID}`,
        `--display-name=${DEPLOYER_DISPLAY_NAME}`,
        `--description=${DEPLOYER_DESCRIPTION}`,
        "--quiet",
      ],
    ));
  }
  const rollback = serviceAccountCatalog.get(ROLLBACK_EMAIL);
  if (rollback) {
    assertExactRollback(rollback);
    assertNoUserManagedKeys(run, ROLLBACK_EMAIL, "rollback service account");
    observations.rollback = "ready";
  } else {
    observations.rollback = "missing";
    plannedMutations.push(mutation(
      "create-rollback",
      "create the keyless protected existing-service rollback account",
      [
        "iam", "service-accounts", "create", ROLLBACK_ID,
        `--project=${PROJECT_ID}`,
        `--display-name=${ROLLBACK_DISPLAY_NAME}`,
        `--description=${ROLLBACK_DESCRIPTION}`,
        "--quiet",
      ],
    ));
  }
  assertNoUserManagedKeys(run, COMMAND_SYNC_EMAIL, "command-sync service account");

  const serviceAccountPolicies = new Map();
  for (const email of [...serviceAccountCatalog.keys()].sort()) {
    serviceAccountPolicies.set(email, requiredServiceAccountPolicy(run, email));
  }
  assertCatalogWideImpersonationBoundary(serviceAccountPolicies);

  const projectRoles = requiredJson(
    run,
    [
      "iam", "roles", "list",
      `--project=${PROJECT_ID}`,
      "--show-deleted",
      "--format=json",
    ],
    "project custom-role catalog",
  );
  if (!Array.isArray(projectRoles)) throw new Error("project custom-role catalog is invalid");
  const matchingDeployerRoles = projectRoles.filter((entry) => entry?.name === DEPLOYER_RUN_ROLE_NAME);
  if (matchingDeployerRoles.length > 1) throw new Error("deployer custom role is ambiguous");
  if (matchingDeployerRoles.length === 1) {
    if (matchingDeployerRoles[0]?.deleted === true) {
      throw new Error("deployer custom role is deleted and cannot be reused");
    }
    const deployerRunRole = requiredJson(
      run,
      [
        "iam", "roles", "describe", DEPLOYER_RUN_ROLE_ID,
        `--project=${PROJECT_ID}`,
        "--format=json",
      ],
      "deployer custom role",
    );
    assertExactDeployerRunRole(deployerRunRole);
    observations.deployerRunRole = "ready";
  } else {
    observations.deployerRunRole = "missing";
    plannedMutations.push(createDeployerRunRoleMutation());
  }
  const matchingRollbackRoles = projectRoles.filter((entry) => entry?.name === ROLLBACK_RUN_ROLE_NAME);
  if (matchingRollbackRoles.length > 1) throw new Error("rollback custom role is ambiguous");
  if (matchingRollbackRoles.length === 1) {
    if (matchingRollbackRoles[0]?.deleted === true) {
      throw new Error("rollback custom role is deleted and cannot be reused");
    }
    const rollbackRunRole = requiredJson(
      run,
      [
        "iam", "roles", "describe", ROLLBACK_RUN_ROLE_ID,
        `--project=${PROJECT_ID}`,
        "--format=json",
      ],
      "rollback custom role",
    );
    if (isExactRollbackRunRole(rollbackRunRole)) {
      observations.rollbackRunRole = "ready";
    } else if (isLegacyDeleteRollbackRunRole(rollbackRunRole)) {
      observations.rollbackRunRole = "legacy-revision-delete-permission";
      plannedMutations.push(updateRollbackRunRoleMutation());
    } else {
      assertExactRollbackRunRole(rollbackRunRole);
    }
  } else {
    observations.rollbackRunRole = "missing";
    plannedMutations.push(createRollbackRunRoleMutation());
  }

  const projectPolicy = requiredJson(
    run,
    ["projects", "get-iam-policy", PROJECT_ID, "--format=json"],
    "project IAM policy",
  );
  assertNoFederatedMembers(projectPolicy, "project IAM policy");
  reconcileProjectRoles({
    policy: projectPolicy,
    member: BUILD_MEMBER,
    desired: BUILD_PROJECT_ROLES,
    removableLegacy: BUILD_REMOVABLE_LEGACY_PROJECT_ROLES,
    idPrefix: "build-project",
    label: "Cloud Build execution project authority",
    plannedMutations,
  });
  reconcileProjectRoles({
    policy: projectPolicy,
    member: ROLLBACK_MEMBER,
    desired: ROLLBACK_PROJECT_ROLES,
    removableLegacy: [],
    idPrefix: "rollback-project",
    label: "rollback project authority",
    plannedMutations,
  });
  reconcileProjectRoles({
    policy: projectPolicy,
    member: BUILDER_MEMBER,
    desired: BUILDER_PROJECT_ROLES,
    removableLegacy: [],
    idPrefix: "builder-project",
    label: "builder project authority",
    plannedMutations,
  });
  reconcileProjectRoles({
    policy: projectPolicy,
    member: DEPLOYER_MEMBER,
    desired: DEPLOYER_PROJECT_ROLES,
    removableLegacy: DEPLOYER_REMOVABLE_LEGACY_PROJECT_ROLES,
    idPrefix: "deployer-project",
    label: "deployer project authority",
    plannedMutations,
  });
  reconcileProjectRoles({
    policy: projectPolicy,
    member: COMMAND_SYNC_MEMBER,
    desired: COMMAND_SYNC_PROJECT_ROLES,
    removableLegacy: COMMAND_SYNC_REMOVABLE_LEGACY_PROJECT_ROLES,
    idPrefix: "command-sync-project",
    label: "command-sync project authority",
    plannedMutations,
  });

  const buildPolicy = serviceAccountPolicies.get(BUILD_EMAIL);
  assertClosedServiceAccountPolicy({
    policy: buildPolicy,
    requiredTuples: BUILD_REQUIRED_POLICY_TUPLES,
    managedTuples: [[BUILDER_MEMBER, "roles/iam.serviceAccountUser"]],
    label: "build service-account IAM policy",
  });
  reconcileResourceRoles({
    policy: buildPolicy,
    member: BUILDER_MEMBER,
    desired: ["roles/iam.serviceAccountUser"],
    resourceId: BUILD_EMAIL,
    idPrefix: "build-act-as",
    label: "build service-account authority",
    plannedMutations,
  });
  assertNoMemberRoles(
    buildPolicy,
    DEPLOYER_MEMBER,
    "deployer must not act as the Cloud Build execution account",
  );
  assertNoMemberRoles(
    buildPolicy,
    COMMAND_SYNC_MEMBER,
    "command sync must not act as the Cloud Build execution account",
  );
  const runtimePolicy = serviceAccountPolicies.get(RUNTIME_EMAIL);
  assertClosedServiceAccountPolicy({
    policy: runtimePolicy,
    requiredTuples: [],
    managedTuples: [[DEPLOYER_MEMBER, "roles/iam.serviceAccountUser"]],
    label: "runtime service-account IAM policy",
  });
  reconcileResourceRoles({
    policy: runtimePolicy,
    member: DEPLOYER_MEMBER,
    desired: ["roles/iam.serviceAccountUser"],
    resourceId: RUNTIME_EMAIL,
    idPrefix: "runtime-act-as",
    label: "runtime service-account authority",
    plannedMutations,
  });
  assertNoMemberRoles(
    runtimePolicy,
    BUILDER_MEMBER,
    "builder must not act as the Cloud Run runtime account",
  );
  assertNoMemberRoles(
    runtimePolicy,
    COMMAND_SYNC_MEMBER,
    "command sync must not act as the Cloud Run runtime account",
  );
  const commandSyncPolicy = serviceAccountPolicies.get(COMMAND_SYNC_EMAIL);
  assertClosedServiceAccountPolicy({
    policy: commandSyncPolicy,
    requiredTuples: COMMAND_SYNC_REQUIRED_POLICY_TUPLES,
    managedTuples: COMMAND_SYNC_WIF_MEMBERS.map((member) => [
      member,
      "roles/iam.workloadIdentityUser",
    ]).concat(COMMAND_SYNC_REMOVABLE_LEGACY_WIF_MEMBERS.map((member) => [
      member,
      "roles/iam.workloadIdentityUser",
    ])),
    label: "command-sync service-account IAM policy",
  });
  assertNoMemberRoles(
    commandSyncPolicy,
    BUILDER_MEMBER,
    "builder must not impersonate command sync",
  );
  assertNoMemberRoles(
    commandSyncPolicy,
    DEPLOYER_MEMBER,
    "deployer must not impersonate command sync",
  );
  if (builder) {
    const builderPolicy = serviceAccountPolicies.get(BUILDER_EMAIL);
    assertClosedServiceAccountPolicy({
      policy: builderPolicy,
      requiredTuples: [],
      managedTuples: BUILDER_WIF_MEMBERS.map((member) => [
        member,
        "roles/iam.workloadIdentityUser",
      ]).concat(BUILDER_REMOVABLE_LEGACY_WIF_MEMBERS.map((member) => [
        member,
        "roles/iam.workloadIdentityUser",
      ])),
      label: "GitHub builder service-account IAM policy",
    });
    reconcileWifMembers({
      policy: builderPolicy,
      desiredMembers: BUILDER_WIF_MEMBERS,
      removableLegacyMembers: BUILDER_REMOVABLE_LEGACY_WIF_MEMBERS,
      email: BUILDER_EMAIL,
      idPrefix: "github-builder-wif",
      label: "GitHub builder Workload Identity authority",
      plannedMutations,
    });
  } else {
    for (const [index, member] of BUILDER_WIF_MEMBERS.entries()) {
      plannedMutations.push(serviceAccountBindingMutation({
        id: `github-builder-wif-add-${index + 1}`,
        action: "add",
        email: BUILDER_EMAIL,
        member,
        role: "roles/iam.workloadIdentityUser",
        reason: "bind only the exact GitHub branch-main builder subject",
      }));
    }
  }
  if (deployer) {
    const deployerPolicy = serviceAccountPolicies.get(DEPLOYER_EMAIL);
    assertClosedServiceAccountPolicy({
      policy: deployerPolicy,
      requiredTuples: [],
      managedTuples: DEPLOYER_WIF_MEMBERS.map((member) => [
        member,
        "roles/iam.workloadIdentityUser",
      ]).concat(DEPLOYER_REMOVABLE_LEGACY_WIF_MEMBERS.map((member) => [
        member,
        "roles/iam.workloadIdentityUser",
      ])),
      label: "GitHub deployer service-account IAM policy",
    });
    reconcileWifMembers({
      policy: deployerPolicy,
      desiredMembers: DEPLOYER_WIF_MEMBERS,
      removableLegacyMembers: DEPLOYER_REMOVABLE_LEGACY_WIF_MEMBERS,
      email: DEPLOYER_EMAIL,
      idPrefix: "github-deployer-wif",
      label: "GitHub deployer Workload Identity authority",
      plannedMutations,
    });
  } else {
    for (const [index, member] of DEPLOYER_WIF_MEMBERS.entries()) {
      plannedMutations.push(serviceAccountBindingMutation({
        id: `github-deployer-wif-add-${index + 1}`,
        action: "add",
        email: DEPLOYER_EMAIL,
        member,
        role: "roles/iam.workloadIdentityUser",
        reason: "bind one exact GitHub deployer subject",
      }));
    }
  }
  if (rollback) {
    const rollbackPolicy = serviceAccountPolicies.get(ROLLBACK_EMAIL);
    assertClosedServiceAccountPolicy({
      policy: rollbackPolicy,
      requiredTuples: [],
      managedTuples: ROLLBACK_WIF_MEMBERS.map((member) => [
        member,
        "roles/iam.workloadIdentityUser",
      ]).concat(ROLLBACK_REMOVABLE_LEGACY_WIF_MEMBERS.map((member) => [
        member,
        "roles/iam.workloadIdentityUser",
      ])),
      label: "GitHub rollback service-account IAM policy",
    });
    reconcileWifMembers({
      policy: rollbackPolicy,
      desiredMembers: ROLLBACK_WIF_MEMBERS,
      removableLegacyMembers: ROLLBACK_REMOVABLE_LEGACY_WIF_MEMBERS,
      email: ROLLBACK_EMAIL,
      idPrefix: "github-rollback-wif",
      label: "GitHub rollback Workload Identity authority",
      plannedMutations,
    });
  } else {
    for (const [index, member] of ROLLBACK_WIF_MEMBERS.entries()) {
      plannedMutations.push(serviceAccountBindingMutation({
        id: `github-rollback-wif-add-${index + 1}`,
        action: "add",
        email: ROLLBACK_EMAIL,
        member,
        role: "roles/iam.workloadIdentityUser",
        reason: "bind one exact protected GitHub rollback subject",
      }));
    }
  }
  reconcileWifMembers({
    policy: commandSyncPolicy,
    desiredMembers: COMMAND_SYNC_WIF_MEMBERS,
    removableLegacyMembers: COMMAND_SYNC_REMOVABLE_LEGACY_WIF_MEMBERS,
    email: COMMAND_SYNC_EMAIL,
    idPrefix: "github-command-sync-wif",
    label: "GitHub command-sync Workload Identity authority",
    plannedMutations,
  });

  const repository = requiredJson(
    run,
    [
      "artifacts", "repositories", "describe", ARTIFACT_REPOSITORY,
      `--project=${PROJECT_ID}`,
      `--location=${REGION}`,
      "--format=json",
    ],
    "Artifact Registry repository",
  );
  assertExactRepository(repository);
  const repositoryPolicy = requiredJson(
    run,
    [
      "artifacts", "repositories", "get-iam-policy", ARTIFACT_REPOSITORY,
      `--project=${PROJECT_ID}`,
      `--location=${REGION}`,
      "--format=json",
    ],
    "Artifact Registry repository IAM policy",
  );
  assertNoFederatedMembers(repositoryPolicy, "Artifact Registry repository IAM policy");
  reconcileRepositoryRoles(repositoryPolicy, plannedMutations);

  const bucket = requiredJson(
    run,
    ["storage", "buckets", "describe", SOURCE_BUCKET, "--format=json"],
    "Cloud Build source bucket",
  );
  assertExactSourceBucket(bucket);
  const bucketPolicy = requiredJson(
    run,
    ["storage", "buckets", "get-iam-policy", SOURCE_BUCKET, "--format=json"],
    "Cloud Build source bucket IAM policy",
  );
  assertNoFederatedMembers(bucketPolicy, "Cloud Build source bucket IAM policy");
  reconcileBucketRoles(bucketPolicy, plannedMutations);

  if (includeSecretBoundaryAudit) {
    const secretInventory = requiredSecretInventory(run);
    assertExactSecretAuthority(secretInventory);
    observations.secretBoundary = "four-global-secrets-exact-accessor-sets";
  } else {
    observations.secretBoundary = "deferred-to-final-full-audit";
  }

  const orderedMutations = orderMutations(plannedMutations);
  return deepFreeze({
    contract: GITHUB_WIF_BOOTSTRAP_CONTRACT,
    status: orderedMutations.length === 0 ? "ready" : "changes-required",
    exactBinding: {
      projectId: PROJECT_ID,
      projectNumber: PROJECT_NUMBER,
      repository: GITHUB_REPOSITORY,
      repositoryId: GITHUB_REPOSITORY_ID,
      repositoryOwnerId: GITHUB_REPOSITORY_OWNER_ID,
      ref: GITHUB_REF,
      workflowRef: GITHUB_WORKFLOW_REF,
      rollbackWorkflowRef: GITHUB_ROLLBACK_WORKFLOW_REF,
      subjects: [
        GITHUB_MAIN_SUBJECT,
        GITHUB_PATH_CONFIRMATION_SUBJECT,
        GITHUB_RUNTIME_ROLLBACK_SUBJECT,
        GITHUB_COMMAND_SYNC_SUBJECT,
      ],
      subjectBindings: {
        builderServiceAccount: [GITHUB_MAIN_SUBJECT],
        deployerServiceAccount: [GITHUB_PATH_CONFIRMATION_SUBJECT],
        rollbackServiceAccount: [GITHUB_RUNTIME_ROLLBACK_SUBJECT],
        commandSyncServiceAccount: [
          GITHUB_COMMAND_SYNC_SUBJECT,
          GITHUB_RUNTIME_ROLLBACK_SUBJECT,
        ],
      },
      principalBindings: {
        builderServiceAccount: [...BUILDER_WIF_MEMBERS],
        deployerServiceAccount: [...DEPLOYER_WIF_MEMBERS],
        rollbackServiceAccount: [...ROLLBACK_WIF_MEMBERS],
        commandSyncServiceAccount: [...COMMAND_SYNC_WIF_MEMBERS],
      },
      workloadIdentityPool: POOL_NAME,
      workloadIdentityProvider: PROVIDER_NAME,
      rollbackWorkloadIdentityPool: ROLLBACK_POOL_NAME,
      rollbackWorkloadIdentityProvider: ROLLBACK_PROVIDER_NAME,
    },
    githubEnvironmentProtection: {
      externalConfigurationRequired: true,
      environments: {
        "discord-global-command-sync": {
          deploymentBranches: "protected-main-only",
          requiredReviewer: true,
        },
        "discord-path-confirmation": {
          deploymentBranches: "protected-main-only",
          requiredReviewer: true,
        },
        "discord-runtime-rollback": {
          deploymentBranches: "protected-main-only",
          requiredReviewer: true,
        },
      },
    },
    leastPrivilege: {
      buildProjectRoles: [...BUILD_PROJECT_ROLES],
      buildForbiddenProjectRoles: [...BUILD_REMOVABLE_LEGACY_PROJECT_ROLES],
      buildArtifactRepositoryRoles: ["roles/artifactregistry.writer"],
      buildSourceBucketRoles: [...BUILD_SOURCE_BUCKET_ROLES],
      buildServiceAccountRequiredHumanBindings: BUILD_REQUIRED_POLICY_TUPLES.map(
        ([member, role]) => ({ member, role }),
      ),
      builderProjectRoles: [...BUILDER_PROJECT_ROLES],
      deployerProjectRoles: [...DEPLOYER_PROJECT_ROLES],
      builderHasRunAdminRole: false,
      builderHasRuntimeActAsRole: false,
      builderHasSecretManagerRole: false,
      deployerHasBuildActAsRole: false,
      deployerHasCloudBuildRole: false,
      deployerHasSecretManagerRole: false,
      deployerArtifactRepositoryRoles: ["roles/artifactregistry.reader"],
      deployerCloudRunCustomRole: DEPLOYER_RUN_ROLE_NAME,
      deployerCloudRunPermissions: [...DEPLOYER_RUN_ROLE_PERMISSIONS],
      rollbackProjectRoles: [...ROLLBACK_PROJECT_ROLES],
      rollbackCloudRunCustomRole: ROLLBACK_RUN_ROLE_NAME,
      rollbackCloudRunPermissions: [...ROLLBACK_RUN_ROLE_PERMISSIONS],
      rollbackArtifactRepositoryRoles: [],
      rollbackSourceBucketRoles: [],
      rollbackRuntimeServiceAccountRoles: [],
      rollbackHasBuildActAsRole: false,
      rollbackHasCloudBuildRole: false,
      rollbackHasSecretManagerRole: false,
      rollbackHasJobLifecyclePermissions: false,
      builderArtifactRepositoryRoles: ["roles/artifactregistry.reader"],
      builderSourceBucketRoles: [...BUILDER_SOURCE_BUCKET_ROLES],
      builderBuildServiceAccountRoles: ["roles/iam.serviceAccountUser"],
      deployerRuntimeServiceAccountRoles: ["roles/iam.serviceAccountUser"],
      crossServiceAccountTokenCreatorRoles: [],
      workloadIdentityUserPrincipals: [...ALL_WIF_MEMBERS],
      commandSyncProjectRoles: [...COMMAND_SYNC_PROJECT_ROLES],
      commandSyncServiceAccountRequiredHumanBindings: COMMAND_SYNC_REQUIRED_POLICY_TUPLES.map(
        ([member, role]) => ({ member, role }),
      ),
      trustedHumanServiceAccountBindings: Object.fromEntries(
        Object.entries(TRUSTED_HUMAN_SERVICE_ACCOUNT_POLICY_TUPLES)
          .sort(([left], [right]) => left.localeCompare(right))
          .map(([email, tuples]) => [
            email,
            tuples.map(([member, role]) => ({ member, role })),
          ]),
      ),
      commandSyncSecretRoles: [SECRET_ACCESSOR_ROLE],
      exactGlobalSecretAccessorServiceAccounts: {
        [DISCORD_SECRET]: [COMMAND_SYNC_EMAIL, INTERACTION_EMAIL],
        [JOB_SECRET]: [RUNTIME_EMAIL, INTERACTION_EMAIL],
        [TELEMETRY_EVENT_SECRET]: [INTERACTION_EMAIL],
        [TELEMETRY_TRANSPORT_SECRET]: [TELEMETRY_EMAIL],
      },
      catalogWideUnmodeledImpersonationAllowed: false,
      officialPermissionReferences: { ...OFFICIAL_PERMISSION_REFERENCES },
      userManagedServiceAccountKeysAllowed: false,
    },
    reconciliation: {
      maximumPropagationMillisecondsPerMutation: MAX_RECONCILE_MILLISECONDS,
      backoffMilliseconds: [...RECONCILE_BACKOFF_MILLISECONDS],
      fullSecretBoundaryAudit: "initial-and-final",
      phaseCount: 8,
      strategy: "phase-batched-fast-plan-boundaries",
    },
    observations,
    githubRepositoryVariables: { ...REPOSITORY_VARIABLES },
    plannedMutations: orderedMutations,
  });
}

export async function applyGitHubWifBootstrap(options = {}, dependencies = {}) {
  assertClosedOptions(options);
  assertApplyDependencies(dependencies);
  const run = dependencies.runGcloud ?? runGcloud;
  const sleep = dependencies.sleep ?? sleepFor;
  const now = dependencies.now ?? Date.now;
  let report = await createGitHubWifBootstrapPlan(options, { runGcloud: run });
  if (report.status === "ready") return report;

  for (;;) {
    while (report.status !== "ready") {
      const first = report.plannedMutations[0];
      if (!first) throw new Error("GitHub WIF bootstrap plan is internally inconsistent");
      const phase = mutationPhase(first);
      const plannedPhase = report.plannedMutations.filter(
        (planned) => mutationPhase(planned) === phase,
      );
      const attempts = [];
      for (const planned of plannedPhase) {
        const attempt = {
          planned,
          propagationStartedAt: exactClockMilliseconds(now, "mutation propagation start"),
          scheduledWaitMilliseconds: 0,
          backoffIndex: 0,
          retryReady: false,
          writeSucceeded: isSuccessful(run([...planned.argv])),
        };
        attempts.push(attempt);
        if (!attempt.writeSucceeded) break;
      }
      report = await observeMutationPhase({
        attempts,
        options,
        phase,
        plannedPhase,
        run,
        sleep,
        now,
      });
    }

    report = await createGitHubWifBootstrapPlan(options, { runGcloud: run });
    if (report.status === "ready" && report.plannedMutations.length === 0) return report;
  }
}

async function observeMutationPhase({
  attempts,
  options,
  phase,
  plannedPhase,
  run,
  sleep,
  now,
}) {
  if (attempts.length === 0) {
    throw new Error("GitHub WIF bootstrap phase contains no mutation attempt");
  }
  const plannedPhaseIds = new Set(plannedPhase.map(({ id }) => id));
  for (;;) {
    let observed;
    try {
      observed = await createGitHubWifBootstrapPlanInternal(
        options,
        { runGcloud: run },
        false,
      );
    } catch (error) {
      if (!isRetryableObservationFailure(error)) throw error;
      await waitForMutationAttempts(attempts, sleep, now);
      continue;
    }
    for (const attempt of attempts) assertMutationAttemptWithinBudget(attempt, now);
    for (const planned of observed.plannedMutations) {
      const observedPhase = mutationPhase(planned);
      const expectedInPhase = observedPhase === phase
        ? plannedPhase.find(({ id }) => id === planned.id)
        : undefined;
      if (
        observedPhase < phase ||
        (observedPhase === phase && !plannedPhaseIds.has(planned.id))
      ) {
        throw new Error(`GitHub WIF bootstrap prerequisite drifted during phase ${phase}`);
      }
      if (expectedInPhase) assertSameMutation(expectedInPhase, planned);
    }
    const pendingAttempts = [];
    for (const attempt of attempts) {
      const pending = observed.plannedMutations.find(
        ({ id }) => id === attempt.planned.id,
      );
      if (!pending) continue;
      assertSameMutation(attempt.planned, pending);
      pendingAttempts.push(attempt);
      if (!attempt.writeSucceeded && attempt.retryReady) {
        attempt.writeSucceeded = isSuccessful(run([...pending.argv]));
        attempt.retryReady = false;
      }
    }
    if (pendingAttempts.length === 0) return observed;
    await waitForMutationAttempts(pendingAttempts, sleep, now);
    for (const attempt of pendingAttempts) {
      if (!attempt.writeSucceeded) attempt.retryReady = true;
    }
  }
}

async function waitForMutationAttempts(attempts, sleep, now) {
  const delays = attempts.map((attempt) => {
    const remaining = MAX_RECONCILE_MILLISECONDS - mutationAttemptElapsed(attempt, now);
    if (remaining <= 0) throwMutationAttemptTimeout(attempt);
    return Math.min(
      RECONCILE_BACKOFF_MILLISECONDS[
        Math.min(attempt.backoffIndex, RECONCILE_BACKOFF_MILLISECONDS.length - 1)
      ],
      remaining,
    );
  });
  const delay = Math.min(...delays);
  await sleep(delay);
  for (const attempt of attempts) {
    attempt.scheduledWaitMilliseconds += delay;
    attempt.backoffIndex += 1;
  }
}

function assertMutationAttemptWithinBudget(attempt, now) {
  if (mutationAttemptElapsed(attempt, now) > MAX_RECONCILE_MILLISECONDS) {
    throwMutationAttemptTimeout(attempt);
  }
}

function isRetryableObservationFailure(error) {
  return error instanceof Error &&
    /(?: lookup failed| returned invalid JSON)$/u.test(error.message);
}

function mutationAttemptElapsed(attempt, now) {
  const clockElapsed = exactClockMilliseconds(now, "mutation propagation clock") -
    attempt.propagationStartedAt;
  if (clockElapsed < 0) {
    throw new Error("GitHub WIF bootstrap reconcile clock moved backwards");
  }
  return Math.max(clockElapsed, attempt.scheduledWaitMilliseconds);
}

function throwMutationAttemptTimeout(attempt) {
  throw new Error(
    `GitHub WIF bootstrap mutation did not converge within ${MAX_RECONCILE_MILLISECONDS}ms: ${attempt.planned.id}`,
  );
}

function assertApplyDependencies(dependencies) {
  if (
    dependencies === null ||
    typeof dependencies !== "object" ||
    Array.isArray(dependencies) ||
    Object.keys(dependencies).some((key) => !["now", "runGcloud", "sleep"].includes(key)) ||
    (dependencies.runGcloud !== undefined && typeof dependencies.runGcloud !== "function") ||
    (dependencies.sleep !== undefined && typeof dependencies.sleep !== "function") ||
    (dependencies.now !== undefined && typeof dependencies.now !== "function")
  ) {
    throw new Error("GitHub WIF bootstrap apply dependencies are invalid");
  }
}

function exactClockMilliseconds(now, label) {
  const value = now();
  if (!Number.isFinite(value) || !Number.isSafeInteger(value)) {
    throw new Error(`${label} is invalid`);
  }
  return value;
}

function assertSameMutation(expected, observed) {
  if (
    observed?.id !== expected?.id ||
    observed?.reason !== expected?.reason ||
    !exactArgv(observed?.argv ?? [], expected?.argv ?? [])
  ) {
    throw new Error(`GitHub WIF bootstrap mutation drifted while reconciling: ${expected?.id ?? "unknown"}`);
  }
}

function sleepFor(milliseconds) {
  return new Promise((resolvePromise) => setTimeout(resolvePromise, milliseconds));
}

function assertClosedOptions(options) {
  if (
    options === null ||
    typeof options !== "object" ||
    Array.isArray(options) ||
    Object.keys(options).length !== 0
  ) {
    throw new Error("GitHub WIF bootstrap has no widenable options");
  }
}

function assertExactProject(run) {
  const project = requiredJson(
    run,
    ["projects", "describe", PROJECT_ID, "--format=json"],
    "Cloud project metadata",
  );
  if (
    project?.projectId !== PROJECT_ID ||
    String(project?.projectNumber ?? "") !== PROJECT_NUMBER ||
    project?.lifecycleState !== "ACTIVE"
  ) {
    throw new Error("Cloud project identity is not the exact active Clearra project");
  }
  const ancestors = requiredJson(
    run,
    ["projects", "get-ancestors", PROJECT_ID, "--format=json(id,type)"],
    "Cloud project ancestry",
  );
  if (
    !Array.isArray(ancestors) ||
    ancestors.length !== 1 ||
    ancestors[0]?.type !== "project" ||
    ancestors[0]?.id !== PROJECT_ID
  ) {
    throw new Error("GitHub WIF bootstrap requires the exact parentless Clearra project");
  }
}

function requiredEnabledServices(run) {
  const services = requiredJson(
    run,
    ["services", "list", "--enabled", `--project=${PROJECT_ID}`, "--format=json(config.name,state)"],
    "enabled service catalog",
  );
  if (!Array.isArray(services)) throw new Error("enabled service catalog is invalid");
  const names = new Set();
  for (const entry of services) {
    const name = entry?.config?.name;
    if (typeof name !== "string" || entry?.state !== "ENABLED" || names.has(name)) {
      throw new Error("enabled service catalog contains invalid or duplicate entries");
    }
    names.add(name);
  }
  return names;
}

function assertNoPersistentApiEndpointOverrides(run) {
  const overrides = requiredJson(
    run,
    ["config", "list", "--all", "--format=json(api_endpoint_overrides)"],
    "gcloud persistent API endpoint override catalog",
  );
  const endpointOverrides = overrides?.api_endpoint_overrides;
  if (
    overrides === null ||
    typeof overrides !== "object" ||
    Array.isArray(overrides) ||
    Object.keys(overrides).length !== 1 ||
    endpointOverrides === null ||
    typeof endpointOverrides !== "object" ||
    Array.isArray(endpointOverrides) ||
    Object.keys(endpointOverrides).length === 0 ||
    Object.entries(endpointOverrides).some(([key, value]) =>
      !/^[a-z][a-z0-9_]*$/u.test(key) || value !== null)
  ) {
    throw new Error("gcloud persistent API endpoint overrides must all be unset");
  }
}

function assertExactPool(pool, expected) {
  if (
    pool?.name !== expected.name ||
    pool?.state !== "ACTIVE" ||
    pool?.disabled === true ||
    pool?.displayName !== expected.displayName ||
    pool?.description !== expected.description
  ) {
    throw new Error(`${expected.label} drifted from its exact global definition`);
  }
}

function assertExactProvider(provider, expected) {
  const mapping = provider?.attributeMapping;
  const audiences = provider?.oidc?.allowedAudiences;
  if (
    provider?.name !== expected.name ||
    provider?.state !== "ACTIVE" ||
    provider?.disabled === true ||
    provider?.displayName !== expected.displayName ||
    provider?.description !== expected.description ||
    provider?.oidc?.issuerUri !== OIDC_ISSUER ||
    (audiences !== undefined && (!Array.isArray(audiences) || audiences.length !== 0)) ||
    canonicalCondition(provider?.attributeCondition) !== expected.attributeCondition ||
    !exactStringRecord(mapping, ATTRIBUTE_MAPPING)
  ) {
    throw new Error(`${expected.label} drifted from its exact repository/main binding`);
  }
}

function canonicalCondition(value) {
  return typeof value === "string" ? value.replace(/\s+/gu, " ").trim() : "";
}

function exactStringRecord(value, expected) {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return false;
  const entries = Object.entries(value).sort(([left], [right]) => left.localeCompare(right));
  const expectedEntries = Object.entries(expected).sort(([left], [right]) => left.localeCompare(right));
  return JSON.stringify(entries) === JSON.stringify(expectedEntries);
}

function createProviderMutation() {
  const mapping = Object.entries(ATTRIBUTE_MAPPING)
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([key, value]) => `${key}=${value}`)
    .join(",");
  return mutation(
    "create-provider",
    "create the exact repository/main OIDC provider",
    [
      "iam", "workload-identity-pools", "providers", "create-oidc", PROVIDER_ID,
      `--project=${PROJECT_ID}`,
      "--location=global",
      `--workload-identity-pool=${POOL_ID}`,
      `--issuer-uri=${OIDC_ISSUER}`,
      `--attribute-mapping=${mapping}`,
      `--attribute-condition=${ATTRIBUTE_CONDITION}`,
      `--display-name=${PROVIDER_DISPLAY_NAME}`,
      `--description=${PROVIDER_DESCRIPTION}`,
      "--quiet",
    ],
  );
}

function createRollbackPoolMutation() {
  return mutation(
    "create-rollback-pool",
    "create the dedicated recovery-workflow GitHub pool",
    [
      "iam", "workload-identity-pools", "create", ROLLBACK_POOL_ID,
      `--project=${PROJECT_ID}`,
      "--location=global",
      `--display-name=${ROLLBACK_POOL_DISPLAY_NAME}`,
      `--description=${ROLLBACK_POOL_DESCRIPTION}`,
      "--quiet",
    ],
  );
}

function createRollbackProviderMutation() {
  const mapping = Object.entries(ATTRIBUTE_MAPPING)
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([key, value]) => `${key}=${value}`)
    .join(",");
  return mutation(
    "create-rollback-provider",
    "create the exact recovery-workflow/main OIDC provider",
    [
      "iam", "workload-identity-pools", "providers", "create-oidc", ROLLBACK_PROVIDER_ID,
      `--project=${PROJECT_ID}`,
      "--location=global",
      `--workload-identity-pool=${ROLLBACK_POOL_ID}`,
      `--issuer-uri=${OIDC_ISSUER}`,
      `--attribute-mapping=${mapping}`,
      `--attribute-condition=${ROLLBACK_ATTRIBUTE_CONDITION}`,
      `--display-name=${ROLLBACK_PROVIDER_DISPLAY_NAME}`,
      `--description=${ROLLBACK_PROVIDER_DESCRIPTION}`,
      "--quiet",
    ],
  );
}

function assertRequiredServiceAccount(catalog, email) {
  const account = catalog.get(email);
  if (!account || account.disabled === true || account.name !== serviceAccountName(email)) {
    throw new Error(`required enabled service account is unavailable: ${email}`);
  }
}

function assertExactServiceAccountCatalog(catalog) {
  if (!Array.isArray(catalog)) throw new Error("service-account catalog is invalid");
  const accounts = new Map();
  for (const account of catalog) {
    const email = account?.email;
    if (
      typeof email !== "string" ||
      !isCanonicalProjectServiceAccountEmail(email) ||
      account?.name !== serviceAccountName(email) ||
      accounts.has(email)
    ) {
      throw new Error("service-account catalog contains an invalid, foreign, or duplicate identity");
    }
    accounts.set(email, account);
  }
  return accounts;
}

function isCanonicalProjectServiceAccountEmail(email) {
  return /^[a-z0-9](?:[a-z0-9.-]{0,126}[a-z0-9])?@[a-z0-9](?:[a-z0-9.-]{0,251}[a-z0-9])?\.gserviceaccount\.com$/u.test(email) &&
    (
      email.endsWith(`@${PROJECT_ID}.iam.gserviceaccount.com`) ||
      email === `${PROJECT_NUMBER}-compute@developer.gserviceaccount.com` ||
      email === `${PROJECT_NUMBER}@cloudbuild.gserviceaccount.com` ||
      new RegExp(`^service-${PROJECT_NUMBER}@gcp-sa-[a-z0-9-]+\\.iam\\.gserviceaccount\\.com$`, "u")
        .test(email)
    );
}

function assertExactDeployer(account) {
  if (
    account?.email !== DEPLOYER_EMAIL ||
    account?.name !== serviceAccountName(DEPLOYER_EMAIL) ||
    account?.disabled === true ||
    account?.displayName !== DEPLOYER_DISPLAY_NAME ||
    account?.description !== DEPLOYER_DESCRIPTION
  ) {
    throw new Error("deployer service account metadata drifted");
  }
}

function assertExactRollback(account) {
  if (
    account?.email !== ROLLBACK_EMAIL ||
    account?.name !== serviceAccountName(ROLLBACK_EMAIL) ||
    account?.disabled === true ||
    account?.displayName !== ROLLBACK_DISPLAY_NAME ||
    account?.description !== ROLLBACK_DESCRIPTION
  ) {
    throw new Error("rollback service account metadata drifted");
  }
}

function assertExactDeployerRunRole(role) {
  const permissions = Array.isArray(role?.includedPermissions)
    ? [...role.includedPermissions].sort()
    : null;
  if (
    role?.name !== DEPLOYER_RUN_ROLE_NAME ||
    role?.title !== DEPLOYER_RUN_ROLE_TITLE ||
    role?.description !== DEPLOYER_RUN_ROLE_DESCRIPTION ||
    role?.stage !== "GA" ||
    role?.deleted === true ||
    JSON.stringify(permissions) !== JSON.stringify([...DEPLOYER_RUN_ROLE_PERMISSIONS].sort())
  ) {
    throw new Error("deployer custom role drifted from the exact Cloud Run workflow authority");
  }
}

function createDeployerRunRoleMutation() {
  return mutation(
    "create-deployer-runtime-role",
    "create the exact workflow-only Cloud Run deployment custom role",
    [
      "iam", "roles", "create", DEPLOYER_RUN_ROLE_ID,
      `--project=${PROJECT_ID}`,
      `--title=${DEPLOYER_RUN_ROLE_TITLE}`,
      `--description=${DEPLOYER_RUN_ROLE_DESCRIPTION}`,
      `--permissions=${DEPLOYER_RUN_ROLE_PERMISSIONS.join(",")}`,
      "--stage=GA",
      "--quiet",
    ],
  );
}

function assertExactRollbackRunRole(role) {
  if (!isExactRollbackRunRole(role)) {
    throw new Error("rollback custom role drifted from the exact existing-service authority");
  }
}

function isExactRollbackRunRole(role) {
  const permissions = Array.isArray(role?.includedPermissions)
    ? [...role.includedPermissions].sort()
    : null;
  return !(
    role?.name !== ROLLBACK_RUN_ROLE_NAME ||
    role?.title !== ROLLBACK_RUN_ROLE_TITLE ||
    role?.description !== ROLLBACK_RUN_ROLE_DESCRIPTION ||
    role?.stage !== "GA" ||
    role?.deleted === true ||
    JSON.stringify(permissions) !== JSON.stringify([...ROLLBACK_RUN_ROLE_PERMISSIONS].sort())
  );
}

function isLegacyDeleteRollbackRunRole(role) {
  if (
    role?.name !== ROLLBACK_RUN_ROLE_NAME ||
    role?.title !== ROLLBACK_RUN_ROLE_TITLE ||
    role?.description !== ROLLBACK_RUN_ROLE_DESCRIPTION ||
    role?.stage !== "GA" ||
    role?.deleted === true ||
    !Array.isArray(role?.includedPermissions)
  ) {
    return false;
  }
  const legacyPermissions = [...ROLLBACK_RUN_ROLE_PERMISSIONS, "run.revisions.delete"].sort();
  return JSON.stringify([...role.includedPermissions].sort()) === JSON.stringify(legacyPermissions);
}

function createRollbackRunRoleMutation() {
  return mutation(
    "create-rollback-runtime-role",
    "create the exact existing-service Cloud Run rollback custom role",
    [
      "iam", "roles", "create", ROLLBACK_RUN_ROLE_ID,
      `--project=${PROJECT_ID}`,
      `--title=${ROLLBACK_RUN_ROLE_TITLE}`,
      `--description=${ROLLBACK_RUN_ROLE_DESCRIPTION}`,
      `--permissions=${ROLLBACK_RUN_ROLE_PERMISSIONS.join(",")}`,
      "--stage=GA",
      "--quiet",
    ],
  );
}

function updateRollbackRunRoleMutation() {
  return mutation(
    "remove-rollback-revision-delete-permission",
    "remove the obsolete revision-delete permission from the exact rollback custom role",
    [
      "iam", "roles", "update", ROLLBACK_RUN_ROLE_ID,
      `--project=${PROJECT_ID}`,
      `--title=${ROLLBACK_RUN_ROLE_TITLE}`,
      `--description=${ROLLBACK_RUN_ROLE_DESCRIPTION}`,
      `--permissions=${ROLLBACK_RUN_ROLE_PERMISSIONS.join(",")}`,
      "--stage=GA",
      "--quiet",
    ],
  );
}

function assertExactBuilder(account) {
  if (
    account?.email !== BUILDER_EMAIL ||
    account?.name !== serviceAccountName(BUILDER_EMAIL) ||
    account?.disabled === true ||
    account?.displayName !== BUILDER_DISPLAY_NAME ||
    account?.description !== BUILDER_DESCRIPTION
  ) {
    throw new Error("builder service account metadata drifted");
  }
}

function assertNoUserManagedKeys(run, email, label) {
  const keys = requiredJson(
    run,
    [
      "iam", "service-accounts", "keys", "list",
      `--iam-account=${email}`,
      `--project=${PROJECT_ID}`,
      "--managed-by=user",
      "--format=json",
    ],
    `${label} user-managed key catalog`,
  );
  if (!Array.isArray(keys) || keys.length !== 0) {
    throw new Error(`${label} must have zero user-managed keys`);
  }
}

function requiredServiceAccountPolicy(run, email) {
  return requiredJson(
    run,
    [
      "iam", "service-accounts", "get-iam-policy", email,
      `--project=${PROJECT_ID}`,
      "--format=json",
    ],
    `service-account IAM policy (${email})`,
  );
}

function assertCatalogWideImpersonationBoundary(serviceAccountPolicies) {
  const tupleKey = (member, role) => `${role}\u0000${member}`;
  const allowedByTarget = new Map([
    [BUILD_EMAIL, new Set([tupleKey(BUILDER_MEMBER, "roles/iam.serviceAccountUser")])],
    [RUNTIME_EMAIL, new Set([tupleKey(DEPLOYER_MEMBER, "roles/iam.serviceAccountUser")])],
    [COMMAND_SYNC_EMAIL, new Set([
      ...COMMAND_SYNC_WIF_MEMBERS,
      ...COMMAND_SYNC_REMOVABLE_LEGACY_WIF_MEMBERS,
    ].map((member) =>
      tupleKey(member, "roles/iam.workloadIdentityUser")))],
    [BUILDER_EMAIL, new Set([
      ...BUILDER_WIF_MEMBERS,
      ...BUILDER_REMOVABLE_LEGACY_WIF_MEMBERS,
    ].map((member) =>
      tupleKey(member, "roles/iam.workloadIdentityUser")))],
    [DEPLOYER_EMAIL, new Set([
      ...DEPLOYER_WIF_MEMBERS,
      ...DEPLOYER_REMOVABLE_LEGACY_WIF_MEMBERS,
    ].map((member) =>
      tupleKey(member, "roles/iam.workloadIdentityUser")))],
    [ROLLBACK_EMAIL, new Set([
      ...ROLLBACK_WIF_MEMBERS,
      ...ROLLBACK_REMOVABLE_LEGACY_WIF_MEMBERS,
    ].map((member) =>
      tupleKey(member, "roles/iam.workloadIdentityUser")))],
  ]);
  const requiredHumanByTarget = new Map();
  for (const [targetEmail, tuples] of Object.entries(TRUSTED_HUMAN_SERVICE_ACCOUNT_POLICY_TUPLES)) {
    const required = new Set(tuples.map(([member, role]) => tupleKey(member, role)));
    requiredHumanByTarget.set(targetEmail, required);
    const allowed = allowedByTarget.get(targetEmail) ?? new Set();
    for (const key of required) allowed.add(key);
    allowedByTarget.set(targetEmail, allowed);
  }
  for (const [targetEmail, policy] of serviceAccountPolicies) {
    if (policy === null || typeof policy !== "object" || Array.isArray(policy)) {
      throw new Error(`service-account IAM policy is invalid: ${targetEmail}`);
    }
    const bindings = policy.bindings ?? [];
    if (!Array.isArray(bindings)) {
      throw new Error(`service-account IAM policy bindings are invalid: ${targetEmail}`);
    }
    const observedAllowed = new Set();
    for (const binding of bindings) {
      if (
        typeof binding?.role !== "string" ||
        !Array.isArray(binding?.members) ||
        binding.members.length === 0 ||
        (binding.condition !== undefined && binding.condition !== null)
      ) {
        throw new Error(`service-account IAM policy contains an invalid binding: ${targetEmail}`);
      }
      for (const member of binding.members) {
        const key = tupleKey(member, binding.role);
        if (
          !allowedByTarget.get(targetEmail)?.has(key) ||
          (binding.condition !== undefined && binding.condition !== null) ||
          observedAllowed.has(key)
        ) {
          throw new Error(
            `service-account catalog contains authority outside the exact trusted tuple set: ${targetEmail}`,
          );
        }
        observedAllowed.add(key);
      }
    }
    for (const requiredHuman of requiredHumanByTarget.get(targetEmail) ?? []) {
      if (!observedAllowed.has(requiredHuman)) {
        throw new Error(
          `service-account catalog is missing required trusted-human authority: ${targetEmail}`,
        );
      }
    }
  }
}

function assertClosedServiceAccountPolicy({
  policy,
  requiredTuples,
  managedTuples,
  label,
}) {
  if (policy === null || typeof policy !== "object" || Array.isArray(policy)) {
    throw new Error(`${label} is invalid`);
  }
  const bindings = policy.bindings ?? [];
  if (!Array.isArray(bindings)) throw new Error(`${label} bindings are invalid`);
  const tupleKey = ([member, role]) => `${role}\u0000${member}`;
  const required = new Set(requiredTuples.map(tupleKey));
  const allowed = new Set([...requiredTuples, ...managedTuples].map(tupleKey));
  if (allowed.size !== requiredTuples.length + managedTuples.length) {
    throw new Error(`${label} expected authority contains duplicate tuples`);
  }
  const observed = new Set();
  for (const binding of bindings) {
    if (
      typeof binding?.role !== "string" ||
      !Array.isArray(binding?.members) ||
      binding.members.length === 0 ||
      (binding.condition !== undefined && binding.condition !== null)
    ) {
      throw new Error(`${label} contains an invalid or conditional binding`);
    }
    for (const member of binding.members) {
      if (typeof member !== "string" || member.length === 0) {
        throw new Error(`${label} contains an invalid member`);
      }
      const key = tupleKey([member, binding.role]);
      if (!allowed.has(key)) {
        throw new Error(`${label} contains unmodeled impersonation authority`);
      }
      if (observed.has(key)) throw new Error(`${label} contains duplicate authority`);
      observed.add(key);
    }
  }
  for (const key of required) {
    if (!observed.has(key)) throw new Error(`${label} is missing required human-admin continuity`);
  }
}

function reconcileProjectRoles({
  policy,
  member,
  desired,
  removableLegacy,
  idPrefix,
  label,
  plannedMutations,
}) {
  const observed = rolesForMember(policy, member, label);
  const allowed = new Set([...desired, ...removableLegacy]);
  const unexpected = observed.filter((role) => !allowed.has(role));
  if (unexpected.length > 0) throw new Error(`${label} contains unexpected authority`);
  for (const role of desired) {
    if (!observed.includes(role)) {
      plannedMutations.push(projectBindingMutation({
        id: `${idPrefix}-add-${roleSlug(role)}`,
        action: "add",
        member,
        role,
        reason: `add required ${label}`,
      }));
    }
  }
  for (const role of removableLegacy) {
    if (observed.includes(role)) {
      plannedMutations.push(projectBindingMutation({
        id: `${idPrefix}-remove-${roleSlug(role)}`,
        action: "remove",
        member,
        role,
        reason: `remove retired ${label}`,
      }));
    }
  }
}

function reconcileResourceRoles({
  policy,
  member,
  desired,
  resourceId,
  idPrefix,
  label,
  plannedMutations,
}) {
  const observed = rolesForMember(policy, member, label);
  const desiredSet = new Set(desired);
  if (observed.some((role) => !desiredSet.has(role))) {
    throw new Error(`${label} contains unexpected authority`);
  }
  for (const role of desired) {
    if (!observed.includes(role)) {
      plannedMutations.push(serviceAccountBindingMutation({
        id: `${idPrefix}-add-${roleSlug(role)}`,
        action: "add",
        email: resourceId,
        member,
        role,
        reason: `add required ${label}`,
      }));
    }
  }
}

function reconcileWifMembers({
  policy,
  desiredMembers,
  removableLegacyMembers,
  email,
  idPrefix,
  label,
  plannedMutations,
}) {
  const observedMembers = closedWifMembers(
    policy,
    [...desiredMembers, ...removableLegacyMembers],
    label,
  );
  for (const member of desiredMembers) {
    const memberRoles = rolesForMember(policy, member, label);
    if (memberRoles.some((role) => role !== "roles/iam.workloadIdentityUser")) {
      throw new Error(`${label} subject has cross-role authority`);
    }
    if (!observedMembers.includes(member)) {
      plannedMutations.push(serviceAccountBindingMutation({
        id: `${idPrefix}-add-${wifMemberSlug(member)}`,
        action: "add",
        email,
        member,
        role: "roles/iam.workloadIdentityUser",
        reason: `bind one exact ${label} subject`,
      }));
    }
  }
  for (const member of removableLegacyMembers) {
    if (observedMembers.includes(member)) {
      plannedMutations.push(serviceAccountBindingMutation({
        id: `${idPrefix}-remove-legacy-${wifMemberSlug(member)}`,
        action: "remove",
        email,
        member,
        role: "roles/iam.workloadIdentityUser",
        reason: `remove replaced legacy name-only ${label} subject`,
      }));
    }
  }
}

function closedWifMembers(policy, allowedMembers, label) {
  const bindings = Array.isArray(policy?.bindings) ? policy.bindings : [];
  const members = [];
  for (const binding of bindings) {
    if (!Array.isArray(binding?.members) || typeof binding?.role !== "string") {
      throw new Error(`${label} contains an invalid IAM binding`);
    }
    const federatedMembers = binding.members.filter(isFederatedPrincipal);
    if (binding.role === "roles/iam.workloadIdentityUser") {
      if (
        binding.members.length === 0 ||
        federatedMembers.length !== binding.members.length ||
        (binding.condition !== undefined && binding.condition !== null)
      ) {
        throw new Error(`${label} contains invalid or conditional federated authority`);
      }
      members.push(...federatedMembers);
      continue;
    }
    if (federatedMembers.length > 0) {
      throw new Error(`${label} gives a federated principal a forbidden impersonation role`);
    }
  }
  if (new Set(members).size !== members.length) {
    throw new Error(`${label} contains duplicate federated authority`);
  }
  const allowed = new Set(allowedMembers);
  if (allowed.size !== allowedMembers.length) {
    throw new Error(`${label} expected federated authority contains duplicates`);
  }
  if (members.some((member) => !allowed.has(member))) {
    throw new Error(`${label} contains an unexpected federated principal`);
  }
  return members.sort();
}

function assertNoFederatedMembers(policy, label) {
  if (policy === null || typeof policy !== "object" || Array.isArray(policy)) {
    throw new Error(`${label} is invalid`);
  }
  const bindings = policy.bindings ?? [];
  if (!Array.isArray(bindings)) throw new Error(`${label} bindings are invalid`);
  for (const binding of bindings) {
    if (!Array.isArray(binding?.members) || typeof binding?.role !== "string") {
      throw new Error(`${label} contains an invalid binding`);
    }
    if (binding.members.some(isFederatedPrincipal)) {
      throw new Error(`${label} must contain zero direct federated principals`);
    }
  }
}

function isFederatedPrincipal(member) {
  return typeof member === "string" &&
    (member.startsWith("principal://") || member.startsWith("principalSet://"));
}

function assertNoMemberRoles(policy, member, label) {
  if (rolesForMember(policy, member, label).length !== 0) {
    throw new Error(label);
  }
}

function wifMemberSlug(member) {
  if (
    member.endsWith(`/subject/${GITHUB_MAIN_SUBJECT}`) ||
    member.endsWith(`/subject/${LEGACY_GITHUB_MAIN_SUBJECT}`)
  ) return "branch-main";
  if (
    member.endsWith(`/subject/${GITHUB_PATH_CONFIRMATION_SUBJECT}`) ||
    member.endsWith(`/subject/${LEGACY_GITHUB_PATH_CONFIRMATION_SUBJECT}`)
  ) {
    return "environment-path-confirmation";
  }
  if (member === githubSubjectMember(POOL_NAME, GITHUB_RUNTIME_ROLLBACK_SUBJECT)) {
    return "primary-environment-runtime-rollback";
  }
  if (member === githubSubjectMember(ROLLBACK_POOL_NAME, GITHUB_RUNTIME_ROLLBACK_SUBJECT)) {
    return "recovery-environment-runtime-rollback";
  }
  if (
    member.endsWith(`/subject/${GITHUB_COMMAND_SYNC_SUBJECT}`) ||
    member.endsWith(`/subject/${LEGACY_GITHUB_COMMAND_SYNC_SUBJECT}`)
  ) {
    return "environment-global-command-sync";
  }
  if (
    member === githubSubjectMember(
      ROLLBACK_POOL_NAME,
      LEGACY_GITHUB_RUNTIME_ROLLBACK_SUBJECT,
    )
  ) return "recovery-environment-runtime-rollback";
  throw new Error("GitHub WIF subject is outside the exact closed set");
}

function assertExactRepository(repository) {
  const expectedNames = new Set([
    `projects/${PROJECT_ID}/locations/${REGION}/repositories/${ARTIFACT_REPOSITORY}`,
    `projects/${PROJECT_NUMBER}/locations/${REGION}/repositories/${ARTIFACT_REPOSITORY}`,
  ]);
  if (!expectedNames.has(repository?.name) || repository?.format !== "DOCKER") {
    throw new Error("Artifact Registry repository identity drifted");
  }
}

function reconcileRepositoryRoles(policy, plannedMutations) {
  const buildRoles = rolesForMember(policy, BUILD_MEMBER, "Artifact Registry build authority");
  if (buildRoles.some((role) => role !== "roles/artifactregistry.writer")) {
    throw new Error("Artifact Registry build authority contains unexpected authority");
  }
  if (!buildRoles.includes("roles/artifactregistry.writer")) {
    plannedMutations.push(mutation(
      "artifact-repository-build-add-writer",
      "allow the Cloud Build execution identity to write only the exact image repository",
      [
        "artifacts", "repositories", "add-iam-policy-binding", ARTIFACT_REPOSITORY,
        `--project=${PROJECT_ID}`,
        `--location=${REGION}`,
        `--member=${BUILD_MEMBER}`,
        "--role=roles/artifactregistry.writer",
        "--condition=None",
        "--quiet",
      ],
    ));
  }
  const builderRoles = rolesForMember(policy, BUILDER_MEMBER, "Artifact Registry builder authority");
  if (builderRoles.some((role) => role !== "roles/artifactregistry.reader")) {
    throw new Error("Artifact Registry builder authority contains unexpected authority");
  }
  if (!builderRoles.includes("roles/artifactregistry.reader")) {
    plannedMutations.push(mutation(
      "artifact-repository-builder-add-reader",
      "allow immutable candidate digest readback",
      [
        "artifacts", "repositories", "add-iam-policy-binding", ARTIFACT_REPOSITORY,
        `--project=${PROJECT_ID}`,
        `--location=${REGION}`,
        `--member=${BUILDER_MEMBER}`,
        "--role=roles/artifactregistry.reader",
        "--condition=None",
        "--quiet",
      ],
    ));
  }
  const deployerRoles = rolesForMember(policy, DEPLOYER_MEMBER, "Artifact Registry deployer authority");
  if (deployerRoles.some((role) => role !== "roles/artifactregistry.reader")) {
    throw new Error("Artifact Registry deployer authority contains unexpected authority");
  }
  if (!deployerRoles.includes("roles/artifactregistry.reader")) {
    plannedMutations.push(mutation(
      "artifact-repository-deployer-add-reader",
      "allow Cloud Run to resolve and deploy only the exact repository image",
      [
        "artifacts", "repositories", "add-iam-policy-binding", ARTIFACT_REPOSITORY,
        `--project=${PROJECT_ID}`,
        `--location=${REGION}`,
        `--member=${DEPLOYER_MEMBER}`,
        "--role=roles/artifactregistry.reader",
        "--condition=None",
        "--quiet",
      ],
    ));
  }
  assertNoMemberRoles(
    policy,
    COMMAND_SYNC_MEMBER,
    "command sync must not read the build Artifact Registry repository",
  );
  assertNoMemberRoles(
    policy,
    ROLLBACK_MEMBER,
    "rollback must not read the build Artifact Registry repository",
  );
}

function assertExactSourceBucket(bucket) {
  const name = String(bucket?.name ?? "").replace(/^gs:\/\//u, "");
  if (name !== `${PROJECT_ID}_cloudbuild`) {
    throw new Error("Cloud Build source bucket identity drifted");
  }
}

function reconcileBucketRoles(policy, plannedMutations) {
  const buildRoles = rolesForMember(policy, BUILD_MEMBER, "source bucket build authority");
  if (buildRoles.some((role) => !BUILD_SOURCE_BUCKET_ROLES.includes(role))) {
    throw new Error("source bucket build authority contains unexpected authority");
  }
  for (const role of BUILD_SOURCE_BUCKET_ROLES) {
    if (!buildRoles.includes(role)) {
      plannedMutations.push(bucketBindingMutation({
        id: `source-bucket-build-add-${roleSlug(role)}`,
        action: "add",
        member: BUILD_MEMBER,
        role,
        reason: "replace project-wide source read with exact Cloud Build source-bucket read",
      }));
    }
  }
  const builderRoles = rolesForMember(policy, BUILDER_MEMBER, "source bucket builder authority");
  if (builderRoles.some((role) => !BUILDER_SOURCE_BUCKET_ROLES.includes(role))) {
    throw new Error("source bucket builder authority contains unexpected authority");
  }
  for (const role of BUILDER_SOURCE_BUCKET_ROLES) {
    if (!builderRoles.includes(role)) {
      plannedMutations.push(bucketBindingMutation({
        id: `source-bucket-builder-add-${roleSlug(role)}`,
        action: "add",
        member: BUILDER_MEMBER,
        role,
        reason: "allow unique exact-source upload and readback",
      }));
    }
  }
  assertNoMemberRoles(
    policy,
    DEPLOYER_MEMBER,
    "deployer must not access the Cloud Build source bucket",
  );
  assertNoMemberRoles(
    policy,
    ROLLBACK_MEMBER,
    "rollback must not access the Cloud Build source bucket",
  );
  const commandRoles = rolesForMember(policy, COMMAND_SYNC_MEMBER, "source bucket command-sync authority");
  if (commandRoles.some((role) => !COMMAND_SYNC_REMOVABLE_LEGACY_BUCKET_ROLES.includes(role))) {
    throw new Error("source bucket command-sync authority contains unexpected authority");
  }
  for (const role of COMMAND_SYNC_REMOVABLE_LEGACY_BUCKET_ROLES) {
    if (commandRoles.includes(role)) {
      plannedMutations.push(bucketBindingMutation({
        id: `source-bucket-command-sync-remove-${roleSlug(role)}`,
        action: "remove",
        member: COMMAND_SYNC_MEMBER,
        role,
        reason: "remove retired Cloud Build source access from command sync",
      }));
    }
  }
}

function requiredSecretInventory(run) {
  const locations = requiredJson(
    run,
    ["secrets", "locations", "list", `--project=${PROJECT_ID}`, "--format=json(name)"],
    "regional Secret location catalog",
    globalSecretManagerExecution(),
  );
  if (!Array.isArray(locations) || locations.length === 0) {
    throw new Error("regional Secret location catalog is invalid or empty");
  }
  const regionalLocations = locations.map((entry) => canonicalSecretLocation(entry?.name));
  if (new Set(regionalLocations).size !== regionalLocations.length) {
    throw new Error("regional Secret location catalog contains duplicates");
  }
  const secrets = [
    ...requiredSecretCatalog(run, null),
  ];
  for (const location of regionalLocations.sort()) {
    secrets.push(...requiredSecretCatalog(run, location));
  }
  secrets.sort((left, right) => left.resourceName.localeCompare(right.resourceName));
  if (new Set(secrets.map((secret) => secret.resourceName)).size !== secrets.length) {
    throw new Error("Secret metadata inventory contains duplicates");
  }
  return secrets.map((secret) => ({
    ...secret,
    policy: requiredJson(
      run,
      secretPolicyArguments(secret),
      `Secret IAM policy (${secret.resourceName})`,
      secret.execution,
    ),
  }));
}

function requiredSecretCatalog(run, location) {
  const arguments_ = ["secrets", "list", `--project=${PROJECT_ID}`];
  if (location !== null) arguments_.push(`--location=${location}`);
  arguments_.push("--format=json(name)");
  const execution = secretManagerExecution(location);
  const catalog = requiredJson(
    run,
    arguments_,
    location === null ? "global Secret catalog" : `regional Secret catalog (${location})`,
    execution,
  );
  if (!Array.isArray(catalog)) throw new Error("Secret metadata catalog is invalid");
  return catalog.map((entry) => canonicalSecretResource(entry?.name, location, execution));
}

function canonicalSecretLocation(resourceName) {
  const patterns = [
    `projects/${PROJECT_ID}/locations/`,
    `projects/${PROJECT_NUMBER}/locations/`,
  ];
  const prefix = patterns.find((candidate) => String(resourceName ?? "").startsWith(candidate));
  const location = prefix ? String(resourceName).slice(prefix.length) : "";
  if (!/^[a-z](?:[a-z0-9-]{0,61}[a-z0-9])?$/u.test(location) || location === "global") {
    throw new Error("regional Secret location resource is invalid");
  }
  return location;
}

function canonicalSecretResource(resourceName, location, execution) {
  const prefixes = location === null
    ? [
      `projects/${PROJECT_ID}/secrets/`,
      `projects/${PROJECT_NUMBER}/secrets/`,
    ]
    : [
      `projects/${PROJECT_ID}/locations/${location}/secrets/`,
      `projects/${PROJECT_NUMBER}/locations/${location}/secrets/`,
    ];
  const prefix = prefixes.find((candidate) => String(resourceName ?? "").startsWith(candidate));
  const name = prefix ? String(resourceName).slice(prefix.length) : "";
  if (!/^[A-Za-z0-9_-]{1,255}$/u.test(name)) {
    throw new Error("Secret metadata resource is invalid");
  }
  return { resourceName: String(resourceName), name, location, execution };
}

function secretPolicyArguments(secret) {
  const arguments_ = ["secrets", "get-iam-policy", secret.name, `--project=${PROJECT_ID}`];
  if (secret.location !== null) arguments_.push(`--location=${secret.location}`);
  arguments_.push("--format=json");
  return arguments_;
}

function assertExactSecretAuthority(secrets) {
  const requiredGlobalSecretNames = [
    DISCORD_SECRET,
    JOB_SECRET,
    TELEMETRY_EVENT_SECRET,
    TELEMETRY_TRANSPORT_SECRET,
  ];
  if (requiredGlobalSecretNames.some((name) =>
    secrets.filter((secret) => secret.location === null && secret.name === name).length !== 1)) {
    throw new Error("required global Secret metadata is unavailable");
  }
  for (const secret of secrets) {
    assertNoFederatedMembers(secret.policy, `Secret IAM policy (${secret.resourceName})`);
    const expectedPolicyTuples = expectedSecretPolicyTuples(secret);
    const builderRoles = rolesForMember(secret.policy, BUILDER_MEMBER, "builder Secret authority");
    if (builderRoles.length !== 0) {
      throw new Error("builder service account must have zero direct Secret authority");
    }
    const deployerRoles = rolesForMember(secret.policy, DEPLOYER_MEMBER, "deployer Secret authority");
    if (deployerRoles.length !== 0) {
      throw new Error("deployer service account must have zero direct Secret authority");
    }
    const rollbackRoles = rolesForMember(secret.policy, ROLLBACK_MEMBER, "rollback Secret authority");
    if (rollbackRoles.length !== 0) {
      throw new Error("rollback service account must have zero direct Secret authority");
    }
    const buildRoles = rolesForMember(secret.policy, BUILD_MEMBER, "build Secret authority");
    if (buildRoles.length !== 0) {
      throw new Error("build service account must have zero direct Secret authority");
    }
    assertExactPolicyTuples(
      secret.policy,
      expectedPolicyTuples,
      `Secret IAM policy (${secret.resourceName})`,
    );
  }
}

function expectedSecretPolicyTuples(secret) {
  if (secret.location !== null) return [];
  if (secret.name === DISCORD_SECRET) {
    return [
      [COMMAND_SYNC_MEMBER, SECRET_ACCESSOR_ROLE],
      [INTERACTION_MEMBER, SECRET_ACCESSOR_ROLE],
    ];
  }
  if (secret.name === JOB_SECRET) {
    return [
      [RUNTIME_MEMBER, SECRET_ACCESSOR_ROLE],
      [INTERACTION_MEMBER, SECRET_ACCESSOR_ROLE],
    ];
  }
  if (secret.name === TELEMETRY_EVENT_SECRET) {
    return [[INTERACTION_MEMBER, SECRET_ACCESSOR_ROLE]];
  }
  if (secret.name === TELEMETRY_TRANSPORT_SECRET) {
    return [[TELEMETRY_MEMBER, SECRET_ACCESSOR_ROLE]];
  }
  return [];
}

function assertExactPolicyTuples(policy, expectedTuples, label) {
  const tupleKey = ([member, role]) => `${role}\u0000${member}`;
  const expected = new Set(expectedTuples.map(tupleKey));
  if (expected.size !== expectedTuples.length) {
    throw new Error(`${label} expected tuple set is invalid`);
  }
  const observed = new Set();
  for (const binding of policy.bindings ?? []) {
    if (
      typeof binding?.role !== "string" ||
      !Array.isArray(binding?.members) ||
      binding.members.length === 0 ||
      (binding.condition !== undefined && binding.condition !== null)
    ) {
      throw new Error(`${label} contains invalid or conditional authority`);
    }
    for (const member of binding.members) {
      const key = tupleKey([member, binding.role]);
      if (!expected.has(key) || observed.has(key)) {
        throw new Error(`${label} contains authority outside its exact closed set`);
      }
      observed.add(key);
    }
  }
  if (observed.size !== expected.size) {
    throw new Error(`${label} is missing required exact authority`);
  }
}

function rolesForMember(policy, member, label) {
  if (policy === null || typeof policy !== "object" || Array.isArray(policy)) {
    throw new Error(`${label} policy is invalid`);
  }
  const bindings = policy.bindings ?? [];
  if (!Array.isArray(bindings)) throw new Error(`${label} policy bindings are invalid`);
  const roles = [];
  for (const binding of bindings) {
    if (!Array.isArray(binding?.members) || typeof binding?.role !== "string") {
      throw new Error(`${label} policy contains an invalid binding`);
    }
    if (!binding.members.includes(member)) continue;
    if (binding.condition !== undefined && binding.condition !== null) {
      throw new Error(`${label} contains conditional authority`);
    }
    roles.push(binding.role);
  }
  if (new Set(roles).size !== roles.length) {
    throw new Error(`${label} contains duplicate authority`);
  }
  return roles.sort();
}

function projectBindingMutation({ id, action, member, role, reason }) {
  return mutation(id, reason, [
    "projects", `${action}-iam-policy-binding`, PROJECT_ID,
    `--member=${member}`,
    `--role=${role}`,
    "--condition=None",
    "--quiet",
  ]);
}

function serviceAccountBindingMutation({ id, action, email, member, role, reason }) {
  return mutation(id, reason, [
    "iam", "service-accounts", `${action}-iam-policy-binding`, email,
    `--project=${PROJECT_ID}`,
    `--member=${member}`,
    `--role=${role}`,
    "--condition=None",
    "--quiet",
  ]);
}

function bucketBindingMutation({ id, action, member, role, reason }) {
  return mutation(id, reason, [
    "storage", "buckets", `${action}-iam-policy-binding`, SOURCE_BUCKET,
    `--member=${member}`,
    `--role=${role}`,
    "--condition=None",
    "--quiet",
  ]);
}

function mutation(id, reason, argv) {
  return { id, reason, argv };
}

function orderMutations(mutations) {
  return [...mutations].sort((left, right) => {
    const phaseDifference = mutationPhase(left) - mutationPhase(right);
    if (phaseDifference !== 0) return phaseDifference;
    const wifRemovalDifference = Number(isWifRemoval(left)) - Number(isWifRemoval(right));
    if (wifRemovalDifference !== 0) return wifRemovalDifference;
    return left.id.localeCompare(right.id);
  });
}

function isWifRemoval(planned) {
  return planned.argv.includes("--role=roles/iam.workloadIdentityUser") &&
    planned.argv.some((argument) => argument.includes("remove-iam-policy-binding"));
}

function mutationPhase(planned) {
  if (planned.id === "enable-wif-services") return 0;
  if (["create-pool", "create-rollback-pool"].includes(planned.id)) return 1;
  if ([
    "create-builder",
    "create-deployer",
    "create-rollback",
    "create-deployer-runtime-role",
    "create-rollback-runtime-role",
    "remove-rollback-revision-delete-permission",
  ].includes(planned.id)) {
    return 2;
  }
  if (planned.id === "create-rollback-provider") return 6;
  if (planned.id === "create-provider") return 7;
  if (planned.argv.includes("--role=roles/iam.workloadIdentityUser")) return 5;
  if (planned.argv.some((argument) => argument.includes("remove-iam-policy-binding"))) return 4;
  return 3;
}

function roleSlug(role) {
  return role.replace(/^roles\//u, "").replace(/[^A-Za-z0-9]+/gu, "-").toLowerCase();
}

function serviceAccountName(email) {
  return `projects/${PROJECT_ID}/serviceAccounts/${email}`;
}

function requiredJson(run, arguments_, label, execution = undefined) {
  const result = run(arguments_, execution);
  if (!isSuccessful(result)) throw new Error(`${label} lookup failed`);
  try {
    return JSON.parse(String(result.stdout ?? ""));
  } catch {
    throw new Error(`${label} returned invalid JSON`);
  }
}

function isSuccessful(result) {
  return result?.status === 0 && !result?.error;
}

function secretManagerExecution(location) {
  return {
    environment: {
      [SECRET_MANAGER_ENDPOINT_ENV]: location === null
        ? GLOBAL_SECRET_MANAGER_ENDPOINT
        : `https://secretmanager.${location}.rep.googleapis.com/`,
    },
  };
}

function globalSecretManagerExecution() {
  return secretManagerExecution(null);
}

function deepFreeze(value) {
  if (value === null || typeof value !== "object" || Object.isFrozen(value)) return value;
  Object.freeze(value);
  for (const child of Object.values(value)) deepFreeze(child);
  return value;
}

function runGcloud(arguments_, execution = undefined) {
  assertClosedGcloudArguments(arguments_);
  const invocation = gcloudProcessInvocation(arguments_);
  const result = spawnSync(invocation.command, invocation.arguments, {
    encoding: "utf8",
    env: gcloudProcessEnvironment(execution),
    maxBuffer: MAX_OUTPUT_BYTES,
    shell: false,
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });
  return {
    status: result.status,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
    error: result.error ?? null,
  };
}

function assertClosedGcloudArguments(arguments_) {
  if (!Array.isArray(arguments_) || arguments_.length === 0 || arguments_.some((entry) => typeof entry !== "string")) {
    throw new Error("gcloud arguments are invalid");
  }
  if (!isAllowedGcloudArguments(arguments_)) {
    throw new Error("gcloud command is outside the metadata/IAM-only bootstrap surface");
  }
}

function isAllowedGcloudArguments(arguments_) {
  const fixed = [
    ["config", "list", "--all", "--format=json(api_endpoint_overrides)"],
    ["projects", "describe", PROJECT_ID, "--format=json"],
    ["projects", "get-ancestors", PROJECT_ID, "--format=json(id,type)"],
    ["projects", "get-iam-policy", PROJECT_ID, "--format=json"],
    ["services", "list", "--enabled", `--project=${PROJECT_ID}`, "--format=json(config.name,state)"],
    [
      "iam", "workload-identity-pools", "list",
      `--project=${PROJECT_ID}`, "--location=global", "--show-deleted", "--format=json",
    ],
    [
      "iam", "workload-identity-pools", "providers", "list",
      `--project=${PROJECT_ID}`, "--location=global", `--workload-identity-pool=${POOL_ID}`,
      "--show-deleted", "--format=json",
    ],
    [
      "iam", "workload-identity-pools", "providers", "list",
      `--project=${PROJECT_ID}`, "--location=global", `--workload-identity-pool=${ROLLBACK_POOL_ID}`,
      "--show-deleted", "--format=json",
    ],
    ["iam", "service-accounts", "list", `--project=${PROJECT_ID}`, "--format=json"],
    ["iam", "roles", "list", `--project=${PROJECT_ID}`, "--show-deleted", "--format=json"],
    [
      "iam", "roles", "describe", DEPLOYER_RUN_ROLE_ID,
      `--project=${PROJECT_ID}`, "--format=json",
    ],
    [
      "iam", "roles", "describe", ROLLBACK_RUN_ROLE_ID,
      `--project=${PROJECT_ID}`, "--format=json",
    ],
    [
      "artifacts", "repositories", "describe", ARTIFACT_REPOSITORY,
      `--project=${PROJECT_ID}`, `--location=${REGION}`, "--format=json",
    ],
    [
      "artifacts", "repositories", "get-iam-policy", ARTIFACT_REPOSITORY,
      `--project=${PROJECT_ID}`, `--location=${REGION}`, "--format=json",
    ],
    ["storage", "buckets", "describe", SOURCE_BUCKET, "--format=json"],
    ["storage", "buckets", "get-iam-policy", SOURCE_BUCKET, "--format=json"],
    ["secrets", "locations", "list", `--project=${PROJECT_ID}`, "--format=json(name)"],
    ["secrets", "list", `--project=${PROJECT_ID}`, "--format=json(name)"],
    [
      "iam", "workload-identity-pools", "create", POOL_ID,
      `--project=${PROJECT_ID}`, "--location=global", `--display-name=${POOL_DISPLAY_NAME}`,
      `--description=${POOL_DESCRIPTION}`, "--quiet",
    ],
    createRollbackPoolMutation().argv,
    createProviderMutation().argv,
    createRollbackProviderMutation().argv,
    createDeployerRunRoleMutation().argv,
    createRollbackRunRoleMutation().argv,
    updateRollbackRunRoleMutation().argv,
  ];
  for (const email of [
    BUILD_EMAIL,
    RUNTIME_EMAIL,
    COMMAND_SYNC_EMAIL,
    BUILDER_EMAIL,
    DEPLOYER_EMAIL,
    ROLLBACK_EMAIL,
  ]) {
    fixed.push([
      "iam", "service-accounts", "get-iam-policy", email,
      `--project=${PROJECT_ID}`, "--format=json",
    ]);
  }
  for (const email of [
    BUILD_EMAIL,
    RUNTIME_EMAIL,
    COMMAND_SYNC_EMAIL,
    BUILDER_EMAIL,
    DEPLOYER_EMAIL,
    ROLLBACK_EMAIL,
  ]) {
    fixed.push([
      "iam", "service-accounts", "keys", "list",
      `--iam-account=${email}`, `--project=${PROJECT_ID}`, "--managed-by=user", "--format=json",
    ]);
  }
  for (const [id, displayName, description] of [
    [BUILDER_ID, BUILDER_DISPLAY_NAME, BUILDER_DESCRIPTION],
    [DEPLOYER_ID, DEPLOYER_DISPLAY_NAME, DEPLOYER_DESCRIPTION],
    [ROLLBACK_ID, ROLLBACK_DISPLAY_NAME, ROLLBACK_DESCRIPTION],
  ]) {
    fixed.push([
      "iam", "service-accounts", "create", id,
      `--project=${PROJECT_ID}`, `--display-name=${displayName}`, `--description=${description}`,
      "--quiet",
    ]);
  }
  for (const role of BUILD_PROJECT_ROLES) {
    fixed.push(projectBindingMutation({
      id: "validator", action: "add", member: BUILD_MEMBER, role, reason: "validator",
    }).argv);
  }
  for (const role of BUILD_REMOVABLE_LEGACY_PROJECT_ROLES) {
    fixed.push(projectBindingMutation({
      id: "validator", action: "remove", member: BUILD_MEMBER, role, reason: "validator",
    }).argv);
  }
  for (const role of BUILDER_PROJECT_ROLES) {
    fixed.push(projectBindingMutation({
      id: "validator", action: "add", member: BUILDER_MEMBER, role, reason: "validator",
    }).argv);
  }
  for (const role of DEPLOYER_PROJECT_ROLES) {
    fixed.push(projectBindingMutation({
      id: "validator", action: "add", member: DEPLOYER_MEMBER, role, reason: "validator",
    }).argv);
  }
  for (const role of DEPLOYER_REMOVABLE_LEGACY_PROJECT_ROLES) {
    fixed.push(projectBindingMutation({
      id: "validator", action: "remove", member: DEPLOYER_MEMBER, role, reason: "validator",
    }).argv);
  }
  for (const role of ROLLBACK_PROJECT_ROLES) {
    fixed.push(projectBindingMutation({
      id: "validator", action: "add", member: ROLLBACK_MEMBER, role, reason: "validator",
    }).argv);
  }
  for (const role of COMMAND_SYNC_PROJECT_ROLES) {
    fixed.push(projectBindingMutation({
      id: "validator", action: "add", member: COMMAND_SYNC_MEMBER, role, reason: "validator",
    }).argv);
  }
  for (const role of COMMAND_SYNC_REMOVABLE_LEGACY_PROJECT_ROLES) {
    fixed.push(projectBindingMutation({
      id: "validator", action: "remove", member: COMMAND_SYNC_MEMBER, role, reason: "validator",
    }).argv);
  }
  fixed.push(serviceAccountBindingMutation({
    id: "validator", action: "add", email: BUILD_EMAIL, member: BUILDER_MEMBER,
    role: "roles/iam.serviceAccountUser", reason: "validator",
  }).argv);
  fixed.push(serviceAccountBindingMutation({
    id: "validator", action: "add", email: RUNTIME_EMAIL, member: DEPLOYER_MEMBER,
    role: "roles/iam.serviceAccountUser", reason: "validator",
  }).argv);
  for (const [email, members] of [
    [BUILDER_EMAIL, BUILDER_WIF_MEMBERS],
    [DEPLOYER_EMAIL, DEPLOYER_WIF_MEMBERS],
    [ROLLBACK_EMAIL, ROLLBACK_WIF_MEMBERS],
    [COMMAND_SYNC_EMAIL, COMMAND_SYNC_WIF_MEMBERS],
  ]) {
    for (const member of members) {
      fixed.push(serviceAccountBindingMutation({
        id: "validator", action: "add", email, member,
        role: "roles/iam.workloadIdentityUser", reason: "validator",
      }).argv);
    }
  }
  for (const [email, members] of [
    [BUILDER_EMAIL, BUILDER_REMOVABLE_LEGACY_WIF_MEMBERS],
    [DEPLOYER_EMAIL, DEPLOYER_REMOVABLE_LEGACY_WIF_MEMBERS],
    [ROLLBACK_EMAIL, ROLLBACK_REMOVABLE_LEGACY_WIF_MEMBERS],
    [COMMAND_SYNC_EMAIL, COMMAND_SYNC_REMOVABLE_LEGACY_WIF_MEMBERS],
  ]) {
    for (const member of members) {
      fixed.push(serviceAccountBindingMutation({
        id: "validator", action: "remove", email, member,
        role: "roles/iam.workloadIdentityUser", reason: "validator",
      }).argv);
    }
  }
  fixed.push([
    "artifacts", "repositories", "add-iam-policy-binding", ARTIFACT_REPOSITORY,
    `--project=${PROJECT_ID}`, `--location=${REGION}`, `--member=${BUILD_MEMBER}`,
    "--role=roles/artifactregistry.writer", "--condition=None", "--quiet",
  ]);
  fixed.push([
    "artifacts", "repositories", "add-iam-policy-binding", ARTIFACT_REPOSITORY,
    `--project=${PROJECT_ID}`, `--location=${REGION}`, `--member=${BUILDER_MEMBER}`,
    "--role=roles/artifactregistry.reader", "--condition=None", "--quiet",
  ]);
  fixed.push([
    "artifacts", "repositories", "add-iam-policy-binding", ARTIFACT_REPOSITORY,
    `--project=${PROJECT_ID}`, `--location=${REGION}`, `--member=${DEPLOYER_MEMBER}`,
    "--role=roles/artifactregistry.reader", "--condition=None", "--quiet",
  ]);
  for (const role of BUILDER_SOURCE_BUCKET_ROLES) {
    fixed.push(bucketBindingMutation({
      id: "validator", action: "add", member: BUILDER_MEMBER, role, reason: "validator",
    }).argv);
  }
  for (const role of BUILD_SOURCE_BUCKET_ROLES) {
    fixed.push(bucketBindingMutation({
      id: "validator", action: "add", member: BUILD_MEMBER, role, reason: "validator",
    }).argv);
  }
  for (const role of COMMAND_SYNC_REMOVABLE_LEGACY_BUCKET_ROLES) {
    fixed.push(bucketBindingMutation({
      id: "validator", action: "remove", member: COMMAND_SYNC_MEMBER, role, reason: "validator",
    }).argv);
  }
  if (fixed.some((candidate) => exactArgv(arguments_, candidate))) return true;

  if (
    arguments_.length === 6 &&
    arguments_[0] === "iam" &&
    arguments_[1] === "service-accounts" &&
    arguments_[2] === "get-iam-policy" &&
    isCanonicalProjectServiceAccountEmail(arguments_[3]) &&
    arguments_[4] === `--project=${PROJECT_ID}` &&
    arguments_[5] === "--format=json"
  ) return true;

  if (
    arguments_[0] === "services" &&
    arguments_[1] === "enable" &&
    arguments_.at(-2) === `--project=${PROJECT_ID}` &&
    arguments_.at(-1) === "--quiet"
  ) {
    const services = arguments_.slice(2, -2);
    return services.length > 0 &&
      new Set(services).size === services.length &&
      services.every((service) => MANAGED_WIF_SERVICES.includes(service));
  }

  const locationPattern = /^--location=([a-z](?:[a-z0-9-]{0,61}[a-z0-9])?)$/u;
  if (
    arguments_.length === 5 &&
    arguments_[0] === "secrets" &&
    arguments_[1] === "list" &&
    arguments_[2] === `--project=${PROJECT_ID}` &&
    locationPattern.test(arguments_[3]) &&
    arguments_[4] === "--format=json(name)"
  ) return true;

  const secretPattern = /^[A-Za-z0-9_-]{1,255}$/u;
  if (
    arguments_.length === 5 &&
    arguments_[0] === "secrets" &&
    arguments_[1] === "get-iam-policy" &&
    secretPattern.test(arguments_[2]) &&
    arguments_[3] === `--project=${PROJECT_ID}` &&
    arguments_[4] === "--format=json"
  ) return true;
  return arguments_.length === 6 &&
    arguments_[0] === "secrets" &&
    arguments_[1] === "get-iam-policy" &&
    secretPattern.test(arguments_[2]) &&
    arguments_[3] === `--project=${PROJECT_ID}` &&
    locationPattern.test(arguments_[4]) &&
    arguments_[5] === "--format=json";
}

function exactArgv(left, right) {
  return left.length === right.length && left.every((entry, index) => entry === right[index]);
}

export function gcloudProcessEnvironment(execution, ambientEnvironment = process.env) {
  if (
    execution !== undefined &&
    (
      execution === null ||
      typeof execution !== "object" ||
      Array.isArray(execution) ||
      Object.keys(execution).length !== 1 ||
      !Object.hasOwn(execution, "environment") ||
      execution.environment === null ||
      typeof execution.environment !== "object" ||
      Array.isArray(execution.environment)
    )
  ) {
    throw new Error("gcloud execution environment is invalid");
  }
  if (
    ambientEnvironment === null ||
    typeof ambientEnvironment !== "object" ||
    Array.isArray(ambientEnvironment)
  ) {
    throw new Error("ambient gcloud environment is invalid");
  }
  const ambientEndpointOverride = Object.keys(ambientEnvironment).find((key) =>
    key.toUpperCase().startsWith("CLOUDSDK_API_ENDPOINT_OVERRIDES_"));
  if (ambientEndpointOverride !== undefined) {
    throw new Error("ambient gcloud API endpoint overrides are forbidden");
  }
  const overrides = execution?.environment ?? {};
  const keys = Object.keys(overrides);
  if (
    execution !== undefined &&
    (keys.length !== 1 || keys[0] !== SECRET_MANAGER_ENDPOINT_ENV)
  ) {
    throw new Error("gcloud execution environment contains an unsupported override");
  }
  const environment = { ...ambientEnvironment };
  if (execution !== undefined) {
    const endpoint = overrides[SECRET_MANAGER_ENDPOINT_ENV];
    if (
      endpoint !== GLOBAL_SECRET_MANAGER_ENDPOINT &&
      !/^https:\/\/secretmanager\.[a-z](?:[a-z0-9-]{0,61}[a-z0-9])?\.rep\.googleapis\.com\/$/u.test(endpoint)
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
  assertClosedGcloudArguments(arguments_);
  if (platform === "win32") {
    return Object.freeze({
      command: environment.ComSpec || environment.COMSPEC || "cmd.exe",
      arguments: Object.freeze(["/d", "/s", "/c", "gcloud.cmd", ...arguments_]),
    });
  }
  return Object.freeze({ command: "gcloud", arguments: Object.freeze([...arguments_]) });
}

async function main() {
  const { positionals } = parseArgs({ allowPositionals: true, strict: true });
  if (positionals.length !== 1 || !["apply", "audit", "plan"].includes(positionals[0])) {
    throw new Error("usage: github-wif-bootstrap.mjs (audit|plan|apply)");
  }
  const mode = positionals[0];
  const report = mode === "apply"
    ? await applyGitHubWifBootstrap()
    : await createGitHubWifBootstrapPlan();
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  if (mode === "audit" && report.status !== "ready") process.exitCode = 3;
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  try {
    await main();
  } catch (error) {
    process.stderr.write("github_wif_bootstrap=failed\n");
    process.stderr.write(
      `github_wif_bootstrap_diagnostic=${JSON.stringify(githubWifBootstrapFailureDiagnostic(error))}\n`,
    );
    process.exitCode = 2;
  }
}
