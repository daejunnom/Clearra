import assert from "node:assert/strict";
import test from "node:test";

import { createPriorRuntimeAuthority } from "./prior-runtime-authority-v080.mjs";

const options = {
  sourceCommit: "1".repeat(40),
  projectId: "clearra-cloud",
  serviceFileSha256: "2".repeat(64),
};

function service(reference = { name: "clearra-job-token", key: "4" }) {
  return {
    metadata: { name: "clearra-current-job" },
    spec: { template: { spec: { containers: [{ env: [{
      name: "CLEARRA_JOB_TOKEN",
      valueFrom: { secretKeyRef: reference },
    }] }] } } },
    status: { traffic: [
      { revisionName: "clearra-current-job-v075-0123456", percent: 100 },
      { revisionName: "clearra-current-job-v080-1111111", percent: 0 },
    ] },
  };
}

test("seals the exact numeric job Secret reference from the active template", () => {
  const report = createPriorRuntimeAuthority({ ...options, service: service() });
  assert.equal(report.prior_revision, "clearra-current-job-v075-0123456");
  assert.equal(report.job_bearer_secret_version, "4");
  assert.match(report.report_sha256, /^[0-9a-f]{64}$/u);
  const v2 = createPriorRuntimeAuthority({
    ...options,
    service: service({ secret: "clearra-job-token", version: "9" }),
  });
  assert.equal(v2.job_bearer_secret_version, "9");
});

test("rejects latest, inline, wrong Secret, and ambiguous traffic", () => {
  assert.throws(
    () => createPriorRuntimeAuthority({ ...options, service: service({ name: "clearra-job-token", key: "latest" }) }),
    /numeric job Secret version/u,
  );
  const inline = service();
  inline.spec.template.spec.containers[0].env[0] = { name: "CLEARRA_JOB_TOKEN", value: "secret" };
  assert.throws(() => createPriorRuntimeAuthority({ ...options, service: inline }), /managed job-token/u);
  assert.throws(
    () => createPriorRuntimeAuthority({ ...options, service: service({ name: "discord-bot-token", key: "4" }) }),
    /Secret identity/u,
  );
  const split = service();
  split.status.traffic[1].percent = 50;
  assert.throws(() => createPriorRuntimeAuthority({ ...options, service: split }), /one exact 100-percent/u);
});
