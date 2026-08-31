# GitHub WIF bootstrap

`github-wif-bootstrap.mjs` prepares four separated, keyless GitHub identities
for the exact `clearra-cloud` project. It creates no service-account key, never
reads a Secret payload or version, and accepts no project, repository, or
identity override.

```powershell
node scripts/release/github/github-wif-bootstrap.mjs plan
node scripts/release/github/github-wif-bootstrap.mjs apply
node scripts/release/github/github-wif-bootstrap.mjs audit
```

`plan` is read-only. `apply` executes independent mutations only within one of
eight closed ordering phases, then re-observes the complete non-Secret
catalog/IAM boundary before it can enter the next phase. The phases are:
managed APIs; pools; service accounts and custom roles; exact non-WIF grants;
legacy authority removals; exact WIF bindings; rollback-provider activation;
and primary-provider activation. Thus a grant is observed before its
corresponding legacy removal, every removal is observed before WIF is attached,
the recovery provider is observed before deployment federation can open, and
the primary provider is the absolute final mutation. A
clean 34-mutation bootstrap with immediately visible writes performs one
initial full plan, eight fast phase-boundary plans, and one final full audit,
instead of 34 complete replans.

Each attempted mutation keeps its own bounded five-minute propagation budget
with a closed 1/2/4/8/16/30-second backoff. Failed or ambiguous writes are
retried only after a fresh fast plan proves the same id, reason, and argv are
still required. Successful writes are never replayed merely because a later
write in the phase failed. This makes interruption/resume idempotent: every
restart begins with a new full plan and reconstructs the remaining ordered
phases from observed state. Full global and regional Secret-policy audits run
before the first mutation and after convergence; intermediate phase readbacks
omit only that expensive Secret inventory. The final full audit must be
mutation-free. Prerequisite drift, timeout, changed argv, an ambient or
persisted gcloud API endpoint override, or an invalid subprocess execution
environment fails closed. `audit` exits with code 3 when changes remain.

The report emits these non-secret GitHub repository variables:

- `GCP_PROJECT_ID`, `GCP_PROJECT_NUMBER`, `GCP_REGION`
- `GCP_WORKLOAD_IDENTITY_PROVIDER`
- `GCP_ROLLBACK_WORKLOAD_IDENTITY_PROVIDER`
- `GCP_BUILD_SERVICE_ACCOUNT`, `GCP_DEPLOY_SERVICE_ACCOUNT`
- `GCP_ROLLBACK_SERVICE_ACCOUNT`, `GCP_COMMAND_SYNC_SERVICE_ACCOUNT`

## Exact federation graph

The primary pool/provider is
`projects/50060711800/locations/global/workloadIdentityPools/clearra-github/providers/clearra-main`.
It pins repository `daejunnom/Clearra`, repository ID `1309293231`, owner ID
`271715321`, ref `refs/heads/main`, and workflow ref
`daejunnom/Clearra/.github/workflows/discord-deploy.yml@refs/heads/main`.
Its only effective service-account bindings are:

- branch-main subject -> `clearra-github-builder`
- `discord-path-confirmation` Environment subject -> `clearra-github-deployer`
- `discord-global-command-sync` Environment subject -> `clearra-command-sync`

Runtime recovery uses the separate pool/provider
`projects/50060711800/locations/global/workloadIdentityPools/clearra-github-rollback/providers/clearra-runtime-rollback`.
It pins the same immutable repository, owner, and ref claims and the distinct
workflow ref
`daejunnom/Clearra/.github/workflows/discord-deploy-recovery.yml@refs/heads/main`.
Only its exact `discord-runtime-rollback` Environment subject can impersonate
`clearra-github-rollback`. The similarly named subject from the primary pool is
forbidden. No `principalSet`, cross-pool reuse, Token Creator path, or
service-account impersonation tuple is accepted outside the exact act-as
bindings below.
The same recovery-provider subject also has one direct Workload Identity User
binding on `clearra-command-sync`, solely so the protected recovery workflow can
restore and read back an already sealed Discord command catalog. It cannot use
the primary provider's global-sync subject or impersonate through the rollback
service account.

All three GitHub Environments (`discord-path-confirmation`,
`discord-global-command-sync`, and `discord-runtime-rollback`) must be
main-only and required-reviewer protected. GitHub Environment configuration
and Environment-scoped private values are external prerequisites; this GCP
helper does not create or weaken them. In particular, it never reads, creates,
or registers the Oracle SSH identity.

## Closed authority sets

`clearra-github-builder` can submit/read Cloud Build work, read build logs,
upload/read only the exact `gs://clearra-cloud_cloudbuild` source bucket, read
only the `clearra` Artifact Registry repository, and act as only
`clearra-build`. It has no Cloud Run, Secret, runtime-account,
rollback-account, or Token Creator authority.

The effective `clearra-build` execution authority is also audited: project
`roles/logging.logWriter`, exact repository
`roles/artifactregistry.writer`, and exact source-bucket
`roles/storage.objectViewer`. The former project-wide
`roles/storage.objectViewer` is removed only after the exact bucket grant is
observed, so the migration has neither a read outage nor broader effective
storage access. Its service-account policy preserves only the bounded trusted
human baseline described below plus the builder's exact
`roles/iam.serviceAccountUser` tuple.

`clearra-github-deployer` trusts only the primary-pool path-confirmation
subject. It can update/read the existing Cloud Run service, manage the
deterministic ephemeral smoke job, read logs and the exact image repository,
and act as only `clearra-current-job`. It has no Cloud Build, source-bucket,
Secret, Token Creator, rollback identity, service IAM-policy mutation, service
deletion, or `roles/run.admin` authority.

`clearra-github-rollback` trusts only the recovery-pool runtime-rollback
subject. It has the exact rollback custom role and Service Usage Consumer. It
can read/update an existing service, list/read revisions, and read the
resulting operation. It cannot create/update/run/delete jobs, delete revisions, read
Artifact Registry, access the source bucket or Secrets, act as the runtime
account, build images, or impersonate another service account.

`clearra-command-sync` directly trusts the primary-pool global-sync subject and
the recovery-pool runtime-rollback subject, and retains Cloud Run Viewer. Both
subjects are exact environment principals; no principal set is accepted.
Secret metadata-policy audit preserves
these exact existing runtime sets without reading payloads:

- `discord-bot-token`: command-sync and interaction accessor
- `clearra-job-token`: current-job runtime and interaction accessor
- `clearra-telemetry-event-key`: interaction accessor
- `clearra-telemetry-transport-key`: telemetry-relay accessor
- every other global/regional Secret: empty direct IAM policy
- builder, build, deployer, and rollback: zero direct Secret role everywhere

Those are complete direct-policy tuple sets: an additional user, group,
domain, public, federated, or service-account member also fails closed.

Every service account returned by the exact project catalog has its IAM policy
read and validated. New GitHub builder/deployer/rollback policies permit only
their exact WIF tuples. Existing operational accounts preserve an explicit,
resource-scoped trusted-human baseline: `user:daejun0311@gmail.com` retains
only its currently modeled Service Account Admin/Owner tuples, and
`user:stemxstudioproject@gmail.com` retains only its currently modeled Service
Account User tuples on build, interaction, and telemetry-relay. The report
lists every exact resource/member/role exception separately from machine
federation. Any missing or additional user, group, domain, public, federated,
service-account, role, or conditional tuple fails closed; this bootstrap does
not silently remove or widen a human binding.

User-managed keys are forbidden on build, runtime, builder, deployer, rollback,
and command-sync identities. Project, Artifact Registry, source-bucket, and all
Secret policies must contain zero direct pool principal or principal-set
binding.

## Cloud Run permission basis

Google separates Cloud Run deployment authority, Artifact Registry image-read
authority, and runtime service-account act-as authority. See
[Cloud Run deployment permissions](https://cloud.google.com/run/docs/reference/iam/roles#deployment-permissions)
and
[Artifact Registry integration with Cloud Run](https://cloud.google.com/artifact-registry/docs/integrate-cloud-run#permissions_required_to_deploy).

The predefined `roles/run.developer` includes resources and operations outside
this workflow. The bootstrap therefore creates
`projects/clearra-cloud/roles/clearraGithubRuntimeDeployer` with only:

- `run.services.get`, `run.services.update`
- `run.revisions.get`
- `run.jobs.create`, `run.jobs.update`, `run.jobs.get`, `run.jobs.run`, `run.jobs.delete`
- `run.executions.get`, `run.operations.get`

Recovery uses the smaller
`projects/clearra-cloud/roles/clearraGithubRuntimeRollback` with only
`run.services.get`, `run.services.update`, `run.revisions.get`,
`run.revisions.list`, and `run.operations.get`. Listing is used only to
distinguish zero or one exact candidate revision before the sealed cleanup
checks. Cloud Run does not allow deletion of the service's latest revision, so
recovery removes the exact candidate tag, proves the prior revision has the
sole 100% traffic allocation, and seals the latest candidate as an unavoidable
0%-traffic residue until a later revision supersedes it. The rollback identity
therefore has no `run.revisions.delete` permission. See
[Manage Cloud Run revisions](https://cloud.google.com/run/docs/managing/revisions#delete_revisions).
Service Usage Consumer is separate API-consumption authority; it adds no Cloud
Run resource mutation permission.
An interrupted earlier bootstrap may have created this exact role with the
otherwise-identical legacy `run.revisions.delete` permission. That single
closed legacy shape is reconciled by removing only that permission before
either OIDC provider can activate; every other role drift still fails closed.
The deploy workflow uses `--no-invoker-iam-check`, so neither custom role needs
`run.services.getIamPolicy` or `run.services.setIamPolicy`.

Google publishes the resource permission meanings in the
[Cloud Run IAM permissions table](https://cloud.google.com/run/docs/reference/iam/permissions),
and both roles use the documented
[project custom-role mechanism](https://cloud.google.com/iam/docs/creating-custom-roles#gcloud).
