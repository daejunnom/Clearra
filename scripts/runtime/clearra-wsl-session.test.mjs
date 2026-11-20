import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
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

test('the guest unit receives the observed distribution and verified host catalog anchor', {
  timeout: 5_000,
  skip: !existsSync(bash) && 'Requires a local Bash, never WSL',
}, () => {
  const script = String.raw`
set -euo pipefail
source scripts/runtime/clearra-wsl-session.sh
environment=()
append_wsl_build_environment environment 'Linux 작업' 'D:\Hosted Workspace\Temp'
if append_wsl_build_environment environment 'Linux 작업' 'D:relative'; then exit 23; fi
if append_wsl_build_environment environment $'Linux\nOTHER=1' 'D:\Temp'; then exit 24; fi
printf '%s\n' "${'$'}{environment[@]}"
`;
  const child = spawnSync(bash, ['--noprofile', '--norc', '-c', script], {
    cwd: root, encoding: 'utf8', timeout: 4_000, windowsHide: true,
  });
  assert.equal(child.status, 0, child.stderr || child.error?.message);
  assert.deepEqual(child.stdout.trimEnd().split(/\r?\n/u), [
    '--setenv', 'WSL_DISTRO_NAME=Linux 작업',
    '--setenv', 'LOCALAPPDATA=D:\\Hosted Workspace\\Temp',
    '--setenv', 'CLEARRA_WSL_DISTRIBUTION=Linux 작업',
  ]);
});

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

test('drain requires a removed exact cgroup and an observed terminal unit state', {
  timeout: 5_000,
  skip: !existsSync(bash) && 'Requires a local Bash, never WSL',
}, async () => {
  const script = `
set -euo pipefail
source scripts/runtime/clearra-wsl-session.sh
expected=/user.slice/mock.service
absent=/dev/null/clearra-absent
sleep() { :; }
owned_cgroup_drained "$absent" "$expected" "" inactive
owned_cgroup_drained "$absent" "$expected" "$expected" failed
if owned_cgroup_drained /dev/null "$expected" "$expected" failed; then exit 31; fi
if owned_cgroup_drained "$absent" "$expected" /other/user.slice inactive; then exit 32; fi
if owned_cgroup_drained "$absent" "$expected" "" ""; then exit 33; fi
if owned_cgroup_drained "$absent" "$expected" "$expected" deactivating; then exit 34; fi
`;
  const child = spawn(bash, ['--noprofile', '--norc', '-c', script], {
    cwd: root,
    stdio: 'ignore',
    windowsHide: true,
  });
  const done = await new Promise((resolve, reject) => {
    child.once('error', reject);
    child.once('close', (code, signal) => resolve({ code, signal }));
  });
  assert.deepEqual(done, { code: 0, signal: null });
});
