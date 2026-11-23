import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { PassThrough } from 'node:stream';
import test from 'node:test';
import { createManagedSearchOwner } from './managed-search-owner.mjs';

function fixture() {
  const signals = new EventEmitter();
  const children = [];
  const owner = createManagedSearchOwner({
    signalSource: signals,
    spawnProcess: (command, args, options) => {
      assert.deepEqual(options.stdio, ['pipe', 'pipe', 'pipe']);
      assert.equal(options.detached, process.platform !== 'win32');
      const child = new EventEmitter();
      child.stdin = new PassThrough();
      child.kill = child.unref = () => assert.fail('manager must remain alive and referenced');
      children.push(child);
      return child;
    },
  });
  return { signals, children, owner };
}

test('a signal closes the manager lease once and forbids the next admission', () => {
  const { signals, children, owner } = fixture();
  const child = owner.spawn('manager', []);
  assert.equal(child.stdin.writableEnded, false);
  signals.emit('SIGTERM');
  signals.emit('SIGINT');
  assert.equal(owner.cancelledBy, 'SIGTERM');
  assert.equal(child.stdin.writableEnded, true);
  assert.throws(() => owner.spawn('manager', []), { code: 'E_CLEARRA_OWNER_CANCELLED' });
  assert.equal(children.length, 1);
  child.emit('close', 143);
  owner.dispose();
  assert.equal(signals.listenerCount('SIGTERM'), 0);
});

test('cancellation during an awaited preflight cannot admit a manager afterwards', async () => {
  const { signals, children, owner } = fixture();
  let finishPreflight;
  const preflight = new Promise((resolve) => { finishPreflight = resolve; });
  const admission = (async () => {
    owner.assertActive();
    await preflight;
    owner.spawn('manager', []);
  })();
  signals.emit('SIGINT');
  finishPreflight();
  await assert.rejects(admission, { code: 'E_CLEARRA_OWNER_CANCELLED' });
  assert.equal(children.length, 0);
  owner.dispose();
});

test('completed managers leave the owner reusable, disposal drains an active lease', () => {
  const { signals, owner } = fixture();
  const first = owner.spawn('manager', []);
  first.emit('close', 0);
  const second = owner.spawn('manager', []);
  owner.dispose();
  owner.dispose();
  assert.equal(second.stdin.writableEnded, true);
  assert.equal(first.stdin.writableEnded, false);
  assert.equal(signals.listenerCount('SIGINT'), 0);
  assert.throws(() => owner.spawn('manager', []), { code: 'E_CLEARRA_OWNER_CANCELLED' });
});
