import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { validateReleaseMetadata } from "./validate-release-metadata.mjs";

test("release metadata requires four matching versions and a nonempty dated changelog", () => {
  const root = mkdtempSync(join(tmpdir(), "clearra-release-metadata-"));
  mkdirSync(join(root, "apps/clearra-desktop/src-tauri"), { recursive: true });
  writeFileSync(join(root, "Cargo.toml"), '[workspace.package]\nversion = "0.7.5"\n');
  writeFileSync(
    join(root, "Cargo.lock"),
    'version = 4\n\n[[package]]\nname = "clearra-cli"\nversion = "0.7.5"\n',
  );
  writeFileSync(
    join(root, "apps/clearra-desktop/package.json"),
    JSON.stringify({ version: "0.7.5" }),
  );
  writeFileSync(
    join(root, "apps/clearra-desktop/src-tauri/Cargo.toml"),
    '[package]\nversion = "0.7.5"\n',
  );
  writeFileSync(
    join(root, "apps/clearra-desktop/src-tauri/Cargo.lock"),
    'version = 4\n\n[[package]]\nname = "clearra-desktop"\nversion = "0.7.5"\n',
  );
  writeFileSync(
    join(root, "apps/clearra-desktop/src-tauri/tauri.conf.json"),
    JSON.stringify({ version: "0.7.5" }),
  );
  writeFileSync(
    join(root, "CHANGELOG.md"),
    "# Changelog\n\n## 0.7.5 - 2026-08-20\n\n- Fixed an exactness bug.\n",
  );

  assert.equal(
    validateReleaseMetadata(root, { tag: "v0.7.5" }).version,
    "0.7.5",
  );
  writeFileSync(
    join(root, "apps/clearra-desktop/src-tauri/tauri.conf.json"),
    JSON.stringify({ version: "0.7.4" }),
  );
  assert.throws(
    () => validateReleaseMetadata(root, { tag: "v0.7.5" }),
    /release version surfaces differ/,
  );

  writeFileSync(
    join(root, "apps/clearra-desktop/src-tauri/tauri.conf.json"),
    JSON.stringify({ version: "0.7.5" }),
  );
  writeFileSync(
    join(root, "Cargo.lock"),
    'version = 4\n\n[[package]]\nname = "clearra-cli"\nversion = "0.7.4"\n',
  );
  assert.throws(
    () => validateReleaseMetadata(root, { tag: "v0.7.5" }),
    /Cargo\.lock Clearra package versions differ/,
  );

  writeFileSync(
    join(root, "Cargo.lock"),
    'version = 4\n\n[[package]]\nname = "clearra-cli"\nversion = "0.7.5"\n',
  );
  writeFileSync(
    join(root, "apps/clearra-desktop/src-tauri/Cargo.lock"),
    'version = 4\n\n[[package]]\nname = "clearra-desktop"\nversion = "0.7.4"\n',
  );
  assert.throws(
    () => validateReleaseMetadata(root, { tag: "v0.7.5" }),
    /src-tauri\/Cargo\.lock Clearra package versions differ/,
  );
});
