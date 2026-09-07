import test from 'node:test';
import assert from 'node:assert/strict';
import { DISCORD_APPROVAL_ENVIRONMENTS } from './discord-approval-transition-plan.mjs';
import { migrateDiscordApprovals } from './discord-approval-transition.mjs';

function fake() {
  const state = new Map(DISCORD_APPROVAL_ENVIRONMENTS.map(name => [name, {
    name, can_admins_bypass:false,
    deployment_branch_policy:{ custom_branch_policies:true, protected_branches:false },
    protection_rules:[{ type:'required_reviewers', prevent_self_review:true,
      reviewers:[{ type:'User', reviewer:{ id:123 } }] }],
  }]));
  const writes = [];
  const api = async (method, path, body) => {
    const [name, sub] = path.split('/');
    if (sub) return { total_count:1, branch_policies:[{ id:17, name:'main', type:'branch' }] };
    if (method === 'PUT') {
      writes.push(name);
      const value = state.get(name);
      value.protection_rules = [
        ...(body.wait_timer ? [{ type:'wait_timer', wait_timer:body.wait_timer }] : []),
        ...(body.reviewers.length ? [{ type:'required_reviewers', prevent_self_review:body.prevent_self_review,
          reviewers:body.reviewers.map(({ type, id }) => ({ type, reviewer:{ id } })) }] : []),
      ];
      value.can_admins_bypass = body.can_admins_bypass;
      value.deployment_branch_policy = body.deployment_branch_policy;
    }
    return structuredClone(state.get(name));
  };
  return { state, writes, api };
}

test('default only reads metadata; reviewed apply changes exactly two scopes, never initial approval', async () => {
  const f = fake();
  const plan = await migrateDiscordApprovals(f.api);
  assert.deepEqual(f.writes, []);
  await assert.rejects(migrateDiscordApprovals(f.api, { apply:true, expectedPlanSha256:'wrong' }));
  assert.deepEqual(f.writes, []);
  const result = await migrateDiscordApprovals(f.api, { apply:true, expectedPlanSha256:plan.plan_sha256 });
  assert(result.applied);
  assert.deepEqual(f.writes, DISCORD_APPROVAL_ENVIRONMENTS.slice(1));
  assert.equal(f.state.get(DISCORD_APPROVAL_ENVIRONMENTS[0]).protection_rules[0].reviewers.length, 1);
});

test('uncertain successful PUT restores original downstream protection, without approving jobs', async () => {
  const f = fake();
  const plan = await migrateDiscordApprovals(f.api);
  let failed = false;
  const flaky = async (...args) => {
    const value = await f.api(...args);
    if (args[0] === 'PUT' && !failed) { failed = true; throw new Error('connection lost after commit'); }
    return value;
  };
  await assert.rejects(migrateDiscordApprovals(flaky, { apply:true, expectedPlanSha256:plan.plan_sha256 }), /restored=true/);
  assert.equal((await migrateDiscordApprovals(f.api)).plan_sha256, plan.plan_sha256);
  assert(!f.writes.includes(DISCORD_APPROVAL_ENVIRONMENTS[0]));
});

test('incomplete branch pages and policy drift stop before mutation', async () => {
  const f = fake();
  await assert.rejects(migrateDiscordApprovals(async (...args) => {
    const value = await f.api(...args);
    if (args[1].includes('/deployment-branch-policies')) value.total_count = 2;
    return value;
  }), /incomplete/);
  const plan = await migrateDiscordApprovals(f.api);
  f.state.get(DISCORD_APPROVAL_ENVIRONMENTS[1]).can_admins_bypass = true;
  await assert.rejects(migrateDiscordApprovals(f.api, { apply:true, expectedPlanSha256:plan.plan_sha256 }), /digest differs/);
  assert.deepEqual(f.writes, []);
});
