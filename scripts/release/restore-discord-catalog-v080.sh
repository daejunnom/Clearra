#!/usr/bin/env bash
set -euo pipefail

[[ "$#" -eq 1 ]] || {
  echo 'usage: restore-discord-catalog-v080.sh catalog-1|catalog-2' >&2
  exit 2
}
generation="$1"
[[ "$generation" == catalog-1 || "$generation" == catalog-2 ]] || {
  echo 'catalog recovery generation must be catalog-1 or catalog-2' >&2
  exit 2
}

for name in SOURCE_COMMIT ORIGINAL_RUN_ID ORIGINAL_RUN_ATTEMPT GITHUB_RUN_ID \
  GITHUB_RUN_ATTEMPT GITHUB_REPOSITORY CATALOG_RECOVERY_REQUIRED RUNNER_TEMP; do
  [[ -n "${!name:-}" ]] || { echo "catalog recovery lacks required authority: $name" >&2; exit 2; }
done
[[ "$SOURCE_COMMIT" =~ ^[0-9a-f]{40}$ ]]
[[ "$ORIGINAL_RUN_ID" =~ ^[1-9][0-9]*$ && "$ORIGINAL_RUN_ATTEMPT" =~ ^[1-9][0-9]*$ ]]
[[ "$GITHUB_RUN_ID" =~ ^[1-9][0-9]*$ && "$GITHUB_RUN_ATTEMPT" =~ ^[1-9][0-9]*$ ]]
[[ "$CATALOG_RECOVERY_REQUIRED" == true || "$CATALOG_RECOVERY_REQUIRED" == false ]]

generation_root="$RUNNER_TEMP/discord-recovery-generations"
if [[ -e "$generation_root" ]]; then
  [[ -d "$generation_root" && ! -L "$generation_root" ]] || {
    echo 'catalog recovery generation root is not an exact directory' >&2
    exit 2
  }
else
  mkdir "$generation_root"
fi
evidence="$generation_root/$generation"
input="$RUNNER_TEMP/discord-catalog-recovery-input"
disposition="$evidence/discord-catalog-recovery-disposition.json"
restore="$evidence/discord-catalog-restore.json"
[[ ! -e "$evidence" && ! -L "$evidence" ]] || {
  echo 'catalog recovery refuses to overwrite an existing evidence generation' >&2
  exit 2
}
mkdir "$evidence"
[[ -d "$evidence" && ! -L "$evidence" ]]

if [[ "$CATALOG_RECOVERY_REQUIRED" == false ]]; then
  node scripts/release/discord-catalog-recovery-authority.mjs seal-disposition \
    --repository "$GITHUB_REPOSITORY" --source-commit "$SOURCE_COMMIT" \
    --original-workflow-run-id "$ORIGINAL_RUN_ID" \
    --original-workflow-run-attempt "$ORIGINAL_RUN_ATTEMPT" \
    --recovery-workflow-run-id "$GITHUB_RUN_ID" \
    --recovery-workflow-run-attempt "$GITHUB_RUN_ATTEMPT" \
    --required false --output "$disposition"
  exit 0
fi

for name in CATALOG_ARTIFACT_ID CATALOG_ARTIFACT_DIGEST DISCORD_APPLICATION_ID GCP_PROJECT_ID; do
  [[ -n "${!name:-}" ]] || { echo "catalog recovery lacks required authority: $name" >&2; exit 2; }
done
[[ "$CATALOG_ARTIFACT_ID" =~ ^[1-9][0-9]*$ ]]
[[ "$CATALOG_ARTIFACT_DIGEST" =~ ^sha256:[0-9a-f]{64}$ ]]
[[ "$DISCORD_APPLICATION_ID" =~ ^[0-9]{17,20}$ ]]

node scripts/release/discord-catalog-recovery-authority.mjs verify \
  --repository "$GITHUB_REPOSITORY" --source-commit "$SOURCE_COMMIT" \
  --workflow-run-id "$ORIGINAL_RUN_ID" --workflow-run-attempt "$ORIGINAL_RUN_ATTEMPT" \
  --application-id "$DISCORD_APPLICATION_ID" \
  --prior-snapshot "$input/discord-prior-catalog.json" \
  --desired-catalog "$input/discord-catalog.json" \
  --sync-authority "$input/discord-sync-authority.json" \
  --report "$input/discord-catalog-recovery-authority.json"

desired="$(jq -r .catalog_sha256 "$input/discord-catalog.json")"
prior="$(jq -r .catalog_sha256 "$input/discord-prior-catalog.json")"
[[ "$desired" =~ ^[0-9a-f]{64}$ && "$prior" =~ ^[0-9a-f]{64}$ ]]
expected="$desired"
sync_args=()
# A completed sync records Discord's exact server-defaulted state. Recover that
# preimage from the original attempt's immutable artifact, never from live state
# or from an operator-supplied digest. Without it the legacy guard stays strict.
sync_name="discord-sync-inputs-$SOURCE_COMMIT-run-$ORIGINAL_RUN_ID-attempt-$ORIGINAL_RUN_ATTEMPT"
artifact_list="$RUNNER_TEMP/discord-recovery-protected-authority/original-artifacts.json"
sync_count="$(jq --arg name "$sync_name" '[.artifacts[] | select(.name == $name)] | length' "$artifact_list")"
[[ "$sync_count" == 0 || "$sync_count" == 1 ]] || {
  echo 'Discord recovery has ambiguous completed sync artifacts' >&2
  exit 2
}
if [[ "$sync_count" == 1 ]]; then
  sync_root="$RUNNER_TEMP/discord-catalog-sync-$generation"
  [[ ! -e "$sync_root" && ! -L "$sync_root" ]]
  mkdir "$sync_root"
  jq -e --arg name "$sync_name" --arg source "$SOURCE_COMMIT" --arg run "$ORIGINAL_RUN_ID" '
    .artifacts[] | select(.name == $name) |
    select(.expired == false and .size_in_bytes > 0 and .size_in_bytes <= 20971520) |
    select((.workflow_run.id | tostring) == $run and .workflow_run.head_sha == $source) |
    select(.workflow_run.head_branch == "main") |
    select(.workflow_run.head_repository_id == .workflow_run.repository_id)
  ' "$artifact_list" > "$sync_root/artifact.json"
  sync_id="$(jq -r .id "$sync_root/artifact.json")"
  sync_digest="$(jq -r .digest "$sync_root/artifact.json")"
  [[ "$sync_id" =~ ^[1-9][0-9]*$ && "$sync_digest" =~ ^sha256:[0-9a-f]{64}$ ]]
  timeout --signal=TERM --kill-after=5s 60s gh api \
    "repos/$GITHUB_REPOSITORY/actions/artifacts/$sync_id/zip" > "$sync_root/sync.zip"
  printf '%s  %s\n' "${sync_digest#sha256:}" "$sync_root/sync.zip" | sha256sum --check --status
  sync_members="$(unzip -Z1 "$sync_root/sync.zip" | awk '$0 == "discord-sync-report.json" { count++ } END { print count+0 }')"
  [[ "$sync_members" == 0 || "$sync_members" == 1 ]]
  if [[ "$sync_members" == 1 ]]; then
    unzip -p "$sync_root/sync.zip" discord-sync-report.json > "$sync_root/discord-sync-report.json"
    sync_args=(--sync-report "$sync_root/discord-sync-report.json")
    expected="$(node scripts/release/discord-catalog-recovery-authority.mjs verify-sync \
      --repository "$GITHUB_REPOSITORY" --source-commit "$SOURCE_COMMIT" \
      --workflow-run-id "$ORIGINAL_RUN_ID" --workflow-run-attempt "$ORIGINAL_RUN_ATTEMPT" \
      --application-id "$DISCORD_APPLICATION_ID" \
      --prior-snapshot "$input/discord-prior-catalog.json" \
      --desired-catalog "$input/discord-catalog.json" \
      --sync-authority "$input/discord-sync-authority.json" \
      --report "$input/discord-catalog-recovery-authority.json" "${sync_args[@]}")"
    [[ "$expected" =~ ^[0-9a-f]{64}$ ]]
  fi
fi
token="$(gcloud secrets versions access latest --secret=discord-bot-token --project="$GCP_PROJECT_ID")"
[[ -n "$token" ]] || { echo 'Discord token access returned empty during recovery' >&2; exit 2; }
echo "::add-mask::$token"
trap 'unset token' EXIT INT TERM
DISCORD_TOKEN="$token" node \
  apps/clearra-discord-bot/scripts/discord-command-catalog-release.mjs restore \
  --source-commit "$SOURCE_COMMIT" --application-id "$DISCORD_APPLICATION_ID" \
  --prior-snapshot "$input/discord-prior-catalog.json" \
  --expected-current-digest "$expected" --also-allow-current-digest "$prior" \
  --output "$restore"
unset token
trap - EXIT INT TERM

node scripts/release/discord-catalog-recovery-authority.mjs seal-disposition \
  --repository "$GITHUB_REPOSITORY" --source-commit "$SOURCE_COMMIT" \
  --original-workflow-run-id "$ORIGINAL_RUN_ID" \
  --original-workflow-run-attempt "$ORIGINAL_RUN_ATTEMPT" \
  --recovery-workflow-run-id "$GITHUB_RUN_ID" \
  --recovery-workflow-run-attempt "$GITHUB_RUN_ATTEMPT" \
  --application-id "$DISCORD_APPLICATION_ID" --required true \
  --artifact-id "$CATALOG_ARTIFACT_ID" --artifact-digest "$CATALOG_ARTIFACT_DIGEST" \
  --prior-snapshot "$input/discord-prior-catalog.json" \
  --desired-catalog "$input/discord-catalog.json" \
  --sync-authority "$input/discord-sync-authority.json" \
  --authority-report "$input/discord-catalog-recovery-authority.json" \
  --restore-report "$restore" "${sync_args[@]}" --output "$disposition"
