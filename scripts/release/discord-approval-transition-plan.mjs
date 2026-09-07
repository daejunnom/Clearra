// Branch-only migration planner. No network, credentials, workflow approval,
// environment writes or deployment authority. Applying its output is a separate
// reviewed migration, including the protection-contract documentation/tests.
import assert from 'node:assert/strict';

export const DISCORD_APPROVAL_ENVIRONMENTS = Object.freeze([
  'discord-path-confirmation', 'discord-global-command-sync', 'discord-runtime-rollback',
]);

export function planDiscordApprovalTransition(snapshots, mode) {
  assert.equal(mode, 'one-approval', 'only the user-selected initial-approval policy is allowed');
  assert(Array.isArray(snapshots) && snapshots.length === 3, 'three exact scoped environments required');
  assert.equal(new Set(snapshots.map(item => item.name)).size, 3, 'duplicate environment');
  const transitions = DISCORD_APPROVAL_ENVIRONMENTS.map(name => {
    const snapshot = snapshots.find(item => item.name === name);
    assert(snapshot, 'missing scoped environment');
    const rules = snapshot.protection_rules;
    assert(Array.isArray(rules), 'missing protection rules');
    assert(rules.every(rule => ['required_reviewers', 'branch_policy', 'wait_timer'].includes(rule.type)),
      'custom protection rules need independent review, never drop them');
    assert.equal(new Set(rules.map(rule => rule.type)).size, rules.length, 'duplicate protection rule');
    assert.deepEqual(snapshot.deployment_branch_policy, { custom_branch_policies:true, protected_branches:false },
      'preserve the existing explicit deployment-branch policy');
    assert(Array.isArray(snapshot.branch_policies) && snapshot.branch_policies.length === 1 &&
      snapshot.branch_policies[0].name === 'main' && snapshot.branch_policies[0].type === 'branch',
    'only exact main branch may access runtime authority');
    const review = rules.find(rule => rule.type === 'required_reviewers');
    assert(review && Array.isArray(review.reviewers) && review.reviewers.length > 0, 'reviewed initial policy required');
    const retained = name === 'discord-path-confirmation';
    const reviewers = review.reviewers.map(entry => {
      assert(['User', 'Team'].includes(entry.type) && Number.isSafeInteger(entry.reviewer?.id) && entry.reviewer.id > 0);
      return { type:entry.type, id:entry.reviewer.id };
    });
    const wait = rules.find(rule => rule.type === 'wait_timer')?.wait_timer ?? 0;
    assert(Number.isSafeInteger(wait) && wait >= 0 && wait <= 43200);
    assert.equal(typeof snapshot.can_admins_bypass, 'boolean');
    return {
      environment:name,
      required_reviewers_before:reviewers.length,
      required_reviewers_after:retained ? reviewers.length : 0,
      proposed_body:{
        wait_timer:wait,
        prevent_self_review:retained ? Boolean(review.prevent_self_review) : false,
        reviewers:retained ? reviewers : [],
        can_admins_bypass:snapshot.can_admins_bypass,
        deployment_branch_policy:{ ...snapshot.deployment_branch_policy },
      },
      preserve:['environment-name', 'environment-secret-scope', 'OIDC-subject', 'main-only-branch-rules', 'wait-timer', 'admin-bypass-policy'],
    };
  });
  return {
    schema:'clearra.discord-approval-transition-plan.v1',
    mode, applied:false, release_authority:false, ready_to_apply:false, transitions,
    integration_prerequisites:[
      'user selected one initial approval; never remove path-confirmation reviewers',
      'update reviewer-protection comments/tests to scoped automated authority consistently',
      'keep source-bound acceptance, debt, artifact, ownership and pre-mutation rechecks',
      'recovery must prove an original protected mutation or bounded owned inactive cleanup',
      'review the exact plan digest before explicitly applying the one-approval migration',
      'apply and read back only the chosen three environment policies; do not approve waiting jobs automatically',
    ],
  };
}
