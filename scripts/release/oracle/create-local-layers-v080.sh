#!/usr/bin/env bash
set -euo pipefail

usage() {
  printf '%s\n' 'usage: create-local-layers-v080.sh <repository-root> <new-output-directory>' >&2
  exit 64
}

[[ "$#" -eq 2 ]] || usage
repository_root="$(cd "$1" && pwd -P)"
output_root="$2"
[[ "$output_root" = /* ]] || usage
[[ -d "$output_root" && ! -L "$output_root" ]] || {
  printf '%s\n' 'output directory must already exist and must not be a symlink' >&2
  exit 64
}
output_root="$(cd "$output_root" && pwd -P)"

[[ -f "$repository_root/apps/clearra-discord-bot/package.json" ]] || usage
[[ -f "$repository_root/packages/ctk3/package.json" ]] || usage

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

dist_root="$repository_root/packages/ctk3/dist"
dependency_root="$repository_root/node_modules/tetris-fumen"
[[ -d "$dist_root" && ! -L "$dist_root" ]] || {
  printf '%s\n' 'the frozen CTK3 distribution has not been built' >&2
  exit 66
}
[[ -d "$dependency_root" && ! -L "$dependency_root" ]] || {
  printf '%s\n' 'the production tetris-fumen dependency is unavailable' >&2
  exit 66
}
for input_root in "$dist_root" "$dependency_root"; do
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
publish_archive "$dist_archive" --directory="$repository_root" packages/ctk3/dist
publish_archive "$dependencies_archive" --directory="$temporary_root" node_modules

for output in "$overlay_archive" "$dist_archive" "$dependencies_archive"; do
  [[ -f "$output" && ! -L "$output" ]] || exit 73
  printf 'oracle_layer=%s sha256=%s size=%s\n' \
    "$(basename "$output")" \
    "$(sha256sum -- "$output" | cut -d ' ' -f 1)" \
    "$(stat -c '%s' -- "$output")"
done

trap - EXIT HUP INT TERM
cleanup
