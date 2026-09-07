import { pathToFileURL } from 'node:url';

export const QUALIFICATION_JOB_FLAGS = Object.freeze([
  ['pages', 'PAGES'], ['desktop_gui', 'GUI'], ['cli', 'CLI'],
  ['discord_gateway', 'DISCORD'], ['heavy_cloud_runtime', 'CLOUD'],
  ['pc4_lookup_service', 'PC4_LOOKUP'], ['pc4_activation_manifest', 'PC4_MANIFEST'],
].map(Object.freeze));

// Diagnostic collection only. The caller must stop before artifact downloads
// and ledger sealing when any requirement fails; this is not release evidence.
export function collectQualificationJobTopology(environment) {
  const failures = [];
  const jobs = [];
  if (!['none', 'focused'].includes(environment.GATE_MODE)) failures.push('gate mode must be none or focused');
  if (environment.CARRY_RESULT !== 'success') failures.push('carry-forward did not pass');
  for (const [label, prefix] of QUALIFICATION_JOB_FLAGS) {
    const selected = environment[`${prefix}_SELECTED`];
    const result = environment[`${prefix}_RESULT`];
    const validSelection = selected === 'true' || selected === 'false';
    const validResult = ['success', 'failure', 'cancelled', 'skipped'].includes(result);
    const expected = selected === 'true' ? 'success' : 'skipped';
    if (!validSelection) failures.push(`${label} selection is invalid`);
    if (!validResult) failures.push(`${label} result is missing or nonterminal`);
    if (validSelection && validResult && result !== expected) {
      failures.push(`${label} expected ${expected} but was ${result}`);
    }
    if (environment.GATE_MODE === 'none' && selected === 'true') failures.push(`${label} selected in no-product mode`);
    jobs.push({ name: label, selected: validSelection ? selected : 'invalid', result: validResult ? result : 'invalid' });
  }
  return { release_authority: false, status: failures.length === 0 ? 'matched' : 'failed', jobs, failures };
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const report = collectQualificationJobTopology(process.env);
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  if (report.failures.length > 0) {
    process.stderr.write(`Qualification job topology failed (${report.failures.length}):\n${report.failures.join('\n')}\n`);
    process.exitCode = 2;
  }
}
