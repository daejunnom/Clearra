import assert from "node:assert/strict";
import test from "node:test";

import {
  canonicalJson,
  canonicalSha256,
} from "../canonical-release-evidence.mjs";
import {
  compilePc4ProductionHostGeneration,
  PC4_PRODUCTION_HOST_GENERATION_SCHEMA,
} from "./compile-production-host-generation.mjs";
import {
  PC4_PARTIAL_GENERATION_MANIFEST_SCHEMA,
  PC4_RULE_PROFILES,
} from "./partial-generation-manifest.mjs";

const REVISION = "a".repeat(40);
const REPOSITORY = "muse918/tetris-4lpc-mdp-vstar-policy";
const PC_SEMANTICS = "clearra.pc4.full-bottom-rows-after-clear.v1";
const SETUP_SEMANTICS = "clearra.pc4.setup-complete-bottom-rows-after-clear.v1";

test("compiles deterministic canonical bytes with one ready profile and four typed unavailable slots", () => {
  const fixture = generationFixture(["jstris-180"], ["pc-search"]);
  const reversed = structuredClone(fixture);
  reversed.partialGeneration.profiles.reverse();
  reversed.readerGeneration.profiles.reverse();

  const first = compilePc4ProductionHostGeneration(fixture);
  const second = compilePc4ProductionHostGeneration(reversed);
  assert.equal(first, second);
  const value = JSON.parse(first);
  assert.equal(first, `${canonicalJson(value)}\n`);
  assert.equal(value.schema, PC4_PRODUCTION_HOST_GENERATION_SCHEMA);
  assert.deepEqual(value.profiles.map(({ profile }) => profile), PC4_RULE_PROFILES);
  assert.deepEqual(
    value.profiles.filter(({ status }) => status === "unavailable")
      .map(({ profile, reason }) => [profile, reason]),
    [
      ["srs", "missing-profile-artifacts"],
      ["srs-plus", "missing-profile-specific-index"],
      ["srs-x", "missing-format-specification"],
      ["no-kick", "missing-known-answers"],
    ],
  );
  const ready = value.profiles.find(({ profile }) => profile === "jstris-180");
  assert.deepEqual(ready.pc_search_target_lines, [4]);
  assert.deepEqual(ready.setup_search_target_lines, []);
  assert.equal(ready.target_qualification_receipts.length, 1);
  assert.equal(ready.target_qualification_receipts[0].receipt_identity,
    fixture.targetQualificationReceipts[0].receipt_identity);
  assert.equal(value.transferred_bytes, 0);
});

test("supports more than one independently admitted profile without borrowing artifacts", () => {
  const fixture = generationFixture(["srs", "jstris-180"], ["pc-search"]);
  const value = JSON.parse(compilePc4ProductionHostGeneration(fixture));
  const ready = value.profiles.filter(({ status }) => status === "ready");
  assert.deepEqual(ready.map(({ profile }) => profile), ["srs", "jstris-180"]);
  assert.notEqual(
    ready[0].artifacts.graph.content_identity,
    ready[1].artifacts.graph.content_identity,
  );
  assert.notEqual(
    ready[0].target_qualification_receipts[0].receipt_identity,
    ready[1].target_qualification_receipts[0].receipt_identity,
  );
});

test("Setup stays disabled unless its separately sealed receipt covers the admitted Setup target", () => {
  const pcOnly = generationFixture(["jstris-180"], ["pc-search"]);
  const pcValue = JSON.parse(compilePc4ProductionHostGeneration(pcOnly));
  assert.deepEqual(pcValue.profiles[3].setup_search_target_lines, []);

  const both = generationFixture(["jstris-180"], ["pc-search", "setup-search"]);
  const missingSetup = structuredClone(both);
  missingSetup.targetQualificationReceipts = missingSetup.targetQualificationReceipts
    .filter(({ use_case: useCase }) => useCase === "pc-search");
  assert.throws(
    () => compilePc4ProductionHostGeneration(missingSetup),
    /receipts do not cover admission exactly|missing an exact target/u,
  );

  const value = JSON.parse(compilePc4ProductionHostGeneration(both));
  const ready = value.profiles[3];
  assert.deepEqual(ready.pc_search_target_lines, [4]);
  assert.deepEqual(ready.setup_search_target_lines, [4]);
  assert.deepEqual(
    ready.target_qualification_receipts.map(({ use_case: useCase }) => useCase),
    ["pc-search", "setup-search"],
  );
});

test("stale, mismatched, tampered, and duplicate receipts fail closed", () => {
  const mutations = [
    (fixture) => reseal(fixture.targetQualificationReceipts[0], { revision: "b".repeat(40) }),
    (fixture) => reseal(fixture.targetQualificationReceipts[0], {
      offline_exact_parity_identity: sha("e"),
    }),
    (fixture) => {
      fixture.targetQualificationReceipts[0].receipt_identity = sha("f");
    },
    (fixture) => {
      fixture.readerGeneration.profiles[3].artifacts.graph.content_identity = sha("e");
    },
  ];
  for (const mutate of mutations) {
    const fixture = generationFixture(["jstris-180"], ["pc-search"]);
    const replacement = mutate(fixture);
    if (replacement) fixture.targetQualificationReceipts[0] = replacement;
    assert.throws(
      () => compilePc4ProductionHostGeneration(fixture),
      /stale|differs|identity|artifact/u,
    );
  }

  const duplicate = generationFixture(["jstris-180"], ["pc-search"]);
  duplicate.targetQualificationReceipts.push(
    structuredClone(duplicate.targetQualificationReceipts[0]),
  );
  assert.throws(
    () => compilePc4ProductionHostGeneration(duplicate),
    /duplicated/u,
  );
});

function generationFixture(qualifiedProfiles, useCases) {
  const qualified = new Set(qualifiedProfiles);
  const partialProfiles = PC4_RULE_PROFILES.map((profile, profileIndex) =>
    qualified.has(profile)
      ? admittedProfile(profile, profileIndex, useCases)
      : unavailableProfile(profile));
  const partialGeneration = {
    schema: PC4_PARTIAL_GENERATION_MANIFEST_SCHEMA,
    repository: REPOSITORY,
    resolved_revision: REVISION,
    generation_id: "pc4-production-candidate-1",
    manifest_content_identity: sha("d"),
    profiles: partialProfiles,
  };
  const readerGeneration = {
    schema: PC4_PRODUCTION_HOST_GENERATION_SCHEMA,
    repository: REPOSITORY,
    revision: REVISION,
    profiles: PC4_RULE_PROFILES.map((profile, profileIndex) =>
      qualified.has(profile)
        ? readyReader(partialProfiles[profileIndex], profileIndex)
        : {
          profile,
          upstream_complete: false,
          status: "unavailable",
          reason: unavailableProfile(profile).reason,
        }),
    transferred_bytes: 73,
  };
  const targetQualificationReceipts = partialProfiles
    .filter(({ status }) => status === "qualified")
    .flatMap((slot, profileIndex) => slot.targets.map((target, targetIndex) =>
      targetReceipt(slot, readerGeneration.profiles[PC4_RULE_PROFILES.indexOf(slot.profile)],
        target, profileIndex * 4 + targetIndex)));
  return { partialGeneration, readerGeneration, targetQualificationReceipts };
}

function admittedProfile(profile, index, useCases) {
  const tag = profile.replaceAll("-", "_");
  const artifacts = profileArtifacts(profile, index);
  return {
    profile,
    status: "qualified",
    profile_binding_identity: `${tag}/binding/v1`,
    rule_identity: `${tag}/rule/v1`,
    graph_format_identity: `${tag}/whole-solution-graph/v1`,
    provenance_identity: sha(hex(index + 1)),
    known_answers_identity: sha(hex(index + 6)),
    artifacts,
    targets: useCases.map((useCase, targetIndex) => {
      const setup = useCase === "setup-search";
      return {
        use_case: useCase,
        lines: 4,
        terminal_identity: `${tag}/${useCase}/4/terminal`,
        outgoing_edge_completeness_identity: sha(hex(index + targetIndex + 8)),
        known_answers_identity: sha(hex(index + targetIndex + 10)),
        offline_exact_parity_identity: sha(hex(index + targetIndex + 12)),
      };
    }),
  };
}

function unavailableProfile(profile) {
  return {
    profile,
    status: "not_qualified",
    reason: ({
      srs: "missing-profile-artifacts",
      "srs-plus": "missing-profile-specific-index",
      "srs-x": "missing-format-specification",
      "jstris-180": "missing-provenance",
      "no-kick": "missing-known-answers",
    })[profile],
  };
}

function profileArtifacts(profile, index) {
  const suffix = ({
    srs: "_no180",
    "srs-plus": "_srsplus",
    "srs-x": "_srsx",
    "jstris-180": "",
    "no-kick": "_nokick",
  })[profile];
  const tag = profile.replaceAll("-", "_");
  return [
    artifact("field-hash-index", `field_hash_to_id${suffix}.v1.bin`, 32, tag, hex(index + 1)),
    artifact("graph-offsets", `graph_offsets${suffix}.u32.bin`, 28, tag, hex(index + 2)),
    artifact("graph", `graph${suffix}.bin`, 27, tag, hex(index + 3)),
  ];
}

function artifact(role, path, byteLength, tag, digest) {
  return {
    role,
    path,
    byte_length: byteLength,
    content_identity: sha(digest),
    profile_binding_identity: `${tag}/${role}/binding/v1`,
  };
}

function readyReader(slot, index) {
  const artifacts = Object.fromEntries([
    ["fields", slot.artifacts.find(({ role }) => role === "field-hash-index")],
    ["offsets", slot.artifacts.find(({ role }) => role === "graph-offsets")],
    ["graph", slot.artifacts.find(({ role }) => role === "graph")],
  ].map(([key, value]) => [key, {
    path: value.path,
    byte_length: value.byte_length,
    content_identity: value.content_identity,
  }]));
  return {
    profile: slot.profile,
    upstream_complete: true,
    status: "ready",
    reader_contract: `hydra-${slot.profile}-complete-graph-v1`,
    field_count: 2,
    target_width: slot.profile === "srs-x" ? 4 : 3,
    target_lines: [4],
    pc_search_target_lines: [],
    setup_search_target_lines: [],
    target_qualification_receipts: [],
    terminal_id: 1,
    artifacts,
    evidence: [
      { id: 0, hash: 0, start: 0, end: 15 },
      { id: 1, hash: 2 ** 40 - 1, start: 15, end: 27 },
    ],
  };
}

function targetReceipt(slot, reader, target, salt) {
  const setup = target.use_case === "setup-search";
  const artifacts = Object.fromEntries([
    ["fields", slot.artifacts.find(({ role }) => role === "field-hash-index")],
    ["offsets", slot.artifacts.find(({ role }) => role === "graph-offsets")],
    ["graph", slot.artifacts.find(({ role }) => role === "graph")],
  ].map(([key, value]) => [key, {
    path: value.path,
    byte_length: value.byte_length,
    content_identity: value.content_identity,
  }]));
  const core = {
    schema: setup
      ? "clearra.pc4.exact-setup-target-qualification.v1"
      : "clearra.pc4.exact-target-qualification.v1",
    repository: REPOSITORY,
    revision: REVISION,
    profile: slot.profile,
    reader_contract: reader.reader_contract,
    use_case: target.use_case,
    target_lines: 4,
    terminal_id: 1,
    terminal_hash: 2 ** 40 - 1,
    terminal_semantics_identity: setup ? SETUP_SEMANTICS : PC_SEMANTICS,
    outgoing_edge_completeness_identity: target.outgoing_edge_completeness_identity,
    known_answer_identity: target.known_answers_identity,
    offline_exact_parity_identity: target.offline_exact_parity_identity,
    evidence: {
      outgoing_statement: {
        receipt_identity: target.outgoing_edge_completeness_identity,
        artifacts,
      },
      family_unique_solution_count: 456_459 + salt,
      family_normalized_solution_set_hash: `cts1:${hex(salt + 1).repeat(16)}`,
    },
    ...(setup ? {
      setup_differential_identities: {
        ranked_joint_identity: sha(hex(salt + 1)),
        ranked_build_probability_identity: sha(hex(salt + 2)),
        ranked_conditional_pc_identity: sha(hex(salt + 3)),
        exact_path_detail_identity: sha(hex(salt + 4)),
      },
    } : {}),
  };
  return seal(core);
}

function reseal(receipt, changes) {
  const { receipt_identity: _identity, ...core } = structuredClone(receipt);
  return seal({ ...core, ...changes });
}

function seal(core) {
  return { ...core, receipt_identity: `sha256:${canonicalSha256(core)}` };
}

function sha(character) {
  return `sha256:${character.repeat(64)}`;
}

function hex(value) {
  return (value % 15 + 1).toString(16);
}
