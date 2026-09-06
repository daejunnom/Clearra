import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import test from 'node:test';
import { DEFAULT_LEASE_MINUTES, EXPERIMENT_PORT, experimentOptions,
  startExperiment, watchOwnedChild, serveOwnedExperiment } from './run-gui-experiment.mjs';

test('experiments bind only 4195 with a finite configurable lease', () => {
  assert.equal(EXPERIMENT_PORT, 4195);
  assert.equal(DEFAULT_LEASE_MINUTES, 30);
  assert.equal(experimentOptions([]).leaseMs, 1_800_000);
  for (const value of ['0', '-1', '121', '1.5', 'NaN', 'Infinity'])
    assert.throws(() => experimentOptions(['--lease-minutes', value]));
  assert.throws(() => experimentOptions(['--port', '4196']));
});

test('an occupied port never starts, adopts or stops another process', async () => {
  let forks = 0;
  await assert.rejects(startExperiment(experimentOptions([]), {
    checkPort: async () => { throw new Error('occupied'); },
    forkProcess: () => { forks++; },
  }), /occupied/);
  assert.equal(forks, 0);
});

test('owned server is hidden, IPC-connected, shell-free and cannot fall back to another port', async () => {
  const child = {};
  await startExperiment(experimentOptions([]), {
    checkPort: async () => {},
    forkProcess: (file, args, options) => {
      assert(file.endsWith('run-gui-experiment.mjs'));
      assert.equal(args[0], '--owned-server');
      assert.equal(options.windowsHide, true);
      assert.equal(options.detached, false);
      assert.equal(options.stdio[3], 'ipc');
      assert.equal(options.shell, undefined);
      return child;
    },
    watch: async value => { assert.equal(value, child); },
  });
});

test('parent signals and lease expiration clean only the owned child and never restart it', async () => {
  for (const reason of ['SIGINT', 'SIGTERM', 'exit', 'lease']) {
    const child = new EventEmitter();
    const parent = new EventEmitter();
    const timers = [];
    let disconnects = 0;
    let kills = 0;
    child.connected = true;
    child.disconnect = () => { disconnects++; child.connected = false; };
    child.kill = () => { kills++; };
    const done = watchOwnedChild(child, { leaseMs:60_000, processEvents:parent,
      setTimer: (fn, delay) => { const timer = { fn, delay }; timers.push(timer); return timer; },
      clearTimer: timer => { timer.cleared = true; },
    });
    if (reason === 'lease') timers[0].fn(); else parent.emit(reason);
    parent.emit('SIGINT');
    assert.equal(disconnects, 1);
    assert.equal(timers[1].delay, 5_000);
    timers[1].fn();
    assert.equal(kills, 1);
    child.emit('exit', 0, null);
    await done;
    assert(timers.every(timer => timer.cleared));
    assert.equal(parent.listenerCount('SIGINT'), 0);
  }
});

test('early child failure is reported, not silently restarted', async () => {
  const child = new EventEmitter();
  const parent = new EventEmitter();
  child.connected = true;
  const done = watchOwnedChild(child, { leaseMs:60_000, processEvents:parent });
  child.emit('exit', 9, null);
  await assert.rejects(done, /owned experiment exited: 9/);
  assert.equal(parent.listenerCount('exit'), 0);
});

test('server uses strict 4195 and shuts down when its parent disappears', async () => {
  const events = new EventEmitter();
  events.connected = true;
  const timers = [];
  const exits = [];
  let closed = 0;
  const handle = await serveOwnedExperiment(experimentOptions([]), {
    events, setTimer: fn => { timers.push(fn); return fn; }, clearTimer: () => {},
    exit: code => exits.push(code),
    loadVite: async () => ({ createServer: async options => {
      assert.equal(options.mode, 'local-audit');
      assert.deepEqual(options.server, { host:'127.0.0.1', port:4195, strictPort:true, hmr:false });
      return { listen:async () => {}, close:async () => { closed++; } };
    } }),
  });
  events.emit('disconnect');
  await Promise.resolve();
  await handle.stop();
  assert.equal(closed, 1);
  assert.deepEqual(exits, [0]);
  handle.clearLease();
});

test('private child entry cannot run as a detached permanent server', async () => {
  await assert.rejects(serveOwnedExperiment(experimentOptions([]), { events:{ connected:false } }), /parent IPC/);
});

test('startup race or Vite failure closes its server and cannot report success', async () => {
  const events = new EventEmitter();
  events.connected = true;
  const exits = [];
  let closed = 0;
  await assert.rejects(serveOwnedExperiment(experimentOptions([]), {
    events, setTimer: fn => fn, clearTimer: () => {}, exit: code => exits.push(code),
    loadVite: async () => ({ createServer: async () => ({
      listen: async () => { throw new Error('4195 was occupied after preflight'); },
      close: async () => { closed++; },
    }) }),
  }), /occupied/);
  assert.equal(closed, 1);
  assert.deepEqual(exits, [1]);
});
