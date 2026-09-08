# Discord recovery image-read prerequisite repair — 2026-09-07

## Observed failure, not a new candidate failure

`Deploy Discord Production` run `34087624675/1` failed its unresolved recovery
debt check for original `33583378208/1`. Protected recovery `34085745591/1`
passed the exact original-attempt authority checks (including the preceding
pending-run fix), but all three runtime-restoration attempts failed while
removing the exact candidate tag from the existing Cloud Run service:

`PERMISSION_DENIED: artifactregistry.repositories.downloadArtifacts`

The caller was the existing `clearra-github-rollback` service account and the
resource was only `clearra-cloud / asia-northeast1 / clearra`. This was not a
CLI/WASM/product regression. An authority-resolution-only successful recovery
report does not clear this debt and must not be substituted for runtime proof.

## Narrow repair

The bootstrap had deliberately modeled no repository role for recovery. The
observed Cloud Run `update-traffic --remove-tags` operation revalidates image
access, so the modeled prerequisite was incomplete. Add only repository-local
`roles/artifactregistry.reader` for the existing recovery identity. The same
exact tuple is admitted by the bootstrap's closed command surface and reported
by its audit. Missing read is repairable; writer/admin/project-wide image read
remain forbidden. No credential file or secret payload is read.

No changes to OIDC subjects, workflow trust, initial environment review,
runtime-account act-as, image writing, Cloud Run custom permissions, debt
clearance, traffic/revision ownership checks, or terminal recovery receipts.
The separately prepared one-initial-approval branch is not applied here.

Focused bootstrap regression tests cover missing-reader repair, idempotence,
ordering before federation, and rejection of broader roles. Live IAM readback
must show exactly the existing builder/deployer readers plus the rollback reader
and the unchanged build writer before retry. Runtime success is not inferred
from IAM success: dispatch protected recovery for `33583378208/1`, then the
exact-source canonical release chain, without waiting for the new runs.

## Permission references

- [Artifact Registry / Cloud Run image-read prerequisite](https://cloud.google.com/artifact-registry/docs/integrate-cloud-run#permissions_required_to_deploy)
- [Cloud Run traffic migration and rollback](https://cloud.google.com/run/docs/rollouts-rollbacks-traffic-migration)

The concrete requirement for candidate tag removal is evidenced by the failed
recovery log above; it is not assumed to be a new deployment of an image.
