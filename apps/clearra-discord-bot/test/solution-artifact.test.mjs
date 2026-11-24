import assert from "node:assert/strict";
import { EventEmitter } from "node:events";
import { existsSync } from "node:fs";
import { lstat, symlink, unlink, writeFile } from "node:fs/promises";
import { PassThrough } from "node:stream";
import test from "node:test";
import { crc32 } from "node:zlib";
import { encodeCtk3 } from "ctk3";

import {
  cleanupSolutionArtifactPlan, createSolutionArtifactPlan, readSolutionArtifact,
  usesNativeSolutionArtifact, validateSolutionTransportArtifact,
} from "../src/clearra/solution-artifact.mjs";
import { ClearraCommandRunner } from "../src/job-service/runner.mjs";

const source = encodeCtk3({ width: 10, pages: [{ height: 1, cells: ["I", "I", "I", "I", null, null, null, null, null, null] }] });
const bytes = Buffer.from(source);

async function writeArtifact(outputPath) {
  await writeFile(outputPath, bytes, { flag: "wx" });
  const identity = await lstat(outputPath, { bigint: true });
  return {
    summary: { normalized_unique_solution_count: "1" },
    solution_artifact: {
      status: "committed", schema: "solution-set-artifact.v2", encoding: "ctk3",
      compression: "ctk3-revision-native", bytes: bytes.length,
      checksum: `crc32:${crc32(bytes).toString(16).padStart(8, "0")}`,
      solution_count: 1, annotated_solution_count: 0, target_owned: true,
      file_identity_kind: "linux-device-inode",
      file_identity: `${identity.dev.toString(16).padStart(16, "0")}:${identity.ino.toString(16).padStart(16, "0")}`,
    },
  };
}

test("native solution transport preserves the complete source and file identity", async () => {
  const plan = await createSolutionArtifactPlan(["pc", "--lines", "2"], 4096);
  try {
    const document = await writeArtifact(plan.outputPath);
    const artifact = await readSolutionArtifact(plan, document);
    assert.equal(artifact.source, source);
    assert.equal(artifact.solutionCount, 1);
    assert.equal(artifact.complete, true);
    assert.deepEqual(validateSolutionTransportArtifact(artifact, 4096), artifact);
    for (const patch of [
      { source: `${source}x` }, { sha256: "0".repeat(64) },
      { solutionCount: 2 }, { complete: false }, { byteLength: 4097 },
    ]) assert.throws(() => validateSolutionTransportArtifact({ ...artifact, ...patch }, 4096));
    for (const patch of [
      { checksum: "crc32:00000000" }, { bytes: bytes.length + 1 },
      { solution_count: 2 }, { target_owned: false },
      { file_identity: "0000000000000000:0000000000000000" },
    ]) await assert.rejects(readSolutionArtifact(plan, {
      ...document, solution_artifact: { ...document.solution_artifact, ...patch },
    }));
  } finally { await cleanupSolutionArtifactPlan(plan); }
  assert.equal(existsSync(plan.directory), false);
});

test("empty native solution sets carry no fabricated CTK3 page or owned file", async () => {
  const plan = await createSolutionArtifactPlan(["pc"], 4096);
  const document = {
    summary: { normalized_unique_solution_count: "0" },
    solution_artifact: { status: "empty", schema: "solution-set-artifact.v2",
      encoding: "ctk3", bytes: 0, solution_count: 0, annotated_solution_count: 0,
      target_owned: false },
  };
  try {
    assert.equal(await readSolutionArtifact(plan, document), null);
    await writeFile(plan.outputPath, bytes);
    await assert.rejects(readSolutionArtifact(plan, document), /inconsistent empty/);
  } finally { await cleanupSolutionArtifactPlan(plan); }
});

test("native artifact reader rejects a replaced link before consuming its target", async () => {
  const plan = await createSolutionArtifactPlan(["pc"], 4096);
  try {
    const document = await writeArtifact(plan.outputPath);
    await unlink(plan.outputPath);
    await symlink("missing-external-target.ctk3", plan.outputPath);
    await assert.rejects(readSolutionArtifact(plan, document), /bounded regular/);
  } finally { await cleanupSolutionArtifactPlan(plan); }
});

test("typed PC Path retains its bounded replay contract instead of a materialized solution export", () => {
  assert.equal(usesNativeSolutionArtifact(["pc", "path"]), false);
  assert.equal(usesNativeSolutionArtifact(["pc", "score"]), false);
  assert.equal(usesNativeSolutionArtifact(["pc-scenario"]), true);
  assert.equal(usesNativeSolutionArtifact(["build-probability"]), true);
  for (const command of ["percent", "cover", "build-coverage"]) {
    assert.equal(usesNativeSolutionArtifact([command]), false);
  }
});

test("runner uses the native file encoder without key stdout or generated path disclosure", async () => {
  let ownedDirectory;
  let observedArguments;
  const runner = new ClearraCommandRunner({
    executable: "clearra", processLogicalProcessors: 8, searchWorkersPerSession: 8,
    useAllLogicalProcessors: true, searchTimeoutMs: 5000,
    maxOutputBytes: 4096, maxArtifactBytes: 4096, terminationGraceMs: 100,
  }, {
    readOomKillCount: async () => null,
    spawn(_executable, arguments_) {
      observedArguments = arguments_;
      const outputPath = arguments_[arguments_.indexOf("--solution-output") + 1];
      ownedDirectory = outputPath.slice(0, outputPath.lastIndexOf("/"));
      const child = new EventEmitter();
      child.stdout = new PassThrough(); child.stderr = new PassThrough();
      child.exitCode = null; child.signalCode = null; child.kill = () => true;
      void writeArtifact(outputPath).then((document) => {
        child.stdout.end(JSON.stringify({ ...document, kind: "pc" }));
        child.exitCode = 0;
        child.emit("close", 0, null);
      }).catch((error) => child.emit("error", error));
      return child;
    },
  });
  const result = await runner.execute({ arguments: ["pc", "--lines", "2"],
    deadlineUnixMs: Date.now() + 5000, maxOutputBytes: 4096, maxArtifactBytes: 4096 });
  assert.equal(observedArguments.includes("--include-solution-data"), false);
  assert.equal(observedArguments[observedArguments.indexOf("--auto-workers") + 1], "8");
  assert.equal(result.artifact.source, source);
  assert.equal(result.failureCode, null);
  assert.equal(JSON.parse(result.stdout).solution_artifact, undefined);
  assert.equal(result.stdout.includes(ownedDirectory), false);
  assert.equal(existsSync(ownedDirectory), false);
});
