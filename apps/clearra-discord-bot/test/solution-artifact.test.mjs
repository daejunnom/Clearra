import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { EventEmitter } from "node:events";
import { existsSync } from "node:fs";
import { lstat, readFile, symlink, unlink, writeFile } from "node:fs/promises";
import { PassThrough } from "node:stream";
import test from "node:test";
import { crc32 } from "node:zlib";
import { encodeCtk3, encodeCtk3Bundle, encodeCtk3Compact, inspectCtk3Exact } from "ctk3";

import {
  cleanupSolutionArtifactPlan, createSolutionArtifactPlan, readSolutionArtifact,
  usesNativeSolutionArtifact, validateSolutionTransportArtifact,
} from "../src/clearra/solution-artifact.mjs";
import { ClearraCommandRunner } from "../src/job-service/runner.mjs";

const source = encodeCtk3({ width: 10, pages: [{ height: 1, cells: ["I", "I", "I", "I", null, null, null, null, null, null] }] });
const bytes = Buffer.from(source);

async function writeArtifact(outputPath, artifactSource = source, solutionCount = 1) {
  const artifactBytes = Buffer.from(artifactSource);
  await writeFile(outputPath, artifactBytes, { flag: "wx" });
  const identity = await lstat(outputPath, { bigint: true });
  return {
    summary: { normalized_unique_solution_count: String(solutionCount) },
    solution_artifact: {
      status: "committed", schema: "solution-set-artifact.v2", encoding: "ctk3",
      compression: "ctk3-revision-native", bytes: artifactBytes.length,
      checksum: `crc32:${crc32(artifactBytes).toString(16).padStart(8, "0")}`,
      solution_count: solutionCount, annotated_solution_count: 0, target_owned: true,
      file_identity_kind: "linux-device-inode",
      file_identity: `${identity.dev.toString(16).padStart(16, "0")}:${identity.ino.toString(16).padStart(16, "0")}`,
    },
  };
}

function transport(artifactSource, solutionCount) {
  const artifactBytes = Buffer.from(artifactSource);
  return {
    contract: "clearra.discord-solution-artifact.v1", mediaType: "text/plain; charset=utf-8",
    filename: "clearra-result.ctk3", source: artifactSource, byteLength: artifactBytes.length,
    sha256: createHash("sha256").update(artifactBytes).digest("hex"),
    solutionCount, complete: true,
  };
}

test("native envelope inspection agrees with the shared native interop fixtures", async () => {
  const fixture = await readFile(new URL(
    "../../../tests/fixtures/contracts/ctk3_native_interop.v1.tsv", import.meta.url), "utf8");
  const revisions = new Set();
  for (const row of fixture.trim().split(/\r?\n/u).slice(1)) {
    const [name, revision, canonical, compact] = row.split("\t");
    revisions.add(Number(revision));
    for (const encoded of new Set([canonical, compact])) {
      const info = inspectCtk3Exact(encoded);
      const artifact = transport(encoded, info.pageCount);
      assert.deepEqual(validateSolutionTransportArtifact(artifact, 24 * 1024 * 1024), artifact, name);
    }
  }
  assert.deepEqual([...revisions].sort(), [1, 2, 3]);
});

test("native bundles retain all pages across segment boundaries and enforce the aggregate limit", () => {
  const page = { height: 1, cells: ["I", "I", "I", "I", null, null, null, null, null, null] };
  const fullSegment = encodeCtk3Compact({ width: 10, pages: Array(4096).fill(page) });
  const encoded = encodeCtk3Bundle([fullSegment, source]);
  assert.equal(inspectCtk3Exact(encoded).pageCount, 4097);
  const artifact = transport(encoded, 4097);
  assert.deepEqual(validateSolutionTransportArtifact(artifact, 24 * 1024 * 1024), artifact);
  assert.throws(() => validateSolutionTransportArtifact(transport(encoded, 4096), 24 * 1024 * 1024), /page count/u);
  const excessive = `ctk3b_${Array(257).fill(fullSegment.slice(5)).join(".")}`;
  assert.throws(() => validateSolutionTransportArtifact(transport(excessive, 257 * 4096),
    24 * 1024 * 1024), /page count/u);
});

test("native envelope rejects malformed framing even with matching transport hashes", () => {
  const mutateHeader = (change) => {
    const payload = Buffer.from(source.slice(5), "base64url");
    change(payload);
    return `ctk3_${payload.toString("base64url")}`;
  };
  const narrow = encodeCtk3Compact({ width: 9, pages: [
    { height: 1, cells: ["I", "I", "I", "I", null, null, null, null, null] },
  ] });
  const malformed = [
    ["ctk3_", 1], ["ctk3_!", 1], [`${source}=`, 1],
    ["ctk3_w0kCAAdPCh", 1], // Valid alphabet, nonzero unused base64 bits.
    ["ctk3b_", 1], [`ctk3b_${source.slice(5)}`, 1],
    [`ctk3b_${source.slice(5)}.`, 1], [`ctk3b_.${source.slice(5)}`, 1],
    [`ctk3b_${source.slice(5)}.${narrow.slice(5)}`, 2],
    [`ctk3_${Buffer.from([0xc3, 0x49, 2, 0]).toString("base64url")}`, 1],
    [mutateHeader(payload => { payload[0] = 0; }), 1],
    [mutateHeader(payload => { payload[1] = (payload[1] & 0xf8) | 4; }), 1],
    [mutateHeader(payload => { payload[1] |= 0xf8; }), 1],
    [mutateHeader(payload => { payload[2] &= 0xe0; }), 1],
    [mutateHeader(payload => { payload[2] |= 0x20; }), 1],
  ];
  for (const [encoded, count] of malformed) {
    assert.throws(() => validateSolutionTransportArtifact(transport(encoded, count), 4096),
      /native CTK3/u, encoded);
  }
});

test("native reader verifies envelope after matching file identity and CRC32", async () => {
  const plan = await createSolutionArtifactPlan(["pc"], 4096);
  try {
    const document = await writeArtifact(plan.outputPath, `ctk3b_${source.slice(5)}.`, 1);
    await assert.rejects(readSolutionArtifact(plan, document), /native CTK3/u);
  } finally { await cleanupSolutionArtifactPlan(plan); }
});

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
