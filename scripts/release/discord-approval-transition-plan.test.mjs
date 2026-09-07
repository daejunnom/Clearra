import test from 'node:test';
import assert from 'node:assert/strict';
import { planDiscordApprovalTransition, DISCORD_APPROVAL_ENVIRONMENTS } from './discord-approval-transition-plan.mjs';

function snapshots() {
  return DISCORD_APPROVAL_ENVIRONMENTS.map(name => ({ name, can_admins_bypass:false,
    deployment_branch_policy:{ custom_branch_policies:true, protected_branches:false },
    branch_policies:[{ name:'main', type:'branch' }],
    protection_rules:[{ type:'branch_policy' }, { type:'required_reviewers', prevent_self_review:true,
      reviewers:[{ type:'User', reviewer:{ id:123 } }] }, { type:'wait_timer', wait_timer:5 }],
  }));
}
test('one explicit initial approval preserves scope and automatically allows bounded recovery', () => {
  const input = snapshots();
  const plan = planDiscordApprovalTransition(input, 'one-approval');
  assert.deepEqual(plan.transitions.map(item => item.required_reviewers_after), [1, 0, 0]);
  assert(plan.transitions.every(item => item.proposed_body.wait_timer === 5 && !item.proposed_body.can_admins_bypass));
  assert.equal(plan.ready_to_apply, false);
  assert.equal(plan.applied, false);
  assert.deepEqual(input, snapshots(), 'planner is non-mutating');
});
test('unattended is forbidden by the selected user contract', () => {
  assert.throws(() => planDiscordApprovalTransition(snapshots(), 'unattended'));
});
test('unknown rules, foreign branches, absent scopes and implicit mode fail closed', () => {
  assert.throws(() => planDiscordApprovalTransition(snapshots()));
  const custom = snapshots(); custom[1].protection_rules.push({ type:'custom' });
  assert.throws(() => planDiscordApprovalTransition(custom, 'one-approval'));
  const branch = snapshots(); branch[0].branch_policies.push({ name:'codex/*', type:'branch' });
  assert.throws(() => planDiscordApprovalTransition(branch, 'one-approval'));
  assert.throws(() => planDiscordApprovalTransition(snapshots().slice(1), 'one-approval'));
});
