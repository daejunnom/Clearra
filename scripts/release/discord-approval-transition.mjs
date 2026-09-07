// Branch-only, explicit policy migration. Default is read-only. This never
// accesses secrets, grants deployment authority, or approves a waiting job.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { pathToFileURL } from 'node:url';
import { DISCORD_APPROVAL_ENVIRONMENTS, planDiscordApprovalTransition } from './discord-approval-transition-plan.mjs';

function bodyOf(snapshot) {
  const review = snapshot.protection_rules.find(rule => rule.type === 'required_reviewers');
  return {
    wait_timer:snapshot.protection_rules.find(rule => rule.type === 'wait_timer')?.wait_timer ?? 0,
    prevent_self_review:review?.prevent_self_review ?? false,
    reviewers:(review?.reviewers ?? []).map(entry => ({ type:entry.type, id:entry.reviewer.id })),
    can_admins_bypass:snapshot.can_admins_bypass,
    deployment_branch_policy:snapshot.deployment_branch_policy,
  };
}

export async function readApprovalSnapshots(api) {
  const snapshots = [];
  for (const name of DISCORD_APPROVAL_ENVIRONMENTS) {
    const environment = await api('GET', name);
    const branches = await api('GET', `${name}/deployment-branch-policies?per_page=100`);
    assert.equal(branches.total_count, branches.branch_policies?.length, 'incomplete branch-policy page');
    snapshots.push({ ...environment, branch_policies:branches.branch_policies });
  }
  return snapshots;
}

export async function migrateDiscordApprovals(api, { apply = false, expectedPlanSha256 } = {}) {
  const before = await readApprovalSnapshots(api);
  const plan = planDiscordApprovalTransition(before, 'one-approval');
  const digest = createHash('sha256').update(JSON.stringify(plan.transitions)).digest('hex');
  if (!apply) return { ...plan, plan_sha256:digest };
  assert.equal(expectedPlanSha256, digest, 'reviewed plan digest differs; no writes performed');

  // Initial approval is NEVER a write target. Re-read it before each scoped
  // downstream policy change; keeping the job needs-chain preserves its use.
  const initial = before[0];
  const changed = [];
  try {
    for (const transition of plan.transitions.slice(1)) {
      assert.deepEqual(bodyOf(await api('GET', initial.name)), bodyOf(initial), 'initial approval drift');
      const original = before.find(item => item.name === transition.environment);
      assert.deepEqual(bodyOf(await api('GET', original.name)), bodyOf(original), 'downstream policy drift');
      // Record before the request: a network error may follow a successful PUT.
      changed.push(original);
      await api('PUT', original.name, transition.proposed_body);
      assert.deepEqual(bodyOf(await api('GET', original.name)), transition.proposed_body, 'policy readback mismatch');
    }
    const after = await readApprovalSnapshots(api);
    for (let i = 0; i < after.length; i += 1) {
      assert.deepEqual(bodyOf(after[i]), plan.transitions[i].proposed_body, 'final policy readback mismatch');
      assert.deepEqual(after[i].branch_policies, before[i].branch_policies, 'branch-rule drift');
    }
    return { ...plan, applied:true, ready_to_apply:true, plan_sha256:digest };
  } catch (error) {
    const restorationFailures = [];
    for (const original of changed.reverse()) {
      try {
        const current = bodyOf(await api('GET', original.name));
        const proposed = plan.transitions.find(item => item.environment === original.name).proposed_body;
        if (JSON.stringify(current) === JSON.stringify(bodyOf(original))) continue;
        assert.deepEqual(current, proposed, 'foreign policy edit must not be overwritten during restoration');
        await api('PUT', original.name, bodyOf(original));
        assert.deepEqual(bodyOf(await api('GET', original.name)), bodyOf(original));
      } catch { restorationFailures.push(original.name); }
    }
    throw new Error(`approval migration failed; original reviewers restored=${restorationFailures.length === 0}; unresolved=${restorationFailures.join(',') || 'none'}`, { cause:error });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try {
    const [mode = 'plan', ...args] = process.argv.slice(2);
    assert(['plan', 'apply'].includes(mode));
    assert(args.length === (mode === 'apply' ? 1 : 0), 'apply requires one reviewed SHA256; plan takes no arguments');
    const api = (method, name, body) => JSON.parse(execFileSync('gh', [
      'api', '--method', method, `repos/daejunnom/Clearra/environments/${name}`,
      ...(body ? ['--input', '-'] : []),
    ], { input:body ? JSON.stringify(body) : undefined, encoding:'utf8', windowsHide:true }));
    process.stdout.write(`${JSON.stringify(await migrateDiscordApprovals(api, {
      apply:mode === 'apply', expectedPlanSha256:args[0],
    }), null, 2)}\n`);
  } catch (error) {
    process.stderr.write(`${error.message}\n`);
    process.exitCode = 1;
  }
}
