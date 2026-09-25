import { spawnSync } from 'node:child_process';
import { appendFile, readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { resolveCanonicalAcceptanceRun } from './canonical-acceptance-run.mjs';

const RELEASE_WORKFLOW = '.github/workflows/release-cli.yml';
const SHA = /^[0-9a-f]{40}$/u;
const ID = /^[1-9][0-9]*$/u;

function decimalId(value, label) {
  if (typeof value === 'number' && !Number.isSafeInteger(value)) throw new Error(`${label} is not exact`);
  const text = String(value ?? '');
  if (!ID.test(text)) throw new Error(`${label} must be a positive decimal ID`);
  return text;
}

// The completion event is a routing input, not acceptance proof. Resolve the
// exact successful canonical run independently before emitting any authority.
export async function resolveDiscordAcceptanceTrigger(context, dependencies = {}) {
  const { repository, ref, eventName, event, currentMain } = context;
  if (repository !== 'daejunnom/Clearra' || ref !== 'refs/heads/main' ||
      event?.repository?.full_name !== repository || !SHA.test(currentMain ?? '')) {
    throw new Error('Discord trigger must belong to the protected repository and current main');
  }
  let sourceCommit, expectedRunId, expectedRunAttempt;
  if (eventName === 'workflow_run') {
    const upstream = event.workflow_run;
    if (event.action !== 'completed' || upstream?.path !== RELEASE_WORKFLOW ||
        upstream?.event !== 'workflow_dispatch' || upstream?.head_branch !== 'main' ||
        upstream?.head_repository?.full_name !== repository ||
        upstream?.status !== 'completed' || upstream?.conclusion !== 'success') {
      throw new Error('Discord automatic deployment requires a successful canonical Release completion');
    }
    sourceCommit = upstream.head_sha;
    expectedRunId = decimalId(upstream.id, 'upstream run');
    expectedRunAttempt = decimalId(upstream.run_attempt, 'upstream attempt');
  } else if (eventName === 'workflow_dispatch') {
    sourceCommit = event.inputs?.accepted_sha;
    const run = event.inputs?.accepted_run_id || '';
    const attempt = event.inputs?.accepted_run_attempt || '';
    if (Boolean(run) !== Boolean(attempt)) throw new Error('Manual acceptance run and attempt must be supplied together');
    if (run) {
      expectedRunId = decimalId(run, 'manual acceptance run');
      expectedRunAttempt = decimalId(attempt, 'manual acceptance attempt');
    }
  } else {
    throw new Error('Unsupported Discord deployment trigger');
  }
  if (!SHA.test(sourceCommit ?? '') || sourceCommit !== currentMain) {
    throw new Error('Discord source must be exact current main');
  }
  if (expectedRunAttempt && expectedRunAttempt !== '1') {
    throw new Error('Canonical Release reruns cannot authorize Discord deployment');
  }
  const acceptance = await resolveCanonicalAcceptanceRun({
    repository, sourceCommit, expectedCount: 1, expectedRunId, expectedRunAttempt,
  }, dependencies);
  return Object.freeze({ sourceCommit, acceptedRunId: acceptance.id, acceptedRunAttempt: acceptance.attempt });
}

function git(...args) {
  const result = spawnSync('git', args, {
    encoding: 'utf8', shell: false, timeout: 120_000, maxBuffer: 1024 * 1024,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  if (result.error || result.status !== 0) throw new Error('Discord source authority Git command failed');
  return result.stdout.trim();
}

export async function main(environment = process.env) {
  if (!environment.GITHUB_EVENT_PATH || !environment.GITHUB_OUTPUT) throw new Error('GitHub event and output paths are required');
  const event = JSON.parse(await readFile(environment.GITHUB_EVENT_PATH, 'utf8'));
  git('fetch', '--tags', 'origin', 'main');
  const currentMain = git('rev-parse', 'origin/main');
  if (git('rev-parse', 'HEAD') !== currentMain) throw new Error('Main moved after Discord workflow checkout');
  const result = await resolveDiscordAcceptanceTrigger({
    repository: environment.GITHUB_REPOSITORY, ref: environment.GITHUB_REF,
    eventName: environment.GITHUB_EVENT_NAME, event,
    currentMain,
  });
  git('checkout', '--detach', result.sourceCommit);
  const remote = git('ls-remote', 'origin', 'refs/heads/main').split(/\s+/u);
  if (remote.length !== 2 || remote[0] !== result.sourceCommit || remote[1] !== 'refs/heads/main') {
    throw new Error('Main moved during Discord acceptance resolution');
  }
  await appendFile(environment.GITHUB_OUTPUT, [
    'canonical_acceptance_count=1', `source_commit=${result.sourceCommit}`,
    `accepted_run_id=${result.acceptedRunId}`, `accepted_run_attempt=${result.acceptedRunAttempt}`, '',
  ].join('\n'));
  console.log(`discord_acceptance=verified source=${result.sourceCommit} run=${result.acceptedRunId} attempt=${result.acceptedRunAttempt}`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main().catch(error => { console.error(error.message); process.exitCode = 2; });
}
