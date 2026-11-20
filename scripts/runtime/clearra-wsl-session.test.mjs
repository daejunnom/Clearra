import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { createInterface } from 'node:readline';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const root = fileURLToPath(new URL('../../', import.meta.url));
// System32/bash.exe launches WSL. For this test on Windows, only use Git Bash.
const bash = process.platform === 'win32'
  ? path.join(process.env.ProgramFiles ?? 'C:/Program Files', 'Git', 'bin', 'bash.exe')
  : '/bin/bash';

test('the guest lease watcher receives host input before treating EOF as parent loss', {
  timeout: 5_000,
  skip: !existsSync(bash) && 'Requires a local Bash (Git Bash on Windows), never WSL',
}, async (t) => {
  const script = `
set -euo pipefail
source scripts/runtime/clearra-wsl-session.sh
lease_watch() {
  while IFS= read -r line; do
    printf 'watcher-read:%s\\n' "$line"
  done
  printf 'watcher-eof\\n'
}
start_lease_watch
wait "$!"
`;
  const child = spawn(bash, ['--noprofile', '--norc', '-c', script], {
    cwd: root,
    stdio: ['pipe', 'pipe', 'pipe'],
    windowsHide: true,
  });
  let stderr = '';
  child.stderr.setEncoding('utf8').on('data', (value) => { stderr += value; });
  const done = new Promise((resolve) => {
    child.once('error', (error) => resolve(error));
    child.once('close', (code, signal) => resolve({ code, signal }));
  });
  t.after(() => {
    child.stdin.destroy();
    if (child.exitCode === null && child.signalCode === null) child.kill();
  });
  const lines = createInterface({ input: child.stdout, crlfDelay: Infinity });
  const messages = lines[Symbol.asyncIterator]();
  // Leave the pipe open until the background watcher proves it read the line.
  const inputError = new Promise((resolve) => child.stdin.once('error', resolve));
  child.stdin.write('host-still-live\n');
  assert.deepEqual(await Promise.race([messages.next(), inputError]),
    { value: 'watcher-read:host-still-live', done: false },
    'an open host pipe must not appear as EOF to the background watcher');
  child.stdin.end();
  assert.deepEqual(await messages.next(), { value: 'watcher-eof', done: false });
  assert.deepEqual(await done, { code: 0, signal: null }, stderr);
});
