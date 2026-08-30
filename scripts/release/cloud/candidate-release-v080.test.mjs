import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { delimiter, join } from "node:path";
import test from "node:test";

import {
  CLOUD_CANDIDATE_CONTRACT,
  CLOUD_SMOKE_CONTRACT,
  buildServiceDeployArguments,
  buildSmokeJobDeployArguments,
  candidateAuthority,
  deployZeroTrafficCandidate,
  gcloudProcessInvocation,
  readSmokeLogAttestation,
  runGcloud,
  smokeZeroTrafficCandidate,
  validateSmokeExecution,
  validateSmokeJobReadback,
  validateZeroTrafficReadback,
} from "./candidate-release-v080.mjs";

const projectId = "clearra-cloud";
const sourceCommit = "1".repeat(40);
const priorRevision = "clearra-current-job-v075-0000000";
const jobBearerSecretVersion = "7";
const digest = `sha256:${"a".repeat(64)}`;
const imageBase = "asia-northeast1-docker.pkg.dev/clearra-cloud/clearra/clearra-current-job";
const imageDigest = `${imageBase}@${digest}`;
const candidateUrl = "https://candidate-1111111---clearra-current-job-test-an.a.run.app";

function authority() {
  return candidateAuthority({
    projectId,
    sourceCommit,
    priorRevision,
    jobBearerSecretVersion,
    imageDigest,
    candidateUrl,
  }, { requireImage: true, requireCandidateUrl: true });
}

function secretEnvironment(name = "CLEARRA_JOB_TOKEN") {
  return {
    name,
    valueFrom: {
      secretKeyRef: { name: "clearra-job-token", key: jobBearerSecretVersion },
    },
  };
}

function serviceFixture(overrides = {}) {
  const expected = authority();
  return {
    metadata: {
      name: "clearra-current-job",
      annotations: {
        "run.googleapis.com/maxScale": "4",
      },
    },
    status: {
      latestCreatedRevisionName: expected.candidateRevision,
      traffic: [
        { revisionName: priorRevision, percent: 100 },
        {
          revisionName: expected.candidateRevision,
          tag: expected.candidateTag,
          url: candidateUrl,
          percent: 0,
        },
      ],
    },
    spec: {
      template: {
        metadata: {
          annotations: { "run.googleapis.com/startup-cpu-boost": "true" },
        },
        spec: {
          serviceAccountName: expected.runtimeServiceAccount,
          containerConcurrency: 1,
          containers: [{
            image: imageDigest,
            env: [secretEnvironment()],
            resources: { limits: { cpu: "8", memory: "16Gi" } },
          }],
        },
      },
    },
    ...overrides,
  };
}

function revisionFixture(overrides = {}) {
  const expected = authority();
  return {
    metadata: {
      name: expected.candidateRevision,
      annotations: {
        "autoscaling.knative.dev/maxScale": "4",
        "run.googleapis.com/startup-cpu-boost": "true",
      },
    },
    status: {
      imageDigest,
      conditions: [{ type: "Ready", status: "True" }],
    },
    spec: {
      serviceAccountName: expected.runtimeServiceAccount,
      containerConcurrency: 1,
      containers: [{
        image: imageDigest,
        env: [secretEnvironment()],
        resources: { limits: { cpu: "8", memory: "16Gi" } },
      }],
    },
    ...overrides,
  };
}

function smokeJobFixture(overrides = {}) {
  const expected = authority();
  return {
    spec: {
      template: {
        spec: {
          taskCount: 1,
          parallelism: 1,
          template: {
            spec: {
              maxRetries: 0,
              timeoutSeconds: "120s",
              serviceAccountName: expected.runtimeServiceAccount,
              containers: [{
                image: imageDigest,
                command: ["node"],
                args: [
                  "./scripts/run-cloud-candidate-smoke-job.mjs",
                  "--candidate-url",
                  candidateUrl,
                  "--source-commit",
                  sourceCommit,
                ],
                env: [secretEnvironment("CLEARRA_CANDIDATE_JOB_TOKEN")],
              }],
            },
          },
        },
      },
    },
    ...overrides,
  };
}

test("deploy resolves one tag to image@sha256 and independently seals zero traffic readback", async () => {
  const calls = [];
  const runJson = async (arguments_) => {
    calls.push(arguments_);
    if (arguments_[0] === "artifacts") {
      return { image_summary: { digest, fully_qualified_digest: imageDigest } };
    }
    if (arguments_[1] === "deploy") return { metadata: { name: "ignored" } };
    if (arguments_[1] === "services") return serviceFixture();
    if (arguments_[1] === "revisions") return revisionFixture();
    throw new Error(`unexpected gcloud call: ${arguments_.join(" ")}`);
  };
  const result = await deployZeroTrafficCandidate({
    projectId,
    sourceCommit,
    priorRevision,
    jobBearerSecretVersion,
  }, { runJson });

  assert.equal(result.contract, CLOUD_CANDIDATE_CONTRACT);
  assert.equal(result.imageDigest, imageDigest);
  assert.equal(result.candidateUrl, candidateUrl);
  const deploy = calls.find((arguments_) => arguments_[1] === "deploy");
  assert.ok(deploy.includes(`--image=${imageDigest}`));
  assert.ok(deploy.includes("--no-traffic"));
  assert.ok(deploy.includes("--min=0"));
  assert.ok(deploy.includes("--min-instances=0"));
  assert.ok(deploy.includes("--max=4"));
  assert.ok(deploy.includes("--max-instances=4"));
  assert.ok(deploy.includes(
    `--set-secrets=CLEARRA_JOB_TOKEN=clearra-job-token:${jobBearerSecretVersion}`,
  ));
  assert.equal(deploy.some((argument) => argument.includes(":latest")), false);
});

test("gcloud runner uses a closed Windows command shim and native non-Windows argv", () => {
  const arguments_ = [
    "run",
    "jobs",
    "logs",
    "read",
    "clearra-candidate-smoke-111111111111",
    '--log-filter=labels.execution_name="execution-1" AND textPayload:"candidate_smoke_job=passed"',
    "--format=json",
  ];
  const comspec = "C:\\Windows\\System32\\cmd.exe";
  assert.deepEqual(
    gcloudProcessInvocation(arguments_, "win32", { ComSpec: comspec }),
    {
      command: comspec,
      arguments: ["/d", "/s", "/c", "gcloud.cmd", ...arguments_],
    },
  );
  assert.deepEqual(
    gcloudProcessInvocation(arguments_, "linux", {}),
    { command: "gcloud", arguments: arguments_ },
  );
  assert.throws(
    () => gcloudProcessInvocation(["version", "safe&whoami"], "win32", {}),
    /not a closed command surface/u,
  );
  for (const argument of ["%COMSPEC%", " --verbosity=debug ", "safe --format=json"]) {
    assert.throws(
      () => gcloudProcessInvocation(["version", argument], "win32", {}),
      /not a closed command surface/u,
    );
  }

  const calls = [];
  const output = runGcloud(["version", "--format=json"], {
    platform: "win32",
    environment: { ComSpec: comspec, SENTINEL: "preserved" },
    spawn(command, childArguments, options) {
      calls.push({ command, childArguments, options });
      return { status: 0, stdout: '{"ok":true}', stderr: "" };
    },
  });
  assert.deepEqual(JSON.parse(output), { ok: true });
  assert.equal(calls.length, 1);
  assert.equal(calls[0].command, comspec);
  assert.deepEqual(
    calls[0].childArguments,
    ["/d", "/s", "/c", "gcloud.cmd", "version", "--format=json"],
  );
  assert.equal(calls[0].options.shell, false);
  assert.equal(calls[0].options.windowsVerbatimArguments, undefined);
  assert.deepEqual(calls[0].options.env, {
    ComSpec: comspec,
    SENTINEL: "preserved",
  });
});

test(
  "Windows gcloud command shim preserves one closed argument vector without cloud access",
  { skip: process.platform !== "win32" },
  async (context) => {
    const directory = await mkdtemp(join(tmpdir(), "clearra-gcloud-shim-"));
    context.after(() => rm(directory, { recursive: true, force: true }));
    await writeFile(
      join(directory, "gcloud-argv-probe.mjs"),
      'process.stdout.write(JSON.stringify(process.argv.slice(2)));\n',
      { encoding: "utf8", mode: 0o600 },
    );
    await writeFile(
      join(directory, "gcloud.cmd"),
      '@echo off\r\nnode "%~dp0gcloud-argv-probe.mjs" %*\r\n',
      { encoding: "utf8", mode: 0o700 },
    );
    const environment = { ...process.env };
    const originalPath = Object.entries(environment)
      .find(([key]) => key.toUpperCase() === "PATH")?.[1] ?? "";
    for (const key of Object.keys(environment)) {
      if (key.toUpperCase() === "PATH") delete environment[key];
    }
    environment.PATH = `${directory}${delimiter}${originalPath}`;
    const arguments_ = [
      "run",
      "jobs",
      "logs",
      "read",
      "clearra-candidate-smoke-111111111111",
      '--log-filter=labels.execution_name="execution-1" AND textPayload:"candidate_smoke_job=passed"',
      "--format=json",
    ];
    assert.deepEqual(
      JSON.parse(runGcloud(arguments_, {
        platform: "win32",
        environment,
        spawn: spawnSync,
      })),
      arguments_,
    );
  },
);

test("deploy and readback reject tag, traffic, image, and Secret-reference drift", async () => {
  const expected = authority();
  assert.throws(
    () => validateZeroTrafficReadback({
      service: serviceFixture({
        status: {
          latestCreatedRevisionName: expected.candidateRevision,
          traffic: [{ revisionName: expected.candidateRevision, percent: 100 }],
        },
      }),
      revision: revisionFixture(),
    }, expected),
    /zero-traffic isolation/u,
  );
  assert.throws(
    () => validateZeroTrafficReadback({
      service: serviceFixture(),
      revision: revisionFixture({ status: { imageDigest: `${imageBase}@sha256:${"b".repeat(64)}` } }),
    }, expected),
    /immutable deployment digest/u,
  );
  const secretDrift = revisionFixture();
  secretDrift.spec.containers[0].env[0] = {
    name: "CLEARRA_JOB_TOKEN",
    value: "forbidden-inline-token",
  };
  assert.throws(
    () => validateZeroTrafficReadback({ service: serviceFixture(), revision: secretDrift }, expected),
    /managed job-bearer Secret reference/u,
  );
  const resourceDrift = revisionFixture();
  resourceDrift.spec.containers[0].resources.limits.cpu = "4";
  assert.throws(
    () => validateZeroTrafficReadback({ service: serviceFixture(), revision: resourceDrift }, expected),
    /resource readback drifted/u,
  );
  assert.throws(
    () => candidateAuthority({
      projectId,
      sourceCommit,
      priorRevision,
      jobBearerSecretVersion: "latest",
    }),
    /Secret version is invalid/u,
  );
});

test("scale readback accepts Cloud Run's omitted default zero and rejects explicit drift", () => {
  const expected = authority();
  assert.equal(
    validateZeroTrafficReadback({
      service: serviceFixture(),
      revision: revisionFixture(),
    }, expected),
    candidateUrl,
  );

  const explicitZeroService = serviceFixture();
  explicitZeroService.metadata.annotations["run.googleapis.com/minScale"] = "0";
  explicitZeroService.spec.scaling = { minInstanceCount: 0, maxInstanceCount: 4 };
  const explicitZeroRevision = revisionFixture();
  explicitZeroRevision.metadata.annotations["autoscaling.knative.dev/minScale"] = "0";
  explicitZeroRevision.spec.scaling = { minInstanceCount: 0, maxInstanceCount: 4 };
  assert.equal(
    validateZeroTrafficReadback({
      service: explicitZeroService,
      revision: explicitZeroRevision,
    }, expected),
    candidateUrl,
  );

  const serviceMinimumDrift = serviceFixture();
  serviceMinimumDrift.metadata.annotations["run.googleapis.com/minScale"] = "1";
  assert.throws(
    () => validateZeroTrafficReadback({
      service: serviceMinimumDrift,
      revision: revisionFixture(),
    }, expected),
    /service scale readback drifted/u,
  );

  const revisionMinimumDrift = revisionFixture();
  revisionMinimumDrift.metadata.annotations["autoscaling.knative.dev/minScale"] = "1";
  assert.throws(
    () => validateZeroTrafficReadback({
      service: serviceFixture(),
      revision: revisionMinimumDrift,
    }, expected),
    /revision scale readback drifted/u,
  );

  const conflictingMinimum = serviceFixture();
  conflictingMinimum.metadata.annotations["run.googleapis.com/minScale"] = "0";
  conflictingMinimum.spec.scaling = { minInstanceCount: 1 };
  assert.throws(
    () => validateZeroTrafficReadback({
      service: conflictingMinimum,
      revision: revisionFixture(),
    }, expected),
    /service scale readback drifted/u,
  );

  for (const invalidMinimum of [null, "", "00", "0.0"]) {
    const malformedMinimum = revisionFixture();
    malformedMinimum.metadata.annotations["autoscaling.knative.dev/minScale"] =
      invalidMinimum;
    assert.throws(
      () => validateZeroTrafficReadback({
        service: serviceFixture(),
        revision: malformedMinimum,
      }, expected),
      /revision scale readback drifted/u,
    );
  }

  const conflictingMaximum = serviceFixture();
  conflictingMaximum.spec.scaling = { maxInstanceCount: 5 };
  assert.throws(
    () => validateZeroTrafficReadback({
      service: conflictingMaximum,
      revision: revisionFixture(),
    }, expected),
    /service scale readback drifted/u,
  );

  const maximumOmitted = revisionFixture();
  delete maximumOmitted.metadata.annotations["autoscaling.knative.dev/maxScale"];
  assert.throws(
    () => validateZeroTrafficReadback({
      service: serviceFixture(),
      revision: maximumOmitted,
    }, expected),
    /revision scale readback drifted/u,
  );
});

test("smoke deploys one digest-bound managed-secret Job against the zero-traffic URL", async () => {
  const calls = [];
  let logReadCount = 0;
  const runJson = async (arguments_) => {
    calls.push(arguments_);
    if (arguments_[1] === "services") return serviceFixture();
    if (arguments_[1] === "revisions") return revisionFixture();
    if (arguments_[2] === "deploy") return { metadata: { name: "ignored" } };
    if (arguments_[2] === "describe") return smokeJobFixture();
    if (arguments_[2] === "execute") {
      return {
        metadata: { name: "projects/p/locations/r/executions/smoke-execution-1" },
        status: {
          conditions: [{ type: "Completed", status: "True" }],
          succeededCount: 1,
          failedCount: 0,
        },
      };
    }
    if (arguments_[2] === "logs" && arguments_[3] === "read") {
      logReadCount += 1;
      if (logReadCount === 1) return [];
      return [{
        labels: { execution_name: "smoke-execution-1" },
        textPayload: `candidate_smoke_job=passed source_commit=${sourceCommit} job_id=candidate-smoke-${sourceCommit.slice(0, 12)}-rs solution_set_hash=cts1:0000000000000000`,
      }];
    }
    throw new Error(`unexpected gcloud call: ${arguments_.join(" ")}`);
  };
  const run = async (arguments_) => calls.push(arguments_);
  const timestamps = [
    new Date("2026-08-30T00:00:00.000Z"),
    new Date("2026-08-30T00:00:01.000Z"),
  ];
  const waits = [];
  const result = await smokeZeroTrafficCandidate({
    projectId,
    sourceCommit,
    priorRevision,
    jobBearerSecretVersion,
    imageDigest,
    candidateUrl,
  }, {
    runJson,
    run,
    now: () => timestamps.shift(),
    wait: async (milliseconds) => waits.push(milliseconds),
  });

  assert.equal(result.schema_id, CLOUD_SMOKE_CONTRACT);
  assert.equal(result.execution_name, "smoke-execution-1");
  assert.equal(result.smoke_job, "clearra-v080-candidate-smoke-1111111");
  assert.equal(result.job_id, "candidate-smoke-111111111111-rs");
  assert.equal(result.solution_set_hash, "cts1:0000000000000000");
  assert.equal(result.zero_traffic_verified, true);
  assert.match(result.service_readback_sha256, /^[0-9a-f]{64}$/u);
  assert.match(result.revision_readback_sha256, /^[0-9a-f]{64}$/u);
  assert.match(result.execution_readback_sha256, /^[0-9a-f]{64}$/u);
  assert.match(result.report_sha256, /^[0-9a-f]{64}$/u);
  assert.deepEqual(waits, [2_000]);
  const deploy = calls.find((arguments_) => arguments_[2] === "deploy");
  assert.ok(deploy.includes(`--image=${imageDigest}`));
  assert.ok(deploy.includes("--command=node"));
  assert.ok(deploy.some((argument) => argument.includes("--candidate-url")));
  assert.ok(deploy.some((argument) => argument.includes(candidateUrl)));
  assert.ok(deploy.includes(
    `--set-secrets=CLEARRA_CANDIDATE_JOB_TOKEN=clearra-job-token:${jobBearerSecretVersion}`,
  ));
  assert.ok(calls.some((arguments_) => arguments_[2] === "delete"));
});

test("managed smoke log readback retries boundedly and rejects ambiguous attestations", async () => {
  const expected = authority();
  const exact = {
    labels: { execution_name: "execution-1" },
    textPayload: `candidate_smoke_job=passed source_commit=${sourceCommit} job_id=candidate-smoke-${sourceCommit.slice(0, 12)}-rs solution_set_hash=cts1:0000000000000000`,
  };
  let attempts = 0;
  const waits = [];
  const attestation = await readSmokeLogAttestation(
    async () => {
      attempts += 1;
      return attempts === 1 ? [] : [exact];
    },
    expected,
    "execution-1",
    { attempts: 2, wait: async (milliseconds) => waits.push(milliseconds) },
  );
  assert.equal(attestation.jobId, "candidate-smoke-111111111111-rs");
  assert.equal(attempts, 2);
  assert.deepEqual(waits, [2_000]);

  await assert.rejects(
    readSmokeLogAttestation(
      async () => [exact, exact],
      expected,
      "execution-1",
      { attempts: 1, wait: async () => {} },
    ),
    /lacks one exact result attestation/u,
  );

  await assert.rejects(
    readSmokeLogAttestation(
      async () => [{
        ...exact,
        labels: { "run.googleapis.com/execution_name": "execution-1" },
      }],
      expected,
      "execution-1",
      { attempts: 1, wait: async () => {} },
    ),
    /lacks one exact result attestation/u,
  );
});

test("smoke fails closed on Job or execution drift and always requests cleanup", async () => {
  const expected = authority();
  assert.throws(
    () => validateSmokeJobReadback(smokeJobFixture({
      spec: { template: { spec: { taskCount: 2 } } },
    }), expected),
    /immutable candidate container|task count/u,
  );
  assert.throws(
    () => validateSmokeExecution({
      metadata: { name: "failed" },
      status: {
        conditions: [{ type: "Completed", status: "False" }],
        succeededCount: 0,
        failedCount: 1,
      },
    }),
    /did not complete exactly once/u,
  );
  assert.throws(
    () => validateSmokeExecution({
      metadata: { name: "safe&whoami" },
      status: {
        conditions: [{ type: "Completed", status: "True" }],
        succeededCount: 1,
        failedCount: 0,
      },
    }),
    /execution identity is unavailable/u,
  );

  const calls = [];
  const runJson = async (arguments_) => {
    calls.push(arguments_);
    if (arguments_[1] === "services") return serviceFixture();
    if (arguments_[1] === "revisions") return revisionFixture();
    if (arguments_[2] === "deploy") return {};
    if (arguments_[2] === "describe") return smokeJobFixture();
    if (arguments_[2] === "execute") throw new Error("synthetic execution failure");
    throw new Error("unexpected call");
  };
  const run = async (arguments_) => calls.push(arguments_);
  await assert.rejects(
    smokeZeroTrafficCandidate({
      projectId,
      sourceCommit,
      priorRevision,
      jobBearerSecretVersion,
      imageDigest,
      candidateUrl,
    }, { runJson, run }),
    /synthetic execution failure/u,
  );
  assert.ok(calls.some((arguments_) => arguments_[2] === "delete"));
});

test("helper never reads a Secret payload or accepts a bearer value", async () => {
  const source = await readFile(new URL("./candidate-release-v080.mjs", import.meta.url), "utf8");
  assert.doesNotMatch(source, /secrets["', ]+versions["', ]+access/u);
  assert.doesNotMatch(source, /CLEARRA_CANDIDATE_JOB_TOKEN\s*[:=]\s*process\.env/u);
  assert.doesNotMatch(source, /authorizationToken|bearerValue|secretPayload/u);
  const expected = authority();
  const serviceArguments = buildServiceDeployArguments(expected);
  const smokeArguments = buildSmokeJobDeployArguments(expected);
  assert.equal(
    [...serviceArguments, ...smokeArguments].some((argument) =>
      argument.includes("forbidden-inline-token")),
    false,
  );
});
