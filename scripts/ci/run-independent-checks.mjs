import { spawn } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

// Diagnostic aggregation, not acceptance authority. Failures remain failures;
// independent commands get one attempt, explicit cancellation stops new work.
// Producer prerequisites belong in the workflow's explicit step guards.
export function parseIndependentChecks(args) {
  const checks = [[]];
  for (const argument of args[0] === '--' ? args.slice(1) : args) {
    if (argument === ':::') checks.push([]);
    else checks.at(-1).push(argument);
  }
  if (checks.some(check => check.length === 0)) {
    throw new Error('independent checks require nonempty argv groups separated by :::');
  }
  return checks;
}

export async function runIndependentChecks(checks, { execute = executeCommand, report = console.log } = {}) {
  const outcomes = [];
  for (const [index, argv] of checks.entries()) {
    const name = `check-${index + 1}`;
    report(`::group::${name} ${argv.map(escapeAnnotation).join(' ')}`);
    let result;
    try { result = await execute(argv); }
    catch (error) { result = { code: 1, error: error.message }; }
    const cancelled = result.cancelled || ['SIGINT', 'SIGTERM'].includes(result.signal) || [130, 143].includes(result.code);
    const status = cancelled ? 'cancelled' : result.code === 0 ? 'passed' : 'failed';
    outcomes.push({ name, argv, status, code: result.code ?? null, signal: result.signal ?? null });
    report(`independent_check=${name} status=${status} code=${result.code ?? 'none'}`);
    if (result.error) report(`::error::${name}: ${escapeAnnotation(result.error)}`);
    if (status !== 'passed') report(`::error::${name} ${status}: ${argv.map(escapeAnnotation).join(' ')}`);
    report('::endgroup::');
    if (cancelled) break;
  }
  const failed = outcomes.filter(outcome => outcome.status !== 'passed');
  report(`independent_checks attempted=${outcomes.length} planned=${checks.length} failed=${failed.length} acceptance_authority=false`);
  return { outcomes, exitCode: failed.some(outcome => outcome.status === 'cancelled') ? 130 : failed.length ? 1 : 0 };
}

function escapeAnnotation(value) {
  return String(value).replaceAll('%', '%25').replaceAll('\r', '%0D').replaceAll('\n', '%0A');
}

function executeCommand(argv) {
  return new Promise(resolveResult => {
    const child = spawn(argv[0], argv.slice(1), { stdio: 'inherit', windowsHide: true });
    let cancelled = false;
    const interrupt = signal => { cancelled = true; child.kill(signal); };
    const interruptTerm = () => interrupt('SIGTERM');
    const interruptInt = () => interrupt('SIGINT');
    process.once('SIGTERM', interruptTerm);
    process.once('SIGINT', interruptInt);
    const detach = () => {
      process.off('SIGTERM', interruptTerm);
      process.off('SIGINT', interruptInt);
    };
    child.once('error', error => { detach(); resolveResult({ code: 1, error: error.message, cancelled }); });
    child.once('close', (code, signal) => { detach(); resolveResult({ code, signal, cancelled }); });
  });
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const result = await runIndependentChecks(parseIndependentChecks(process.argv.slice(2)));
  process.exitCode = result.exitCode;
}
