import assert from "node:assert/strict";
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  ACCEPTED_CTK3_MANIFEST,
  sealAcceptedCtk3Dist,
  verifyAcceptedCtk3Dist,
} from "./accepted-ctk3-dist.mjs";

const SOURCE_COMMIT = "0123456789abcdef0123456789abcdef01234567";
const OTHER_COMMIT = "89abcdef0123456789abcdef0123456789abcdef";
const RUN_ID = "33180374868";
const RUN_ATTEMPT = "2";

test("seals and verifies the exact accepted CTK3 file set", async () => {
  await withFixture(async (dist) => {
    const manifest = await sealAcceptedCtk3Dist(
      dist,
      SOURCE_COMMIT,
      RUN_ID,
      RUN_ATTEMPT,
    );
    assert.equal(manifest.source_commit, SOURCE_COMMIT);
    assert.equal(manifest.run_id, RUN_ID);
    assert.equal(manifest.run_attempt, RUN_ATTEMPT);
    assert.deepEqual(
      manifest.files.map((entry) => entry.path),
      ["decodeWorker.js", "index.cjs", "index.d.ts", "index.js", "nested/map.js.map"],
    );
    assert.deepEqual(
      await verifyAcceptedCtk3Dist(
        dist,
        SOURCE_COMMIT,
        RUN_ID,
        RUN_ATTEMPT,
      ),
      manifest,
    );
  });
});

test("rejects payload mutation and unsealed extra files", async () => {
  await withFixture(async (dist) => {
    await sealAcceptedCtk3Dist(dist, SOURCE_COMMIT, RUN_ID, RUN_ATTEMPT);
    await writeFile(join(dist, "index.js"), "mutated", "utf8");
    await assert.rejects(
      verifyAcceptedCtk3Dist(dist, SOURCE_COMMIT, RUN_ID, RUN_ATTEMPT),
      /does not match its sealed file set and hashes/u,
    );
  });
  await withFixture(async (dist) => {
    await sealAcceptedCtk3Dist(dist, SOURCE_COMMIT, RUN_ID, RUN_ATTEMPT);
    await writeFile(join(dist, "extra.js"), "extra", "utf8");
    await assert.rejects(
      verifyAcceptedCtk3Dist(dist, SOURCE_COMMIT, RUN_ID, RUN_ATTEMPT),
      /does not match its sealed file set and hashes/u,
    );
  });
});

test("rejects source or accepted-run drift and malformed manifest authority", async () => {
  await withFixture(async (dist) => {
    await sealAcceptedCtk3Dist(dist, SOURCE_COMMIT, RUN_ID, RUN_ATTEMPT);
    await assert.rejects(
      verifyAcceptedCtk3Dist(dist, OTHER_COMMIT, RUN_ID, RUN_ATTEMPT),
      /source commit mismatch/u,
    );
    await assert.rejects(
      verifyAcceptedCtk3Dist(dist, SOURCE_COMMIT, "33180374869", RUN_ATTEMPT),
      /run ID mismatch/u,
    );
    await assert.rejects(
      verifyAcceptedCtk3Dist(dist, SOURCE_COMMIT, RUN_ID, "3"),
      /run attempt mismatch/u,
    );
    const manifestPath = join(dist, ACCEPTED_CTK3_MANIFEST);
    const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
    manifest.unexpected = true;
    await writeFile(manifestPath, JSON.stringify(manifest), "utf8");
    await assert.rejects(
      verifyAcceptedCtk3Dist(dist, SOURCE_COMMIT, RUN_ID, RUN_ATTEMPT),
      /unexpected keys/u,
    );
  });
});

test("rejects missing runtime surfaces and resealing", async () => {
  await withFixture(async (dist) => {
    await rm(join(dist, "index.d.ts"));
    await assert.rejects(
      sealAcceptedCtk3Dist(dist, SOURCE_COMMIT, RUN_ID, RUN_ATTEMPT),
      /missing index\.d\.ts/u,
    );
  });
  await withFixture(async (dist) => {
    await sealAcceptedCtk3Dist(dist, SOURCE_COMMIT, RUN_ID, RUN_ATTEMPT);
    await assert.rejects(
      sealAcceptedCtk3Dist(dist, SOURCE_COMMIT, RUN_ID, RUN_ATTEMPT),
      /manifest already exists/u,
    );
  });
});

test("rejects non-canonical run authority and stale manifest residue", async () => {
  await withFixture(async (dist) => {
    await assert.rejects(
      sealAcceptedCtk3Dist(dist, SOURCE_COMMIT, "033180374868", RUN_ATTEMPT),
      /run ID must be a canonical positive decimal string/u,
    );
  });
  await withFixture(async (dist) => {
    await writeFile(
      join(dist, "clearra-accepted-ctk3.v1.json"),
      "{}\n",
      "utf8",
    );
    await assert.rejects(
      sealAcceptedCtk3Dist(dist, SOURCE_COMMIT, RUN_ID, RUN_ATTEMPT),
      /stale authority manifest/u,
    );
  });
});

async function withFixture(body) {
  const root = await mkdtemp(join(tmpdir(), "clearra-accepted-ctk3-"));
  const dist = join(root, "dist");
  try {
    await mkdir(join(dist, "nested"), { recursive: true });
    await Promise.all([
      writeFile(join(dist, "decodeWorker.js"), "worker", "utf8"),
      writeFile(join(dist, "index.cjs"), "cjs", "utf8"),
      writeFile(join(dist, "index.d.ts"), "types", "utf8"),
      writeFile(join(dist, "index.js"), "esm", "utf8"),
      writeFile(join(dist, "nested", "map.js.map"), "map", "utf8"),
    ]);
    await body(dist);
  } finally {
    await rm(root, { force: true, recursive: true });
  }
}
