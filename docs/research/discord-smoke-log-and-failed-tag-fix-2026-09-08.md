# Discord smoke, recovery topology and failed candidate tag — 2026-09-08

## Observed baseline and failure

Main at investigation: `b0555487153a0fc4999410c42198758119545f0c`.
Canonical acceptance `34144145162` succeeded. Deploy `34146085169` failed
after a successful zero-traffic deployment and smoke Job execution; recovery
`34148176301` rejected the original job-step topology before mutation.
Earlier timestamp, traffic-only request and actAs diagnostic fixes are retained.

The exact smoke execution `clearra-v080-candidate-smoke-b055548-4s9lq`
logged success at `2026-09-07T17:32:49.896524Z` and exited zero. On live
readback, `gcloud run jobs logs read ... --format=json` prints a human-formatted
line but returns `[]` as JSON. Thus its label-bound attestation reader exhausted
30 ingestion attempts despite an already present success. This is not a product
search or Cloud CLI parity failure.

Replace that human log printer with `gcloud logging read`, bound to project,
region, job, exact execution label and success marker. Keep execution success,
source commit, uniqueness and solution-set hash validation; raw log text alone
does not grant authority. The existing validator successfully consumed the
actual failed-run log through the new query. Safe static phase labels identify
future smoke failures without logging arbitrary error/credential payloads.

Recovery's allowed ordered topology omitted the two existing warm CLI diagnostic
steps. They are now included explicitly, with a source-to-contract parity test.
Foreign steps, reordering, truncated jobs and missing runtime receipts still fail.

## Candidate-tag lifecycle

The user clarified the deleted tag was Cloud Run's candidate tag, not a Git tag.
Read-only service inspection confirmed `candidate-b055548` absent, latest
candidate revision still present and prior `clearra-current-job-v075-701454b`
still serving 100%. Unrelated historical tags were not altered.

A failed primary promotion now attempts exact tag cleanup before losing its
already approved deployer identity. It validates the sealed source/run/attempt/
nonce intent, prior 100%, candidate zero traffic, image, latest revision, etag,
and unchanged unrelated tags through the existing traffic-only planner and
post-write verifier. Only the owned candidate tag is removed, never a revision,
image, Git tag or foreign tag. Successful primary runs skip this failure step.
The failure stays failed even if cleanup succeeds. A marker records a Cloud
deploy attempt so partial deployment failures also attempt bounded cleanup.

This is not rollback-to-deployer authentication fallback: the primary promote
job uses its original approved identity. Recovery retains its distinct identity,
no-actAs policy, environment protection and final Oracle/Cloud proof requirements.
Hard cancellation, lost credentials, unsettled/absent candidate readback or
foreign drift can still require protected recovery or operator intervention;
cleanup failure must never fabricate recovery clearance.

## Retry and hotfix boundary

Dispatch protected recovery for original `34146085169/1` from the new trusted
main, then canonical exact-source acceptance and its automatic Discord chain.
Queue the matching Pages authority needed by global sync. Do not wait for the
new deployments. External solver comparisons and minimum hotfix code remain in
the separate local-only experiment branch, not this production commit.
