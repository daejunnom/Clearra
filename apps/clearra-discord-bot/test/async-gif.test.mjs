import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import test from "node:test";

import { BoundedGifRenderer } from "../src/viewer/async-gif.mjs";

test("GIF worker does not inherit incompatible parent execution flags", () => {
  const moduleUrl = new URL("../src/viewer/async-gif.mjs", import.meta.url).href;
  const script = `
    import { BoundedGifRenderer } from ${JSON.stringify(moduleUrl)};
    const renderer = new BoundedGifRenderer({ timeoutMs: 5_000, maxPending: 0 });
    try {
      const bytes = await renderer.render({
        width: 10,
        pages: [{ height: 0, cells: [] }],
      });
      console.log(new TextDecoder().decode(bytes.subarray(0, 6)));
    } finally {
      renderer.stop();
    }
  `;
  const result = spawnSync(process.execPath, ["--input-type=module"], {
    input: script,
    encoding: "utf8",
    timeout: 10_000,
  });

  assert.equal(result.status, 0, result.stderr);
  assert.equal(result.stdout.trim(), "GIF89a");
});

test("GIF worker rejects zero frames and remains usable for one frame", async () => {
  const renderer = new BoundedGifRenderer({ timeoutMs: 5_000, maxPending: 0 });
  try {
    await assert.rejects(
      renderer.render({ width: 10, pages: [] }),
      /viewer document is invalid/,
    );
    const bytes = await renderer.render({
      width: 10,
      pages: [{ height: 0, cells: [] }],
    });
    assert.equal(new TextDecoder().decode(bytes.subarray(0, 6)), "GIF89a");
    assert.equal(bytes.at(-1), 0x3b);
  } finally {
    renderer.stop();
  }
});
