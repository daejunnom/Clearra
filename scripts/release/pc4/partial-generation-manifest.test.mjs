import assert from "node:assert/strict";
import test from "node:test";

import {
  PC4_PARTIAL_GENERATION_MANIFEST_SCHEMA,
  PC4_RULE_PROFILES,
  validatePc4PartialGenerationManifest,
} from "./partial-generation-manifest.mjs";

test("one qualified profile activates while four retain exact not-qualified reasons", () => {
  const input = candidate([qualified("srs", [["pc-search", 4]])]);
  const manifest = validatePc4PartialGenerationManifest(input);

  assert.equal(manifest.profiles.length, 5);
  assert.deepEqual(manifest.profiles.map(({ profile }) => profile), PC4_RULE_PROFILES);
  assert.equal(manifest.profiles[0].status, "qualified");
  assert.deepEqual(
    manifest.profiles.slice(1).map(({ status, reason }) => [status, reason]),
    [
      ["not_qualified", "missing-profile-specific-index"],
      ["not_qualified", "missing-format-specification"],
      ["not_qualified", "missing-provenance"],
      ["not_qualified", "missing-known-answers"],
    ],
  );
  assert.ok(Object.isFrozen(manifest));
  assert.ok(Object.isFrozen(manifest.profiles));
});

test("qualified targets stay independent by use case and line count", () => {
  const manifest = validatePc4PartialGenerationManifest(candidate([
    qualified("srs-x", [["pc-search", 2], ["setup-search", 4]]),
  ]));
  const slot = manifest.profiles.find(({ profile }) => profile === "srs-x");
  assert.deepEqual(
    slot.targets.map(({ use_case, lines }) => [use_case, lines]),
    [["pc-search", 2], ["setup-search", 4]],
  );
  assert.equal(
    manifest.profiles.find(({ profile }) => profile === "srs").status,
    "not_qualified",
  );
});

test("a generation with no qualified profile is rejected", () => {
  const input = candidate([]);
  assert.throws(
    () => validatePc4PartialGenerationManifest(input),
    /without a qualified profile/u,
  );
});

test("the fixed five-profile set cannot omit duplicate or invent a slot", () => {
  const missing = candidate([qualified("srs", [["pc-search", 4]])]);
  missing.profiles.pop();
  assert.throws(
    () => validatePc4PartialGenerationManifest(missing),
    /exactly five/u,
  );

  const duplicate = candidate([qualified("srs", [["pc-search", 4]])]);
  duplicate.profiles[4] = { ...duplicate.profiles[3] };
  assert.throws(
    () => validatePc4PartialGenerationManifest(duplicate),
    /duplicated|incomplete/u,
  );
});

test("graph index and qualification evidence cannot be borrowed across profiles", () => {
  const srs = qualified("srs", [["pc-search", 4]]);
  const plus = qualified("srs-plus", [["pc-search", 4]]);
  for (const mutate of [
    (slot) => { slot.artifacts[0].path = srs.artifacts[0].path; },
    (slot) => { slot.artifacts[1].content_identity = srs.artifacts[1].content_identity; },
    (slot) => { slot.graph_format_identity = srs.graph_format_identity; },
    (slot) => {
      slot.targets[0].offline_exact_parity_identity =
        srs.targets[0].offline_exact_parity_identity;
    },
  ]) {
    const changed = structuredClone(plus);
    mutate(changed);
    assert.throws(
      () => validatePc4PartialGenerationManifest(candidate([srs, changed])),
      /cannot be borrowed across profiles/u,
    );
  }
});

test("a qualified slot needs all three profile-bound artifacts and one target", () => {
  const missingArtifact = qualified("no-kick", [["pc-search", 4]]);
  missingArtifact.artifacts.pop();
  assert.throws(
    () => validatePc4PartialGenerationManifest(candidate([missingArtifact])),
    /exactly three/u,
  );

  const noTarget = qualified("no-kick", [["pc-search", 4]]);
  noTarget.targets = [];
  assert.throws(
    () => validatePc4PartialGenerationManifest(candidate([noTarget])),
    /at least one bounded target/u,
  );
});

function candidate(qualifiedSlots) {
  const replacements = new Map(qualifiedSlots.map((slot) => [slot.profile, slot]));
  const reasons = new Map([
    ["srs", "missing-profile-artifacts"],
    ["srs-plus", "missing-profile-specific-index"],
    ["srs-x", "missing-format-specification"],
    ["jstris-180", "missing-provenance"],
    ["no-kick", "missing-known-answers"],
  ]);
  return {
    schema: PC4_PARTIAL_GENERATION_MANIFEST_SCHEMA,
    repository: "muse918/tetris-4lpc-mdp-vstar-policy",
    resolved_revision: "a".repeat(40),
    generation_id: "synthetic-generation-1",
    manifest_content_identity: "synthetic-manifest-content-1",
    profiles: [...PC4_RULE_PROFILES].reverse().map((profile) =>
      replacements.get(profile) ?? {
        profile,
        status: "not_qualified",
        reason: reasons.get(profile),
      }),
  };
}

function qualified(profile, targetPairs) {
  const tag = profile.replaceAll("-", "_");
  return {
    profile,
    status: "qualified",
    profile_binding_identity: `${tag}/binding`,
    rule_identity: `${tag}/rule`,
    graph_format_identity: `${tag}/graph-format`,
    provenance_identity: `${tag}/provenance`,
    known_answers_identity: `${tag}/profile-kat`,
    artifacts: [
      artifact(profile, "field-hash-index", "field.idx", 80),
      artifact(profile, "graph-offsets", "offsets.idx", 52),
      artifact(profile, "graph", "graph.bin", 1024),
    ],
    targets: targetPairs.map(([useCase, lines]) => ({
      use_case: useCase,
      lines,
      terminal_identity: `${tag}/${useCase}/${lines}/terminal`,
      outgoing_edge_completeness_identity: `${tag}/${useCase}/${lines}/outgoing`,
      known_answers_identity: `${tag}/${useCase}/${lines}/kat`,
      offline_exact_parity_identity: `${tag}/${useCase}/${lines}/parity`,
    })),
  };
}

function artifact(profile, role, leaf, byteLength) {
  const tag = profile.replaceAll("-", "_");
  return {
    role,
    path: `${tag}/${leaf}`,
    byte_length: byteLength,
    content_identity: `${tag}/${role}/content`,
    profile_binding_identity: `${tag}/${role}/binding`,
  };
}
