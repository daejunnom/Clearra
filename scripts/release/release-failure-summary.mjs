import { appendFileSync } from 'node:fs';
import { pathToFileURL } from 'node:url';

// These are diagnostic dependencies, not a replacement acceptance manifest.
export const DIAGNOSTIC_JOB_DEPENDENCIES = Object.freeze({
  metadata: [],
  ctk3: ['metadata'],
  'linux-cli': ['metadata', 'ctk3'],
  'discord-bot': ['metadata', 'ctk3'],
  'release-acceptance-foundation-no-product-debt': ['metadata'],
  'release-acceptance-foundation-adversarial-correctness': ['metadata'],
  'release-acceptance-foundation-desktop-host': ['metadata'],
  'release-acceptance-sanitizer': ['metadata'],
  'release-acceptance-rust': ['metadata', 'ctk3'],
  'release-acceptance-wasm-build': ['metadata'],
  'release-acceptance-pages': ['metadata', 'ctk3', 'release-acceptance-wasm-build'],
  'release-acceptance': [
    'metadata', 'release-acceptance-foundation-no-product-debt',
    'release-acceptance-foundation-adversarial-correctness',
    'release-acceptance-foundation-desktop-host', 'release-acceptance-sanitizer',
    'release-acceptance-rust', 'release-acceptance-pages',
  ],
  'windows-cli': ['metadata'],
  'windows-gui': ['metadata'],
  'canonical-evidence': [
    'metadata', 'ctk3', 'linux-cli', 'discord-bot', 'release-acceptance',
    'windows-cli', 'windows-gui',
  ],
});

export function collectReleaseFailureSummary(needs) {
  if (!needs || Array.isArray(needs) || typeof needs !== 'object' ||
      Object.keys(needs).sort().join(',') !== Object.keys(DIAGNOSTIC_JOB_DEPENDENCIES).sort().join(',')) {
    throw new Error('Release diagnostics require the exact canonical job set.');
  }
  const terminal = new Set(['success', 'failure', 'cancelled', 'skipped']);
  for (const [name, state] of Object.entries(needs)) {
    if (!terminal.has(state?.result)) throw new Error(`Nonterminal or invalid job result: ${name}`);
  }
  const jobs = Object.entries(DIAGNOSTIC_JOB_DEPENDENCIES).map(([name, dependencies]) => {
    const result = needs[name].result;
    const blockedBy = dependencies.filter((dependency) => needs[dependency].result !== 'success');
    if (result === 'success' && blockedBy.length > 0) {
      throw new Error(`Successful job violated its prerequisite contract: ${name}`);
    }
    return {
      name,
      result,
      diagnostic_status: result === 'skipped' && blockedBy.length > 0 ? 'blocked' : result,
      blocked_by: result === 'skipped' ? blockedBy : [],
    };
  });
  return {
    schema_id: 'clearra.release-failure-summary.v1',
    release_authority: false,
    status: jobs.every((job) => job.result === 'success') ? 'all-jobs-succeeded' : 'failed-or-blocked',
    jobs,
  };
}

export function renderReleaseFailureSummary(report) {
  return [
    '## Canonical release diagnostics', '',
    'Diagnostic only; this summary cannot authorize acceptance or publication.', '',
    '| Job | Result | Blocking prerequisites |', '| --- | --- | --- |',
    ...report.jobs.map((job) => `| ${job.name} | ${job.diagnostic_status} | ${job.blocked_by.join(', ') || '-'} |`),
    '',
  ].join('\n');
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const report = collectReleaseFailureSummary(JSON.parse(process.env.CLEARRA_DIAGNOSTIC_NEEDS ?? 'null'));
    const markdown = renderReleaseFailureSummary(report);
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n${markdown}`);
    if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, markdown);
    if (report.status !== 'all-jobs-succeeded') process.exitCode = 1;
  } catch (error) {
    process.stderr.write(`Release diagnostic summary failed: ${error.message}\n`);
    process.exitCode = 1;
  }
}
