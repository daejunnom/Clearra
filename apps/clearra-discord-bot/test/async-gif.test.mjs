import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { EventEmitter } from "node:events";
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

test("expired and cancelled render requests never start a worker", async () => {
  let workers = 0;
  const renderer = new BoundedGifRenderer({ createWorker() { workers++; throw new Error("unexpected worker"); } });
  const abort = new AbortController(); abort.abort();
  await assert.rejects(renderer.render({}, { deadlineUnixMs: Date.now() - 1 }), { name: "DeadlineExceededError" });
  await assert.rejects(renderer.render({}, { signal: abort.signal }), { name: "AbortError" });
  assert.equal(workers, 0);
});

test("a cancelled render retains its worker slot until termination drains", async () => {
  const drained = Promise.withResolvers();
  const workers = [];
  const renderer = new BoundedGifRenderer({ maxPending: 1, createWorker(_document, options) {
    assert.equal(options.signal, undefined);
    assert.equal(options.deadlineUnixMs, undefined);
    const worker = new EventEmitter();
    worker.terminate = () => workers.length === 1 ? drained.promise : Promise.resolve(0);
    workers.push(worker);
    return worker;
  } });
  const abort = new AbortController();
  const first = renderer.render({}, { signal: abort.signal });
  const rejected = assert.rejects(first, { name: "AbortError" });
  const second = renderer.render({});
  abort.abort();
  assert.equal(workers.length, 1);
  drained.resolve(1);
  await rejected;
  assert.equal(workers.length, 2);
  workers[1].emit("message", { ok: true, bytes: new Uint8Array([1, 2]) });
  assert.deepEqual(await second, new Uint8Array([1, 2]));
  renderer.stop();
});

test("queued render deadlines expire without spawning their worker", async () => {
  let worker;
  let count = 0;
  const renderer = new BoundedGifRenderer({ maxPending: 1, createWorker() {
    count++; worker = new EventEmitter(); worker.terminate = async () => 0; return worker;
  } });
  const first = renderer.render({});
  const queued = renderer.render({}, { deadlineUnixMs: Date.now() + 15 });
  const keepAlive = setTimeout(() => {}, 100);
  try {
    await assert.rejects(queued, { name: "DeadlineExceededError" });
    assert.equal(count, 1);
    worker.emit("message", { ok: true, bytes: new Uint8Array([1]) });
    await first;
  } finally { clearTimeout(keepAlive); renderer.stop(); }
});
