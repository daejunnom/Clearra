import assert from 'node:assert/strict';
import { closeSync, constants, mkdtempSync, openSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';
import { cargoJobserverStdio } from './cargo-jobserver-stdio.mjs';

const flags = '-j --jobserver-fds=3,5 --jobserver-auth=3,5';
const pipe = () => ({ isFIFO: () => true, dev: 1, ino: 2 });
test('only the two verified Cargo pipe descriptors accompany stdio', () => {
  assert.deepEqual(cargoJobserverStdio({ CARGO_MAKEFLAGS: flags }, 'linux', pipe),
    ['inherit', 'inherit', 'inherit', 3, 'ignore', 5]);
});
test('Windows named semaphore, FIFO names and absent flags retain the normal launch', () => {
  const noInspect = () => { throw new Error('must not inspect'); };
  for (const [platform, value] of [['win32', flags], ['linux', ''], ['linux', '-j --jobserver-auth=fifo:/tmp/jobtokens']]) {
    assert.equal(cargoJobserverStdio({ CARGO_MAKEFLAGS: value }, platform, noInspect), 'inherit');
  }
});
test('invalid, conflicting, missing and non-pipe descriptors fail without forwarding other handles', () => {
  for (const value of ['--jobserver-auth=2,5', '--jobserver-auth=3,3', '--jobserver-auth=3,1000000', '--jobserver-auth=3,5 --jobserver-fds=4,6', '--jobserver-auth=-1,-1', '--jobserver-auth=invalid']) {
    assert.throws(() => cargoJobserverStdio({ CARGO_MAKEFLAGS: value }, 'linux', pipe), /Invalid Cargo/u);
  }
  assert.throws(() => cargoJobserverStdio({ CARGO_MAKEFLAGS: flags }, 'linux', () => { throw new Error('EBADF'); }), /not inherited/u);
  assert.throws(() => cargoJobserverStdio({ CARGO_MAKEFLAGS: flags }, 'linux', () => ({ ...pipe(), isFIFO: () => false })), /same pipe/u);
  assert.throws(() => cargoJobserverStdio({ CARGO_MAKEFLAGS: flags }, 'linux', fd => ({ ...pipe(), ino: fd })), /same pipe/u);
});
test('real POSIX pipe survives the intermediary Node process and returns its token', { skip: process.platform === 'win32' }, () => {
  const directory = mkdtempSync(join(tmpdir(), 'clearra-jobserver-test-'));
  const descriptors = [];
  try {
    const fifo = join(directory, 'tokens');
    const made = spawnSync('mkfifo', [fifo], { encoding: 'utf8' });
    assert.equal(made.status, 0, made.stderr || made.error?.message);
    descriptors.push(openSync(fifo, constants.O_RDWR), openSync(fifo, constants.O_RDWR));
    const consumer = `const fs = require('node:fs');
      if (!fs.fstatSync(3).isFIFO() || !fs.fstatSync(5).isFIFO()) process.exit(8);
      fs.writeSync(5, Buffer.from('x'));
      const token = Buffer.alloc(1); fs.readSync(3, token, 0, 1, null);
      process.stdout.write(token);`;
    const intermediary = `import { spawnSync } from 'node:child_process';
      import { cargoJobserverStdio } from ${JSON.stringify(new URL('./cargo-jobserver-stdio.mjs', import.meta.url).href)};
      const child = spawnSync(process.execPath, ['-e', ${JSON.stringify(consumer)}], {stdio: cargoJobserverStdio()});
      process.exit(child.status ?? 9);`;
    const result = spawnSync(process.execPath, ['--input-type=module', '-e', intermediary], {
      env: { ...process.env, CARGO_MAKEFLAGS: flags }, encoding: 'utf8', timeout: 5000,
      stdio: ['ignore', 'pipe', 'pipe', descriptors[0], 'ignore', descriptors[1]],
    });
    assert.equal(result.status, 0, result.stderr || result.error?.message);
    assert.equal(result.stdout, 'x');
  } finally {
    for (const fd of descriptors) closeSync(fd);
    assert.equal(dirname(resolve(directory)), resolve(tmpdir()));
    assert.ok(basename(directory).startsWith('clearra-jobserver-test-'));
    rmSync(directory, { recursive: true, force: true });
  }
});
