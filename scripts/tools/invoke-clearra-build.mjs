import { spawn } from 'node:child_process';
import { resolve, basename } from 'node:path';
import { setTimeout as wait } from 'node:timers/promises';
import { acquireBuildOwner } from './clearra-build-owner.mjs';

let sourceRoot = process.cwd();
let purpose = process.env.CLEARRA_BUILD_PURPOSE || 'experiment';
const input = process.argv.slice(2);
let separator = input.indexOf('--');
if (separator < 0) throw new Error('Expected -- followed by the build command');
for (let index = 0; index < separator; index += 2) {
  if (input[index] === '--source-root') sourceRoot = resolve(input[index + 1]);
  else if (input[index] === '--purpose') purpose = input[index + 1];
  else throw new Error(`Unknown build option: ${input[index]}`);
}
const [command, ...arguments_] = input.slice(separator + 1);
if (!command) throw new Error('Build command is missing');
const cargoArguments = arguments_.slice(0, arguments_.includes('--') ? arguments_.indexOf('--') : arguments_.length);
if (/^cargo(?:\.exe)?$/iu.test(basename(command)) && cargoArguments.some(value => /^(?:--target-dir|--build-dir|--artifact-dir|--out-dir|--config)(?:=|$)/u.test(value))) throw new Error('Cargo output/config overrides are forbidden');
const owner = await acquireBuildOwner({ sourceRoot, purpose });
// Do not escape the Rust supervisor's process group or create a new group
// for a nested transaction. Node has no Windows Job Object reservation here.
const ownsProcessGroup = process.platform !== 'win32' &&
  !process.env.CLEARRA_RUNTIME_SUPERVISED && !process.env.CLEARRA_BUILD_SESSION_ID;
let success = false;
let drainUncertain = false;
let capacityError;
try {
  const child = spawn(command, arguments_, { cwd: sourceRoot, env: owner.environment,
    stdio: 'inherit', windowsHide: true, detached: ownsProcessGroup });
  let childActive = true;
  const sendGroup = signal => {
    if (!Number.isSafeInteger(child.pid) || child.pid < 1) return false;
    try { process.kill(-child.pid, signal); return true; }
    catch (error) {
      if (error.code === 'ESRCH') return false;
      if (signal === 0 && error.code === 'EPERM') return true;
      throw error;
    }
  };
  const forward = signal => {
    if (!childActive) return;
    if (ownsProcessGroup) sendGroup(signal);
    else child.kill(signal);
  };
  const interrupt = () => forward('SIGINT');
  const terminate = () => forward('SIGTERM');
  process.on('SIGINT', interrupt);
  process.on('SIGTERM', terminate);
  const stopMonitoring = owner.monitor(error => {
    capacityError = error;
    process.stderr.write(`${error.message}\n`);
    if (childActive && ownsProcessGroup && child.exitCode === null && child.signalCode === null) {
      try { sendGroup('SIGKILL'); }
      catch { drainUncertain = true; }
    } else if (childActive) {
      // A direct-child signal is not proof of descendant termination. Keep
      // the storage lease and files until a reviewed, supervised drain.
      drainUncertain = true;
    }
  });
  try {
    const code = await new Promise((accept, reject) => {
      child.once('error', error => { childActive = false; reject(error); });
      child.once('exit', code => { childActive = false; accept(code ?? 1); });
    });
    if (ownsProcessGroup) {
      const deadline = performance.now() + 3_000;
      while (sendGroup(0) && performance.now() < deadline) await wait(50);
      if (sendGroup(0)) drainUncertain = true;
    }
    await stopMonitoring();
    if (drainUncertain) throw new Error('Clearra build process drain is not proven; storage lease and output preserved');
    if (capacityError) throw capacityError;
    success = code === 0;
    process.exitCode = code;
  } finally {
    await stopMonitoring();
    process.off('SIGINT', interrupt);
    process.off('SIGTERM', terminate);
  }
} finally {
  if (!drainUncertain) await owner.finish(success, { preserveOutputs: Boolean(capacityError) });
}
