# Discord concurrency-pending recovery correction

Previous canonical acceptance 34082105759 and Pages publication succeeded;
Discord deployment 34083826879 failed before candidate work on outstanding
33583378208/1 recovery debt. Protected recovery 34082104334 failed its second
authority resolution with `before run catalog snapshot status is invalid`.
No protected restore was performed by that failed recovery.

The shared workflow concurrency group held the new deployment while recovery
was executing. GitHub exposes that workflow state as `pending`, distinct from
`queued`. The parser admitted only queued/in_progress/completed. This was a
state-model omission, not another Oracle cleanup-tool or bootstrap-deadline bug.

The correction recognizes GitHub's nonterminal statuses in inventory parsing.
Only a pending attempt with an exact, complete, single zero-job API response is
admitted as not yet executing under the shared concurrency group. The sealed
freshness decision binds its source, run and attempt. Timestamps alone are not
used to infer absence of work. Existing before/after snapshot equality and exact
attempt binding remain mandatory; pending-to-in_progress drift is still fatal.
Waiting/requested/in_progress attempts remain ambiguous for protected recovery,
even with an empty job list. Nonterminal recoveries never clear debt.

Both pre-approval and immediately-before-mutation resolvers now collect the
pending attempt's job inventory. Missing, partial, contradictory or nonempty
jobs refuse recovery. No environment reviewer, credential scope, OIDC trust,
immutable artifact digest, runtime-mutation guard or recovery debt is waived.
Any later approval-flow improvement belongs to a separate unmerged branch.

Focused tests cover zero-job pending with null/populated run-started timestamp,
malformed inventories, state drift, ambiguity, report roundtrip, nonterminal
debt and both workflow collectors. These are source/mock tests, not a successful
production restore. Fresh run results must be checked on a later user request.

[GitHub workflow-run status contract](https://docs.github.com/en/rest/actions/workflow-runs#list-workflow-runs-for-a-repository)
and [concurrency contract](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency)
were checked against the current documentation. The rest of the release success
evidence is not invalidated or claimed to have fixed the outstanding recovery.
