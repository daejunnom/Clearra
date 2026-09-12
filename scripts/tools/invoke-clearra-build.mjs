import { spawn } from 'node:child_process';
import { resolve, basename } from 'node:path';
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
let success = false;
try {
  const child = spawn(command, arguments_, { cwd: sourceRoot, env: owner.environment, stdio: 'inherit', windowsHide: true });
  const forward = signal => child.kill(signal);
  const interrupt = () => forward('SIGINT');
  const terminate = () => forward('SIGTERM');
  process.on('SIGINT', interrupt);
  process.on('SIGTERM', terminate);
  try {
    const code = await new Promise((accept, reject) => { child.once('error', reject); child.once('exit', code => accept(code ?? 1)); });
    success = code === 0;
    process.exitCode = code;
  } finally { process.off('SIGINT', interrupt); process.off('SIGTERM', terminate); }
} finally { await owner.finish(success); }
