import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { EventEmitter, once } from 'node:events';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';
import vm from 'node:vm';

const helper = fileURLToPath(new URL('./clearra-node-gc.cjs', import.meta.url));
const temporaryRoot = path.resolve('_local/tmp/management-node-gc');
const protocol = 'clearra.memory-pressure.v1';

async function waitForAcknowledgement(file, requestId) {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    try {
      const value = JSON.parse(fs.readFileSync(file, 'utf8'));
      if (value.request_id === requestId) return value;
    } catch { /* The root has not completed the requested collection yet. */ }
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
  throw new Error(`GC acknowledgement did not arrive for ${requestId}`);
}

function requestCollection(file, requestId, pid) {
  const temporary = `${file}.tmp`;
  fs.writeFileSync(temporary, JSON.stringify({
    schema_id: protocol,
    request_id: requestId,
    action: 'full-gc',
    child_pid: pid,
  }));
  fs.renameSync(temporary, file);
}

test('only the supervised Node root acknowledges a completed GC', async () => {
  fs.mkdirSync(temporaryRoot, { recursive: true });
  const directory = fs.mkdtempSync(path.join(temporaryRoot, 'run-'));
  const request = path.join(directory, 'gc.request');
  const acknowledgement = path.join(directory, 'gc.ack');
  const child = spawn(process.execPath, [
    '--expose-gc', '--require', helper, '-e', 'setInterval(() => {}, 1000)',
  ], {
    env: {
      ...process.env,
      NODE_OPTIONS: '',
      CLEARRA_RUNTIME_GC_PROTOCOL: protocol,
      CLEARRA_RUNTIME_GC_REQUEST_PATH: request,
      CLEARRA_RUNTIME_GC_ACK_PATH: acknowledgement,
    },
    stdio: 'ignore',
  });
  const exited = once(child, 'exit');
  try {
    assert.ok(child.pid);
    // The watcher must reject a request addressed to a different process.
    requestCollection(request, 'wrong-pid', child.pid + 1);
    await new Promise((resolve) => setTimeout(resolve, 150));
    assert.equal(fs.existsSync(acknowledgement), false);

    requestCollection(request, 'first', child.pid);
    const first = await waitForAcknowledgement(acknowledgement, 'first');
    assert.deepEqual(first, {
      schema_id: protocol,
      request_id: 'first',
      action: 'full-gc',
      status: 'completed',
      child_pid: child.pid,
      scope: 'node-main-isolate',
      thread_id: 0,
    });

    requestCollection(request, 'second', child.pid);
    const second = await waitForAcknowledgement(acknowledgement, 'second');
    assert.equal(second.child_pid, child.pid);
  } finally {
    child.kill();
    await exited;
    fs.rmSync(directory, { recursive: true, force: true });
  }
});

test('a worker isolate sharing the root PID cannot claim root collection', () => {
  let filesystemOperations = 0;
  let collections = 0;
  let timers = 0;
  vm.runInNewContext(fs.readFileSync(helper, 'utf8'), {
    require(name) {
      if (name === 'node:worker_threads') return { isMainThread: false, threadId: 7 };
      if (name === 'node:path') return path;
      if (name === 'node:fs') return new Proxy({}, {
        get() {
          filesystemOperations += 1;
          throw new Error('a worker must not touch the root collection channel');
        },
      });
      throw new Error(`unexpected module: ${name}`);
    },
    process: {
      pid: process.pid,
      env: {
        CLEARRA_RUNTIME_GC_PROTOCOL: protocol,
        CLEARRA_RUNTIME_GC_REQUEST_PATH: 'root.request',
        CLEARRA_RUNTIME_GC_ACK_PATH: 'root.ack',
      },
    },
    global: { gc() { collections += 1; } },
    setInterval() { timers += 1; return { unref() {} }; },
  });
  assert.equal(filesystemOperations, 0);
  assert.equal(collections, 0);
  assert.equal(timers, 0);
});

function verifyPollingAfterWatcherFailure(asynchronousFailure) {
  fs.mkdirSync(temporaryRoot, { recursive: true });
  const directory = fs.mkdtempSync(path.join(temporaryRoot, 'polling-'));
  const request = path.join(directory, 'gc.request');
  const acknowledgement = path.join(directory, 'gc.ack');
  let poll;
  let collections = 0;
  let unreferenced = false;
  let watcherClosed = false;
  let watcherUnreferenced = false;
  const watcher = new EventEmitter();
  watcher.close = () => { watcherClosed = true; };
  watcher.unref = () => { watcherUnreferenced = true; };
  try {
    vm.runInNewContext(fs.readFileSync(helper, 'utf8'), {
      require(name) {
        if (name === 'node:worker_threads') return { isMainThread: true, threadId: 0 };
        if (name === 'node:path') return path;
        if (name === 'node:fs') return {
          ...fs,
          watch() {
            if (!asynchronousFailure) throw new Error('watcher unavailable');
            return watcher;
          },
        };
        throw new Error(`unexpected module: ${name}`);
      },
      process: {
        pid: process.pid,
        env: {
          CLEARRA_RUNTIME_GC_PROTOCOL: protocol,
          CLEARRA_RUNTIME_GC_REQUEST_PATH: request,
          CLEARRA_RUNTIME_GC_ACK_PATH: acknowledgement,
        },
      },
      global: { gc() { collections += 1; } },
      setInterval(callback, interval) {
        assert.equal(interval, 1_000);
        poll = callback;
        return { unref() { unreferenced = true; } };
      },
    });
    assert.equal(unreferenced, true);
    assert.equal(typeof poll, 'function');
    requestCollection(request, 'polled', process.pid);
    poll();
    poll();
    const completed = JSON.parse(fs.readFileSync(acknowledgement, 'utf8'));
    assert.equal(completed.request_id, 'polled');
    assert.equal(completed.scope, 'node-main-isolate');
    assert.equal(completed.thread_id, 0);
    assert.equal(collections, 1);
    if (asynchronousFailure) {
      assert.equal(watcherUnreferenced, true);
      assert.doesNotThrow(() => watcher.emit('error', new Error('watcher stopped')));
      assert.equal(watcherClosed, true);
    }
    requestCollection(request, 'escalated', process.pid);
    poll();
    assert.equal(collections, 2);
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
}

for (const asynchronousFailure of [false, true]) {
  test(`a filesystem watcher failing ${asynchronousFailure ? 'after' : 'during'} creation still polls`, () => {
    verifyPollingAfterWatcherFailure(asynchronousFailure);
  });
}

test('a Node root without exposed GC cannot claim completion', async () => {
  fs.mkdirSync(temporaryRoot, { recursive: true });
  const directory = fs.mkdtempSync(path.join(temporaryRoot, 'unavailable-'));
  const request = path.join(directory, 'gc.request');
  const acknowledgement = path.join(directory, 'gc.ack');
  const child = spawn(process.execPath, [
    '--require', helper, '-e', 'setInterval(() => {}, 1000)',
  ], {
    env: {
      ...process.env,
      NODE_OPTIONS: '',
      CLEARRA_RUNTIME_GC_PROTOCOL: protocol,
      CLEARRA_RUNTIME_GC_REQUEST_PATH: request,
      CLEARRA_RUNTIME_GC_ACK_PATH: acknowledgement,
    },
    stdio: 'ignore',
  });
  const exited = once(child, 'exit');
  try {
    assert.ok(child.pid);
    requestCollection(request, 'unavailable', child.pid);
    await new Promise((resolve) => setTimeout(resolve, 250));
    assert.equal(fs.existsSync(acknowledgement), false);
  } finally {
    child.kill();
    await exited;
    fs.rmSync(directory, { recursive: true, force: true });
  }
});
