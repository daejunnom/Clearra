import assert from "node:assert/strict";
import test from "node:test";

import {
  GITHUB_WIF_BOOTSTRAP_CONTRACT,
  applyGitHubWifBootstrap,
  createGitHubWifBootstrapPlan,
  gcloudProcessEnvironment,
  gcloudProcessInvocation,
  githubWifBootstrapFailureDiagnostic,
} from "./github-wif-bootstrap.mjs";

const PROJECT_ID = "clearra-cloud";
const PROJECT_NUMBER = "50060711800";
const POOL_ID = "clearra-github";
const PROVIDER_ID = "clearra-main";
const POOL_NAME = `projects/${PROJECT_NUMBER}/locations/global/workloadIdentityPools/${POOL_ID}`;
const PROVIDER_NAME = `${POOL_NAME}/providers/${PROVIDER_ID}`;
const ROLLBACK_POOL_ID = "clearra-github-rollback";
const ROLLBACK_PROVIDER_ID = "clearra-runtime-rollback";
const ROLLBACK_POOL_NAME =
  `projects/${PROJECT_NUMBER}/locations/global/workloadIdentityPools/${ROLLBACK_POOL_ID}`;
const ROLLBACK_PROVIDER_NAME = `${ROLLBACK_POOL_NAME}/providers/${ROLLBACK_PROVIDER_ID}`;
const BUILDER_EMAIL = `clearra-github-builder@${PROJECT_ID}.iam.gserviceaccount.com`;
const DEPLOYER_EMAIL = `clearra-github-deployer@${PROJECT_ID}.iam.gserviceaccount.com`;
const ROLLBACK_EMAIL = `clearra-github-rollback@${PROJECT_ID}.iam.gserviceaccount.com`;
const COMMAND_EMAIL = `clearra-command-sync@${PROJECT_ID}.iam.gserviceaccount.com`;
const BUILD_EMAIL = `clearra-build@${PROJECT_ID}.iam.gserviceaccount.com`;
const RUNTIME_EMAIL = `clearra-current-job@${PROJECT_ID}.iam.gserviceaccount.com`;
const EXTRA_EMAIL = `clearra-interaction@${PROJECT_ID}.iam.gserviceaccount.com`;
const TELEMETRY_EMAIL = `clearra-telemetry-relay@${PROJECT_ID}.iam.gserviceaccount.com`;
const JOB_RUNNER_EMAIL = `clearra-job-runner@${PROJECT_ID}.iam.gserviceaccount.com`;
const DEFAULT_COMPUTE_EMAIL = `${PROJECT_NUMBER}-compute@developer.gserviceaccount.com`;
const PRIMARY_HUMAN_MEMBER = "user:daejun0311@gmail.com";
const LEGACY_BUILD_USER_MEMBER = "user:stemxstudioproject@gmail.com";
const BUILD_MEMBER = `serviceAccount:${BUILD_EMAIL}`;
const RUNTIME_MEMBER = `serviceAccount:${RUNTIME_EMAIL}`;
const EXTRA_MEMBER = `serviceAccount:${EXTRA_EMAIL}`;
const BUILDER_MEMBER = `serviceAccount:${BUILDER_EMAIL}`;
const DEPLOYER_MEMBER = `serviceAccount:${DEPLOYER_EMAIL}`;
const ROLLBACK_MEMBER = `serviceAccount:${ROLLBACK_EMAIL}`;
const COMMAND_MEMBER = `serviceAccount:${COMMAND_EMAIL}`;
const WIF_PREFIX = `principal://iam.googleapis.com/${POOL_NAME}/subject/`;
const ROLLBACK_WIF_PREFIX = `principal://iam.googleapis.com/${ROLLBACK_POOL_NAME}/subject/`;
const BUILDER_WIF_MEMBER = `${WIF_PREFIX}repo:daejunnom/Clearra:ref:refs/heads/main`;
const PATH_WIF_MEMBER =
  `${WIF_PREFIX}repo:daejunnom/Clearra:environment:discord-path-confirmation`;
const PRIMARY_RUNTIME_ROLLBACK_WIF_MEMBER =
  `${WIF_PREFIX}repo:daejunnom/Clearra:environment:discord-runtime-rollback`;
const RECOVERY_RUNTIME_ROLLBACK_WIF_MEMBER =
  `${ROLLBACK_WIF_PREFIX}repo:daejunnom/Clearra:environment:discord-runtime-rollback`;
const GLOBAL_SYNC_WIF_MEMBER =
  `${WIF_PREFIX}repo:daejunnom/Clearra:environment:discord-global-command-sync`;
const SOURCE_BUCKET = `gs://${PROJECT_ID}_cloudbuild`;
const ACCESSOR = "roles/secretmanager.secretAccessor";
const SECRET_ENDPOINT_ENV = "CLOUDSDK_API_ENDPOINT_OVERRIDES_SECRETMANAGER";
const DEPLOYER_RUN_ROLE_ID = "clearraGithubRuntimeDeployer";
const DEPLOYER_RUN_ROLE = `projects/${PROJECT_ID}/roles/${DEPLOYER_RUN_ROLE_ID}`;
const DEPLOYER_RUN_PERMISSIONS = Object.freeze([
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
const ROLLBACK_RUN_ROLE = `projects/${PROJECT_ID}/roles/${ROLLBACK_RUN_ROLE_ID}`;
const ROLLBACK_RUN_PERMISSIONS = Object.freeze([
  "run.operations.get",
  "run.revisions.get",
  "run.revisions.list",
  "run.services.get",
  "run.services.update",
]);

const REQUIRED_SERVICES = Object.freeze([
  "artifactregistry.googleapis.com",
  "cloudbuild.googleapis.com",
  "iam.googleapis.com",
  "logging.googleapis.com",
  "policytroubleshooter.googleapis.com",
  "run.googleapis.com",
  "secretmanager.googleapis.com",
  "serviceusage.googleapis.com",
]);
const WIF_SERVICES = Object.freeze([
  "iamcredentials.googleapis.com",
  "sts.googleapis.com",
]);
const BUILDER_PROJECT_ROLES = Object.freeze([
  "roles/cloudbuild.builds.editor",
  "roles/logging.viewer",
  "roles/serviceusage.serviceUsageConsumer",
]);
const DEPLOYER_PROJECT_ROLES = Object.freeze([
  DEPLOYER_RUN_ROLE,
  "roles/logging.viewer",
  "roles/serviceusage.serviceUsageConsumer",
]);
const BUILD_PROJECT_ROLES = Object.freeze(["roles/logging.logWriter"]);
const ROLLBACK_PROJECT_ROLES = Object.freeze([
  ROLLBACK_RUN_ROLE,
  "roles/serviceusage.serviceUsageConsumer",
]);

test("plan is exact-bound, keyless, Secret-free for deployer, and emits only non-secret variables", async () => {
  const cli = new FakeGcloud();
  const report = await plan(cli);

  assert.equal(report.contract, GITHUB_WIF_BOOTSTRAP_CONTRACT);
  assert.equal(report.status, "changes-required");
  for (const mutation of report.plannedMutations) {
    const displayName = mutation.argv.find((argument) =>
      argument.startsWith("--display-name="),
    );
    if (displayName !== undefined) {
      assert.ok(
        [...displayName.slice("--display-name=".length)].length <= 32,
        `${mutation.id} exceeds the GCP display-name limit`,
      );
    }
  }
  assert.deepEqual(report.exactBinding, {
    projectId: PROJECT_ID,
    projectNumber: PROJECT_NUMBER,
    repository: "daejunnom/Clearra",
    repositoryId: "1309293231",
    repositoryOwnerId: "271715321",
    ref: "refs/heads/main",
    workflowRef: "daejunnom/Clearra/.github/workflows/discord-deploy.yml@refs/heads/main",
    rollbackWorkflowRef:
      "daejunnom/Clearra/.github/workflows/discord-deploy-recovery.yml@refs/heads/main",
    subjects: [
      "repo:daejunnom/Clearra:ref:refs/heads/main",
      "repo:daejunnom/Clearra:environment:discord-path-confirmation",
      "repo:daejunnom/Clearra:environment:discord-runtime-rollback",
      "repo:daejunnom/Clearra:environment:discord-global-command-sync",
    ],
    subjectBindings: {
      builderServiceAccount: ["repo:daejunnom/Clearra:ref:refs/heads/main"],
      deployerServiceAccount: ["repo:daejunnom/Clearra:environment:discord-path-confirmation"],
      rollbackServiceAccount: [
        "repo:daejunnom/Clearra:environment:discord-runtime-rollback",
      ],
      commandSyncServiceAccount: [
        "repo:daejunnom/Clearra:environment:discord-global-command-sync",
        "repo:daejunnom/Clearra:environment:discord-runtime-rollback",
      ],
    },
    principalBindings: {
      builderServiceAccount: [BUILDER_WIF_MEMBER],
      deployerServiceAccount: [PATH_WIF_MEMBER],
      rollbackServiceAccount: [RECOVERY_RUNTIME_ROLLBACK_WIF_MEMBER],
      commandSyncServiceAccount: [
        GLOBAL_SYNC_WIF_MEMBER,
        RECOVERY_RUNTIME_ROLLBACK_WIF_MEMBER,
      ],
    },
    workloadIdentityPool: POOL_NAME,
    workloadIdentityProvider: PROVIDER_NAME,
    rollbackWorkloadIdentityPool: ROLLBACK_POOL_NAME,
    rollbackWorkloadIdentityProvider: ROLLBACK_PROVIDER_NAME,
  });
  assert.deepEqual(report.githubEnvironmentProtection, {
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
  });
  assert.deepEqual(report.leastPrivilege.builderProjectRoles, [...BUILDER_PROJECT_ROLES]);
  assert.deepEqual(report.leastPrivilege.deployerProjectRoles, [...DEPLOYER_PROJECT_ROLES]);
  assert.deepEqual(report.leastPrivilege.buildProjectRoles, [...BUILD_PROJECT_ROLES]);
  assert.deepEqual(report.leastPrivilege.buildForbiddenProjectRoles, ["roles/storage.objectViewer"]);
  assert.deepEqual(report.leastPrivilege.buildArtifactRepositoryRoles, ["roles/artifactregistry.writer"]);
  assert.deepEqual(report.leastPrivilege.buildSourceBucketRoles, ["roles/storage.objectViewer"]);
  assert.deepEqual(report.leastPrivilege.rollbackProjectRoles, [...ROLLBACK_PROJECT_ROLES]);
  assert.deepEqual(report.leastPrivilege.rollbackCloudRunPermissions, [...ROLLBACK_RUN_PERMISSIONS]);
  assert.deepEqual(report.leastPrivilege.rollbackArtifactRepositoryRoles, []);
  assert.deepEqual(report.leastPrivilege.rollbackRuntimeServiceAccountRoles, []);
  assert.equal(report.leastPrivilege.rollbackHasJobLifecyclePermissions, false);
  assert.equal(report.leastPrivilege.catalogWideUnmodeledImpersonationAllowed, false);
  assert.deepEqual(report.leastPrivilege.exactGlobalSecretAccessorServiceAccounts, {
    "clearra-job-token": [RUNTIME_EMAIL, EXTRA_EMAIL],
    "discord-bot-token": [COMMAND_EMAIL, EXTRA_EMAIL],
    "clearra-telemetry-event-key": [EXTRA_EMAIL],
    "clearra-telemetry-transport-key": [TELEMETRY_EMAIL],
  });
  assert.deepEqual(
    Object.keys(report.leastPrivilege.trustedHumanServiceAccountBindings).sort(),
    [
      BUILD_EMAIL,
      COMMAND_EMAIL,
      DEFAULT_COMPUTE_EMAIL,
      EXTRA_EMAIL,
      JOB_RUNNER_EMAIL,
      RUNTIME_EMAIL,
      TELEMETRY_EMAIL,
    ].sort(),
  );
  assert.deepEqual(report.leastPrivilege.trustedHumanServiceAccountBindings[RUNTIME_EMAIL], []);
  assert.deepEqual(
    report.leastPrivilege.trustedHumanServiceAccountBindings[EXTRA_EMAIL],
    [
      { member: PRIMARY_HUMAN_MEMBER, role: "roles/iam.serviceAccountAdmin" },
      { member: PRIMARY_HUMAN_MEMBER, role: "roles/owner" },
      { member: LEGACY_BUILD_USER_MEMBER, role: "roles/iam.serviceAccountUser" },
    ],
  );
  assert.equal(report.leastPrivilege.builderHasRunAdminRole, false);
  assert.equal(report.leastPrivilege.builderHasRuntimeActAsRole, false);
  assert.equal(report.leastPrivilege.builderHasSecretManagerRole, false);
  assert.equal(report.leastPrivilege.deployerHasBuildActAsRole, false);
  assert.equal(report.leastPrivilege.deployerHasCloudBuildRole, false);
  assert.equal(report.leastPrivilege.deployerHasSecretManagerRole, false);
  assert.deepEqual(report.leastPrivilege.deployerArtifactRepositoryRoles, [
    "roles/artifactregistry.reader",
  ]);
  assert.equal(report.leastPrivilege.deployerCloudRunCustomRole, DEPLOYER_RUN_ROLE);
  assert.deepEqual(
    report.leastPrivilege.deployerCloudRunPermissions,
    [...DEPLOYER_RUN_PERMISSIONS],
  );
  assert.equal(report.leastPrivilege.deployerProjectRoles.includes("roles/run.admin"), false);
  assert.match(
    report.leastPrivilege.officialPermissionReferences.cloudRunDeployment,
    /^https:\/\/cloud\.google\.com\/run\/docs\//u,
  );
  assert.deepEqual(report.reconciliation, {
    maximumPropagationMillisecondsPerMutation: 300_000,
    backoffMilliseconds: [1_000, 2_000, 4_000, 8_000, 16_000, 30_000],
    fullSecretBoundaryAudit: "initial-and-final",
    phaseCount: 8,
    strategy: "phase-batched-fast-plan-boundaries",
  });
  assert.equal(report.observations.secretBoundary, "four-global-secrets-exact-accessor-sets");
  assert.deepEqual(report.leastPrivilege.workloadIdentityUserPrincipals, [
    BUILDER_WIF_MEMBER,
    PATH_WIF_MEMBER,
    RECOVERY_RUNTIME_ROLLBACK_WIF_MEMBER,
    GLOBAL_SYNC_WIF_MEMBER,
  ]);
  assert.equal(
    report.leastPrivilege.workloadIdentityUserPrincipals.every((member) =>
      member.startsWith("principal://iam.googleapis.com/projects/50060711800/locations/global/")),
    true,
  );
  assert.equal(
    report.leastPrivilege.deployerProjectRoles.some((role) => role.includes("secretmanager")),
    false,
  );
  assert.equal(
    report.githubRepositoryVariables.GCP_BUILD_SERVICE_ACCOUNT,
    BUILDER_EMAIL,
  );
  assert.equal(
    report.githubRepositoryVariables.GCP_WORKLOAD_IDENTITY_PROVIDER,
    PROVIDER_NAME,
  );
  assert.equal(
    report.githubRepositoryVariables.GCP_DEPLOY_SERVICE_ACCOUNT,
    DEPLOYER_EMAIL,
  );
  assert.equal(
    report.githubRepositoryVariables.GCP_ROLLBACK_SERVICE_ACCOUNT,
    ROLLBACK_EMAIL,
  );
  assert.equal(
    report.githubRepositoryVariables.GCP_ROLLBACK_WORKLOAD_IDENTITY_PROVIDER,
    ROLLBACK_PROVIDER_NAME,
  );
  assert.deepEqual(Object.keys(report.githubRepositoryVariables).sort(), [
    "GCP_BUILD_SERVICE_ACCOUNT",
    "GCP_COMMAND_SYNC_SERVICE_ACCOUNT",
    "GCP_DEPLOY_SERVICE_ACCOUNT",
    "GCP_PROJECT_ID",
    "GCP_PROJECT_NUMBER",
    "GCP_REGION",
    "GCP_ROLLBACK_SERVICE_ACCOUNT",
    "GCP_ROLLBACK_WORKLOAD_IDENTITY_PROVIDER",
    "GCP_WORKLOAD_IDENTITY_PROVIDER",
  ]);

  const ids = new Set(report.plannedMutations.map(({ id }) => id));
  for (const expected of [
    "enable-wif-services",
    "create-pool",
    "create-provider",
    "create-rollback-pool",
    "create-rollback-provider",
    "create-builder",
    "create-deployer",
    "create-rollback",
    "create-deployer-runtime-role",
    "create-rollback-runtime-role",
    "github-builder-wif-add-1",
    "github-deployer-wif-add-1",
    "github-rollback-wif-add-1",
    "github-command-sync-wif-add-environment-global-command-sync",
    "command-sync-project-add-run-viewer",
    "command-sync-project-remove-logging-logwriter",
    "source-bucket-command-sync-remove-storage-objectviewer",
    "source-bucket-build-add-storage-objectviewer",
    "build-project-remove-storage-objectviewer",
    "artifact-repository-deployer-add-reader",
  ]) {
    assert.equal(ids.has(expected), true, `missing mutation ${expected}`);
  }
  const provider = report.plannedMutations.find(({ id }) => id === "create-provider");
  assert.ok(provider.argv.includes(
    "--attribute-condition=assertion.repository == 'daejunnom/Clearra' && assertion.repository_id == '1309293231' && assertion.repository_owner_id == '271715321' && assertion.ref == 'refs/heads/main' && assertion.workflow_ref == 'daejunnom/Clearra/.github/workflows/discord-deploy.yml@refs/heads/main'",
  ));
  const orderedIds = report.plannedMutations.map(({ id }) => id);
  const firstWifIndex = report.plannedMutations.findIndex(({ argv }) =>
    argv.includes("--role=roles/iam.workloadIdentityUser"));
  const providerIndex = orderedIds.indexOf("create-provider");
  const rollbackProviderIndex = orderedIds.indexOf("create-rollback-provider");
  assert.ok(orderedIds.indexOf("source-bucket-command-sync-remove-storage-objectviewer") < firstWifIndex);
  assert.ok(orderedIds.indexOf("artifact-repository-deployer-add-reader") < firstWifIndex);
  assert.ok(
    orderedIds.indexOf("source-bucket-build-add-storage-objectviewer") <
      orderedIds.indexOf("build-project-remove-storage-objectviewer"),
  );
  assert.ok(orderedIds.indexOf("build-project-remove-storage-objectviewer") < firstWifIndex);
  assert.ok(firstWifIndex < providerIndex);
  assert.ok(firstWifIndex < rollbackProviderIndex);
  assert.deepEqual(orderedIds.slice(-2), ["create-rollback-provider", "create-provider"]);
  assert.ok(provider.argv.includes(
    "--attribute-mapping=attribute.ref=assertion.ref,attribute.repository=assertion.repository,attribute.repository_id=assertion.repository_id,attribute.repository_owner_id=assertion.repository_owner_id,attribute.workflow_ref=assertion.workflow_ref,google.subject=assertion.sub",
  ));

  const allCommands = [
    ...cli.calls.map(({ argv }) => argv.join(" ")),
    ...report.plannedMutations.map(({ argv }) => argv.join(" ")),
  ];
  assert.equal(allCommands.some((command) => command.includes("secrets versions")), false);
  assert.equal(allCommands.some((command) => command.includes("keys create")), false);
  assert.equal(allCommands.some((command) => command.includes("print-access-token")), false);
  assert.equal(
    report.plannedMutations.some(({ argv }) =>
      argv.some((argument) => argument.includes("secretmanager")) &&
      argv.some((argument) => argument.includes(DEPLOYER_MEMBER))),
    false,
  );
  for (const planned of report.plannedMutations) {
    assert.doesNotThrow(() => gcloudProcessInvocation(planned.argv, "linux"));
  }
  for (const { argv, execution } of cli.calls) {
    assert.doesNotThrow(() => gcloudProcessInvocation(argv, "linux"));
    assert.doesNotThrow(() => gcloudProcessEnvironment(execution, {}));
  }
});

test("apply converges once and the second plan is mutation-free", async () => {
  const cli = new FakeGcloud();
  const applied = await applyGitHubWifBootstrap({}, { runGcloud: cli.run.bind(cli) });

  assert.equal(applied.status, "ready");
  assert.equal(applied.plannedMutations.length, 0);
  const mutationCount = cli.mutationCount;
  assert.ok(mutationCount > 0);
  assert.equal(
    cli.calls.filter(({ argv }) =>
      argv.join(" ") === `secrets locations list --project=${PROJECT_ID} --format=json(name)`).length,
    2,
  );
  assert.equal(
    cli.calls.filter(({ argv }) =>
      argv.join(" ") === "config list --all --format=json(api_endpoint_overrides)").length,
    10,
    "34 immediately visible mutations must use one initial full plan, eight fast phase readbacks, and one final full audit",
  );
  const commands = cli.calls.map(({ argv }) => argv.join(" "));
  const planStart = "config list --all --format=json(api_endpoint_overrides)";
  const assertObservedBetween = (beforePrefix, afterPrefix, label) => {
    const before = commands.findIndex((command) => command.startsWith(beforePrefix));
    const after = commands.findIndex((command) => command.startsWith(afterPrefix));
    assert.ok(before >= 0 && after > before, `${label}: mutation order is invalid`);
    assert.ok(
      commands.slice(before + 1, after).includes(planStart),
      `${label}: complete fast readback is missing`,
    );
  };
  assertObservedBetween(
    "services enable ",
    `iam workload-identity-pools create ${POOL_ID} `,
    "managed APIs before pools",
  );
  assertObservedBetween(
    `iam workload-identity-pools create ${ROLLBACK_POOL_ID} `,
    "iam service-accounts create clearra-github-builder ",
    "pools before service accounts and roles",
  );
  assertObservedBetween(
    `iam roles create ${ROLLBACK_RUN_ROLE_ID} `,
    "artifacts repositories add-iam-policy-binding clearra ",
    "service accounts and roles before non-WIF grants",
  );
  assertObservedBetween(
    `storage buckets add-iam-policy-binding ${SOURCE_BUCKET} --member=${BUILD_MEMBER} `,
    `projects remove-iam-policy-binding ${PROJECT_ID} --member=${BUILD_MEMBER} `,
    "replacement source-bucket read before project-wide read removal",
  );
  assertObservedBetween(
    `storage buckets remove-iam-policy-binding ${SOURCE_BUCKET} --member=${COMMAND_MEMBER} `,
    `iam service-accounts add-iam-policy-binding ${BUILDER_EMAIL} `,
    "legacy removals before federation bindings",
  );
  assertObservedBetween(
    `iam service-accounts add-iam-policy-binding ${ROLLBACK_EMAIL} `,
    `iam workload-identity-pools providers create-oidc ${ROLLBACK_PROVIDER_ID} `,
    "federation bindings before recovery-provider activation",
  );
  assertObservedBetween(
    `iam workload-identity-pools providers create-oidc ${ROLLBACK_PROVIDER_ID} `,
    `iam workload-identity-pools providers create-oidc ${PROVIDER_ID} `,
    "recovery provider before primary-provider activation",
  );

  const repeated = await plan(cli);
  assert.equal(repeated.status, "ready");
  assert.equal(repeated.plannedMutations.length, 0);
  assert.equal(cli.mutationCount, mutationCount);
  assert.deepEqual(cli.projectRoles(BUILD_MEMBER), [...BUILD_PROJECT_ROLES]);
  assert.deepEqual(cli.projectRoles(BUILDER_MEMBER), [...BUILDER_PROJECT_ROLES].sort());
  assert.deepEqual(cli.projectRoles(DEPLOYER_MEMBER), [...DEPLOYER_PROJECT_ROLES].sort());
  assert.deepEqual(cli.projectRoles(ROLLBACK_MEMBER), [...ROLLBACK_PROJECT_ROLES].sort());
  assert.deepEqual(cli.projectRoles(COMMAND_MEMBER), ["roles/run.viewer"]);
  assert.deepEqual(cli.bucketRoles(COMMAND_MEMBER), []);
  assert.deepEqual(cli.bucketRoles(BUILDER_MEMBER), [
    "roles/storage.objectCreator",
    "roles/storage.objectViewer",
  ]);
  assert.deepEqual(cli.bucketRoles(BUILD_MEMBER), ["roles/storage.objectViewer"]);
  assert.deepEqual(cli.bucketRoles(DEPLOYER_MEMBER), []);
  assert.deepEqual(cli.bucketRoles(ROLLBACK_MEMBER), []);
  assert.deepEqual(cli.repositoryRoles(BUILD_MEMBER), ["roles/artifactregistry.writer"]);
  assert.deepEqual(cli.repositoryRoles(BUILDER_MEMBER), ["roles/artifactregistry.reader"]);
  assert.deepEqual(cli.repositoryRoles(DEPLOYER_MEMBER), ["roles/artifactregistry.reader"]);
  assert.deepEqual(cli.repositoryRoles(ROLLBACK_MEMBER), []);
  assert.deepEqual(cli.repositoryRoles(COMMAND_MEMBER), []);
});

test("ambiguous successful mutation is re-observed instead of duplicated", async () => {
  const cli = new FakeGcloud({
    ready: true,
    removeProjectRole: [DEPLOYER_MEMBER, DEPLOYER_RUN_ROLE],
    ambiguousMutation: "projects add-iam-policy-binding clearra-cloud",
  });
  const applied = await applyGitHubWifBootstrap({}, { runGcloud: cli.run.bind(cli) });
  assert.equal(applied.status, "ready");
  assert.equal(cli.mutationsMatching("projects add-iam-policy-binding clearra-cloud"), 1);
});

test("apply retries API, pool, provider, service-account, role, and IAM propagation with injected sleep", async () => {
  const prefixes = {
    "services enable ": 1,
    "iam workload-identity-pools create ": 1,
    "iam workload-identity-pools providers create-oidc ": 1,
    "iam service-accounts create clearra-github-builder ": 1,
    "iam roles create clearraGithubRuntimeDeployer ": 1,
    "projects add-iam-policy-binding clearra-cloud ": 1,
  };
  const cli = new FakeGcloud({ transientMutationFailures: prefixes });
  const clock = fakeReconcileClock();
  const applied = await applyGitHubWifBootstrap({}, {
    runGcloud: cli.run.bind(cli),
    now: clock.now,
    sleep: clock.sleep,
  });

  assert.equal(applied.status, "ready");
  for (const prefix of Object.keys(prefixes)) {
    assert.ok(cli.mutationsMatching(prefix) >= 2, `missing retry for ${prefix}`);
  }
  assert.ok(clock.waits.length >= Object.keys(prefixes).length);
  assert.ok(clock.waits.every((delay) => [1_000, 2_000, 4_000, 8_000, 16_000, 30_000].includes(delay)));
  assert.ok(clock.elapsed() < 300_000);
});

test("successful mutations wait for delayed re-observation without duplicate writes", async () => {
  const cli = new FakeGcloud({ failPlanStartsAfterMutation: 1 });
  const clock = fakeReconcileClock();
  const applied = await applyGitHubWifBootstrap({}, {
    runGcloud: cli.run.bind(cli),
    now: clock.now,
    sleep: clock.sleep,
  });

  assert.equal(applied.status, "ready");
  assert.ok(clock.waits.length > 0);
  assert.equal(cli.mutationsMatching("services enable "), 1);
  assert.equal(cli.mutationsMatching("iam workload-identity-pools create "), 2);
  assert.equal(
    cli.mutationsMatching("iam roles create clearraGithubRuntimeDeployer "),
    1,
  );
  assert.equal(
    cli.mutationsMatching("iam roles create clearraGithubRuntimeRollback "),
    1,
  );
});

test("rollback provider converges before the primary provider can activate", async () => {
  const rollbackProviderPrefix =
    `iam workload-identity-pools providers create-oidc ${ROLLBACK_PROVIDER_ID} `;
  const primaryProviderPrefix =
    `iam workload-identity-pools providers create-oidc ${PROVIDER_ID} `;
  const cli = new FakeGcloud({ permanentMutationFailure: rollbackProviderPrefix });
  const clock = fakeReconcileClock();

  await assert.rejects(
    applyGitHubWifBootstrap({}, {
      runGcloud: cli.run.bind(cli),
      now: clock.now,
      sleep: clock.sleep,
    }),
    /did not converge within 300000ms: create-rollback-provider/,
  );
  assert.ok(cli.mutationsMatching(rollbackProviderPrefix) > 0);
  assert.equal(cli.mutationsMatching(primaryProviderPrefix), 0);
  assert.equal(cli.provider, null);
});

test("legacy rollback revision-delete authority is narrowed before either provider activates", async () => {
  const cli = new FakeGcloud({ ready: true });
  cli.rollbackRunRole.includedPermissions.push("run.revisions.delete");
  cli.rollbackProvider = null;
  cli.rollbackProviders = [];
  cli.provider = null;
  cli.providers = [];

  const planned = await plan(cli);
  const ids = planned.plannedMutations.map((entry) => entry.id);
  assert.deepEqual(ids, [
    "remove-rollback-revision-delete-permission",
    "create-rollback-provider",
    "create-provider",
  ]);

  const applied = await applyGitHubWifBootstrap({}, { runGcloud: cli.run.bind(cli) });
  assert.equal(applied.status, "ready");
  assert.equal(
    cli.mutationsMatching(`iam roles update ${ROLLBACK_RUN_ROLE_ID} `),
    1,
  );
  const commands = cli.mutationCommands;
  assert.ok(
    commands.findIndex((entry) => entry.startsWith(`iam roles update ${ROLLBACK_RUN_ROLE_ID} `)) <
      commands.findIndex((entry) => entry.startsWith(
        `iam workload-identity-pools providers create-oidc ${ROLLBACK_PROVIDER_ID} `,
      )),
  );
});

test("phase-boundary authority drift fails before removal or federation can continue", async () => {
  const cli = new FakeGcloud();
  const initial = await plan(cli);
  const firstRemoval = initial.plannedMutations.findIndex(({ argv }) =>
    argv.some((argument) => argument.includes("remove-iam-policy-binding")));
  assert.ok(firstRemoval > 0);
  const finalGrantCommand = initial.plannedMutations[firstRemoval - 1].argv.join(" ");
  let injected = false;
  const runWithDrift = (argv, execution = undefined) => {
    const result = cli.run(argv, execution);
    if (!injected && result.status === 0 && argv.join(" ") === finalGrantCommand) {
      injected = true;
      cli.addProjectRole(DEPLOYER_MEMBER, "roles/owner");
    }
    return result;
  };
  const clock = fakeReconcileClock();

  await assert.rejects(
    applyGitHubWifBootstrap({}, {
      runGcloud: runWithDrift,
      now: clock.now,
      sleep: clock.sleep,
    }),
    /deployer project authority contains unexpected authority/,
  );
  assert.equal(injected, true);
  assert.deepEqual(clock.waits, []);
  assert.equal(
    cli.mutationCommands.some((command) => command.includes(" remove-iam-policy-binding ")),
    false,
  );
  assert.equal(
    cli.mutationCommands.some((command) =>
      command.includes("--role=roles/iam.workloadIdentityUser")),
    false,
  );
  assert.equal(
    cli.mutationCommands.some((command) =>
      command.startsWith("iam workload-identity-pools providers create-oidc ")),
    false,
  );
});

test("apply fails closed when reconcile cannot converge within five minutes", async () => {
  const cli = new FakeGcloud({
    permanentMutationFailure: "iam workload-identity-pools create ",
  });
  const clock = fakeReconcileClock();
  await assert.rejects(
    applyGitHubWifBootstrap({}, {
      runGcloud: cli.run.bind(cli),
      now: clock.now,
      sleep: clock.sleep,
    }),
    /did not converge within 300000ms: create-pool/,
  );
  assert.equal(clock.elapsed(), 300_000);
  assert.ok(Math.max(...clock.waits) <= 30_000);
  assert.equal(cli.provider, null);
});

test("five-minute convergence budget resets per mutation and includes fresh-plan latency", async () => {
  const clock = fakeReconcileClock();
  const cli = new FakeGcloud({
    planReadMilliseconds: 120_000,
    advanceClock: clock.advance,
  });
  const applied = await applyGitHubWifBootstrap({}, {
    runGcloud: cli.run.bind(cli),
    now: clock.now,
    sleep: clock.sleep,
  });
  assert.equal(applied.status, "ready");
  assert.ok(clock.elapsed() > 300_000);

  const timeoutClock = fakeReconcileClock();
  const timeoutCli = new FakeGcloud({
    planReadMilliseconds: 300_001,
    advanceClock: timeoutClock.advance,
  });
  await assert.rejects(
    applyGitHubWifBootstrap({}, {
      runGcloud: timeoutCli.run.bind(timeoutCli),
      now: timeoutClock.now,
      sleep: timeoutClock.sleep,
    }),
    /did not converge within 300000ms/,
  );
});

test("legacy run.admin is removed only after the exact custom role binding is planned first", async () => {
  const cli = new FakeGcloud({ ready: true });
  removeRole(cli.projectPolicy, DEPLOYER_MEMBER, DEPLOYER_RUN_ROLE);
  cli.addProjectRole(DEPLOYER_MEMBER, "roles/run.admin");

  const migration = await plan(cli);
  assert.deepEqual(migration.plannedMutations.map(({ id }) => id), [
    "deployer-project-add-projects-clearra-cloud-roles-clearragithubruntimedeployer",
    "deployer-project-remove-run-admin",
  ]);
  const applied = await applyGitHubWifBootstrap({}, { runGcloud: cli.run.bind(cli) });
  assert.equal(applied.status, "ready");
  assert.deepEqual(cli.projectRoles(DEPLOYER_MEMBER), [...DEPLOYER_PROJECT_ROLES].sort());
});

test("project-wide build storage read is replaced by exact source-bucket read before removal", async () => {
  const cli = new FakeGcloud({ ready: true });
  removeRole(cli.bucketPolicy, BUILD_MEMBER, "roles/storage.objectViewer");
  cli.addProjectRole(BUILD_MEMBER, "roles/storage.objectViewer");

  const migration = await plan(cli);
  assert.deepEqual(migration.plannedMutations.map(({ id }) => id), [
    "source-bucket-build-add-storage-objectviewer",
    "build-project-remove-storage-objectviewer",
  ]);
  const applied = await applyGitHubWifBootstrap({}, { runGcloud: cli.run.bind(cli) });
  assert.equal(applied.status, "ready");
  assert.deepEqual(cli.bucketRoles(BUILD_MEMBER), ["roles/storage.objectViewer"]);
  assert.deepEqual(cli.projectRoles(BUILD_MEMBER), BUILD_PROJECT_ROLES);
});

test("legacy cleanup precedes WIF additions and provider activation is always last", async () => {
  const activeProvider = new FakeGcloud({ ready: true });
  activeProvider.addBucketRole(COMMAND_MEMBER, "roles/storage.objectViewer");
  removeRole(
    activeProvider.serviceAccountPolicies.get(DEPLOYER_EMAIL),
    PATH_WIF_MEMBER,
    "roles/iam.workloadIdentityUser",
  );
  const activePlan = await plan(activeProvider);
  assert.deepEqual(activePlan.plannedMutations.map(({ id }) => id), [
    "source-bucket-command-sync-remove-storage-objectviewer",
    "github-deployer-wif-add-environment-path-confirmation",
  ]);

  const inactiveProvider = new FakeGcloud({ ready: true });
  inactiveProvider.provider = null;
  inactiveProvider.providers = [];
  inactiveProvider.addBucketRole(COMMAND_MEMBER, "roles/storage.objectViewer");
  removeRole(
    inactiveProvider.serviceAccountPolicies.get(DEPLOYER_EMAIL),
    PATH_WIF_MEMBER,
    "roles/iam.workloadIdentityUser",
  );
  const inactivePlan = await plan(inactiveProvider);
  assert.deepEqual(inactivePlan.plannedMutations.map(({ id }) => id), [
    "source-bucket-command-sync-remove-storage-objectviewer",
    "github-deployer-wif-add-environment-path-confirmation",
    "create-provider",
  ]);
});

test("provider repository, ref, issuer, mapping, and pool exclusivity drift fail closed", async () => {
  for (const mutate of [
    (cli) => { cli.provider.attributeCondition = "assertion.repository == 'other/repo'"; },
    (cli) => { cli.provider.oidc.issuerUri = "https://example.test"; },
    (cli) => { cli.provider.attributeMapping["attribute.ref"] = "assertion.environment"; },
    (cli) => { cli.provider.attributeMapping["attribute.repository_id"] = "assertion.actor_id"; },
    (cli) => { cli.provider.attributeMapping["attribute.workflow_ref"] = "assertion.job_workflow_ref"; },
    (cli) => { cli.providers.push({ name: `${POOL_NAME}/providers/unexpected`, state: "ACTIVE" }); },
    (cli) => { cli.rollbackProvider.attributeCondition = cli.provider.attributeCondition; },
    (cli) => { cli.rollbackProvider.attributeMapping["attribute.workflow_ref"] = "assertion.job_workflow_ref"; },
    (cli) => {
      cli.rollbackProviders.push({
        name: `${ROLLBACK_POOL_NAME}/providers/unexpected`,
        state: "ACTIVE",
      });
    },
  ]) {
    const cli = new FakeGcloud({ ready: true });
    mutate(cli);
    await assert.rejects(plan(cli), /Provider drifted|unexpected provider/);
    assert.equal(cli.mutationCount, 0);
  }
});

test("WIF audit requires five exact service-account tuples across primary and recovery boundaries", async () => {
  const missingPath = new FakeGcloud({ ready: true });
  removeRole(
    missingPath.serviceAccountPolicies.get(DEPLOYER_EMAIL),
    PATH_WIF_MEMBER,
    "roles/iam.workloadIdentityUser",
  );
  const missingPathReport = await plan(missingPath);
  assert.equal(missingPathReport.status, "changes-required");
  assert.deepEqual(
    missingPathReport.plannedMutations.map(({ id }) => id),
    ["github-deployer-wif-add-environment-path-confirmation"],
  );

  const missingRollback = new FakeGcloud({ ready: true });
  removeRole(
    missingRollback.serviceAccountPolicies.get(ROLLBACK_EMAIL),
    RECOVERY_RUNTIME_ROLLBACK_WIF_MEMBER,
    "roles/iam.workloadIdentityUser",
  );
  const missingRollbackReport = await plan(missingRollback);
  assert.deepEqual(
    missingRollbackReport.plannedMutations.map(({ id }) => id),
    ["github-rollback-wif-add-recovery-environment-runtime-rollback"],
  );

  const missingCommand = new FakeGcloud({ ready: true });
  removeRole(
    missingCommand.serviceAccountPolicies.get(COMMAND_EMAIL),
    GLOBAL_SYNC_WIF_MEMBER,
    "roles/iam.workloadIdentityUser",
  );
  const missingCommandReport = await plan(missingCommand);
  assert.deepEqual(
    missingCommandReport.plannedMutations.map(({ id }) => id),
    ["github-command-sync-wif-add-environment-global-command-sync"],
  );

  const missingRecoveryCommand = new FakeGcloud({ ready: true });
  removeRole(
    missingRecoveryCommand.serviceAccountPolicies.get(COMMAND_EMAIL),
    RECOVERY_RUNTIME_ROLLBACK_WIF_MEMBER,
    "roles/iam.workloadIdentityUser",
  );
  const missingRecoveryCommandReport = await plan(missingRecoveryCommand);
  assert.deepEqual(
    missingRecoveryCommandReport.plannedMutations.map(({ id }) => id),
    ["github-command-sync-wif-add-recovery-environment-runtime-rollback"],
  );

  for (const mutate of [
    (cli) => cli.addServiceAccountRole(
      BUILDER_EMAIL,
      PATH_WIF_MEMBER,
      "roles/iam.workloadIdentityUser",
    ),
    (cli) => cli.addServiceAccountRole(
      DEPLOYER_EMAIL,
      BUILDER_WIF_MEMBER,
      "roles/iam.workloadIdentityUser",
    ),
    (cli) => cli.addServiceAccountRole(
      DEPLOYER_EMAIL,
      GLOBAL_SYNC_WIF_MEMBER,
      "roles/iam.workloadIdentityUser",
    ),
    (cli) => cli.addServiceAccountRole(
      ROLLBACK_EMAIL,
      PRIMARY_RUNTIME_ROLLBACK_WIF_MEMBER,
      "roles/iam.workloadIdentityUser",
    ),
    (cli) => cli.addServiceAccountRole(
      DEPLOYER_EMAIL,
      RECOVERY_RUNTIME_ROLLBACK_WIF_MEMBER,
      "roles/iam.workloadIdentityUser",
    ),
    (cli) => cli.addServiceAccountRole(
      COMMAND_EMAIL,
      `${WIF_PREFIX}repo:daejunnom/Clearra:environment:unapproved`,
      "roles/iam.workloadIdentityUser",
    ),
  ]) {
    const cli = new FakeGcloud({ ready: true });
    mutate(cli);
    await assert.rejects(plan(cli), /exact trusted tuple|unexpected federated principal/);
  }

  const conditional = new FakeGcloud({ ready: true });
  conditional.serviceAccountPolicies.get(DEPLOYER_EMAIL).bindings[0].condition = {
    title: "temporary",
    expression: "request.time < timestamp('2030-01-01T00:00:00Z')",
  };
  await assert.rejects(
    plan(conditional),
    /invalid binding|conditional impersonation|conditional federated authority/,
  );

  for (const mutate of [
    (cli) => cli.addServiceAccountRole(
      BUILDER_EMAIL,
      BUILDER_WIF_MEMBER,
      "roles/iam.serviceAccountTokenCreator",
    ),
    (cli) => cli.addServiceAccountRole(
      DEPLOYER_EMAIL,
      PATH_WIF_MEMBER,
      "roles/iam.serviceAccountTokenCreator",
    ),
    (cli) => cli.addServiceAccountRole(
      COMMAND_EMAIL,
      GLOBAL_SYNC_WIF_MEMBER,
      "roles/iam.serviceAccountTokenCreator",
    ),
    (cli) => cli.addServiceAccountRole(
      ROLLBACK_EMAIL,
      RECOVERY_RUNTIME_ROLLBACK_WIF_MEMBER,
      "roles/iam.serviceAccountTokenCreator",
    ),
    (cli) => cli.addServiceAccountRole(
      COMMAND_EMAIL,
      `principalSet://iam.googleapis.com/${POOL_NAME}/attribute.repository/daejunnom/Clearra`,
      "roles/iam.workloadIdentityUser",
    ),
  ]) {
    const cli = new FakeGcloud({ ready: true });
    mutate(cli);
    await assert.rejects(
      plan(cli),
      /exact trusted tuple|forbidden impersonation role|unexpected federated principal/,
    );
  }
});

test("catalog-wide service-account policies reject every tuple outside the trusted human and machine sets", async () => {
  for (const [member, role] of [
    [`serviceAccount:unknown@${PROJECT_ID}.iam.gserviceaccount.com`, "roles/owner"],
    [`serviceAccount:unknown@${PROJECT_ID}.iam.gserviceaccount.com`, "roles/editor"],
    [`serviceAccount:unknown@${PROJECT_ID}.iam.gserviceaccount.com`, `projects/${PROJECT_ID}/roles/custom`],
    [BUILDER_MEMBER, "roles/viewer"],
    [PRIMARY_RUNTIME_ROLLBACK_WIF_MEMBER, "roles/iam.workloadIdentityUser"],
    ["user:attacker@example.com", "roles/iam.serviceAccountTokenCreator"],
    ["group:attacker@example.com", "roles/iam.serviceAccountUser"],
    ["domain:example.com", "roles/iam.serviceAccountAdmin"],
    ["allUsers", "roles/owner"],
    ["allAuthenticatedUsers", "roles/editor"],
  ]) {
    const cli = new FakeGcloud({ ready: true });
    cli.addServiceAccountRole(EXTRA_EMAIL, member, role);
    await assert.rejects(plan(cli), /catalog contains authority outside the exact trusted tuple set/);
  }

  for (const [email, member, role] of [
    [BUILD_EMAIL, PRIMARY_HUMAN_MEMBER, "roles/iam.serviceAccountAdmin"],
    [COMMAND_EMAIL, PRIMARY_HUMAN_MEMBER, "roles/owner"],
  ]) {
    const missing = new FakeGcloud({ ready: true });
    removeRole(missing.serviceAccountPolicies.get(email), member, role);
    await assert.rejects(plan(missing), /missing required trusted-human authority/);
  }

  const conditionalHuman = new FakeGcloud({ ready: true });
  conditionalHuman.serviceAccountPolicies.get(BUILD_EMAIL).bindings[0].condition = {
    title: "temporary",
    expression: "request.time < timestamp('2030-01-01T00:00:00Z')",
  };
  await assert.rejects(plan(conditionalHuman), /invalid binding|invalid or conditional binding/);
});

test("project, repository, bucket, and Secret policies reject every direct federated principal", async () => {
  const cases = [
    (cli) => addRole(cli.projectPolicy, BUILDER_WIF_MEMBER, "roles/run.admin"),
    (cli) => addRole(cli.repositoryPolicy, BUILDER_WIF_MEMBER, "roles/artifactregistry.reader"),
    (cli) => addRole(cli.bucketPolicy, PATH_WIF_MEMBER, "roles/storage.objectViewer"),
    (cli) => cli.addSecretRole("discord-bot-token", GLOBAL_SYNC_WIF_MEMBER, ACCESSOR),
    (cli) => addRole(
      cli.projectPolicy,
      `principalSet://iam.googleapis.com/${POOL_NAME}/attribute.repository/daejunnom/Clearra`,
      "roles/run.admin",
    ),
  ];
  for (const mutate of cases) {
    const cli = new FakeGcloud({ ready: true });
    mutate(cli);
    await assert.rejects(plan(cli), /zero direct federated principals/);
  }
});

test("disabled, deleted, or metadata-drifted pool and deployer fail closed", async () => {
  const disabledPool = new FakeGcloud({ ready: true });
  disabledPool.pool.disabled = true;
  await assert.rejects(plan(disabledPool), /Pool drifted/);

  const deletedPool = new FakeGcloud({ ready: true });
  deletedPool.pool.state = "DELETED";
  await assert.rejects(plan(deletedPool), /Pool drifted/);

  const deployerDrift = new FakeGcloud({ ready: true });
  deployerDrift.deployer.description = "widened identity";
  await assert.rejects(plan(deployerDrift), /metadata drifted/);

  const roleDrift = new FakeGcloud({ ready: true });
  roleDrift.deployerRunRole.includedPermissions.push("run.services.delete");
  await assert.rejects(plan(roleDrift), /custom role drifted/);

  const rollbackRoleDrift = new FakeGcloud({ ready: true });
  rollbackRoleDrift.rollbackRunRole.includedPermissions.push("run.services.delete");
  await assert.rejects(plan(rollbackRoleDrift), /rollback custom role drifted/);
});

test("unexpected deployer, impersonation, repository, bucket, or command-sync role fails closed", async () => {
  const cases = [
    (cli) => cli.addProjectRole(BUILD_MEMBER, "roles/storage.objectAdmin"),
    (cli) => cli.addProjectRole(BUILDER_MEMBER, "roles/run.admin"),
    (cli) => cli.addProjectRole(DEPLOYER_MEMBER, "roles/owner"),
    (cli) => cli.addProjectRole(ROLLBACK_MEMBER, "roles/run.admin"),
    (cli) => cli.addServiceAccountRole(BUILD_EMAIL, BUILDER_MEMBER, "roles/iam.serviceAccountTokenCreator"),
    (cli) => cli.addServiceAccountRole(BUILD_EMAIL, DEPLOYER_MEMBER, "roles/iam.serviceAccountUser"),
    (cli) => cli.addServiceAccountRole(BUILD_EMAIL, COMMAND_MEMBER, "roles/iam.serviceAccountUser"),
    (cli) => cli.addServiceAccountRole(RUNTIME_EMAIL, BUILDER_MEMBER, "roles/iam.serviceAccountUser"),
    (cli) => cli.addServiceAccountRole(RUNTIME_EMAIL, COMMAND_MEMBER, "roles/iam.serviceAccountUser"),
    (cli) => cli.addServiceAccountRole(RUNTIME_EMAIL, ROLLBACK_MEMBER, "roles/iam.serviceAccountUser"),
    (cli) => cli.addServiceAccountRole(COMMAND_EMAIL, DEPLOYER_MEMBER, "roles/iam.serviceAccountAdmin"),
    (cli) => cli.addRepositoryRole(BUILD_MEMBER, "roles/artifactregistry.reader"),
    (cli) => cli.addRepositoryRole(BUILDER_MEMBER, "roles/artifactregistry.writer"),
    (cli) => cli.addRepositoryRole(DEPLOYER_MEMBER, "roles/artifactregistry.writer"),
    (cli) => cli.addRepositoryRole(COMMAND_MEMBER, "roles/artifactregistry.reader"),
    (cli) => cli.addRepositoryRole(ROLLBACK_MEMBER, "roles/artifactregistry.reader"),
    (cli) => cli.addBucketRole(BUILD_MEMBER, "roles/storage.objectCreator"),
    (cli) => cli.addBucketRole(BUILDER_MEMBER, "roles/storage.objectAdmin"),
    (cli) => cli.addBucketRole(DEPLOYER_MEMBER, "roles/storage.objectViewer"),
    (cli) => cli.addBucketRole(ROLLBACK_MEMBER, "roles/storage.objectViewer"),
    (cli) => cli.addProjectRole(COMMAND_MEMBER, "roles/run.admin"),
  ];
  for (const mutate of cases) {
    const cli = new FakeGcloud({ ready: true });
    mutate(cli);
    await assert.rejects(plan(cli), /unexpected authority|must not|exact trusted tuple/);
    assert.equal(cli.mutationCount, 0);
  }
});

test("deployer has no direct Secret role and command-sync has only the exact global Discord accessor", async () => {
  const builderSecret = new FakeGcloud({ ready: true });
  builderSecret.addSecretRole("discord-bot-token", BUILDER_MEMBER, "roles/secretmanager.viewer");
  await assert.rejects(plan(builderSecret), /builder service account must have zero direct Secret authority/);

  const deployerSecret = new FakeGcloud({ ready: true });
  deployerSecret.addSecretRole("discord-bot-token", DEPLOYER_MEMBER, "roles/secretmanager.viewer");
  await assert.rejects(plan(deployerSecret), /zero direct Secret authority/);

  const jobAccess = new FakeGcloud({ ready: true });
  jobAccess.addSecretRole("clearra-job-token", COMMAND_MEMBER, ACCESSOR);
  await assert.rejects(plan(jobAccess), /outside its exact closed set/);

  const regionalAccess = new FakeGcloud({ ready: true, regionalSecret: true });
  regionalAccess.addSecretRole("asia-northeast1/regional-private", COMMAND_MEMBER, ACCESSOR);
  await assert.rejects(plan(regionalAccess), /outside its exact closed set/);

  const missingAccessor = new FakeGcloud({ ready: true });
  missingAccessor.secretPolicies.set("discord-bot-token", { bindings: [] });
  await assert.rejects(plan(missingAccessor), /missing required exact authority/);

  const conditional = new FakeGcloud({ ready: true });
  conditional.secretPolicies.set("discord-bot-token", {
    bindings: [{
      role: ACCESSOR,
      members: [COMMAND_MEMBER],
      condition: { title: "temporary", expression: "request.time < timestamp('2030-01-01T00:00:00Z')" },
    }],
  });
  await assert.rejects(plan(conditional), /invalid or conditional authority/);

  const buildSecret = new FakeGcloud({ ready: true });
  buildSecret.addSecretRole("clearra-job-token", BUILD_MEMBER, ACCESSOR);
  await assert.rejects(plan(buildSecret), /build service account must have zero direct Secret authority/);

  const rollbackSecret = new FakeGcloud({ ready: true });
  rollbackSecret.addSecretRole("clearra-job-token", ROLLBACK_MEMBER, ACCESSOR);
  await assert.rejects(plan(rollbackSecret), /rollback service account must have zero direct Secret authority/);

  const missingRuntime = new FakeGcloud({ ready: true });
  removeRole(missingRuntime.secretPolicies.get("clearra-job-token"), RUNTIME_MEMBER, ACCESSOR);
  await assert.rejects(plan(missingRuntime), /missing required exact authority/);

  const missingInteraction = new FakeGcloud({ ready: true });
  removeRole(missingInteraction.secretPolicies.get("discord-bot-token"), EXTRA_MEMBER, ACCESSOR);
  await assert.rejects(plan(missingInteraction), /missing required exact authority/);

  const missingTelemetryEvent = new FakeGcloud({ ready: true });
  removeRole(
    missingTelemetryEvent.secretPolicies.get("clearra-telemetry-event-key"),
    EXTRA_MEMBER,
    ACCESSOR,
  );
  await assert.rejects(plan(missingTelemetryEvent), /missing required exact authority/);

  const missingTelemetryTransport = new FakeGcloud({ ready: true });
  removeRole(
    missingTelemetryTransport.secretPolicies.get("clearra-telemetry-transport-key"),
    `serviceAccount:${TELEMETRY_EMAIL}`,
    ACCESSOR,
  );
  await assert.rejects(plan(missingTelemetryTransport), /missing required exact authority/);

  const widenedTelemetry = new FakeGcloud({ ready: true });
  widenedTelemetry.addSecretRole("clearra-telemetry-event-key", COMMAND_MEMBER, ACCESSOR);
  await assert.rejects(plan(widenedTelemetry), /outside its exact closed set/);

  const duplicateTelemetry = new FakeGcloud({ ready: true });
  duplicateTelemetry.secretPolicies.get("clearra-telemetry-event-key").bindings.push({
    role: ACCESSOR,
    members: [EXTRA_MEMBER],
  });
  await assert.rejects(plan(duplicateTelemetry), /outside its exact closed set/);

  const unmodeledUser = new FakeGcloud({ ready: true });
  unmodeledUser.addSecretRole("discord-bot-token", "user:attacker@example.com", ACCESSOR);
  await assert.rejects(plan(unmodeledUser), /outside its exact closed set/);

  for (const member of [
    `serviceAccount:unknown@${PROJECT_ID}.iam.gserviceaccount.com`,
    "domain:example.com",
    "allUsers",
    "allAuthenticatedUsers",
  ]) {
    const unmodeledMember = new FakeGcloud({ ready: true });
    unmodeledMember.addSecretRole("clearra-telemetry-event-key", member, ACCESSOR);
    await assert.rejects(plan(unmodeledMember), /outside its exact closed set/);
  }

  const unmodeledRegional = new FakeGcloud({ ready: true, regionalSecret: true });
  unmodeledRegional.addSecretRole(
    "asia-northeast1/regional-private",
    "group:attacker@example.com",
    "roles/secretmanager.viewer",
  );
  await assert.rejects(plan(unmodeledRegional), /outside its exact closed set/);
});

test("user-managed build, runtime, GitHub, and command-sync service-account keys fail closed", async () => {
  const builderKey = new FakeGcloud({ ready: true });
  builderKey.userKeys.set(BUILDER_EMAIL, [{ name: "projects/example/keys/0" }]);
  await assert.rejects(plan(builderKey), /zero user-managed keys/);

  const deployerKey = new FakeGcloud({ ready: true });
  deployerKey.userKeys.set(DEPLOYER_EMAIL, [{ name: "projects/example/keys/1" }]);
  await assert.rejects(plan(deployerKey), /zero user-managed keys/);

  const commandKey = new FakeGcloud({ ready: true });
  commandKey.userKeys.set(COMMAND_EMAIL, [{ name: "projects/example/keys/2" }]);
  await assert.rejects(plan(commandKey), /zero user-managed keys/);

  const buildKey = new FakeGcloud({ ready: true });
  buildKey.userKeys.set(BUILD_EMAIL, [{ name: "projects/example/keys/3" }]);
  await assert.rejects(plan(buildKey), /zero user-managed keys/);

  const runtimeKey = new FakeGcloud({ ready: true });
  runtimeKey.userKeys.set(RUNTIME_EMAIL, [{ name: "projects/example/keys/4" }]);
  await assert.rejects(plan(runtimeKey), /zero user-managed keys/);

  const rollbackKey = new FakeGcloud({ ready: true });
  rollbackKey.userKeys.set(ROLLBACK_EMAIL, [{ name: "projects/example/keys/5" }]);
  await assert.rejects(plan(rollbackKey), /zero user-managed keys/);
});

test("wrong project identity, parent attachment, and missing prerequisite API fail before planning", async () => {
  const endpointOverride = new FakeGcloud();
  endpointOverride.persistentEndpointOverrides.api_endpoint_overrides.iam =
    "https://attacker.invalid/";
  await assert.rejects(plan(endpointOverride), /persistent API endpoint overrides must all be unset/);

  const wrongProject = new FakeGcloud();
  wrongProject.project.projectNumber = "123456789";
  await assert.rejects(plan(wrongProject), /exact active Clearra project/);

  const parented = new FakeGcloud();
  parented.ancestors.push({ id: "123", type: "organization" });
  await assert.rejects(plan(parented), /parentless/);

  const missingApi = new FakeGcloud();
  missingApi.services.delete("run.googleapis.com");
  await assert.rejects(plan(missingApi), /required deployment API is not enabled/);
});

test("bootstrap has no widenable input options", async () => {
  const cli = new FakeGcloud({ ready: true });
  await assert.rejects(
    createGitHubWifBootstrapPlan(
      { projectId: "other-project" },
      { runGcloud: cli.run.bind(cli) },
    ),
    /no widenable options/,
  );
  assert.equal(cli.calls.length, 0);
});

test("Windows gcloud launcher is shell-free and endpoint overrides are subprocess-local", () => {
  const invocation = gcloudProcessInvocation(
    ["projects", "describe", PROJECT_ID, "--format=json"],
    "win32",
    { ComSpec: "C:\\Windows\\System32\\cmd.exe" },
  );
  assert.equal(invocation.command, "C:\\Windows\\System32\\cmd.exe");
  assert.deepEqual(invocation.arguments, [
    "/d", "/s", "/c", "gcloud.cmd",
    "projects", "describe", PROJECT_ID, "--format=json",
  ]);

  const environment = gcloudProcessEnvironment(
    { environment: { [SECRET_ENDPOINT_ENV]: "https://secretmanager.asia-northeast1.rep.googleapis.com/" } },
    { Path: "test" },
  );
  assert.equal(environment.Path, "test");
  assert.equal(
    environment[SECRET_ENDPOINT_ENV],
    "https://secretmanager.asia-northeast1.rep.googleapis.com/",
  );
  assert.equal(Object.hasOwn(environment, SECRET_ENDPOINT_ENV.toLowerCase()), false);
  for (const ambientOverride of [
    { [SECRET_ENDPOINT_ENV.toLowerCase()]: "https://attacker.invalid/" },
    { CLOUDSDK_API_ENDPOINT_OVERRIDES_IAM: "https://attacker.invalid/" },
  ]) {
    assert.throws(
      () => gcloudProcessEnvironment(undefined, ambientOverride),
      /ambient gcloud API endpoint overrides are forbidden/,
    );
  }

  assert.doesNotThrow(() => gcloudProcessEnvironment(undefined, {}));
  assert.doesNotThrow(() => gcloudProcessEnvironment({
    environment: {
      [SECRET_ENDPOINT_ENV]: "https://secretmanager.googleapis.com/",
    },
  }, {}));
  for (const invalidExecution of [
    null,
    [],
    {},
    { environment: {} },
    {
      environment: {
        [SECRET_ENDPOINT_ENV]: "https://secretmanager.googleapis.com/",
      },
      widened: true,
    },
    {
      environment: {
        [SECRET_ENDPOINT_ENV]: "https://secretmanager.googleapis.com/",
        EXTRA: "widened",
      },
    },
    {
      environment: {
        [SECRET_ENDPOINT_ENV]: "https://attacker.invalid/",
      },
    },
  ]) {
    assert.throws(
      () => gcloudProcessEnvironment(invalidExecution, {}),
      /execution environment is invalid|unsupported override|endpoint override is invalid/,
    );
  }

  assert.throws(
    () => gcloudProcessInvocation(["secrets", "versions", "access", "latest"]),
    /metadata\/IAM-only/,
  );
  assert.throws(
    () => gcloudProcessInvocation(["iam", "service-accounts", "keys", "create", "key.json"]),
    /metadata\/IAM-only/,
  );
  for (const destructive of [
    ["secrets", "delete", "discord-bot-token", `--project=${PROJECT_ID}`],
    ["storage", "rm", `${SOURCE_BUCKET}/source.tgz`],
    ["projects", "delete", PROJECT_ID],
    ["services", "disable", "run.googleapis.com", `--project=${PROJECT_ID}`],
    ["iam", "service-accounts", "delete", DEPLOYER_EMAIL, `--project=${PROJECT_ID}`],
    ["iam", "roles", "delete", DEPLOYER_RUN_ROLE_ID, `--project=${PROJECT_ID}`],
    [
      "iam", "roles", "create", DEPLOYER_RUN_ROLE_ID,
      `--project=${PROJECT_ID}`, "--title=Clearra GitHub runtime deployer",
      "--description=Exact Cloud Run service update, traffic rollback, and ephemeral smoke-job authority",
      `--permissions=${[...DEPLOYER_RUN_PERMISSIONS, "run.services.delete"].join(",")}`,
      "--stage=GA", "--quiet",
    ],
    [
      "iam", "roles", "create", ROLLBACK_RUN_ROLE_ID,
      `--project=${PROJECT_ID}`, "--title=Clearra GitHub runtime rollback",
      "--description=Exact existing-service Cloud Run traffic rollback and readback authority",
      `--permissions=${[...ROLLBACK_RUN_PERMISSIONS, "run.services.delete"].join(",")}`,
      "--stage=GA", "--quiet",
    ],
    [
      "projects", "add-iam-policy-binding", PROJECT_ID,
      `--member=${DEPLOYER_MEMBER}`, "--role=roles/owner", "--condition=None", "--quiet",
    ],
    [
      "iam", "service-accounts", "add-iam-policy-binding", COMMAND_EMAIL,
      `--project=${PROJECT_ID}`, `--member=${DEPLOYER_MEMBER}`,
      "--role=roles/iam.serviceAccountTokenCreator", "--condition=None", "--quiet",
    ],
    [
      "iam", "service-accounts", "add-iam-policy-binding", ROLLBACK_EMAIL,
      `--project=${PROJECT_ID}`, `--member=${PRIMARY_RUNTIME_ROLLBACK_WIF_MEMBER}`,
      "--role=roles/iam.workloadIdentityUser", "--condition=None", "--quiet",
    ],
  ]) {
    assert.throws(() => gcloudProcessInvocation(destructive), /metadata\/IAM-only/);
  }
});

test("failure diagnostics expose actionable invariants but redact injected credential material", () => {
  assert.deepEqual(
    githubWifBootstrapFailureDiagnostic(
      new Error("Secret IAM policy (projects/clearra-cloud/secrets/clearra-telemetry-event-key) is missing required exact authority"),
    ),
    {
      contract: GITHUB_WIF_BOOTSTRAP_CONTRACT,
      status: "failed",
      detail:
        "Secret IAM policy (projects/clearra-cloud/secrets/clearra-telemetry-event-key) is missing required exact authority",
    },
  );
  for (const error of [
    new Error("private_key=credential-material"),
    new Error("failure\naccess_token=credential-material"),
    new Error(`opaque=${"A".repeat(100)}`),
    "not-an-error",
  ]) {
    assert.deepEqual(githubWifBootstrapFailureDiagnostic(error), {
      contract: GITHUB_WIF_BOOTSTRAP_CONTRACT,
      status: "failed",
      detail: "redacted failure detail",
    });
  }
});

async function plan(cli) {
  return createGitHubWifBootstrapPlan({}, { runGcloud: cli.run.bind(cli) });
}

function fakeReconcileClock() {
  let milliseconds = 0;
  const waits = [];
  return {
    waits,
    advance: (delay) => {
      assert.ok(Number.isSafeInteger(delay) && delay >= 0);
      milliseconds += delay;
    },
    elapsed: () => milliseconds,
    now: () => milliseconds,
    sleep: async (delay) => {
      assert.ok(Number.isSafeInteger(delay) && delay > 0);
      waits.push(delay);
      milliseconds += delay;
    },
  };
}

class FakeGcloud {
  constructor(options = {}) {
    this.calls = [];
    this.mutationCount = 0;
    this.mutationCommands = [];
    this.ambiguousMutation = options.ambiguousMutation ?? null;
    this.ambiguousReturned = false;
    this.failPlanStartsAfterMutation = options.failPlanStartsAfterMutation ?? 0;
    this.pendingPlanStartFailures = 0;
    this.permanentMutationFailure = options.permanentMutationFailure ?? null;
    this.transientMutationFailures = new Map(
      Object.entries(options.transientMutationFailures ?? {}),
    );
    this.planReadMilliseconds = options.planReadMilliseconds ?? 0;
    this.advanceClock = options.advanceClock ?? (() => {});
    this.persistentEndpointOverrides = {
      api_endpoint_overrides: { iam: null, run: null, secretmanager: null },
    };
    this.project = {
      projectId: PROJECT_ID,
      projectNumber: PROJECT_NUMBER,
      lifecycleState: "ACTIVE",
    };
    this.ancestors = [{ id: PROJECT_ID, type: "project" }];
    this.services = new Set([
      ...REQUIRED_SERVICES,
      ...(options.ready ? WIF_SERVICES : []),
    ]);
    this.pool = options.ready ? exactPool() : null;
    this.provider = options.ready ? exactProvider() : null;
    this.providers = this.provider ? [this.provider] : [];
    this.rollbackPool = options.ready ? exactRollbackPool() : null;
    this.rollbackProvider = options.ready ? exactRollbackProvider() : null;
    this.rollbackProviders = this.rollbackProvider ? [this.rollbackProvider] : [];
    this.deployerRunRole = options.ready ? exactDeployerRunRole() : null;
    this.rollbackRunRole = options.ready ? exactRollbackRunRole() : null;
    this.builder = options.ready ? exactBuilder() : null;
    this.deployer = options.ready ? exactDeployer() : null;
    this.rollback = options.ready ? exactRollback() : null;
    this.projectPolicy = { bindings: [] };
    this.serviceAccountPolicies = new Map([
      [BUILD_EMAIL, { bindings: [
        { role: "roles/iam.serviceAccountAdmin", members: [PRIMARY_HUMAN_MEMBER] },
        { role: "roles/iam.serviceAccountUser", members: [LEGACY_BUILD_USER_MEMBER] },
        { role: "roles/owner", members: [PRIMARY_HUMAN_MEMBER] },
      ] }],
      [RUNTIME_EMAIL, { bindings: [] }],
      [COMMAND_EMAIL, { bindings: [
        { role: "roles/iam.serviceAccountAdmin", members: [PRIMARY_HUMAN_MEMBER] },
        { role: "roles/owner", members: [PRIMARY_HUMAN_MEMBER] },
      ] }],
      [EXTRA_EMAIL, trustedHumanPolicy(true)],
      [TELEMETRY_EMAIL, trustedHumanPolicy(true)],
      [DEFAULT_COMPUTE_EMAIL, trustedHumanPolicy(false)],
      [JOB_RUNNER_EMAIL, trustedHumanPolicy(false)],
    ]);
    this.repositoryPolicy = { bindings: [] };
    this.bucketPolicy = { bindings: [] };
    this.userKeys = new Map([
      [BUILD_EMAIL, []],
      [RUNTIME_EMAIL, []],
      [BUILDER_EMAIL, []],
      [COMMAND_EMAIL, []],
      [DEPLOYER_EMAIL, []],
      [ROLLBACK_EMAIL, []],
    ]);
    this.regionalLocations = ["asia-northeast1"];
    this.globalSecrets = [
      "discord-bot-token",
      "clearra-job-token",
      "clearra-telemetry-event-key",
      "clearra-telemetry-transport-key",
    ];
    this.regionalSecrets = new Map([["asia-northeast1", options.regionalSecret ? ["regional-private"] : []]]);
    this.secretPolicies = new Map([
      ["discord-bot-token", {
        bindings: [{ role: ACCESSOR, members: [COMMAND_MEMBER, EXTRA_MEMBER] }],
      }],
      ["clearra-job-token", {
        bindings: [{ role: ACCESSOR, members: [RUNTIME_MEMBER, EXTRA_MEMBER] }],
      }],
      ["clearra-telemetry-event-key", {
        bindings: [{ role: ACCESSOR, members: [EXTRA_MEMBER] }],
      }],
      ["clearra-telemetry-transport-key", {
        bindings: [{
          role: ACCESSOR,
          members: [`serviceAccount:${TELEMETRY_EMAIL}`],
        }],
      }],
    ]);
    if (options.regionalSecret) {
      this.secretPolicies.set("asia-northeast1/regional-private", { bindings: [] });
    }

    if (options.ready) this.installReadyAuthority();
    else {
      this.addProjectRole(BUILD_MEMBER, "roles/logging.logWriter");
      this.addProjectRole(BUILD_MEMBER, "roles/storage.objectViewer");
      this.addRepositoryRole(BUILD_MEMBER, "roles/artifactregistry.writer");
      this.addProjectRole(COMMAND_MEMBER, "roles/logging.logWriter");
      this.addBucketRole(COMMAND_MEMBER, "roles/storage.objectViewer");
    }
    if (options.removeProjectRole) {
      removeRole(this.projectPolicy, options.removeProjectRole[0], options.removeProjectRole[1]);
    }
  }

  installReadyAuthority() {
    for (const role of BUILD_PROJECT_ROLES) this.addProjectRole(BUILD_MEMBER, role);
    for (const role of BUILDER_PROJECT_ROLES) this.addProjectRole(BUILDER_MEMBER, role);
    for (const role of DEPLOYER_PROJECT_ROLES) this.addProjectRole(DEPLOYER_MEMBER, role);
    for (const role of ROLLBACK_PROJECT_ROLES) this.addProjectRole(ROLLBACK_MEMBER, role);
    this.addProjectRole(COMMAND_MEMBER, "roles/run.viewer");
    this.addServiceAccountRole(BUILD_EMAIL, BUILDER_MEMBER, "roles/iam.serviceAccountUser");
    this.addServiceAccountRole(RUNTIME_EMAIL, DEPLOYER_MEMBER, "roles/iam.serviceAccountUser");
    this.serviceAccountPolicies.set(BUILDER_EMAIL, {
      bindings: [{ role: "roles/iam.workloadIdentityUser", members: [BUILDER_WIF_MEMBER] }],
    });
    this.serviceAccountPolicies.set(DEPLOYER_EMAIL, {
      bindings: [{
        role: "roles/iam.workloadIdentityUser",
        members: [PATH_WIF_MEMBER],
      }],
    });
    this.serviceAccountPolicies.set(ROLLBACK_EMAIL, {
      bindings: [{
        role: "roles/iam.workloadIdentityUser",
        members: [RECOVERY_RUNTIME_ROLLBACK_WIF_MEMBER],
      }],
    });
    this.addServiceAccountRole(
      COMMAND_EMAIL,
      GLOBAL_SYNC_WIF_MEMBER,
      "roles/iam.workloadIdentityUser",
    );
    this.addServiceAccountRole(
      COMMAND_EMAIL,
      RECOVERY_RUNTIME_ROLLBACK_WIF_MEMBER,
      "roles/iam.workloadIdentityUser",
    );
    this.addRepositoryRole(BUILD_MEMBER, "roles/artifactregistry.writer");
    this.addRepositoryRole(BUILDER_MEMBER, "roles/artifactregistry.reader");
    this.addRepositoryRole(DEPLOYER_MEMBER, "roles/artifactregistry.reader");
    this.addBucketRole(BUILD_MEMBER, "roles/storage.objectViewer");
    this.addBucketRole(BUILDER_MEMBER, "roles/storage.objectCreator");
    this.addBucketRole(BUILDER_MEMBER, "roles/storage.objectViewer");
  }

  projectRoles(member) {
    return roles(this.projectPolicy, member);
  }

  bucketRoles(member) {
    return roles(this.bucketPolicy, member);
  }

  repositoryRoles(member) {
    return roles(this.repositoryPolicy, member);
  }

  addProjectRole(member, role) {
    addRole(this.projectPolicy, member, role);
  }

  addServiceAccountRole(email, member, role) {
    const policy = this.serviceAccountPolicies.get(email) ?? { bindings: [] };
    addRole(policy, member, role);
    this.serviceAccountPolicies.set(email, policy);
  }

  addRepositoryRole(member, role) {
    addRole(this.repositoryPolicy, member, role);
  }

  addBucketRole(member, role) {
    addRole(this.bucketPolicy, member, role);
  }

  addSecretRole(secret, member, role) {
    const policy = this.secretPolicies.get(secret) ?? { bindings: [] };
    addRole(policy, member, role);
    this.secretPolicies.set(secret, policy);
  }

  mutationsMatching(prefix) {
    return this.mutationCommands.filter((command) => command.startsWith(prefix)).length;
  }

  run(argv, execution = undefined) {
    gcloudProcessEnvironment(execution, {});
    this.calls.push({ argv: [...argv], execution });
    const command = argv.join(" ");

    if (command === "config list --all --format=json(api_endpoint_overrides)") {
      this.advanceClock(this.planReadMilliseconds);
      return ok(this.persistentEndpointOverrides);
    }
    if (command === `projects describe ${PROJECT_ID} --format=json`) {
      if (this.pendingPlanStartFailures > 0) {
        this.pendingPlanStartFailures -= 1;
        return failure("eventual-consistency observation delay");
      }
      return ok(this.project);
    }
    if (command === `projects get-ancestors ${PROJECT_ID} --format=json(id,type)`) return ok(this.ancestors);
    if (command === `services list --enabled --project=${PROJECT_ID} --format=json(config.name,state)`) {
      return ok([...this.services].sort().map((name) => ({ config: { name }, state: "ENABLED" })));
    }
    if (command === `iam workload-identity-pools list --project=${PROJECT_ID} --location=global --show-deleted --format=json`) {
      return ok([
        ...(this.pool ? [this.pool] : []),
        ...(this.rollbackPool ? [this.rollbackPool] : []),
      ]);
    }
    if (command === `iam workload-identity-pools providers list --project=${PROJECT_ID} --location=global --workload-identity-pool=${POOL_ID} --show-deleted --format=json`) {
      return ok(this.providers);
    }
    if (command === `iam workload-identity-pools providers list --project=${PROJECT_ID} --location=global --workload-identity-pool=${ROLLBACK_POOL_ID} --show-deleted --format=json`) {
      return ok(this.rollbackProviders);
    }
    if (command === `iam service-accounts list --project=${PROJECT_ID} --format=json`) {
      return ok([
        serviceAccount(BUILD_EMAIL),
        serviceAccount(RUNTIME_EMAIL),
        serviceAccount(COMMAND_EMAIL),
        serviceAccount(EXTRA_EMAIL),
        serviceAccount(TELEMETRY_EMAIL),
        serviceAccount(DEFAULT_COMPUTE_EMAIL),
        serviceAccount(JOB_RUNNER_EMAIL),
        ...(this.builder ? [this.builder] : []),
        ...(this.deployer ? [this.deployer] : []),
        ...(this.rollback ? [this.rollback] : []),
      ]);
    }
    if (command === `iam roles list --project=${PROJECT_ID} --show-deleted --format=json`) {
      return ok([
        ...(this.deployerRunRole ? [this.deployerRunRole] : []),
        ...(this.rollbackRunRole ? [this.rollbackRunRole] : []),
      ]);
    }
    if (command === `iam roles describe ${DEPLOYER_RUN_ROLE_ID} --project=${PROJECT_ID} --format=json`) {
      return this.deployerRunRole ? ok(this.deployerRunRole) : failure("not found");
    }
    if (command === `iam roles describe ${ROLLBACK_RUN_ROLE_ID} --project=${PROJECT_ID} --format=json`) {
      return this.rollbackRunRole ? ok(this.rollbackRunRole) : failure("not found");
    }
    if (command === `projects get-iam-policy ${PROJECT_ID} --format=json`) return ok(this.projectPolicy);
    if (command.startsWith("iam service-accounts get-iam-policy ")) {
      const email = argv[3];
      return this.serviceAccountPolicies.has(email)
        ? ok(this.serviceAccountPolicies.get(email))
        : failure("not found");
    }
    if (command.startsWith("iam service-accounts keys list ")) {
      const email = argv.find((entry) => entry.startsWith("--iam-account="))?.slice(14);
      return ok(this.userKeys.get(email) ?? []);
    }
    if (command === `artifacts repositories describe clearra --project=${PROJECT_ID} --location=asia-northeast1 --format=json`) {
      return ok({
        name: `projects/${PROJECT_ID}/locations/asia-northeast1/repositories/clearra`,
        format: "DOCKER",
      });
    }
    if (command === `artifacts repositories get-iam-policy clearra --project=${PROJECT_ID} --location=asia-northeast1 --format=json`) {
      return ok(this.repositoryPolicy);
    }
    if (command === `storage buckets describe ${SOURCE_BUCKET} --format=json`) {
      return ok({ name: `${PROJECT_ID}_cloudbuild` });
    }
    if (command === `storage buckets get-iam-policy ${SOURCE_BUCKET} --format=json`) {
      return ok(this.bucketPolicy);
    }
    if (command === `secrets locations list --project=${PROJECT_ID} --format=json(name)`) {
      this.assertEndpoint(execution, null);
      return ok(this.regionalLocations.map((location) => ({
        name: `projects/${PROJECT_ID}/locations/${location}`,
      })));
    }
    if (command === `secrets list --project=${PROJECT_ID} --format=json(name)`) {
      this.assertEndpoint(execution, null);
      return ok(this.globalSecrets.map((name) => ({ name: `projects/${PROJECT_ID}/secrets/${name}` })));
    }
    if (command.startsWith(`secrets list --project=${PROJECT_ID} --location=`)) {
      const location = argv.find((entry) => entry.startsWith("--location="))?.slice(11);
      this.assertEndpoint(execution, location);
      return ok((this.regionalSecrets.get(location) ?? []).map((name) => ({
        name: `projects/${PROJECT_ID}/locations/${location}/secrets/${name}`,
      })));
    }
    if (command.startsWith("secrets get-iam-policy ")) {
      const name = argv[2];
      const location = argv.find((entry) => entry.startsWith("--location="))?.slice(11) ?? null;
      this.assertEndpoint(execution, location);
      return ok(this.secretPolicies.get(location === null ? name : `${location}/${name}`) ?? { bindings: [] });
    }

    const mutationResult = this.applyMutation(argv);
    if (mutationResult) return mutationResult;
    throw new Error(`unexpected fake gcloud call: ${command}`);
  }

  applyMutation(argv) {
    const command = argv.join(" ");
    const isMutation =
      command.startsWith("services enable ") ||
      command.startsWith("iam workload-identity-pools create ") ||
      command.startsWith("iam workload-identity-pools providers create-oidc ") ||
      command.startsWith("iam service-accounts create ") ||
      command.startsWith("iam roles create ") ||
      command.startsWith("iam roles update ") ||
      command.includes(" add-iam-policy-binding ") ||
      command.includes(" remove-iam-policy-binding ");
    if (!isMutation) return null;
    this.mutationCount += 1;
    this.mutationCommands.push(command);

    if (this.permanentMutationFailure && command.startsWith(this.permanentMutationFailure)) {
      return failure("eventual-consistency mutation delay");
    }
    for (const [prefix, remaining] of this.transientMutationFailures) {
      if (remaining > 0 && command.startsWith(prefix)) {
        this.transientMutationFailures.set(prefix, remaining - 1);
        return failure("eventual-consistency mutation delay");
      }
    }

    if (command.startsWith("services enable ")) {
      for (const service of argv.slice(2).filter((entry) => entry.endsWith(".googleapis.com"))) {
        this.services.add(service);
      }
    } else if (command.startsWith("iam workload-identity-pools create ")) {
      if (argv[3] === POOL_ID) this.pool = exactPool();
      else if (argv[3] === ROLLBACK_POOL_ID) this.rollbackPool = exactRollbackPool();
      else throw new Error(`unexpected workload identity pool creation: ${command}`);
    } else if (command.startsWith("iam workload-identity-pools providers create-oidc ")) {
      if (argv[4] === PROVIDER_ID) {
        this.provider = exactProvider();
        this.providers = [this.provider];
      } else if (argv[4] === ROLLBACK_PROVIDER_ID) {
        this.rollbackProvider = exactRollbackProvider();
        this.rollbackProviders = [this.rollbackProvider];
      } else {
        throw new Error(`unexpected workload identity provider creation: ${command}`);
      }
    } else if (command.startsWith("iam service-accounts create ")) {
      if (argv[3] === "clearra-github-builder") {
        this.builder = exactBuilder();
        this.serviceAccountPolicies.set(BUILDER_EMAIL, { bindings: [] });
      } else if (argv[3] === "clearra-github-deployer") {
        this.deployer = exactDeployer();
        this.serviceAccountPolicies.set(DEPLOYER_EMAIL, { bindings: [] });
      } else if (argv[3] === "clearra-github-rollback") {
        this.rollback = exactRollback();
        this.serviceAccountPolicies.set(ROLLBACK_EMAIL, { bindings: [] });
      } else {
        throw new Error(`unexpected service account creation: ${command}`);
      }
    } else if (command.startsWith("iam roles create ")) {
      if (argv[3] === DEPLOYER_RUN_ROLE_ID) this.deployerRunRole = exactDeployerRunRole();
      else if (argv[3] === ROLLBACK_RUN_ROLE_ID) this.rollbackRunRole = exactRollbackRunRole();
      else throw new Error(`unexpected custom role creation: ${command}`);
    } else if (command.startsWith("iam roles update ")) {
      if (argv[3] === ROLLBACK_RUN_ROLE_ID) this.rollbackRunRole = exactRollbackRunRole();
      else throw new Error(`unexpected custom role update: ${command}`);
    } else {
      const action = command.includes(" remove-iam-policy-binding ") ? "remove" : "add";
      const member = value(argv, "--member=");
      const role = value(argv, "--role=");
      let policy;
      if (argv[0] === "projects") policy = this.projectPolicy;
      else if (argv[0] === "iam") policy = this.serviceAccountPolicies.get(argv[3]);
      else if (argv[0] === "artifacts") policy = this.repositoryPolicy;
      else if (argv[0] === "storage") policy = this.bucketPolicy;
      if (!policy) throw new Error(`fake mutation policy unavailable: ${command}`);
      if (action === "add") addRole(policy, member, role);
      else removeRole(policy, member, role);
    }

    if (
      this.ambiguousMutation &&
      !this.ambiguousReturned &&
      command.startsWith(this.ambiguousMutation)
    ) {
      this.ambiguousReturned = true;
      return failure("ambiguous transport failure");
    }
    this.pendingPlanStartFailures = this.failPlanStartsAfterMutation;
    return ok({});
  }

  assertEndpoint(execution, location) {
    const expected = location === null
      ? "https://secretmanager.googleapis.com/"
      : `https://secretmanager.${location}.rep.googleapis.com/`;
    assert.equal(execution?.environment?.[SECRET_ENDPOINT_ENV], expected);
  }
}

function exactPool() {
  return {
    name: POOL_NAME,
    state: "ACTIVE",
    disabled: false,
    displayName: "Clearra GitHub Actions",
    description: "Exact GitHub OIDC pool for daejunnom/Clearra",
  };
}

function exactRollbackPool() {
  return {
    name: ROLLBACK_POOL_NAME,
    state: "ACTIVE",
    disabled: false,
    displayName: "Clearra GitHub rollback",
    description: "Exact recovery-workflow OIDC pool for daejunnom/Clearra",
  };
}

function exactProvider() {
  return {
    name: PROVIDER_NAME,
    state: "ACTIVE",
    disabled: false,
    displayName: "Clearra main GitHub OIDC",
    description: "Exact workflow/main provider for daejunnom/Clearra",
    oidc: { issuerUri: "https://token.actions.githubusercontent.com" },
    attributeCondition:
      "assertion.repository == 'daejunnom/Clearra' && assertion.repository_id == '1309293231' && assertion.repository_owner_id == '271715321' && assertion.ref == 'refs/heads/main' && assertion.workflow_ref == 'daejunnom/Clearra/.github/workflows/discord-deploy.yml@refs/heads/main'",
    attributeMapping: {
      "attribute.ref": "assertion.ref",
      "attribute.repository": "assertion.repository",
      "attribute.repository_id": "assertion.repository_id",
      "attribute.repository_owner_id": "assertion.repository_owner_id",
      "attribute.workflow_ref": "assertion.workflow_ref",
      "google.subject": "assertion.sub",
    },
  };
}

function exactRollbackProvider() {
  return {
    name: ROLLBACK_PROVIDER_NAME,
    state: "ACTIVE",
    disabled: false,
    displayName: "Clearra rollback GitHub OIDC",
    description: "Exact recovery-workflow/main provider for Clearra runtime rollback",
    oidc: { issuerUri: "https://token.actions.githubusercontent.com" },
    attributeCondition:
      "assertion.repository == 'daejunnom/Clearra' && assertion.repository_id == '1309293231' && assertion.repository_owner_id == '271715321' && assertion.ref == 'refs/heads/main' && assertion.workflow_ref == 'daejunnom/Clearra/.github/workflows/discord-deploy-recovery.yml@refs/heads/main'",
    attributeMapping: {
      "attribute.ref": "assertion.ref",
      "attribute.repository": "assertion.repository",
      "attribute.repository_id": "assertion.repository_id",
      "attribute.repository_owner_id": "assertion.repository_owner_id",
      "attribute.workflow_ref": "assertion.workflow_ref",
      "google.subject": "assertion.sub",
    },
  };
}

function exactDeployerRunRole() {
  return {
    name: DEPLOYER_RUN_ROLE,
    title: "Clearra GitHub runtime deployer",
    description: "Exact Cloud Run service update, traffic rollback, and ephemeral smoke-job authority",
    stage: "GA",
    deleted: false,
    includedPermissions: [...DEPLOYER_RUN_PERMISSIONS],
  };
}

function exactRollbackRunRole() {
  return {
    name: ROLLBACK_RUN_ROLE,
    title: "Clearra GitHub runtime rollback",
    description: "Exact existing-service Cloud Run traffic rollback and readback authority",
    stage: "GA",
    deleted: false,
    includedPermissions: [...ROLLBACK_RUN_PERMISSIONS],
  };
}

function exactBuilder() {
  return {
    ...serviceAccount(BUILDER_EMAIL),
    displayName: "Clearra GitHub builder",
    description: "Keyless least-privilege exact-source image build identity",
  };
}

function exactRollback() {
  return {
    ...serviceAccount(ROLLBACK_EMAIL),
    displayName: "Clearra GitHub rollback",
    description: "Keyless protected existing-service traffic recovery identity",
  };
}

function exactDeployer() {
  return {
    ...serviceAccount(DEPLOYER_EMAIL),
    displayName: "Clearra GitHub deployer",
    description: "Keyless least-privilege protected Cloud Run deployment identity",
  };
}

function serviceAccount(email) {
  return {
    email,
    name: `projects/${PROJECT_ID}/serviceAccounts/${email}`,
    disabled: false,
  };
}

function trustedHumanPolicy(includeLegacyBuildUser) {
  return {
    bindings: [
      { role: "roles/iam.serviceAccountAdmin", members: [PRIMARY_HUMAN_MEMBER] },
      ...(includeLegacyBuildUser
        ? [{ role: "roles/iam.serviceAccountUser", members: [LEGACY_BUILD_USER_MEMBER] }]
        : []),
      { role: "roles/owner", members: [PRIMARY_HUMAN_MEMBER] },
    ],
  };
}

function addRole(policy, member, role) {
  const existing = policy.bindings.find((binding) => binding.role === role && binding.condition === undefined);
  if (existing) {
    if (!existing.members.includes(member)) existing.members.push(member);
    return;
  }
  policy.bindings.push({ role, members: [member] });
}

function removeRole(policy, member, role) {
  for (const binding of policy.bindings) {
    if (binding.role === role) binding.members = binding.members.filter((entry) => entry !== member);
  }
  policy.bindings = policy.bindings.filter((binding) => binding.members.length > 0);
}

function roles(policy, member) {
  return policy.bindings
    .filter((binding) => binding.members.includes(member))
    .map((binding) => binding.role)
    .sort();
}

function value(argv, prefix) {
  return argv.find((entry) => entry.startsWith(prefix))?.slice(prefix.length);
}

function ok(value_) {
  return { status: 0, stdout: JSON.stringify(value_), stderr: "", error: null };
}

function failure(stderr) {
  return { status: 1, stdout: "", stderr, error: null };
}
