#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf '%s\n' 'usage: create-local-layers-v080.sh <repository-root> <accepted-ctk3-dist-directory> <source-commit> <accepted-run-id> <accepted-run-attempt> <new-output-directory>' >&2
  exit 64
}

[[ "$#" -eq 6 ]] || usage
repository_root="$(cd "$1" && pwd -P)"
accepted_ctk3_root="$2"
source_commit="$3"
accepted_run_id="$4"
accepted_run_attempt="$5"
output_root="$6"
[[ "$accepted_ctk3_root" = /* && -d "$accepted_ctk3_root" && ! -L "$accepted_ctk3_root" ]] || {
  printf '%s\n' 'accepted CTK3 distribution must be an absolute regular directory' >&2
  exit 64
}
accepted_ctk3_root="$(cd "$accepted_ctk3_root" && pwd -P)"
[[ "$source_commit" =~ ^[0-9a-f]{40}$ ]] || usage
[[ "$accepted_run_id" =~ ^[1-9][0-9]{0,19}$ ]] || usage
[[ "$accepted_run_attempt" =~ ^[1-9][0-9]{0,19}$ ]] || usage
[[ "$output_root" = /* ]] || usage
[[ -d "$output_root" && ! -L "$output_root" ]] || {
  printf '%s\n' 'output directory must already exist and must not be a symlink' >&2
  exit 64
}
output_root="$(cd "$output_root" && pwd -P)"

[[ "$accepted_ctk3_root" != "$repository_root/packages/ctk3/dist" ]] || {
  printf '%s\n' 'repo-local packages/ctk3/dist is not accepted artifact authority' >&2
  exit 64
}
case "$output_root/" in
  "$accepted_ctk3_root/"*)
    printf '%s\n' 'output directory must be disjoint from accepted CTK3 authority' >&2
    exit 64
    ;;
esac
case "$accepted_ctk3_root/" in
  "$output_root/"*)
    printf '%s\n' 'accepted CTK3 authority must be disjoint from the output directory' >&2
    exit 64
    ;;
esac

[[ -f "$repository_root/apps/clearra-discord-bot/package.json" ]] || usage
[[ -f "$repository_root/packages/ctk3/package.json" ]] || usage
accepted_ctk3_verifier="$repository_root/scripts/tools/accepted-ctk3-dist.mjs"
[[ -f "$accepted_ctk3_verifier" && ! -L "$accepted_ctk3_verifier" ]] || usage

overlay_archive="$output_root/private-overlay-no-config.tar"
dist_archive="$output_root/ctk3-dist.tar"
dependencies_archive="$output_root/node_modules.tar"
for output in "$overlay_archive" "$dist_archive" "$dependencies_archive"; do
  [[ ! -e "$output" && ! -L "$output" ]] || {
    printf 'refusing to overwrite frozen layer: %s\n' "$output" >&2
    exit 73
  }
done

overlay_paths=(
  apps/clearra-discord-bot/src/admin
  apps/clearra-discord-bot/src/admin/access-runtime.mjs
  apps/clearra-discord-bot/src/admin/command-identity.mjs
  apps/clearra-discord-bot/src/admin/discord-history-hydrator.mjs
  apps/clearra-discord-bot/src/admin/discord-observer.mjs
  apps/clearra-discord-bot/src/admin/document.mjs
  apps/clearra-discord-bot/src/admin/identity.mjs
  apps/clearra-discord-bot/src/admin/local-publisher.mjs
  apps/clearra-discord-bot/src/admin/main.mjs
  apps/clearra-discord-bot/src/admin/oracle-telemetry.conf
  apps/clearra-discord-bot/src/admin/oracle-usage-tracker.mjs
  apps/clearra-discord-bot/src/admin/runtime-extension.mjs
  apps/clearra-discord-bot/src/admin/server.mjs
  apps/clearra-discord-bot/src/admin/slash-runtime.mjs
  apps/clearra-discord-bot/src/admin/TELEMETRY-OPERATIONS.md
  apps/clearra-discord-bot/src/admin/usage-store.mjs
  apps/clearra-discord-bot/src/admin/telemetry
  apps/clearra-discord-bot/src/admin/telemetry/hmac.mjs
  apps/clearra-discord-bot/src/admin/telemetry/rate-limiter.mjs
  apps/clearra-discord-bot/src/admin/telemetry/schema.mjs
  apps/clearra-discord-bot/src/admin/deploy
  apps/clearra-discord-bot/src/admin/deploy/ORACLE_GATEWAY.md
  apps/clearra-discord-bot/src/admin/deploy/oracle
  apps/clearra-discord-bot/src/admin/deploy/oracle/clearra-gateway-vault-run
  apps/clearra-discord-bot/src/admin/deploy/oracle/clearra-gateway.service
)

for relative in "${overlay_paths[@]}"; do
  path="$repository_root/$relative"
  if [[ "$relative" = */admin || "$relative" = */telemetry || "$relative" = */deploy || "$relative" = */oracle ]]; then
    [[ -d "$path" && ! -L "$path" ]] || {
      printf 'required private overlay directory is unavailable: %s\n' "$relative" >&2
      exit 66
    }
  else
    [[ -f "$path" && ! -L "$path" ]] || {
      printf 'required private overlay file is unavailable: %s\n' "$relative" >&2
      exit 66
    }
  fi
done

if [[ -e "$repository_root/apps/clearra-discord-bot/src/admin/config.mjs" ]]; then
  [[ -f "$repository_root/apps/clearra-discord-bot/src/admin/config.mjs" &&
     ! -L "$repository_root/apps/clearra-discord-bot/src/admin/config.mjs" ]] || {
    printf '%s\n' 'private config must be a regular local file when present' >&2
    exit 77
  }
fi

dependency_root="$repository_root/node_modules/tetris-fumen"
[[ -d "$dependency_root" && ! -L "$dependency_root" ]] || {
  printf '%s\n' 'the production tetris-fumen dependency is unavailable' >&2
  exit 66
}
for input_root in "$accepted_ctk3_root" "$dependency_root"; do
  unsupported="$(find "$input_root" -xdev ! -type f ! -type d -print -quit)"
  [[ -z "$unsupported" ]] || {
    printf 'unsupported frozen layer entry: %s\n' "$unsupported" >&2
    exit 77
  }
done

temporary_root="$(mktemp -d /tmp/clearra-v080-local-layers.XXXXXXXX)"
cleanup() {
  status=$?
  trap - EXIT HUP INT TERM
  case "$temporary_root" in
    /tmp/clearra-v080-local-layers.*)
      [[ ! -L "$temporary_root" ]] && rm -rf -- "$temporary_root"
      ;;
  esac
  exit "$status"
}
trap cleanup EXIT HUP INT TERM

mkdir -p "$temporary_root/node_modules/@clearra"
mkdir -p "$temporary_root/packages/ctk3"
cp -a -- "$accepted_ctk3_root" "$temporary_root/packages/ctk3/dist"
node "$accepted_ctk3_verifier" \
  --verify "$temporary_root/packages/ctk3/dist" \
  --expected-source-commit "$source_commit" \
  --expected-run-id "$accepted_run_id" \
  --expected-run-attempt "$accepted_run_attempt" >/dev/null
cp -a -- "$dependency_root" "$temporary_root/node_modules/tetris-fumen"
ln -s ../packages/ctk3 "$temporary_root/node_modules/ctk3"
ln -s ../../apps/clearra-discord-bot "$temporary_root/node_modules/@clearra/discord-bot"

tar_flags=(
  --create
  --format=posix
  --sort=name
  --mtime=@0
  --owner=0
  --group=0
  --numeric-owner
  --pax-option=delete=atime,delete=ctime
)

publish_archive() {
  destination=$1
  shift
  temporary_archive="$(mktemp "$output_root/.clearra-v080-layer.XXXXXXXX")"
  tar "${tar_flags[@]}" --file="$temporary_archive" "$@"
  chmod 0600 "$temporary_archive"
  if ! ln -- "$temporary_archive" "$destination"; then
    rm -f -- "$temporary_archive"
    printf 'frozen layer appeared during publication: %s\n' "$destination" >&2
    exit 73
  fi
  rm -f -- "$temporary_archive"
}

publish_archive "$overlay_archive" --no-recursion --directory="$repository_root" "${overlay_paths[@]}"
publish_archive "$dist_archive" --directory="$temporary_root" packages/ctk3/dist
publish_archive "$dependencies_archive" --directory="$temporary_root" node_modules

printf 'oracle_ctk3_authority=accepted source_commit=%s run_id=%s run_attempt=%s\n' \
  "$source_commit" "$accepted_run_id" "$accepted_run_attempt"

for output in "$overlay_archive" "$dist_archive" "$dependencies_archive"; do
  [[ -f "$output" && ! -L "$output" ]] || exit 73
  printf 'oracle_layer=%s sha256=%s size=%s\n' \
    "$(basename "$output")" \
    "$(sha256sum -- "$output" | cut -d ' ' -f 1)" \
    "$(stat -c '%s' -- "$output")"
done

trap - EXIT HUP INT TERM
cleanup
