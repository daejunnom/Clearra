import assert from "node:assert/strict";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import {
  CURRENT_PRODUCT_TAG,
  CURRENT_PRODUCT_VERSION,
  expectedProductArtifactName,
  readCurrentProductRelease,
} from "./current-product-release.mjs";

test("the checked-out workspace owns one current release identity", () => {
  assert.equal(CURRENT_PRODUCT_VERSION, "0.8.1");
  assert.equal(CURRENT_PRODUCT_TAG, "v0.8.1");
  assert.equal(
    expectedProductArtifactName("linux-cli"),
    "Clearra-CLI-v0.8.1-linux-x86_64",
  );
  assert.equal(
    expectedProductArtifactName("windows-cli"),
    "Clearra-CLI-v0.8.1-windows-x86_64.exe",
  );
  assert.equal(
    expectedProductArtifactName("windows-gui"),
    "Clearra-GUI-v0.8.1-windows-x86_64.exe",
  );
});

test("release parsing is restricted to workspace.package", async () => {
  const root = await mkdtemp(join(tmpdir(), "clearra-current-release-"));
  try {
    await writeFile(
      join(root, "Cargo.toml"),
      '[package]\nversion = "9.9.9"\n\n[workspace.package]\nversion = "0.9.0"\n\n[dependencies]\n',
      "utf8",
    );
    assert.deepEqual(readCurrentProductRelease(root), {
      version: "0.9.0",
      tag: "v0.9.0",
    });
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("unknown product artifact roles fail closed", () => {
  assert.throws(
    () => expectedProductArtifactName("discord"),
    /unsupported product artifact role/u,
  );
});
