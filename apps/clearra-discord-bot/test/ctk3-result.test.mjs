import assert from "node:assert/strict";
import test from "node:test";

import {
  CTK3_MAX_BUNDLE_PAGES,
  decodeCtk3,
  inspectCtk3,
  operationCells,
} from "ctk3";
import { encoder as fumenEncoder, Field } from "tetris-fumen";

import {
  buildCtk3Result,
  Ctk3ResultError,
} from "../src/clearra/ctk3-result.mjs";
import { findSlashCommand } from "../src/discord/slash-command-catalog.mjs";
import { buildSlashCommandArguments } from "../src/discord/slash-command-input.mjs";

const ARTIFACT_SCHEMA = "clearra.solution-data.v1";
const SIMPLE_KEY =
  "ctk1|initial=0000000000000003|placements=T:000000000000003c,I:0000000000003c00";
const SEARCH_CLEAR_KEY =
  "ctk1|initial=000000000000003f|placements=I:00000000000003c0";
const SEARCH_ORDER_KEY =
  "ctk1|initial=0000000000000000|placements=O:0000000000000c03,I:000000000000003c";

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
      solution_probabilities_requested: false,
      probability_complete: false,
      resource_probability_complete: false,
      objective_search_complete: false,
      objective_search_incomplete_reason:
        "coverage_not_requested_for_unique_solution_set",
      objective_complete: false,
      objective_incomplete_reason:
        "coverage_not_requested_for_unique_solution_set",
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

test("requested or unexplained incomplete postprocessing remains partial", () => {
  const artifacts = {
    schema_version: ARTIFACT_SCHEMA,
    solution_keys: [SIMPLE_KEY],
  };
  for (const summary of [
    {
      solution_probabilities_requested: true,
      probability_complete: false,
    },
    {
      objective_search_complete: false,
      objective_search_incomplete_reason: "budget-exceeded",
    },
    {
      objective_complete: false,
      objective_incomplete_reason: "not-calculated",
    },
  ]) {
    const result = buildCtk3Result({
      schema_version: 2,
      summary,
      contract: { artifacts },
    });
    assert.ok(result);
    assert.equal(result.complete, false);
    assert.equal(result.warnings.length, 1);
  }
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

test("spin solution classes are preserved as per-page CTK3 comments", () => {
  const result = buildCtk3Result({
    schema_version: ARTIFACT_SCHEMA,
    solution_keys: [SIMPLE_KEY, SIMPLE_KEY],
    solution_classes: ["regular", "mini"],
  });
  assert.ok(result);
  assert.deepEqual(
    decodeCtk3(result.source).pages.map((page) => page.comment),
    ["Spin: Regular", "Spin: Mini"],
  );

  assert.throws(
    () => buildCtk3Result({
      schema_version: ARTIFACT_SCHEMA,
      solution_keys: [SIMPLE_KEY],
      solution_classes: [],
    }),
    (error) =>
      error instanceof Ctk3ResultError &&
      error.code === "solution-class-key-mismatch",
  );
  assert.throws(
    () => buildCtk3Result({
      schema_version: ARTIFACT_SCHEMA,
      solution_keys: [SIMPLE_KEY],
      solution_classes: ["unknown"],
    }),
    (error) =>
      error instanceof Ctk3ResultError &&
      error.code === "invalid-solution-class",
  );
});

test("typed finesse averages add only the minimum per-solution input cost", () => {
  const result = buildCtk3Result({
    schema_version: ARTIFACT_SCHEMA,
    solution_keys: [SEARCH_CLEAR_KEY],
    solution_classes: ["regular"],
    solution_probabilities: [
      {
        solution_key: SEARCH_CLEAR_KEY,
        probability: 0.25,
        probability_complete: true,
      },
    ],
    finesse_report: {
      mode: "search",
      metric: "inputs",
      pattern_knowledge: "both",
      complete: true,
      exact_total_inputs: "1",
      representative_witness: {
        policy: "oracle",
        solution_key: SEARCH_CLEAR_KEY,
        pattern_ids: [0],
        queue: ["I"],
        total_inputs: 1,
        input_sequence: ["hard-drop"],
        placements: [{ piece: "I", rotation: 0, x: 6, y: 0 }],
      },
      policy_results: [
        {
          policy: "oracle",
          backend: "private-backend-name",
          complete: true,
          solution_averages: [
            {
              solution_key: SEARCH_CLEAR_KEY,
              average_inputs: "9.5000",
              complete: true,
            },
          ],
        },
        {
          policy: "visible-7",
          complete: true,
          solution_averages: [
            {
              solution_key: SEARCH_CLEAR_KEY,
              average_inputs: "8.25",
              complete: true,
            },
          ],
        },
      ],
    },
  });

  assert.ok(result);
  const page = decodeCtk3(result.source).pages[0];
  const comment = page.comment;
  assert.equal(comment, "Spin: Regular | P=25% | F=8.25");
  assert.doesNotMatch(comment, /private|policy|worker|backend|server|tap|rotate|hard/i);
  assert.deepEqual(page.cells.slice(0, 10), [
    "G", "G", "G", "G", "G", "G", null, null, null, null,
  ]);
  assert.deepEqual(page.operation, {
    piece: "I",
    rotation: "spawn",
    x: 7,
    y: 0,
  });
});

test("finesse witness schema is checked without expanding its route into CTK3 comments", () => {
  assert.throws(
    () => buildCtk3Result({
      schema_version: ARTIFACT_SCHEMA,
      solution_keys: [SIMPLE_KEY],
      finesse_report: {
        ...searchReport([
          {
            policy: "oracle",
            complete: true,
            solution_averages: [
              { solution_key: SIMPLE_KEY, average_inputs: "3", complete: true },
            ],
          },
        ], "3"),
        representative_witness: {
          policy: "oracle",
          solution_key: SIMPLE_KEY,
          pattern_ids: [0],
          queue: ["T"],
          total_inputs: 2,
          input_sequence: ["tap-left", "hard-drop"],
          placements: [{ piece: "T", rotation: 0, x: 2, y: 0 }],
        },
      },
    }),
    (error) =>
      error instanceof Ctk3ResultError &&
      error.code === "invalid-finesse-witness" &&
      error.path.endsWith("total_inputs"),
  );
});

test("search CTK3 uses the selected fixed-queue placement order", () => {
  const result = buildCtk3Result({
    schema_version: ARTIFACT_SCHEMA,
    solution_keys: [SEARCH_ORDER_KEY],
    finesse_report: {
      ...searchReport([
        {
          policy: "oracle",
          complete: true,
          solution_averages: [
            { solution_key: SEARCH_ORDER_KEY, average_inputs: "2", complete: true },
          ],
        },
      ], "2"),
      representative_witness: {
        policy: "oracle",
        solution_key: SEARCH_ORDER_KEY,
        pattern_ids: [0],
        queue: ["I", "O"],
        total_inputs: 2,
        input_sequence: ["hard-drop", "hard-drop"],
        placements: [
          { piece: "I", rotation: 0, x: 2, y: 0 },
          { piece: "O", rotation: 0, x: 0, y: 0 },
        ],
      },
    },
  });

  assert.ok(result);
  assert.equal(result.pageCount, 2);
  const pages = decodeCtk3(result.source).pages;
  assert.equal(pages[0].operation.piece, "I");
  assert.equal(pages[0].comment, "F=2");
  assert.deepEqual(pages[1].cells.slice(0, 6), [null, null, "I", "I", "I", "I"]);
  assert.equal(pages[1].operation.piece, "O");
});

test("pattern search CTK3 keeps one colored representative path and the average comment", () => {
  const result = buildCtk3Result({
    schema_version: ARTIFACT_SCHEMA,
    solution_keys: [SEARCH_ORDER_KEY],
    finesse_report: {
      ...searchReport([{
        policy: "oracle",
        complete: false,
        successful_unique_queue_count: 2,
        solution_averages: [
          { solution_key: SEARCH_ORDER_KEY, average_inputs: "2.5", complete: false },
        ],
      }]),
      complete: false,
      representative_witness: {
        policy: "oracle",
        solution_key: SEARCH_ORDER_KEY,
        pattern_ids: [1, 4],
        queue: ["I", "O"],
        total_inputs: 2,
        input_sequence: ["hard-drop", "hard-drop"],
        placements: [
          { piece: "I", rotation: 0, x: 2, y: 0 },
          { piece: "O", rotation: 0, x: 0, y: 0 },
        ],
      },
    },
  });

  assert.ok(result);
  const pages = decodeCtk3(result.source).pages;
  assert.equal(pages.length, 2);
  assert.equal(pages[0].comment, "F=2.5");
  assert.equal(pages[0].operation.piece, "I");
  assert.deepEqual(pages[1].cells.slice(0, 6), [null, null, "I", "I", "I", "I"]);
  assert.equal(pages[1].operation.piece, "O");
});

test("search CTK3 operations preserve each typed witness orientation", () => {
  const placements = [
    {
      witness: { piece: "O", rotation: 3, x: 0, y: 0 },
      operation: { piece: "O", rotation: "spawn", x: 0, y: 0 },
    },
    {
      witness: { piece: "I", rotation: 3, x: 2, y: 0 },
      operation: { piece: "I", rotation: "right", x: 2, y: 2 },
    },
    {
      witness: { piece: "S", rotation: 2, x: 4, y: 0 },
      operation: { piece: "S", rotation: "spawn", x: 5, y: 0 },
    },
    {
      witness: { piece: "Z", rotation: 3, x: 8, y: 0 },
      operation: { piece: "Z", rotation: "right", x: 8, y: 1 },
    },
  ];
  const solutionKey = `ctk1|initial=${hex(0n, 16)}|placements=${placements
    .map(({ witness, operation }) =>
      `${witness.piece}:${hex(maskForOperation(operation), 16)}`)
    .join(",")}`;
  const result = buildCtk3Result({
    schema_version: ARTIFACT_SCHEMA,
    solution_keys: [solutionKey],
    finesse_report: {
      ...searchReport([{
        policy: "oracle",
        complete: true,
        solution_averages: [
          { solution_key: solutionKey, average_inputs: "4", complete: true },
        ],
      }], "4"),
      representative_witness: {
        policy: "oracle",
        solution_key: solutionKey,
        pattern_ids: [0],
        queue: placements.map(({ witness }) => witness.piece),
        total_inputs: 4,
        input_sequence: Array(4).fill("hard-drop"),
        placements: placements.map(({ witness }) => witness),
      },
    },
  });

  assert.ok(result);
  assert.deepEqual(
    decodeCtk3(result.source).pages.map((page) => page.operation),
    placements.map(({ operation }) => operation),
  );
});

test("search CTK3 requires the engine representative field to be initially precleared", () => {
  const unnormalizedKey =
    "ctk1|initial=00000000000003ff|placements=O:0000000000300c00";
  assert.throws(
    () => buildCtk3Result({
      schema_version: ARTIFACT_SCHEMA,
      solution_keys: [unnormalizedKey],
      finesse_report: {
        ...searchReport([
          {
            policy: "oracle",
            complete: true,
            solution_averages: [
              {
                solution_key: unnormalizedKey,
                average_inputs: "1",
                complete: true,
              },
            ],
          },
        ], "1"),
        representative_witness: {
          policy: "oracle",
          solution_key: unnormalizedKey,
          pattern_ids: [0],
          queue: ["O"],
          total_inputs: 1,
          input_sequence: ["hard-drop"],
          placements: [{ piece: "O", rotation: 0, x: 0, y: 0 }],
        },
      },
    }),
    (error) =>
      error instanceof Ctk3ResultError &&
      error.code === "invalid-finesse-search" &&
      /already have its complete rows cleared/.test(error.message),
  );
});

test("search CTK3 rejects a placement path unrelated to the selected solution", () => {
  assert.throws(
    () => buildCtk3Result({
      schema_version: ARTIFACT_SCHEMA,
      solution_keys: [SEARCH_CLEAR_KEY],
      finesse_report: {
        ...searchReport([
          {
            policy: "oracle",
            complete: true,
            solution_averages: [
              { solution_key: SEARCH_CLEAR_KEY, average_inputs: "1", complete: true },
            ],
          },
        ], "1"),
        representative_witness: {
          policy: "oracle",
          solution_key: SEARCH_CLEAR_KEY,
          pattern_ids: [0],
          queue: ["I"],
          total_inputs: 1,
          input_sequence: ["hard-drop"],
          placements: [{ piece: "I", rotation: 0, x: 0, y: 1 }],
        },
      },
    }),
    (error) => error instanceof Ctk3ResultError &&
      error.code === "invalid-finesse-search-final-field",
  );
});

test("search CTK3 rejects a witness that swaps the selected solution colors", () => {
  assert.throws(
    () => buildCtk3Result({
      schema_version: ARTIFACT_SCHEMA,
      solution_keys: [SEARCH_ORDER_KEY],
      finesse_report: {
        ...searchReport([
          {
            policy: "oracle",
            complete: true,
            solution_averages: [
              { solution_key: SEARCH_ORDER_KEY, average_inputs: "2", complete: true },
            ],
          },
        ], "2"),
        representative_witness: {
          policy: "oracle",
          solution_key: SEARCH_ORDER_KEY,
          pattern_ids: [0],
          queue: ["I", "O"],
          total_inputs: 2,
          input_sequence: ["hard-drop", "hard-drop"],
          placements: [
            { piece: "I", rotation: 0, x: 0, y: 0 },
            { piece: "O", rotation: 0, x: 4, y: 0 },
          ],
        },
      },
    }),
    (error) => error instanceof Ctk3ResultError &&
      error.code === "invalid-finesse-search-final-field",
  );
});

test("search finesse comments reject invalid policy and solution coverage", () => {
  const average = {
    solution_key: SIMPLE_KEY,
    average_inputs: "8",
    complete: true,
  };
  const policy = {
    policy: "oracle",
    complete: true,
    solution_averages: [average],
  };
  assert.throws(
    () => buildCtk3Result({
      schema_version: ARTIFACT_SCHEMA,
      solution_keys: [SIMPLE_KEY],
      finesse_report: searchReport([{ ...policy, policy: "unknown" }]),
    }),
    (error) =>
      error instanceof Ctk3ResultError &&
      error.code === "invalid-finesse-policy",
  );
  assert.throws(
    () => buildCtk3Result({
      schema_version: ARTIFACT_SCHEMA,
      solution_keys: [SIMPLE_KEY],
      finesse_report: searchReport([policy, policy]),
    }),
    (error) =>
      error instanceof Ctk3ResultError &&
      error.code === "duplicate-finesse-policy",
  );
  assert.throws(
    () => buildCtk3Result({
      schema_version: ARTIFACT_SCHEMA,
      solution_keys: [SIMPLE_KEY],
      finesse_report: searchReport([
        { ...policy, solution_averages: [] },
      ]),
    }),
    (error) =>
      error instanceof Ctk3ResultError &&
      error.code === "finesse-solution-average-key-mismatch",
  );
});

test("search exact totals never substitute for a missing solution average", () => {
  const result = buildCtk3Result({
    schema_version: ARTIFACT_SCHEMA,
    solution_keys: [SIMPLE_KEY],
    finesse_report: searchReport([
      {
        policy: "oracle",
        complete: true,
        solution_averages: [
          {
            solution_key: SIMPLE_KEY,
            average_inputs: "not-calculated",
            complete: true,
          },
        ],
      },
    ], "3"),
  });

  assert.ok(result);
  assert.equal(decodeCtk3(result.source).pages[0].comment, undefined);
});

test("search finesse emits no file when every requested policy has zero successes", () => {
  assert.equal(buildCtk3Result({
    schema_version: ARTIFACT_SCHEMA,
    solution_keys: [SIMPLE_KEY],
    finesse_report: searchReport([
      {
        policy: "oracle",
        complete: true,
        successful_unique_queue_count: 0,
        solution_averages: [
          {
            solution_key: SIMPLE_KEY,
            average_inputs: "unavailable",
            complete: true,
          },
        ],
      },
    ]),
  }), null);
});

test("finesse score paths become color-preserving CTK3 page operations", () => {
  const result = buildCtk3Result({
    schema_version: ARTIFACT_SCHEMA,
    finesse_report: scoreReport("7"),
    finesse_score: {
      initial_board: words(0x3fn),
      height: 4,
      representative_path: [
        { piece: "I", rotation: 0, x: 6, y: 0, cleared_lines: 1 },
        { piece: "O", rotation: 0, x: 0, y: 0, cleared_lines: 0 },
        { piece: "I", rotation: 0, x: 2, y: 0, cleared_lines: 0 },
      ],
    },
  });

  assert.ok(result);
  assert.equal(result.pageCount, 3);
  const pages = decodeCtk3(result.source).pages;
  assert.deepEqual(pages[0].cells.slice(0, 10), [
    "G", "G", "G", "G", "G", "G", null, null, null, null,
  ]);
  assert.deepEqual(pages[0].operation, {
    piece: "I",
    rotation: "spawn",
    x: 7,
    y: 0,
  });
  assert.equal(pages[0].comment, "F=7");
  assert.deepEqual(pages[1].cells, []);
  assert.deepEqual(pages[1].operation, {
    piece: "O",
    rotation: "spawn",
    x: 0,
    y: 0,
  });
  assert.equal(pages[1].comment, undefined);
  assert.deepEqual(pages[2].cells.slice(0, 2), ["O", "O"]);
  assert.deepEqual(pages[2].cells.slice(10, 12), ["O", "O"]);
});

test("Fumen operation canonicalization crosses typed score argv and CTK3 report output", () => {
  const fumen = fumenEncoder.encode([{
    field: Field.create("XXXXXX____"),
    operation: { type: "I", rotation: "spawn", x: 7, y: 0 },
    comment: "input comment must not become authoritative output",
  }]);
  const arguments_ = buildSlashCommandArguments(
    findSlashCommand("finesse").subcommands.score,
    [
      { name: "document", value: fumen },
      { name: "next", value: "I" },
      { name: "options", value: "hold=avoid knowledge=oracle" },
    ],
  );
  assert.deepEqual(arguments_, [
    "finesse", "score",
    "--initial-mask", `${"0".repeat(58)}3f`,
    "--height", "1",
    "--placements", "I:spawn:6:0",
    "--queue", "I",
    "--no-hold",
    "--pattern-knowledge", "oracle",
  ]);
  assert.equal(arguments_.includes(fumen), false);

  const result = buildCtk3Result({
    schema_version: ARTIFACT_SCHEMA,
    finesse_report: scoreReport("1"),
    finesse_score: {
      initial_board: words(0x3fn),
      height: 1,
      representative_path: [
        { piece: "I", rotation: 0, x: 6, y: 0, cleared_lines: 1 },
      ],
    },
  });
  assert.ok(result);
  const [page] = decodeCtk3(result.source).pages;
  assert.deepEqual(page.cells, [
    "G", "G", "G", "G", "G", "G", null, null, null, null,
  ]);
  assert.deepEqual(page.operation, {
    piece: "I", rotation: "spawn", x: 7, y: 0,
  });
  assert.equal(page.comment, "F=1");
});

test("finesse score operations use only the declared canonical orientation", () => {
  const cases = [
    ["O", 3, 1, 1, { piece: "O", rotation: "spawn", x: 1, y: 1 }],
    ["I", 2, 1, 1, { piece: "I", rotation: "spawn", x: 2, y: 1 }],
    ["I", 3, 1, 1, { piece: "I", rotation: "right", x: 1, y: 3 }],
    ["S", 2, 1, 1, { piece: "S", rotation: "spawn", x: 2, y: 1 }],
    ["S", 3, 1, 1, { piece: "S", rotation: "right", x: 1, y: 2 }],
    ["Z", 2, 1, 1, { piece: "Z", rotation: "spawn", x: 2, y: 1 }],
    ["Z", 3, 1, 1, { piece: "Z", rotation: "right", x: 1, y: 2 }],
    ["J", 2, 1, 1, { piece: "J", rotation: "reverse", x: 2, y: 2 }],
    ["L", 3, 1, 1, { piece: "L", rotation: "left", x: 2, y: 2 }],
    ["T", 1, 1, 1, { piece: "T", rotation: "right", x: 1, y: 2 }],
  ];

  for (const [piece, rotation, x, y, expected] of cases) {
    const result = buildCtk3Result({
      schema_version: ARTIFACT_SCHEMA,
      finesse_report: scoreReport("1"),
      finesse_score: {
        initial_board: words(0n),
        height: 8,
        representative_path: [
          { piece, rotation, x, y, cleared_lines: 0 },
        ],
      },
    });
    assert.ok(result, `${piece}:${rotation}`);
    assert.deepEqual(
      decodeCtk3(result.source).pages[0].operation,
      expected,
      `${piece}:${rotation}`,
    );
  }
});

test("finesse score replay fails closed when a declared line clear is wrong", () => {
  assert.throws(
    () => buildCtk3Result({
      schema_version: ARTIFACT_SCHEMA,
      finesse_report: scoreReport("1"),
      finesse_score: {
        initial_board: words(0x3fn),
        height: 4,
        representative_path: [
          { piece: "I", rotation: 0, x: 6, y: 0, cleared_lines: 0 },
        ],
      },
    }),
    (error) =>
      error instanceof Ctk3ResultError &&
      error.code === "invalid-finesse-score" &&
      error.path.endsWith("cleared_lines"),
  );
});

test("an empty finesse representative path does not create an initial-only file", () => {
  assert.equal(buildCtk3Result({
    schema_version: ARTIFACT_SCHEMA,
    finesse_report: scoreReport(null),
    finesse_score: {
      initial_board: words(0x3fn),
      height: 4,
      representative_path: [],
    },
  }), null);
});

test("finesse score geometry is suppressed when every policy has zero successes", () => {
  const result = buildCtk3Result({
    schema_version: ARTIFACT_SCHEMA,
    finesse_report: scoreReport(null, [
      scorePolicy("oracle", 0, "unavailable"),
      scorePolicy("visible-7", 0, "unavailable"),
    ]),
    finesse_score: {
      initial_board: words(0n),
      height: 4,
      representative_path: [
        { piece: "O", rotation: 0, x: 0, y: 0, cleared_lines: 0 },
      ],
    },
  });

  assert.equal(result, null);
});

test("one successful score policy permits the representative operation pages", () => {
  const result = buildCtk3Result({
    schema_version: ARTIFACT_SCHEMA,
    finesse_report: {
      ...scoreReport(null, [scorePolicy("oracle", 1, "6.5")]),
      representative_witness: {
        policy: "oracle",
        solution_key: "given-operation-sequence",
        pattern_ids: [3],
        queue: ["O"],
        total_inputs: 2,
        input_sequence: ["das-left", "hard-drop"],
        placements: [{ piece: "O", rotation: 0, x: 0, y: 0 }],
      },
    },
    finesse_score: {
      initial_board: words(0n),
      height: 4,
      representative_path: [
        { piece: "O", rotation: 0, x: 0, y: 0, cleared_lines: 0 },
      ],
    },
  });

  assert.ok(result);
  assert.equal(result.pageCount, 1);
  const page = decodeCtk3(result.source).pages[0];
  assert.equal(page.comment, "F=6.5");
  assert.equal(page.operation.piece, "O");
});

test("finesse score success counts and typed averages must agree", () => {
  assert.throws(
    () => buildCtk3Result({
      schema_version: ARTIFACT_SCHEMA,
      finesse_report: scoreReport(null, [scorePolicy("oracle", 0, "5")]),
      finesse_score: {
        initial_board: words(0n),
        height: 4,
        representative_path: [
          { piece: "O", rotation: 0, x: 0, y: 0, cleared_lines: 0 },
        ],
      },
    }),
    (error) =>
      error instanceof Ctk3ResultError &&
      error.code === "finesse-score-success-mismatch",
  );
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
  assert.equal(page.comment, "#1 | Q=IO | D=4 | T-spin 2L");
});

test("REN Discord projection keeps only the core-supplied canonical candidate", () => {
  const outcome = (id, sourceQueue) => ({
    id,
    source_queue: sourceQueue,
    ren_count: 0,
    total_damage: 0,
    spin_piece: null,
    spin_mini: false,
    spin_lines: 0,
    final_board: words(0xfn),
    path: [{
      piece: "I",
      placement_mask: words(0xfn),
      cleared_row_mask: 0,
      board_after: words(0xfn),
    }],
  });
  const canonical = outcome("3", "I");
  const secondary = outcome("9", "I");
  const input = {
    kind: "ren",
    artifacts: {
      schema_version: ARTIFACT_SCHEMA,
      forward: {
        initial_board: words(0n),
        canonical_selection: "smallest-canonical-candidate-id",
        canonical_outcome: structuredClone(canonical),
        outcomes: [canonical, secondary],
      },
    },
  };
  const result = buildCtk3Result(input);

  assert.ok(result);
  assert.equal(result.pageCount, 1);
  assert.match(decodeCtk3(result.source).pages[0].comment, /^#1\b/);
  assert.doesNotMatch(decodeCtk3(result.source).pages[0].comment, /#(?:3|9)\b/u);

  const missing = structuredClone(input);
  delete missing.artifacts.forward.canonical_outcome;
  assert.throws(
    () => buildCtk3Result(missing),
    (error) => error instanceof Ctk3ResultError &&
      error.code === "invalid-forward-canonical-witness",
  );

  const mismatched = structuredClone(input);
  mismatched.artifacts.forward.canonical_outcome = structuredClone(secondary);
  assert.throws(
    () => buildCtk3Result(mismatched),
    (error) => error instanceof Ctk3ResultError &&
      error.code === "invalid-forward-canonical-witness",
  );

  for (const zeroId of ["0", 0]) {
    const zeroCandidate = structuredClone(input);
    zeroCandidate.artifacts.forward.canonical_outcome.id = zeroId;
    zeroCandidate.artifacts.forward.outcomes[0].id = zeroId;
    assert.throws(
      () => buildCtk3Result(zeroCandidate),
      (error) => error instanceof Ctk3ResultError &&
        error.code === "invalid-forward-candidate-id",
      `zero candidate id ${JSON.stringify(zeroId)} must fail closed`,
    );
  }
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

test("zero-solution summaries suppress stale initial-only CTK3 artifacts", () => {
  const initialOnlyKey =
    "ctk1|initial=000000000000003f|placements=";
  for (const [field, value] of [
    ["result_count", 0],
    ["total_solution_count", 0],
    ["unique_solution_count", "0"],
    ["normalized_unique_solution_count", 0],
  ]) {
    assert.equal(buildCtk3Result({
      schema_version: 2,
      summary: {
        count_complete: true,
        [field]: value,
      },
      contract: {
        artifacts: {
          schema_version: ARTIFACT_SCHEMA,
          solution_keys: [initialOnlyKey],
        },
      },
    }), null, field);
  }
});

function words(mask) {
  return `0x${hex(mask, 64)}`;
}

function scoreReport(exactTotalInputs, policyResults) {
  return {
    mode: "score",
    metric: "inputs",
    pattern_knowledge: "oracle",
    complete: true,
    exact_total_inputs: exactTotalInputs,
    policy_results: policyResults ?? [
      scorePolicy(
        "oracle",
        exactTotalInputs === null ? 0 : 1,
        exactTotalInputs ?? "unavailable",
      ),
    ],
  };
}

function scorePolicy(policy, successfulQueueCount, averageInputs) {
  return {
    policy,
    complete: true,
    successful_unique_queue_count: successfulQueueCount,
    solution_averages: [
      {
        solution_key: "given-operation-sequence",
        average_inputs: averageInputs,
        complete: true,
      },
    ],
  };
}

function searchReport(policyResults, exactTotalInputs = null) {
  return {
    mode: "search",
    metric: "inputs",
    pattern_knowledge: "both",
    complete: true,
    exact_total_inputs: exactTotalInputs,
    policy_results: policyResults,
  };
}

function hex(mask, width) {
  return mask.toString(16).padStart(width, "0");
}

function maskForOperation(operation) {
  return operationCells(operation).reduce(
    (mask, { x, y }) => mask | (1n << BigInt(y * 10 + x)),
    0n,
  );
}
