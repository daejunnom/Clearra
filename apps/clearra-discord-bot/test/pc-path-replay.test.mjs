import assert from "node:assert/strict";
import test from "node:test";

import { Clearrabot } from "../src/bot.mjs";
import {
  ClearraJobExecutor,
  assertDiscordCanonicalOnlyResult,
} from "../src/clearra/command.mjs";
import { ClearraDirectExecutor } from "../src/clearra/direct-executor.mjs";
import { renderDocumentGif } from "../src/viewer/gif.mjs";
import {
  PC_PATH_REPLAY_FRAME_DELAY_MS,
  buildCanonicalPathReplayDocument,
  buildCanonicalPcPathReplayDocument,
} from "../src/viewer/pc-path-replay.mjs";

test("canonical pc.path replay keeps initial, lock, and final cleared field at 500ms", () => {
  const replay = buildCanonicalPcPathReplayDocument(result());
  assert.ok(replay);
  assert.equal(replay.kind, "pc");
  assert.equal(replay.frameCount, 3);
  assert.deepEqual(replay.document.pages[0].cells.slice(0, 10), [
    "G", "G", "G", "G", "G", "G", null, null, null, null,
  ]);
  assert.deepEqual(replay.document.pages[1].cells.slice(0, 10), [
    "G", "G", "G", "G", "G", "G", "I", "I", "I", "I",
  ]);
  assert.ok(replay.document.pages.at(-1).cells.every((cell) => cell === null));

  const gif = renderDocumentGif(replay.document, {
    delayMs: PC_PATH_REPLAY_FRAME_DELAY_MS,
  });
  assert.equal(new TextDecoder().decode(gif.subarray(0, 6)), "GIF89a");
  assert.deepEqual(gifFrameDelays(gif), [50, 50, 50]);
});

test("pc.path replay fails closed instead of fabricating a cleared final field", () => {
  const corrupted = structuredClone(result());
  corrupted.summary.canonical_witness.steps[0].board_after_line_clear_mask =
    "0x0000000000000001";
  assert.throws(
    () => buildCanonicalPcPathReplayDocument(corrupted),
    /inconsistent path replay clear field/u,
  );
});

test("pc.path replay rejects a declared clear on a non-full row", () => {
  const corrupted = structuredClone(result());
  corrupted.summary.canonical_witness.steps[0].cleared_row_mask =
    "0x0000000000000002";
  corrupted.summary.canonical_witness.steps[0].board_after_line_clear_mask =
    "0x00000000000003ff";
  assert.throws(
    () => buildCanonicalPcPathReplayDocument(corrupted),
    /inconsistent path replay clear count/u,
  );
});

test("Discord emits every lock and skips duplicate no-clear frames", () => {
  const structured = structuredClone(result());
  const template = structured.summary.canonical_witness.steps[0];
  structured.summary.canonical_witness.consumed_piece_count = "2";
  structured.summary.canonical_witness.steps = [
    {
      ...template,
      step_index: "0",
      operation_id: "0",
      input_cursor: "0",
      output_cursor: "1",
      x: "2",
      placement_mask: "0x000000000000003c",
      board_before_mask: "0x0000000000000003",
      board_after_placement_mask: "0x000000000000003f",
      board_after_line_clear_mask: "0x000000000000003f",
      cleared_row_mask: "0x0000000000000000",
      cleared_lines: "0",
      line_clear_identity: "rows:0000000000000000:count:0",
    },
    {
      ...template,
      step_index: "1",
      operation_id: "1",
      input_cursor: "1",
      output_cursor: "2",
      x: "6",
      placement_mask: "0x00000000000003c0",
      board_before_mask: "0x000000000000003f",
      board_after_placement_mask: "0x00000000000003ff",
      board_after_line_clear_mask: "0x0000000000000000",
      cleared_row_mask: "0x0000000000000001",
      cleared_lines: "1",
      line_clear_identity: "rows:0000000000000001:count:1",
    },
  ];

  const replay = buildCanonicalPcPathReplayDocument(structured);
  assert.equal(replay.frameCount, 4);
  assert.deepEqual(replay.document.pages[0].cells.slice(0, 10), [
    "G", "G", null, null, null, null, null, null, null, null,
  ]);
  assert.deepEqual(replay.document.pages[1].cells.slice(0, 10), [
    "G", "G", "I", "I", "I", "I", null, null, null, null,
  ]);
  assert.ok(replay.document.pages.at(-1).cells.every((cell) => cell === null));
});

test("non-path structured results do not request a replay attachment", () => {
  assert.equal(buildCanonicalPcPathReplayDocument({ kind: "pc-score-summary.v2" }), null);
});

test("direct executor canonical boundary reaches Discord PC replay without re-projection", async () => {
  let renderedDocument = null;
  let renderedOptions = null;
  const bot = new Clearrabot(
    {},
    { maxConcurrentSearches: 1, maxGifBytes: 1024 * 1024 },
    {
      executor: { async execute() { throw new Error("unexpected search"); } },
      gifRenderer: {
        async render(document, options) {
          renderedDocument = document;
          renderedOptions = options;
          return renderDocumentGif(document, options);
        },
        stop() {},
      },
    },
  );
  // ClearraCommandRunner owns the first projection in the real local path;
  // ClearraDirectExecutor must accept that canonical envelope idempotently.
  const serviceResult = assertDiscordCanonicalOnlyResult(success(exhaustiveResult()));
  const executor = new ClearraDirectExecutor(directConfig(), {
    runner: { async execute() { return serviceResult; } },
  });
  const canonicalResult = await executor.execute(["pc", "path"]);
  const canonicalPayload = JSON.parse(canonicalResult.stdout);
  assert.equal(canonicalPayload.summary.payload_kind, "canonical-pc-path-witness");
  assert.equal(Object.hasOwn(canonicalPayload.summary, "witnesses"), false);
  const outgoing = await bot.buildResultMessage(
    canonicalResult,
    false,
    { locale: "ko", resultKind: "path" },
  );

  assert.equal(renderedOptions.delayMs, PC_PATH_REPLAY_FRAME_DELAY_MS);
  assert.equal(renderedDocument.pages.length, 3);
  assert.match(outgoing.payload.content, /3프레임/u);
  assert.match(outgoing.payload.content, /500ms/u);
  assert.equal(outgoing.files.length, 1);
  assert.equal(outgoing.files[0].name, "pc-path-1.gif");
  assert.equal(outgoing.files[0].contentType, "image/gif");
  assert.equal(new TextDecoder().decode(outgoing.files[0].bytes.subarray(0, 6)), "GIF89a");
});

test("HTTP executor canonical boundary renders one Build target replay at 500ms", async () => {
  let renderedOptions = null;
  const bot = new Clearrabot(
    {},
    { maxConcurrentSearches: 1, maxGifBytes: 1024 * 1024 },
    {
      executor: { async execute() { throw new Error("unexpected search"); } },
      gifRenderer: {
        async render(document, options) {
          renderedOptions = options;
          return renderDocumentGif(document, options);
        },
        stop() {},
      },
    },
  );
  const serviceResult = assertDiscordCanonicalOnlyResult(success(buildExhaustiveResult()));
  const executor = new ClearraJobExecutor({
    endpoint: "https://jobs.example.test/jobs",
    authorizationToken: "job-token",
    createJobId: () => "build-path-replay-1",
    fetch: async () => new Response(JSON.stringify({
      protocol: "clearra.job.v1",
      id: "build-path-replay-1",
      state: "completed",
      result: serviceResult,
    }), {
      status: 200,
      headers: { "content-type": "application/json" },
    }),
  });
  const canonicalResult = await executor.execute(["build-probability"]);
  const canonicalPayload = JSON.parse(canonicalResult.stdout);
  assert.equal(canonicalPayload.summary.payload_kind, "canonical-build-path-witness");
  assert.equal(Object.hasOwn(canonicalPayload.summary, "witnesses"), false);

  const replay = buildCanonicalPathReplayDocument(canonicalPayload);
  assert.equal(replay.kind, "build");
  assert.equal(replay.frameCount, 2);
  assert.deepEqual(replay.document.pages.at(-1).cells.slice(0, 10), [
    "I", "I", "I", "I", null, null, null, null, null, null,
  ]);

  const outgoing = await bot.buildResultMessage(
    canonicalResult,
    false,
    { locale: "en", resultKind: "probability" },
  );
  assert.equal(renderedOptions.delayMs, PC_PATH_REPLAY_FRAME_DELAY_MS);
  assert.match(outgoing.payload.content, /Build replay: 2 frames at 500ms each/u);
  assert.equal(outgoing.files.length, 1);
  assert.equal(outgoing.files[0].name, "build-path-1.gif");
  assert.equal(outgoing.files[0].contentType, "image/gif");
  assert.deepEqual(gifFrameDelays(outgoing.files[0].bytes), [50, 50]);
  assert.doesNotMatch(outgoing.payload.content, /candidate|pattern|trace|problem/iu);
  assert.doesNotMatch(outgoing.files[0].name, /candidate|pattern|trace|problem/iu);
});

function result() {
  return {
    kind: "pc-path-family.v2",
    contract: { command: { kind: "pc-path-family.v2" } },
    summary: {
      capability_id: "pc.path",
      result_contract: "pc-path-family.v2",
      payload_kind: "canonical-pc-path-witness",
      witness_contract: "pc-path-witness.v2",
      ordering: "candidate-id-ascending-then-pattern-id-ascending-then-trace-key-ascending",
      canonical_selection: "smallest-canonical-candidate-id",
      problem_id: "problem",
      complete: true,
      canonical_witness: {
        candidate_id: "1",
        producer_candidate_id: "37",
        pattern_id: "0",
        trace_identity: "trace-a",
        normalized_trace_key: "trk1:trace-a",
        consumed_piece_count: "1",
        terminal_hold_piece: null,
        steps: [{
          step_index: "0",
          operation_id: "0",
          active_piece: "I",
          input_cursor: "0",
          output_cursor: "1",
          input_hold_piece: null,
          output_hold_piece: null,
          hold_decision: "none",
          rotation: "0",
          x: "6",
          y: "0",
          placement_mask: "0x00000000000003c0",
          board_before_mask: "0x000000000000003f",
          board_after_placement_mask: "0x00000000000003ff",
          board_after_line_clear_mask: "0x0000000000000000",
          cleared_row_mask: "0x0000000000000001",
          cleared_lines: "1",
          line_clear_identity: "rows:0000000000000001:count:1",
        }],
      },
    },
  };
}

function exhaustiveResult() {
  const canonical = structuredClone(result().summary.canonical_witness);
  const second = structuredClone(canonical);
  second.candidate_id = "2";
  second.producer_candidate_id = "91";
  second.pattern_id = "1";
  second.trace_identity = "trace-b";
  second.normalized_trace_key = "trk1:trace-b";
  return {
    kind: "pc-path-family.v2",
    contract: { command: { kind: "pc-path-family.v2" } },
    summary: {
      capability_id: "pc.path",
      result_contract: "pc-path-family.v2",
      witness_contract: "pc-path-witness.v2",
      ordering: "candidate-id-ascending-then-pattern-id-ascending-then-trace-key-ascending",
      canonical_selection: "smallest-canonical-candidate-id",
      problem_id: "problem",
      materialized_pattern_count: "2",
      witness_count: "2",
      complete: true,
      canonical_witness: structuredClone(canonical),
      witnesses: [canonical, second],
    },
  };
}

function buildExhaustiveResult() {
  const canonical = structuredClone(result().summary.canonical_witness);
  canonical.trace_identity = "build-trace-a";
  canonical.normalized_trace_key = "trk1:build-trace-a";
  canonical.steps[0] = {
    ...canonical.steps[0],
    x: "0",
    placement_mask: "0x000000000000000f",
    board_before_mask: "0x0000000000000000",
    board_after_placement_mask: "0x000000000000000f",
    board_after_line_clear_mask: "0x000000000000000f",
    cleared_row_mask: "0x0000000000000000",
    cleared_lines: "0",
    line_clear_identity: "rows:0000000000000000:count:0",
  };
  const second = structuredClone(canonical);
  second.candidate_id = "2";
  second.producer_candidate_id = "91";
  second.pattern_id = "1";
  second.trace_identity = "build-trace-b";
  second.normalized_trace_key = "trk1:build-trace-b";
  return {
    kind: "build-path-family.v1",
    contract: { command: { kind: "build-path-family.v1" } },
    summary: {
      capability_id: "build.complete-replay-paths",
      result_contract: "build-path-family.v1",
      witness_contract: "build-path-witness.v1",
      ordering: "candidate-id-ascending-then-pattern-id-ascending-then-trace-key-ascending",
      canonical_selection: "smallest-canonical-candidate-id",
      problem_id: "build-problem",
      target_terminal_board_mask: "0x000000000000000f",
      materialized_pattern_count: "2",
      witness_count: "2",
      complete: true,
      canonical_witness: structuredClone(canonical),
      witnesses: [canonical, second],
    },
  };
}

function success(payload) {
  return {
    exitCode: 0,
    signal: null,
    stderr: "",
    stdout: JSON.stringify(payload),
  };
}

function directConfig() {
  return {
    executable: "clearra",
    processLogicalProcessors: 4,
    searchWorkersPerSession: 4,
    useAllLogicalProcessors: true,
    searchTimeoutMs: 3_000,
    interactionDeadlineMs: 4_000,
    maxOutputBytes: 64 * 1024,
    maxGifBytes: 1024 * 1024,
    terminationGraceMs: 100,
  };
}

function gifFrameDelays(bytes) {
  const delays = [];
  for (let index = 0; index + 7 < bytes.length; index += 1) {
    if (
      bytes[index] === 0x21 &&
      bytes[index + 1] === 0xf9 &&
      bytes[index + 2] === 0x04
    ) {
      delays.push(bytes[index + 4] | (bytes[index + 5] << 8));
      index += 7;
    }
  }
  return delays;
}
