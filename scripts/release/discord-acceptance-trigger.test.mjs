import assert from 'node:assert/strict';
import test from 'node:test';
import { resolveDiscordAcceptanceTrigger } from './discord-acceptance-trigger.mjs';

const sha = '7'.repeat(40);
function run(overrides = {}) {
  return { id: 123, run_attempt: 1, event: 'workflow_dispatch', status: 'completed',
    conclusion: 'success', head_branch: 'main', head_sha: sha,
    head_repository: { full_name: 'daejunnom/Clearra' },
    path: '.github/workflows/release-cli.yml', ...overrides };
}
function context(overrides = {}) {
  return { repository: 'daejunnom/Clearra', ref: 'refs/heads/main', currentMain: sha,
    eventName: 'workflow_run', event: { action: 'completed', repository: { full_name: 'daejunnom/Clearra' }, workflow_run: run() }, ...overrides };
}
function api(runs = [run()]) {
  const calls = [];
  return { calls, run(command, args) {
    calls.push({ command, args });
    return JSON.stringify({ total_count: runs.length, workflow_runs: runs });
  } };
}

test('Release completion resolves one independently verified exact-SHA acceptance without dispatching a second workflow', async () => {
  const provider = api();
  assert.deepEqual(await resolveDiscordAcceptanceTrigger(context(), provider), {
    sourceCommit: sha, acceptedRunId: '123', acceptedRunAttempt: '1',
  });
  assert.equal(provider.calls.length, 1);
  assert.equal(provider.calls[0].command, 'gh');
  assert.ok(provider.calls[0].args.includes(`head_sha=${sha}`));
  assert.ok(provider.calls[0].args.includes('event=workflow_dispatch'));
  assert.ok(provider.calls[0].args.includes('GET'));
  assert.ok(!provider.calls[0].args.includes('POST'));
});

for (const [name, mutate] of [
  ['failed gate', c => { c.event.workflow_run.conclusion = 'failure'; }],
  ['cancelled gate', c => { c.event.workflow_run.conclusion = 'cancelled'; }],
  ['running gate', c => { c.event.workflow_run.status = 'in_progress'; }],
  ['wrong workflow with the same display name', c => { c.event.workflow_run.path = '.github/workflows/clearra-surface-repair.yml'; }],
  ['foreign repository', c => { c.event.workflow_run.head_repository.full_name = 'other/fork'; }],
  ['foreign event owner', c => { c.event.repository.full_name = 'other/fork'; }],
  ['unpromoted candidate', c => { c.event.workflow_run.head_branch = 'candidate'; }],
  ['tag run', c => { c.event.workflow_run.event = 'push'; }],
  ['pull-request event', c => { c.eventName = 'pull_request'; }],
  ['moved main', c => { c.currentMain = '8'.repeat(40); }],
  ['noncompletion action', c => { c.event.action = 'requested'; }],
  ['rerun authority', c => { c.event.workflow_run.run_attempt = 2; }],
  ['imprecise run ID', c => { c.event.workflow_run.id = Number.MAX_SAFE_INTEGER + 1; }],
  ['nonmain receiver', c => { c.ref = 'refs/heads/candidate'; }],
]) {
  test(`${name} cannot reach acceptance lookup or produce deployment authority`, async () => {
    const input = context(); mutate(input); const provider = api();
    await assert.rejects(resolveDiscordAcceptanceTrigger(input, provider));
    assert.equal(provider.calls.length, 0);
  });
}

for (const [name, runs] of [
  ['no accepted run', []],
  ['different run than triggering event', [run({ id: 124 })]],
  ['unverified success', [run({ conclusion: 'failure' })]],
  ['duplicate canonical successes', [run(), run({ id: 124 })]],
]) {
  test(`${name} cannot authorize automatic deployment`, async () => {
    await assert.rejects(resolveDiscordAcceptanceTrigger(context(), api(runs)));
  });
}

test('manual recovery can bind the same exact run and attempt or use the existing unique-acceptance lookup', async () => {
  for (const inputs of [
    { accepted_sha: sha },
    { accepted_sha: sha, accepted_run_id: '123', accepted_run_attempt: '1' },
  ]) {
    const input = context({ eventName: 'workflow_dispatch' }); input.event = { repository: input.event.repository, inputs };
    assert.equal((await resolveDiscordAcceptanceTrigger(input, api())).acceptedRunId, '123');
  }
  for (const inputs of [
    { accepted_sha: sha, accepted_run_id: '123' },
    { accepted_sha: sha, accepted_run_attempt: '1' },
    { accepted_sha: sha, accepted_run_id: '124', accepted_run_attempt: '1' },
  ]) {
    const input = context({ eventName: 'workflow_dispatch' }); input.event = { repository: input.event.repository, inputs };
    await assert.rejects(resolveDiscordAcceptanceTrigger(input, api()));
  }
});
