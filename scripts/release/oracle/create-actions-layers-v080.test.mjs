import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const script = await readFile(
  new URL("./create-actions-layers-v080.sh", import.meta.url),
  "utf8",
);

test("Actions layer freeze consumes accepted CTK3 and only the production dependency", () => {
  assert.match(script, /accepted-ctk3-dist\.mjs/u);
  assert.match(script, /node_modules\/tetris-fumen/u);
  assert.match(script, /ctk3-dist\.tar/u);
  assert.match(script, /node_modules\.tar/u);
  assert.doesNotMatch(script, /src\/admin/u);
  assert.doesNotMatch(script, /private-overlay/u);
});

test("Actions layer freeze is deterministic and refuses overwrite", () => {
  for (const marker of [
    "--format=posix",
    "--sort=name",
    "--mtime=@0",
    "--owner=0",
    "--group=0",
    "--numeric-owner",
    "ln -- \"$temporary_archive\" \"$destination\"",
  ]) {
    assert.ok(script.includes(marker), marker);
  }
  assert.match(script, /! -type f ! -type d/u);
  assert.match(script, /! -L/u);
});
