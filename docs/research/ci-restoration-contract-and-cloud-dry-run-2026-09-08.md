# CI restore contract and Cloud dry-run correction — 2026-09-08

Canonical run 34209481859 failed only the NoProductDebt leaf: its source gate
still required the previous immediate Oracle cwd error sentence. The runtime
had already changed to bounded exact-prior readiness. Other product leaves
passed. Match the actual bounded loop, exact cwd and fail-closed stop contract;
add a fast regression checking that all static restore markers exist in the
executable source. Do not discard the restoration guard or turn failure green.

Recovery 34209478041 failed the existing rollback account's validate-only PATCH
with explicit `iam.serviceAccounts.actAs` denial. No role, identity fallback or
approval condition is changed. A separately authorized local operator using the
existing login rechecked the sealed original 34201495681/1 intent, latest exact
candidate image, prior 100% and candidate 0%, before removing only the owned
`candidate-3571fd9` tag.

This exposed a second implementation bug: Cloud Run returned HTTP success with
an Operation containing validated Service metadata, no `done` field, and a
non-persisted operation name. Polling that validation-only name returned 404.
Accept the narrowly checked non-persisted dry-run response, then independently
reread the exact etag-bound preimage. Actual writes still require bounded
operation completion and full post-read verification; neither a validation nor
an arbitrary 404 grants success.

The operator's actual traffic-only PATCH removed the tag, but the strict full
post-read comparison rejected an unclassified non-traffic difference. Do not
retroactively mark that operation verified. Independent fresh read-only planning
confirmed prior 100%, exact latest immutable candidate/image at 0% and tag absent.
Protected recovery must recheck the entire runtime and seal its own evidence.
Preserve exact comparisons; add allowlisted changed-field-name diagnostics for
future discrepancies, without logging API bodies, template values or secrets.

No revision was deleted, no candidate promoted, no settings or IAM changed by
this tag operation. This is not proof that rollback-account traffic writes are
authorized. Remaining automatic traffic rollback/no-actAs policy conflict stays
separate. A fresh recovery run can now use its existing tagless verification
path without performing the denied PATCH.

## Evidence and retry boundary

- Focused Oracle rollback tests: 3/3, including the new static/runtime parity.
- Focused traffic/transport/preflight tests: 69/69 before the changed-field
  diagnostic; rerun that affected test suite after the diagnostic edit.
- Source-only Release Identity Gate passed; no native Rust product execution.
- Dispatch new exact-main recovery and canonical acceptance after committing.
  Pages continuation remains acceptance-bound. Do not wait for these new runs
  or describe dispatch as deployment success.

Cloud Run documents validateOnly as validating/defaulting without persisting the
request or updating resources. Its actual response shape was observed through
the existing local operator account; only keys/type/name were logged.

Reference: [Cloud Run v2 Service PATCH](https://docs.cloud.google.com/run/docs/reference/rest/v2/projects.locations.services/patch).
