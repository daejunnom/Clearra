import assert from "node:assert/strict";
import { mkdtemp, mkdir, rm, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import test from "node:test";

import { releaseTreeSha256 } from "../scripts/release-tree-digest.mjs";

test("Oracle release tree digest is deterministic and observes file and symlink drift", async () => {
  const first = await mkdtemp(resolve(tmpdir(), "clearra-release-tree-first-"));
  const second = await mkdtemp(resolve(tmpdir(), "clearra-release-tree-second-"));
  try {
    for (const root of [first, second]) {
      await mkdir(resolve(root, "nested"));
      await writeFile(resolve(root, "nested", "source.mjs"), "export const value = 1;\n");
      await symlink("nested/source.mjs", resolve(root, "entry.mjs"));
    }
    const initial = releaseTreeSha256(first);
    assert.match(initial, /^[0-9a-f]{64}$/u);
    assert.equal(releaseTreeSha256(second), initial);

    await writeFile(resolve(second, "nested", "source.mjs"), "export const value = 2;\n");
    assert.notEqual(releaseTreeSha256(second), initial);
    await writeFile(resolve(second, "nested", "source.mjs"), "export const value = 1;\n");
    await rm(resolve(second, "entry.mjs"));
    await symlink("nested/missing.mjs", resolve(second, "entry.mjs"));
    assert.throws(() => releaseTreeSha256(second), /symlink is dangling/u);
  } finally {
    await rm(first, { recursive: true, force: true });
    await rm(second, { recursive: true, force: true });
  }
});

test("Oracle release tree digest rejects an external or mutable symlink target", async () => {
  const root = await mkdtemp(resolve(tmpdir(), "clearra-release-tree-root-"));
  const external = await mkdtemp(resolve(tmpdir(), "clearra-release-tree-external-"));
  try {
    const externalFile = resolve(external, "mutable.mjs");
    await writeFile(externalFile, "export const mutable = true;\n");
    await symlink(externalFile, resolve(root, "escaped.mjs"));
    assert.throws(() => releaseTreeSha256(root), /symlink escapes the immutable root/u);
  } finally {
    await rm(root, { recursive: true, force: true });
    await rm(external, { recursive: true, force: true });
  }
});
