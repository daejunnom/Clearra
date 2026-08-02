import assert from "node:assert/strict";
import test from "node:test";

import {
  CTK3_MAX_BUNDLE_PAGES,
  decodeCtk3,
  inspectCtk3,
} from "ctk3";

import {
  buildCtk3Result,
  Ctk3ResultError,
} from "../src/clearra/ctk3-result.mjs";

const ARTIFACT_SCHEMA = "clearra.solution-data.v1";
const SIMPLE_KEY =
  "ctk1|initial=0000000000000003|placements=T:000000000000003c,I:0000000000003c00";

test("strict ctk1/ctk2 keys become colored CTK3 pages with gray initial cells", () => {
  const highPlacement = 0xfn << 70n;
  const extendedKey =
    `ctk2|height=8|initial=${hex(0n, 64)}|placements=L:${hex(highPlacement, 64)}`;
  const result = buildCtk3Result(JSON.stringify({
    schema_version: 2,
    summary: { count_complete: true },
    contract: {
      artifacts: {
        schema_version: ARTIFACT_SCHEMA,
        solution_keys: [SIMPLE_KEY, extendedKey],
        solution_probabilities: [
          {
            solution_key: SIMPLE_KEY,
            probability: 0.5,
            probability_complete: true,
          },
          {
            solution_key: extendedKey,
            probability: 0.125,
            probability_complete: true,
          },
        ],
      },
    },
  }));

  assert.ok(result);
  assert.equal(result.pageCount, 2);
  assert.equal(result.complete, true);
  assert.deepEqual(result.warnings, []);
  const document = decodeCtk3(result.source);
  assert.equal(document.pages.length, 2);
  assert.deepEqual(document.pages[0].cells.slice(0, 6), [
    "G", "G", "T", "T", "T", "T",
  ]);
  assert.deepEqual(document.pages[0].cells.slice(10, 14), ["I", "I", "I", "I"]);
  assert.equal(document.pages[0].comment, "P=50%");
  assert.equal(document.pages[1].height, 8);
  assert.deepEqual(document.pages[1].cells.slice(70, 74), ["L", "L", "L", "L"]);
  assert.equal(document.pages[1].comment, "P=12.5%");
});

test("inactive optional postprocessing does not mark a complete solution set incomplete", () => {
  const result = buildCtk3Result({
    schema_version: 2,
    summary: {
      packing_count_complete: true,
      solution_keys_complete: true,
      count_complete: true,
      probability_complete: true,
      objective_complete: true,
      minimum_cover_requested: false,
      minimum_cover_complete: false,
      postprocess_scoring_requested: false,
      postprocess_execution_complete: false,
    },
    contract: {
      artifacts: {
        schema_version: ARTIFACT_SCHEMA,
        solution_keys: [SIMPLE_KEY],
      },
    },
  });

  assert.ok(result);
  assert.equal(result.complete, true);
  assert.deepEqual(result.warnings, []);
});

test("malformed or overlapping solution keys fail closed", () => {
  assert.throws(
    () => buildCtk3Result({
      schema_version: ARTIFACT_SCHEMA,
      solution_keys: [
        "ctk1|initial=0000000000000001|placements=T:000000000000000f",
      ],
    }),
    (error) =>
      error instanceof Ctk3ResultError && error.code === "invalid-solution-key",
  );
  assert.throws(
    () => buildCtk3Result({
      schema_version: ARTIFACT_SCHEMA,
      solution_keys: [SIMPLE_KEY.replace("ctk1", "CTK1")],
    }),
    (error) =>
      error instanceof Ctk3ResultError && error.code === "invalid-solution-key",
  );
});

test("solution probability comments require a complete one-to-one key match", () => {
  assert.throws(
    () => buildCtk3Result({
      schema_version: ARTIFACT_SCHEMA,
      solution_keys: [SIMPLE_KEY],
      solution_probabilities: [],
    }),
    (error) =>
      error instanceof Ctk3ResultError &&
      error.code === "solution-probability-key-mismatch",
  );

  const result = buildCtk3Result({
    schema_version: ARTIFACT_SCHEMA,
    solution_keys: [SIMPLE_KEY],
    solution_probabilities: [
      {
        solution_key: SIMPLE_KEY,
        probability: "0.25",
        probability_complete: false,
      },
    ],
  });
  assert.ok(result);
  assert.equal(result.complete, false);
  assert.match(result.warnings[0], /probability_complete=false/);
  assert.equal(decodeCtk3(result.source).pages[0].comment, "P=25%");
});

test("setup representative paths are replayed, colored, and marked incomplete without truncation", () => {
  const result = buildCtk3Result({
    schema_version: ARTIFACT_SCHEMA,
    setup_conditions: [
      {
        complete: false,
        result_truncated: true,
        candidates: [
          {
            board_mask: "0xc03",
            representative_path: [
              {
                piece: "O",
                rotation: 0,
                x: 0,
                y: 0,
                hold: "none",
                cleared_lines: 0,
              },
            ],
          },
        ],
      },
    ],
  });

  assert.ok(result);
  assert.equal(result.pageCount, 1);
  assert.equal(result.complete, false);
  assert.equal(result.warnings.length, 2);
  const page = decodeCtk3(result.source).pages[0];
  assert.deepEqual(page.cells.slice(0, 2), ["O", "O"]);
  assert.deepEqual(page.cells.slice(10, 12), ["O", "O"]);
});

test("setup replay rejects a final board mask mismatch", () => {
  assert.throws(
    () => buildCtk3Result({
      schema_version: ARTIFACT_SCHEMA,
      setup_conditions: [
        {
          complete: true,
          result_truncated: false,
          candidates: [
            {
              board_mask: "0x1",
              representative_path: [
                { piece: "O", rotation: 0, x: 0, y: 0, cleared_lines: 0 },
              ],
            },
          ],
        },
      ],
    }),
    (error) =>
      error instanceof Ctk3ResultError &&
      error.code === "invalid-setup-final-mask",
  );
});

test("forward exact masks preserve cleared history colors and validate the final board", () => {
  const result = buildCtk3Result({
    schema_version: ARTIFACT_SCHEMA,
    forward: {
      initial_board: words(0x3fn),
      outcomes: [
        {
          id: 7,
          source_queue: "IO",
          total_damage: 4,
          spin_piece: "T",
          spin_mini: false,
          spin_lines: 2,
          final_board: words(0xc03n),
          path: [
            {
              piece: "I",
              placement_mask: words(0x3c0n),
              cleared_row_mask: 1,
              board_after: words(0n),
            },
            {
              piece: "O",
              placement_mask: words(0xc03n),
              cleared_row_mask: 0,
              board_after: words(0xc03n),
            },
          ],
        },
      ],
    },
  });

  assert.ok(result);
  assert.equal(result.pageCount, 1);
  const page = decodeCtk3(result.source).pages[0];
  assert.equal(page.height, 3);
  assert.deepEqual(page.cells.slice(0, 10), [
    "G", "G", "G", "G", "G", "G", "I", "I", "I", "I",
  ]);
  assert.deepEqual(page.cells.slice(10, 12), ["O", "O"]);
  assert.deepEqual(page.cells.slice(20, 22), ["O", "O"]);
  assert.equal(page.comment, "#7 | Q=IO | D=4 | T-spin 2L");
});

test("forward replay rejects board-after and final-mask mismatches", () => {
  const base = {
    schema_version: ARTIFACT_SCHEMA,
    forward: {
      initial_board: words(0n),
      outcomes: [
        {
          final_board: words(0xfn),
          path: [
            {
              piece: "I",
              placement_mask: words(0xfn),
              cleared_row_mask: 0,
              board_after: words(0n),
            },
          ],
        },
      ],
    },
  };
  assert.throws(
    () => buildCtk3Result(base),
    (error) =>
      error instanceof Ctk3ResultError &&
      error.code === "invalid-forward-board-after",
  );

  base.forward.outcomes[0].path[0].board_after = words(0xfn);
  base.forward.outcomes[0].final_board = words(0n);
  assert.throws(
    () => buildCtk3Result(base),
    (error) =>
      error instanceof Ctk3ResultError &&
      error.code === "invalid-forward-final-mask",
  );
});

test("more than 4,096 pages are bundled without truncation", () => {
  const pageCount = 4097;
  const result = buildCtk3Result({
    schema_version: ARTIFACT_SCHEMA,
    solution_keys: Array(pageCount).fill(SIMPLE_KEY),
  });

  assert.ok(result);
  assert.equal(result.pageCount, pageCount);
  assert.equal(inspectCtk3(result.source).bundled, true);
  assert.equal(decodeCtk3(result.source).pages.length, pageCount);
});

test("the CTK3 bundle page limit is rejected before reading page values", () => {
  const oversized = new Array(CTK3_MAX_BUNDLE_PAGES + 1);
  assert.throws(
    () => buildCtk3Result({
      schema_version: ARTIFACT_SCHEMA,
      solution_keys: oversized,
    }),
    (error) =>
      error instanceof Ctk3ResultError && error.code === "ctk3-page-limit",
  );
});

test("results without solution artifacts return null", () => {
  assert.equal(buildCtk3Result({ schema_version: 2, contract: {} }), null);
});

function words(mask) {
  return `0x${hex(mask, 64)}`;
}

function hex(mask, width) {
  return mask.toString(16).padStart(width, "0");
}
