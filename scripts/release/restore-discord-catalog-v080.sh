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

token="$(gcloud secrets versions access latest --secret=discord-bot-token --project="$GCP_PROJECT_ID")"
[[ -n "$token" ]] || { echo 'Discord token access returned empty during recovery' >&2; exit 2; }
echo "::add-mask::$token"
trap 'unset token' EXIT INT TERM
desired="$(jq -r .catalog_sha256 "$input/discord-catalog.json")"
prior="$(jq -r .catalog_sha256 "$input/discord-prior-catalog.json")"
[[ "$desired" =~ ^[0-9a-f]{64}$ && "$prior" =~ ^[0-9a-f]{64}$ ]]
DISCORD_TOKEN="$token" node \
  apps/clearra-discord-bot/scripts/discord-command-catalog-release.mjs restore \
  --source-commit "$SOURCE_COMMIT" --application-id "$DISCORD_APPLICATION_ID" \
  --prior-snapshot "$input/discord-prior-catalog.json" \
  --expected-current-digest "$desired" --also-allow-current-digest "$prior" \
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
  --restore-report "$restore" --output "$disposition"
