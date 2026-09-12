import assert from "node:assert/strict";
import test from "node:test";

import {
  PC4_UPSTREAM_DISCOVERY_SCHEMA,
  discoverPc4UpstreamGeneration,
} from "./discover-upstream-generation.mjs";

const REVISION = "a".repeat(40);

test("moving discovery resolves once then inventories the immutable revision without profile inference", async () => {
  const calls = [];
  const result = await discoverPc4UpstreamGeneration({}, {
    requestJson: async (url, label) => {
      calls.push([url, label]);
      if (calls.length === 1) {
        return {
          id: "muse918/tetris-4lpc-mdp-vstar-policy",
          sha: REVISION,
          private: false,
          gated: false,
        };
      }
      return tree();
    },
  });

  assert.equal(result.schema, PC4_UPSTREAM_DISCOVERY_SCHEMA);
  assert.equal(result.resolved_revision, REVISION);
  assert.equal(result.qualification_status, "unqualified");
  assert.match(calls[0][0], /\/revision\/main$/u);
  assert.match(calls[1][0], new RegExp(`/tree/${REVISION}\\?`, "u"));
  assert.deepEqual(result.candidates.map(({ path }) => path), [
    "field_hash_to_id.v1.bin",
    "graph.bin",
    "graph_nokick.bin",
    "graph_offsets.u32.bin",
  ]);
  assert.ok(result.candidates.every((candidate) => !("profile" in candidate)));
  assert.ok(Object.isFrozen(result.candidates));
});

test("value-like files are not candidates and cannot supply missing graph helpers", async () => {
  const entries = tree().filter(({ path }) => path !== "graph_offsets.u32.bin");
  entries.push(file("layer0.values.f32.bin", 40, "f".repeat(64)));
  await assert.rejects(
    discoverPc4UpstreamGeneration({}, { requestJson: sequence(entries) }),
    /required graph\/index candidates/u,
  );
});

test("candidate artifact content and size must be self-consistent", async () => {
  for (const mutate of [
    (entries) => { entries[0].lfs.oid = "not-a-digest"; },
    (entries) => { entries[1].lfs.size += 1; },
    (entries) => { entries[2].size = 0; },
  ]) {
    const entries = tree();
    mutate(entries);
    await assert.rejects(
      discoverPc4UpstreamGeneration({}, { requestJson: sequence(entries) }),
      /invalid|differ/u,
    );
  }
});

test("metadata cannot redirect discovery to another repository or non-immutable identity", async () => {
  for (const metadata of [
    { id: "other/repository", sha: REVISION, private: false, gated: false },
    { id: "muse918/tetris-4lpc-mdp-vstar-policy", sha: "main", private: false, gated: false },
    { id: "muse918/tetris-4lpc-mdp-vstar-policy", sha: REVISION, private: true, gated: false },
  ]) {
    await assert.rejects(
      discoverPc4UpstreamGeneration({}, {
        requestJson: async (url) => url.includes("/revision/") ? metadata : tree(),
      }),
      /invalid|public repository/u,
    );
  }
});

function sequence(entries) {
  let count = 0;
  return async () => count++ === 0
    ? {
        id: "muse918/tetris-4lpc-mdp-vstar-policy",
        sha: REVISION,
        private: false,
        gated: false,
      }
    : entries;
}

function tree() {
  return [
    file("field_hash_to_id.v1.bin", 80, "1".repeat(64)),
    file("graph.bin", 1024, "2".repeat(64)),
    file("graph_nokick.bin", 800, "3".repeat(64)),
    file("graph_offsets.u32.bin", 52, "4".repeat(64)),
    file("layer0.policy.u16.bin", 32, "5".repeat(64)),
  ];
}

function file(path, size, digest) {
  return {
    type: "file",
    path,
    size,
    lfs: { oid: digest, size },
  };
}
