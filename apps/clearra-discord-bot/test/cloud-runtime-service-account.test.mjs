import assert from "node:assert/strict";
import test from "node:test";

import {
  gcloudProcessEnvironment,
  gcloudProcessInvocation,
  prepareCloudRuntimeServiceAccount,
} from "../scripts/prepare-cloud-runtime-service-account.mjs";

const PROJECT_ID = "clearra-cloud";
const PROJECT_NUMBER = "50060711800";
const CALLER = "user:release-owner@example.test";
const CALLER_EMAIL = "release-owner@example.test";
const RUNTIME_EMAIL = `clearra-current-job@${PROJECT_ID}.iam.gserviceaccount.com`;
const RUNTIME_MEMBER = `serviceAccount:${RUNTIME_EMAIL}`;
const BUILD_EMAIL = `clearra-build@${PROJECT_ID}.iam.gserviceaccount.com`;
const ACCESSOR = "roles/secretmanager.secretAccessor";
const SECRET_MANAGER_ENDPOINT_ENV = "CLOUDSDK_API_ENDPOINT_OVERRIDES_SECRETMANAGER";
const GLOBAL_SECRET_MANAGER_ENDPOINT = "https://secretmanager.googleapis.com/";
const DEFAULT_REGIONAL_LOCATIONS = Object.freeze(["asia-northeast1", "us-central1"]);
const REQUIRED_LEAST_PRIVILEGE_PROJECT_ROLES = Object.freeze([
  "roles/cloudbuild.builds.editor",
  "roles/run.admin",
  "roles/iam.serviceAccountUser",
  "roles/secretmanager.viewer",
  "roles/iam.serviceAccountViewer",
  "roles/serviceusage.serviceUsageConsumer",
  "roles/iam.securityReviewer",
  "roles/iam.denyReviewer",
]);

test("cloud runtime service account bootstrap is idempotent and reads no Secret values", async () => {
  const cli = new FakeGcloud();
  const first = await prepare(cli);

  assert.equal(first.created, true);
  assert.equal(first.accessorAdded, true);
  assert.equal(first.runtimeServiceAccount, RUNTIME_EMAIL);
  assert.equal(cli.mutations("iam service-accounts create"), 1);
  assert.equal(cli.mutations("secrets add-iam-policy-binding"), 1);

  const second = await prepare(cli);
  assert.equal(second.created, false);
  assert.equal(second.accessorAdded, false);
  assert.equal(cli.mutations("iam service-accounts create"), 1);
  assert.equal(cli.mutations("secrets add-iam-policy-binding"), 1);
  assert.equal(
    cli.calls.some((arguments_) => arguments_.join(" ").includes("versions access")),
    false,
  );
  assert.equal(
    cli.calls.some((arguments_) => arguments_.join(" ").includes("print-access-token")),
    false,
  );
});

test("cloud runtime service account rejects project roles and non-job Secret access", async () => {
  const projectRole = new FakeGcloud({ runtimeExists: true, jobAccessor: true });
  projectRole.projectPolicy.bindings.push({
    role: "roles/logging.logWriter",
    members: [RUNTIME_MEMBER],
  });
  await assert.rejects(prepare(projectRole), /zero project-level roles/);
  assert.equal(projectRole.mutationCount, 0);

  const discordAccess = new FakeGcloud({ runtimeExists: true, jobAccessor: true });
  discordAccess.secretPolicies.get("discord-bot-token").bindings.push({
    role: ACCESSOR,
    members: [RUNTIME_MEMBER],
  });
  await assert.rejects(prepare(discordAccess), /must not access the Discord token Secret/);
  assert.equal(discordAccess.mutationCount, 0);

  const otherAccess = new FakeGcloud({ runtimeExists: true, jobAccessor: true });
  otherAccess.secrets.push("unrelated-private-key");
  otherAccess.secretPolicies.set("unrelated-private-key", {
    bindings: [{ role: "roles/secretmanager.viewer", members: [RUNTIME_MEMBER] }],
  });
  await assert.rejects(prepare(otherAccess), /must not access any non-job Secret/);
  assert.equal(otherAccess.mutationCount, 0);

  const conditionalAccessor = new FakeGcloud({ runtimeExists: true });
  conditionalAccessor.secretPolicies.set("clearra-job-token", {
    bindings: [{
      role: ACCESSOR,
      members: [RUNTIME_MEMBER],
      condition: { title: "temporary", expression: "request.time < timestamp('2030-01-01T00:00:00Z')" },
    }],
  });
  await assert.rejects(prepare(conditionalAccessor), /non-canonical job Secret authority/);
  assert.equal(conditionalAccessor.mutationCount, 0);
});

test("cloud runtime service account rejects inherited access and unknown group evaluation", async () => {
  const inheritedAccess = new FakeGcloud({ runtimeExists: true, jobAccessor: true });
  inheritedAccess.effectiveSecretAccess.set("discord-bot-token", true);
  await assert.rejects(prepare(inheritedAccess), /effective non-job Secret access/);
  assert.equal(inheritedAccess.mutationCount, 0);

  const unknownGroup = new FakeGcloud({
    runtimeExists: true,
    jobAccessor: true,
    unknownEffectiveSecrets: ["discord-bot-token"],
  });
  await assert.rejects(prepare(unknownGroup), /unknown policy state/);
  assert.equal(unknownGroup.mutationCount, 0);

  const disabledApi = new FakeGcloud({ policyTroubleshooterEnabled: false });
  await assert.rejects(prepare(disabledApi), /must be enabled as an explicit prerequisite/);
  assert.equal(disabledApi.mutationCount, 0);

  const troubleshooterError = new FakeGcloud({ troubleshooterFault: "error" });
  await assert.rejects(prepare(troubleshooterError), /unavailable or incomplete/);
  assert.equal(troubleshooterError.mutationCount, 0);

  const mismatchedIdentity = new FakeGcloud({ troubleshooterFault: "identity" });
  await assert.rejects(prepare(mismatchedIdentity), /response identity is invalid/);
  assert.equal(mismatchedIdentity.mutationCount, 0);

  const evaluationError = new FakeGcloud({ troubleshooterFault: "evaluation-error" });
  await assert.rejects(prepare(evaluationError), /contains evaluation errors/);
  assert.equal(evaluationError.mutationCount, 0);

  const incomplete = new FakeGcloud({ troubleshooterFault: "missing-explanation" });
  await assert.rejects(prepare(incomplete), /response is incomplete/);
  assert.equal(incomplete.mutationCount, 0);
});

test("cloud runtime service account proves effective exclusivity before a job binding write", async () => {
  const existingOverprivileged = new FakeGcloud({
    runtimeExists: true,
    effectiveSecretAccess: [["discord-bot-token", true]],
  });
  await assert.rejects(prepare(existingOverprivileged), /effective non-job Secret access/);
  assert.equal(existingOverprivileged.mutations("secrets add-iam-policy-binding"), 0);
  assert.equal(existingOverprivileged.mutationCount, 0);

  const newlyCreatedOverprivileged = new FakeGcloud({
    effectiveSecretAccess: [["discord-bot-token", true]],
  });
  await assert.rejects(prepare(newlyCreatedOverprivileged), /effective non-job Secret access/);
  assert.equal(newlyCreatedOverprivileged.mutations("iam service-accounts create"), 1);
  assert.equal(newlyCreatedOverprivileged.mutations("secrets add-iam-policy-binding"), 0);

  const inheritedJobAccess = new FakeGcloud({
    runtimeExists: true,
    effectiveSecretAccess: [["clearra-job-token", true]],
  });
  await assert.rejects(prepare(inheritedJobAccess), /inherited effective job Secret access/);
  assert.equal(inheritedJobAccess.mutations("secrets add-iam-policy-binding"), 0);
});

test("cloud runtime service account checks every regional Secret with its exact endpoint", async () => {
  const regionalKey = secretKey("asia-northeast1", "regional-private-key");
  const inheritedRegionalAccess = new FakeGcloud({
    runtimeExists: true,
    regionalSecrets: [["asia-northeast1", ["regional-private-key"]]],
    effectiveSecretAccess: [[regionalKey, true]],
  });
  await assert.rejects(prepare(inheritedRegionalAccess), /effective non-job Secret access/);
  assert.equal(inheritedRegionalAccess.mutations("secrets add-iam-policy-binding"), 0);

  const directRegionalAccess = new FakeGcloud({
    runtimeExists: true,
    jobAccessor: true,
    regionalSecrets: [["asia-northeast1", ["regional-private-key"]]],
  });
  directRegionalAccess.secretPolicies.set(regionalKey, {
    bindings: [{ role: ACCESSOR, members: [RUNTIME_MEMBER] }],
  });
  await assert.rejects(prepare(directRegionalAccess), /must not access any non-job Secret/);
  assert.equal(directRegionalAccess.mutations("secrets add-iam-policy-binding"), 0);

  const unknownRegionalGroup = new FakeGcloud({
    runtimeExists: true,
    jobAccessor: true,
    regionalSecrets: [["asia-northeast1", ["regional-private-key"]]],
    unknownEffectiveSecrets: [regionalKey],
  });
  await assert.rejects(prepare(unknownRegionalGroup), /unknown policy state/);
  assert.equal(unknownRegionalGroup.mutations("secrets add-iam-policy-binding"), 0);

  const regionalJobLeaf = secretKey("asia-northeast1", "clearra-job-token");
  const sameLeafButNonJob = new FakeGcloud({
    runtimeExists: true,
    jobAccessor: true,
    regionalSecrets: [["asia-northeast1", ["clearra-job-token"]]],
  });
  const result = await prepare(sameLeafButNonJob);
  assert.equal(result.accessorAdded, false);
  sameLeafButNonJob.effectiveSecretAccess.set(regionalJobLeaf, true);
  await assert.rejects(prepare(sameLeafButNonJob), /effective non-job Secret access/);
  assert.equal(sameLeafButNonJob.mutations("secrets add-iam-policy-binding"), 0);
});

test("cloud runtime service account rejects incomplete regional catalogs before mutation", async () => {
  const cases = [
    [new FakeGcloud({ locationCatalogFault: "empty" }), /location catalog is invalid or empty/],
    [new FakeGcloud({ locationCatalogFault: "duplicate" }), /duplicate resources/],
    [new FakeGcloud({ locationCatalogFault: "malformed" }), /invalid resource/],
    [new FakeGcloud({ locationResourceParent: "999999999999" }), /invalid resource/],
    [new FakeGcloud({ regionalCatalogFailure: "asia-northeast1" }), /regional Secret metadata catalog .* lookup failed/],
    [new FakeGcloud({ regionalResourceLocation: "us-west1", regionalSecrets: [["asia-northeast1", ["private"]]] }), /invalid resource/],
  ];
  for (const [cli, expectedError] of cases) {
    await assert.rejects(prepare(cli), expectedError);
    assert.equal(cli.mutationCount, 0);
  }
});

test("cloud runtime service account rejects Secret catalog drift around post-binding proof", async () => {
  for (const [drift, expectedBindingMutations] of [
    [{ secretCatalogDriftsAfterFirstRead: true }, 0],
    [{ locationCatalogDriftsAfterFirstRead: true }, 0],
    [{ secretCatalogDriftsAfterPreBinding: true }, 1],
    [{ locationCatalogDriftsAfterPreBinding: true }, 1],
  ]) {
    const cli = new FakeGcloud({
      runtimeExists: true,
      ...drift,
    });
    await assert.rejects(prepare(cli), /catalog drifted during bootstrap/);
    assert.equal(
      cli.mutations("secrets add-iam-policy-binding"),
      expectedBindingMutations,
    );
  }
});

test("cloud runtime service account accepts the exact numeric Secret parent only", async () => {
  const numericParent = new FakeGcloud({ runtimeExists: true, jobAccessor: true });
  const result = await prepare(numericParent);
  assert.equal(result.runtimeServiceAccount, RUNTIME_EMAIL);

  const wrongParent = new FakeGcloud({
    runtimeExists: true,
    jobAccessor: true,
    secretResourceParent: "999999999999",
  });
  await assert.rejects(prepare(wrongParent), /invalid resource/);
  assert.equal(wrongParent.mutationCount, 0);
});

test("cloud runtime service account pins parentless ancestry and zero PAB bindings", async () => {
  const numericProjectId = new FakeGcloud({
    ancestry: [{ id: PROJECT_NUMBER, type: "project" }],
  });
  await assert.rejects(prepare(numericProjectId), /requires an exact parentless project/);
  assert.equal(numericProjectId.mutationCount, 0);

  const parented = new FakeGcloud({
    ancestry: [
      { id: PROJECT_ID, type: "project" },
      { id: "example-folder", type: "folder" },
    ],
  });
  await assert.rejects(prepare(parented), /requires an exact parentless project/);
  assert.equal(parented.mutationCount, 0);

  const pabBound = new FakeGcloud({ pabBindings: [{ name: "policyBindings/example" }] });
  await assert.rejects(prepare(pabBound), /requires zero project PAB bindings/);
  assert.equal(pabBound.mutationCount, 0);

  const deniedPabSearch = new FakeGcloud({ denyPabSearch: true });
  await assert.rejects(prepare(deniedPabSearch), /project PAB binding search .* lookup failed/);
  assert.equal(deniedPabSearch.mutationCount, 0);

  const ancestryDrift = new FakeGcloud({
    runtimeExists: true,
    jobAccessor: true,
    ancestryDriftsAfterFirstCheck: true,
  });
  await assert.rejects(prepare(ancestryDrift), /requires an exact parentless project/);

  const pabDrift = new FakeGcloud({
    runtimeExists: true,
    jobAccessor: true,
    pabDriftsAfterFirstCheck: true,
  });
  await assert.rejects(prepare(pabDrift), /requires zero project PAB bindings/);
});

test("cloud runtime service account enforces fake metadata and getIamPolicy permissions", async () => {
  const noSecretViewer = new FakeGcloud({
    runtimeExists: true,
    jobAccessor: true,
    callerProjectRoles: REQUIRED_LEAST_PRIVILEGE_PROJECT_ROLES.filter(
      (role) => !["roles/secretmanager.viewer", "roles/iam.securityReviewer"].includes(role),
    ),
    callerRepositoryRoles: ["roles/artifactregistry.reader"],
  });
  await assert.rejects(prepare(noSecretViewer), /lacks project Secret Manager Viewer/);
  assert.equal(noSecretViewer.mutationCount, 0);

  const noServiceAccountViewer = new FakeGcloud({
    runtimeExists: true,
    jobAccessor: true,
    callerProjectRoles: REQUIRED_LEAST_PRIVILEGE_PROJECT_ROLES.filter(
      (role) => !["roles/iam.serviceAccountViewer", "roles/iam.securityReviewer"].includes(role),
    ),
    callerRepositoryRoles: ["roles/artifactregistry.reader"],
  });
  await assert.rejects(prepare(noServiceAccountViewer), /lacks service-account getIamPolicy/);
  assert.equal(noServiceAccountViewer.mutationCount, 0);

  const fakeDeniedRead = new FakeGcloud({
    runtimeExists: true,
    jobAccessor: true,
    denyServiceAccountPolicyRead: true,
  });
  await assert.rejects(prepare(fakeDeniedRead), /build service account IAM policy lookup failed/);
  assert.equal(fakeDeniedRead.mutationCount, 0);

  const fakeDeniedSecretMetadata = new FakeGcloud({
    runtimeExists: true,
    jobAccessor: true,
    denySecretMetadataRead: true,
  });
  await assert.rejects(prepare(fakeDeniedSecretMetadata), /Secret metadata catalog lookup failed/);
  assert.equal(fakeDeniedSecretMetadata.mutationCount, 0);
});

test("cloud runtime service account requires build and deploy caller authority", async () => {
  const leastPrivilege = new FakeGcloud({
    runtimeExists: true,
    jobAccessor: true,
    callerProjectRoles: REQUIRED_LEAST_PRIVILEGE_PROJECT_ROLES,
    callerRepositoryRoles: ["roles/artifactregistry.reader"],
  });
  const result = await prepare(leastPrivilege);
  assert.equal(result.created, false);

  const exactSecretPolicyWriter = new FakeGcloud({
    runtimeExists: true,
    callerProjectRoles: REQUIRED_LEAST_PRIVILEGE_PROJECT_ROLES,
    callerRepositoryRoles: ["roles/artifactregistry.reader"],
    callerCanSetJobPolicy: true,
  });
  const exactWriterResult = await prepare(exactSecretPolicyWriter);
  assert.equal(exactWriterResult.accessorAdded, true);
  assert.equal(exactSecretPolicyWriter.mutations("secrets add-iam-policy-binding"), 1);

  const noBuildActAs = new FakeGcloud({
    runtimeExists: true,
    jobAccessor: true,
    callerProjectRoles: REQUIRED_LEAST_PRIVILEGE_PROJECT_ROLES.filter(
      (role) => role !== "roles/iam.serviceAccountUser",
    ),
    callerRepositoryRoles: ["roles/artifactregistry.reader"],
  });
  await assert.rejects(prepare(noBuildActAs), /cannot act as the build service account/);

  const noArtifactRead = new FakeGcloud({
    runtimeExists: true,
    jobAccessor: true,
    callerProjectRoles: REQUIRED_LEAST_PRIVILEGE_PROJECT_ROLES,
  });
  await assert.rejects(prepare(noArtifactRead), /cannot read the release image repository/);

  const noRunAdmin = new FakeGcloud({
    runtimeExists: true,
    jobAccessor: true,
    callerProjectRoles: [
      ...REQUIRED_LEAST_PRIVILEGE_PROJECT_ROLES.filter((role) => role !== "roles/run.admin"),
      "roles/artifactregistry.reader",
    ],
  });
  await assert.rejects(prepare(noRunAdmin), /cannot update the public Cloud Run service/);

  const noSecretAdmin = new FakeGcloud({
    runtimeExists: true,
    callerProjectRoles: [
      ...REQUIRED_LEAST_PRIVILEGE_PROJECT_ROLES,
      "roles/artifactregistry.reader",
    ],
    callerCanSetJobPolicy: false,
  });
  await assert.rejects(prepare(noSecretAdmin), /cannot bind the exact job bearer Secret/);
  assert.equal(noSecretAdmin.mutationCount, 0);
});

test("cloud runtime service account re-observes ambiguous IAM mutations", async () => {
  const appliedDespiteError = new FakeGcloud({
    ambiguousCreate: true,
    ambiguousAccessor: true,
  });
  const result = await prepare(appliedDespiteError);
  assert.equal(result.created, true);
  assert.equal(result.accessorAdded, true);

  const unappliedCreate = new FakeGcloud({ failedCreate: true });
  await assert.rejects(prepare(unappliedCreate), /runtime service account creation failed/);
  assert.equal(unappliedCreate.mutations("secrets add-iam-policy-binding"), 0);
});

test("cloud runtime service account invokes the Windows gcloud shim without a Node shell", () => {
  assert.deepEqual(
    gcloudProcessInvocation(
      ["projects", "get-iam-policy", PROJECT_ID, "--format=json"],
      "win32",
      { ComSpec: "C:\\Windows\\System32\\cmd.exe" },
    ),
    {
      command: "C:\\Windows\\System32\\cmd.exe",
      arguments: [
        "/d",
        "/s",
        "/c",
        "gcloud.cmd",
        "projects",
        "get-iam-policy",
        PROJECT_ID,
        "--format=json",
      ],
    },
  );
  assert.deepEqual(
    gcloudProcessInvocation(["config", "get-value", "account"], "linux", {}),
    {
      command: "gcloud",
      arguments: ["config", "get-value", "account"],
    },
  );

  const ambientEnvironment = {
    SENTINEL: "preserved",
    [SECRET_MANAGER_ENDPOINT_ENV]: "https://attacker.invalid/",
    cloudsdk_api_endpoint_overrides_secretmanager: "https://case-folded-attacker.invalid/",
  };
  assert.deepEqual(
    gcloudProcessEnvironment(undefined, ambientEnvironment),
    { SENTINEL: "preserved" },
  );
  assert.deepEqual(
    gcloudProcessEnvironment(
      {
        environment: {
          [SECRET_MANAGER_ENDPOINT_ENV]: GLOBAL_SECRET_MANAGER_ENDPOINT,
        },
      },
      ambientEnvironment,
    ),
    {
      SENTINEL: "preserved",
      [SECRET_MANAGER_ENDPOINT_ENV]: GLOBAL_SECRET_MANAGER_ENDPOINT,
    },
  );
  assert.equal(
    ambientEnvironment[SECRET_MANAGER_ENDPOINT_ENV],
    "https://attacker.invalid/",
  );
});

async function prepare(cli) {
  return prepareCloudRuntimeServiceAccount(
    { projectId: PROJECT_ID, callerMember: CALLER },
    {
      runGcloud: (arguments_, execution) => cli.run(arguments_, execution),
      wait: async () => {},
    },
  );
}

class FakeGcloud {
  constructor(options = {}) {
    this.calls = [];
    this.executionCalls = [];
    this.mutationCount = 0;
    this.runtimeExists = options.runtimeExists ?? false;
    this.jobAccessor = options.jobAccessor ?? false;
    this.ambiguousCreate = options.ambiguousCreate ?? false;
    this.ambiguousAccessor = options.ambiguousAccessor ?? false;
    this.failedCreate = options.failedCreate ?? false;
    this.policyTroubleshooterEnabled = options.policyTroubleshooterEnabled ?? true;
    this.projectNumber = options.projectNumber ?? PROJECT_NUMBER;
    this.secretResourceParent = options.secretResourceParent ?? this.projectNumber;
    this.locationResourceParent = options.locationResourceParent ?? this.projectNumber;
    this.regionalSecretResourceParent = options.regionalSecretResourceParent ?? this.projectNumber;
    this.regionalResourceLocation = options.regionalResourceLocation ?? null;
    this.denyServiceAccountPolicyRead = options.denyServiceAccountPolicyRead ?? false;
    this.denySecretMetadataRead = options.denySecretMetadataRead ?? false;
    this.denyLocationCatalogRead = options.denyLocationCatalogRead ?? false;
    this.regionalCatalogFailure = options.regionalCatalogFailure ?? null;
    this.locationCatalogFault = options.locationCatalogFault ?? null;
    this.locationCatalogDriftsAfterFirstRead = options.locationCatalogDriftsAfterFirstRead ?? false;
    this.locationCatalogDriftsAfterPreBinding = options.locationCatalogDriftsAfterPreBinding ?? false;
    this.secretCatalogDriftsAfterFirstRead = options.secretCatalogDriftsAfterFirstRead ?? false;
    this.secretCatalogDriftsAfterPreBinding = options.secretCatalogDriftsAfterPreBinding ?? false;
    this.ancestry = options.ancestry ?? [{ id: PROJECT_ID, type: "project" }];
    this.pabBindings = options.pabBindings ?? [];
    this.denyPabSearch = options.denyPabSearch ?? false;
    this.ancestryDriftsAfterFirstCheck = options.ancestryDriftsAfterFirstCheck ?? false;
    this.pabDriftsAfterFirstCheck = options.pabDriftsAfterFirstCheck ?? false;
    this.ancestryChecks = 0;
    this.pabChecks = 0;
    this.locationCatalogChecks = 0;
    this.globalSecretCatalogChecks = 0;
    this.unknownEffectiveSecrets = new Set(options.unknownEffectiveSecrets ?? []);
    this.troubleshooterFault = options.troubleshooterFault ?? null;
    this.effectiveSecretAccess = new Map(options.effectiveSecretAccess ?? []);
    this.secrets = ["clearra-job-token", "discord-bot-token"];
    this.regionalLocations = [...(options.regionalLocations ?? DEFAULT_REGIONAL_LOCATIONS)];
    this.regionalSecrets = new Map(
      this.regionalLocations.map((location) => [location, []]),
    );
    for (const [location, names] of options.regionalSecrets ?? []) {
      this.regionalSecrets.set(location, [...names]);
    }
    const callerProjectRoles = options.callerProjectRoles ?? ["roles/owner"];
    this.callerCanSetJobPolicy = options.callerCanSetJobPolicy ??
      callerProjectRoles.some((role) => ["roles/owner", "roles/secretmanager.admin"].includes(role));
    this.projectPolicy = {
      bindings: callerProjectRoles.map((role) => ({ role, members: [CALLER] })),
    };
    this.repositoryPolicy = {
      bindings: (options.callerRepositoryRoles ?? []).map((role) => ({
        role,
        members: [CALLER],
      })),
    };
    this.buildPolicy = { bindings: [] };
    this.runtimePolicy = { bindings: [] };
    this.secretPolicies = new Map([
      [
        "clearra-job-token",
        {
          bindings: this.jobAccessor
            ? [{ role: ACCESSOR, members: [RUNTIME_MEMBER] }]
            : [],
        },
      ],
      ["discord-bot-token", { bindings: [] }],
    ]);
    for (const [location, names] of this.regionalSecrets) {
      for (const name of names) {
        this.secretPolicies.set(secretKey(location, name), { bindings: [] });
      }
    }
  }

  mutations(prefix) {
    return this.calls.filter((arguments_) => arguments_.join(" ").startsWith(prefix)).length;
  }

  assertSecretManagerExecution(execution, location = null) {
    const endpoint = location === null
      ? GLOBAL_SECRET_MANAGER_ENDPOINT
      : `https://secretmanager.${location}.rep.googleapis.com/`;
    assert.deepEqual(execution, {
      environment: {
        [SECRET_MANAGER_ENDPOINT_ENV]: endpoint,
      },
    });
  }

  run(arguments_, execution = undefined) {
    this.calls.push([...arguments_]);
    this.executionCalls.push({ arguments: [...arguments_], execution });
    const command = arguments_.join(" ");

    if (command.startsWith("projects describe ")) {
      return ok({ projectId: PROJECT_ID, projectNumber: this.projectNumber });
    }
    if (command.startsWith("projects get-ancestors ")) {
      this.ancestryChecks += 1;
      if (this.ancestryDriftsAfterFirstCheck && this.ancestryChecks > 1) {
        return ok([
          { id: PROJECT_ID, type: "project" },
          { id: "example-folder", type: "folder" },
        ]);
      }
      return ok(this.ancestry);
    }
    if (command.startsWith("projects get-iam-policy ")) {
      return ok(this.projectPolicy);
    }
    if (command.startsWith("services list --enabled ")) {
      return ok(this.policyTroubleshooterEnabled
        ? [{ config: { name: "policytroubleshooter.googleapis.com" }, state: "ENABLED" }]
        : []);
    }
    if (command.startsWith("iam policy-bindings search-target-policy-bindings ")) {
      this.pabChecks += 1;
      if (this.denyPabSearch) return failure("PERMISSION_DENIED");
      if (this.pabDriftsAfterFirstCheck && this.pabChecks > 1) {
        return ok([{ name: "policyBindings/drifted" }]);
      }
      return ok(this.pabBindings);
    }
    if (command.startsWith("artifacts repositories get-iam-policy ")) {
      return ok(this.repositoryPolicy);
    }
    if (command.startsWith("iam service-accounts describe ")) {
      const email = arguments_[3];
      if (email === BUILD_EMAIL) return ok({ email: BUILD_EMAIL, disabled: false });
      if (email === RUNTIME_EMAIL && this.runtimeExists) {
        return ok({ email: RUNTIME_EMAIL, disabled: false });
      }
      return failure("NOT_FOUND: Unknown service account");
    }
    if (command.startsWith("iam service-accounts get-iam-policy ")) {
      if (this.denyServiceAccountPolicyRead) return failure("PERMISSION_DENIED");
      return ok(arguments_[3] === BUILD_EMAIL ? this.buildPolicy : this.runtimePolicy);
    }
    if (command.startsWith("secrets locations list ")) {
      this.assertSecretManagerExecution(execution);
      if (this.denyLocationCatalogRead) return failure("PERMISSION_DENIED");
      if (this.locationCatalogFault === "empty") return ok([]);
      this.locationCatalogChecks += 1;
      const locations = [...this.regionalLocations];
      if (
        (this.locationCatalogDriftsAfterFirstRead && this.locationCatalogChecks > 1) ||
        (this.locationCatalogDriftsAfterPreBinding && this.locationCatalogChecks > 2)
      ) {
        locations.push("us-west1");
      }
      const entries = locations.map((location) => ({
        name: `projects/${this.locationResourceParent}/locations/${location}`,
      }));
      if (this.locationCatalogFault === "duplicate" && entries.length > 0) {
        entries.push({ ...entries[0] });
      }
      if (this.locationCatalogFault === "malformed") {
        entries.push({ name: `projects/${this.locationResourceParent}/locations/GLOBAL` });
      }
      return ok(entries);
    }
    if (command.startsWith("secrets list ")) {
      const locationArgument = arguments_.find((value) => value.startsWith("--location="));
      const location = locationArgument?.slice("--location=".length) ?? null;
      this.assertSecretManagerExecution(execution, location);
      if (this.denySecretMetadataRead) return failure("PERMISSION_DENIED");
      if (location !== null) {
        if (this.regionalCatalogFailure === location) return failure("regional catalog unavailable");
        const resourceLocation = this.regionalResourceLocation ?? location;
        return ok((this.regionalSecrets.get(location) ?? []).map((name) => ({
          name: `projects/${this.regionalSecretResourceParent}/locations/${resourceLocation}/secrets/${name}`,
        })));
      }
      this.globalSecretCatalogChecks += 1;
      const names = [...this.secrets];
      if (
        (this.secretCatalogDriftsAfterFirstRead && this.globalSecretCatalogChecks > 1) ||
        (this.secretCatalogDriftsAfterPreBinding && this.globalSecretCatalogChecks > 2)
      ) {
        names.push("created-during-bootstrap");
      }
      return ok(names.map((name) => ({
        name: `projects/${this.secretResourceParent}/secrets/${name}`,
      })));
    }
    if (command.startsWith("secrets versions describe latest ")) {
      this.assertSecretManagerExecution(execution);
      return ok({ state: "ENABLED" });
    }
    if (command.startsWith("secrets get-iam-policy ")) {
      const locationArgument = arguments_.find((value) => value.startsWith("--location="));
      const location = locationArgument?.slice("--location=".length) ?? null;
      this.assertSecretManagerExecution(execution, location);
      const name = arguments_[2];
      return ok(this.secretPolicies.get(secretKey(location, name)) ?? { bindings: [] });
    }
    if (command.startsWith("policy-intelligence troubleshoot-policy iam ")) {
      assert.equal(execution?.environment?.[SECRET_MANAGER_ENDPOINT_ENV], undefined);
      const fullResourceName = arguments_[3];
      const resourceKey = secretKeyFromFullResourceName(fullResourceName);
      const principal = arguments_.find((value) => value.startsWith("--principal-email="))
        ?.slice("--principal-email=".length);
      const permission = arguments_.find((value) => value.startsWith("--permission="))
        ?.slice("--permission=".length);
      if (this.troubleshooterFault === "error") return failure("PERMISSION_DENIED");
      if (this.unknownEffectiveSecrets.has(resourceKey) && principal === RUNTIME_EMAIL) {
        return ok(troubleshooterResponse({
          principal,
          fullResourceName,
          permission,
          granted: false,
          unknown: true,
        }));
      }
      let granted = false;
      if (permission === "secretmanager.secrets.setIamPolicy") {
        granted = principal === CALLER_EMAIL && this.callerCanSetJobPolicy;
      } else if (permission === "secretmanager.versions.access" && principal === RUNTIME_EMAIL) {
        granted = this.effectiveSecretAccess.has(resourceKey)
          ? this.effectiveSecretAccess.get(resourceKey)
          : this.secretPolicies.get(resourceKey)?.bindings?.some((binding) =>
            binding.role === ACCESSOR && binding.members.includes(RUNTIME_MEMBER)
          ) ?? false;
      }
      const response = troubleshooterResponse({
        principal,
        fullResourceName,
        permission,
        granted,
      });
      if (this.troubleshooterFault === "identity") {
        response.accessTuple.fullResourceName += "-wrong";
      }
      if (this.troubleshooterFault === "evaluation-error") {
        response.allowPolicyExplanation.errors = [{ code: 7 }];
      }
      if (this.troubleshooterFault === "missing-explanation") {
        delete response.denyPolicyExplanation;
      }
      return ok(response);
    }
    if (command.startsWith("iam service-accounts create ")) {
      this.mutationCount += 1;
      if (this.failedCreate) return failure("creation failed");
      this.runtimeExists = true;
      return this.ambiguousCreate ? failure("ambiguous backend failure") : ok({});
    }
    if (command.startsWith("secrets add-iam-policy-binding ")) {
      this.assertSecretManagerExecution(execution);
      this.mutationCount += 1;
      this.jobAccessor = true;
      this.secretPolicies.set("clearra-job-token", {
        bindings: [{ role: ACCESSOR, members: [RUNTIME_MEMBER] }],
      });
      return this.ambiguousAccessor ? failure("ambiguous backend failure") : ok({});
    }
    throw new Error(`unexpected fake gcloud call: ${command}`);
  }
}

function secretKey(location, name) {
  return location === null ? name : `${location}/${name}`;
}

function secretKeyFromFullResourceName(fullResourceName) {
  const match = /^\/\/secretmanager\.googleapis\.com\/projects\/[^/]+\/(?:locations\/([^/]+)\/)?secrets\/([^/]+)$/u
    .exec(fullResourceName);
  assert.ok(match, "Policy Troubleshooter must receive a canonical Secret full resource name");
  return secretKey(match[1] ?? null, match[2]);
}

function ok(value) {
  return { status: 0, stdout: JSON.stringify(value), stderr: "", error: null };
}

function failure(stderr) {
  return { status: 1, stdout: "", stderr, error: null };
}

function troubleshooterResponse({
  principal,
  fullResourceName,
  permission,
  granted,
  unknown = false,
}) {
  const allowAccessState = granted
    ? "ALLOW_ACCESS_STATE_GRANTED"
    : "ALLOW_ACCESS_STATE_NOT_GRANTED";
  return {
    overallAccessState: granted ? "CAN_ACCESS" : "CANNOT_ACCESS",
    accessTuple: { principal, fullResourceName, permission },
    allowPolicyExplanation: {
      allowAccessState,
      explainedPolicies: unknown
        ? [{
          allowAccessState,
          bindingExplanations: [{
            allowAccessState,
            combinedMembership: { membership: "MEMBERSHIP_UNKNOWN_INFO" },
          }],
        }]
        : [],
    },
    denyPolicyExplanation: {
      denyAccessState: "DENY_ACCESS_STATE_NOT_DENIED",
      explainedResources: [],
    },
  };
}
