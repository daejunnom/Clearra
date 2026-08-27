import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const contractUrl = new URL("../fixtures/contracts/search_option_contract.tsv", import.meta.url);
const COLUMNS = Object.freeze([
  "family",
  "option",
  "kind",
  "valid",
  "invalid",
  "discordDefault",
  "nativeDefault",
  "disposition",
  "discordPath",
  "exposure",
  "lowering",
  "reason",
  "dependencies",
]);
const DISPOSITIONS = new Set(["named", "preset", "excluded"]);

function contractRows() {
  const source = readFileSync(contractUrl, "utf8");
  assert.match(source.split(/\r?\n/u)[0], /contract v2\.$/u);
  return source
    .split(/\r?\n/u)
    .filter((line) => line && !line.startsWith("#"))
    .map((line) => {
      const columns = line.split("\t");
      assert.equal(columns.length, COLUMNS.length, `thirteen columns: ${line}`);
      const row = Object.fromEntries(COLUMNS.map((name, index) => [name, columns[index]]));
      row.valid = representatives(row.valid);
      row.invalid = representatives(row.invalid);
      return row;
    });
}

test("v2 splits forward damage, ordered spin, and unordered structure option authority", () => {
  assert.deepEqual(
    [...new Set(contractRows().map(({ family }) => family))].sort(),
    [
      "build",
      "finesse-score",
      "forward-damage",
      "forward-spin",
      "pc",
      "sequence",
      "sequence-dependencies",
      "setup",
      "spin-structure",
    ],
  );
});

test("every option is unique and decision-complete without hidden or sfinder-baked gaps", () => {
  const rows = contractRows();
  const identities = rows.map(({ family, option }) => `${family}.${option}`);
  assert.equal(new Set(identities).size, identities.length);
  for (const row of rows) {
    assert.ok(DISPOSITIONS.has(row.disposition), `${row.family}.${row.option}`);
    assert.ok(row.valid.length > 0, `${row.family}.${row.option} representatives`);
    assert.notEqual(row.discordDefault, "", `${row.family}.${row.option} Discord default`);
    assert.notEqual(row.nativeDefault, "", `${row.family}.${row.option} native default`);
    assert.notEqual(row.reason, "", `${row.family}.${row.option} reason`);
    assert.notEqual(row.dependencies, "", `${row.family}.${row.option} dependencies`);
    assert.doesNotMatch(
      [row.disposition, row.discordPath, row.exposure, row.reason].join(" "),
      /hidden|sfinder-baked|packed:/i,
      `${row.family}.${row.option}`,
    );
    if (row.disposition === "named") {
      assert.match(row.discordPath, /^\//u, `${row.family}.${row.option} path`);
      assert.notEqual(row.exposure, "-", `${row.family}.${row.option} exposure`);
      assert.notEqual(row.lowering, "none", `${row.family}.${row.option} lowering`);
    } else if (row.disposition === "preset") {
      assert.match(row.discordPath, /^\//u, `${row.family}.${row.option} preset path`);
      assert.notEqual(row.lowering, "none", `${row.family}.${row.option} preset lowering`);
    } else {
      assert.equal(row.discordPath, "-", `${row.family}.${row.option} excluded path`);
      assert.equal(row.lowering, "none", `${row.family}.${row.option} excluded lowering`);
    }
  }
});

test("host and performance controls remain explicit result-neutral exclusions", () => {
  const rows = contractRows();
  const hostOptions = new Set([
    "backend",
    "fallback",
    "workers",
    "logical-processors",
    "tablebase",
    "dependency-dag",
    "gpu-device",
  ]);
  const exclusions = rows.filter(({ option }) => hostOptions.has(option));
  assert.ok(exclusions.length >= 12);
  for (const row of exclusions) {
    assert.equal(row.disposition, "excluded", `${row.family}.${row.option}`);
    assert.match(row.exposure, /host-policy/u);
    assert.match(row.reason, /host-owned-(?:execution|performance)-policy/u);
    assert.equal(row.dependencies, "does-not-change-search-result");
  }
});

test("v2 freezes compatibility defaults and fail-closed dependencies", () => {
  const rows = contractRows();
  const row = (family, option) => {
    const value = rows.find((candidate) =>
      candidate.family === family && candidate.option === option
    );
    assert.ok(value, `${family}.${option}`);
    return value;
  };
  assert.equal(row("pc", "lines").discordDefault, "auto");
  assert.equal(row("pc", "lines").nativeDefault, "2");
  assert.equal(row("pc", "objective").discordDefault, "all");
  assert.equal(row("pc", "objective").nativeDefault, "all");
  assert.match(
    row("pc", "queue-knowledge").dependencies,
    /visible-7-incompatible-with-objective=minimum-cover/u,
  );
  assert.equal(row("pc", "queue-knowledge").disposition, "named");
  assert.equal(row("pc", "spin-profile").disposition, "named");
  assert.equal(row("pc", "preserve-b2b").disposition, "named");
  assert.equal(row("pc", "solution-probabilities").disposition, "named");
  assert.equal(row("build", "aggregation").disposition, "named");
  assert.deepEqual(row("build", "solution-probabilities").valid, ["off", "on"]);
  assert.equal(row("build", "solution-probabilities").disposition, "named");
  assert.equal(row("build", "solution-probabilities").discordDefault, "off");
  assert.equal(row("build", "solution-probabilities").nativeDefault, "off");
  assert.equal(row("build", "solution-probabilities").exposure, "solution-probabilities");
  assert.equal(row("build", "solution-probabilities").lowering, "--solution-probabilities");
  assert.equal(row("build", "solution-probabilities").dependencies, "unavailable-with-tiling");
  assert.equal(row("build", "source-pieces").exposure, "source-pieces");
  assert.equal(row("build", "source-pieces").discordDefault, "auto");
  assert.equal(row("build", "source-pieces").nativeDefault, "auto");
  assert.match(
    row("build", "source-pieces").dependencies,
    /target-piece-count\+hold-enabled-occupied-initial-hold/u,
  );
  assert.equal(row("setup", "mode").exposure, "mode");
  assert.equal(row("forward-damage", "spin-profile").discordDefault, "all-mini-plus");
  assert.equal(row("forward-damage", "spin-profile").nativeDefault, "all-mini-plus");
  assert.equal(row("forward-spin", "category").exposure, "spin-category");
  assert.equal(row("spin-structure", "minimality").exposure, "minimality");
});

function representatives(value) {
  return value === "-" ? [] : value.split("|");
}
