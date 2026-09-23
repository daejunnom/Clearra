import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

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
