#!/usr/bin/env bash
set -euo pipefail

[[ $# == 2 ]]
repository="$1"
authority_root="$2"
[[ "$repository" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]]
[[ -d "$authority_root" && ! -L "$authority_root" ]]

before="$authority_root/primary-run-catalog-before.json"
after="$authority_root/primary-run-catalog-after.json"
attempts_jsonl="$authority_root/primary-attempts.jsonl"
attempts="$authority_root/primary-attempts.json"
catalog="$authority_root/primary-run-catalog.json"

gh api --paginate --slurp --method GET \
  "repos/$repository/actions/workflows/discord-deploy.yml/runs?branch=main&per_page=100" \
  > "$before"
: > "$attempts_jsonl"
jq -e -r '
  (.[0].total_count // -1) as $total |
  select(length > 0 and $total >= 0) |
  select(all(.[]; .total_count == $total and (.workflow_runs | type) == "array")) |
  [.[] | .workflow_runs[]] as $runs |
  select(($runs | length) == $total) |
  select(($runs | map(.id) | unique | length) == $total) |
  $runs[] | [(.id | tostring), (.run_attempt | tostring)] | @tsv
' "$before" | sort -u | while IFS=$'\t' read -r run_id max_attempt; do
  [[ "$run_id" =~ ^[1-9][0-9]*$ && "$max_attempt" =~ ^[1-9][0-9]*$ ]]
  for ((attempt = 1; attempt <= max_attempt; attempt += 1)); do
    gh api --method GET \
      "repos/$repository/actions/runs/$run_id/attempts/$attempt" \
      >> "$attempts_jsonl"
    printf '\n' >> "$attempts_jsonl"
  done
done
jq -s '{schema_id:"clearra.discord-deployment-primary-attempt-catalog.v1",attempts:.}' \
  "$attempts_jsonl" > "$attempts"
gh api --paginate --slurp --method GET \
  "repos/$repository/actions/workflows/discord-deploy.yml/runs?branch=main&per_page=100" \
  > "$after"
node scripts/release/discord-deployment-recovery.mjs validate-run-catalog-snapshots \
  --repository "$repository" --run-list-before "$before" --run-list-after "$after"
mv --no-clobber "$after" "$catalog"
