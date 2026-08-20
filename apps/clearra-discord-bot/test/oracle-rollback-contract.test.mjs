import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import test from "node:test";

const scriptsDirectory = resolve(import.meta.dirname, "..", "scripts");
const restorePath = resolve(scriptsDirectory, "restore-oracle-release");
const releaseDigestPath = resolve(scriptsDirectory, "release-tree-digest.mjs");

test("Oracle rollback leaves only a fully verified restored service active", async () => {
  const source = await readFile(restorePath, "utf8");
  for (const marker of [
    "Prior Oracle release is outside the immutable release root.",
    "Prior Oracle settings backup is outside the approved backup namespace.",
    "Prior Oracle settings backup must be root-owned.",
    "Prior Oracle settings backup does not match its captured digest.",
    "Prior Oracle release does not match its captured tree digest.",
    'mv -Tf -- "$temporary_link" "$current_link"',
    'mv -f -- "$temporary_settings" "$settings_path"',
    '"$systemctl_path" is-active --quiet "$service_name"',
    "Restored Oracle settings digest mismatch.",
    "Restored Oracle process does not run from the prior immutable release.",
    "service_transition_started=1",
    "restore_verified=1",
  ]) {
    assert.match(source, new RegExp(escapeRegex(marker)));
  }

  const cleanupStart = source.indexOf("cleanup() {");
  const cleanupEnd = source.indexOf("\n}\ntrap cleanup EXIT", cleanupStart);
  assert.ok(cleanupStart >= 0 && cleanupEnd > cleanupStart);
  const cleanup = source.slice(cleanupStart, cleanupEnd);
  assert.match(cleanup, /restore_verified" -ne 1/u);
  assert.match(cleanup, /"\$systemctl_path" stop "\$service_name"/u);
  assert.doesNotMatch(cleanup, /"\$systemctl_path" start "\$service_name"/u);

  const transition = source.indexOf("service_transition_started=1");
  const stop = source.indexOf('"$systemctl_path" stop "$service_name"', transition);
  const release = source.indexOf('mv -Tf -- "$temporary_link" "$current_link"');
  const settings = source.indexOf('mv -f -- "$temporary_settings" "$settings_path"');
  const start = source.indexOf('"$systemctl_path" start "$service_name"', settings);
  const ready = source.indexOf('"$systemctl_path" is-active --quiet "$service_name"');
  const pid = source.indexOf('main_pid=$("$systemctl_path" show', ready);
  const cwd = source.indexOf('process_cwd=$(readlink -f -- "/proc/$main_pid/cwd")', pid);
  const verified = source.indexOf("restore_verified=1", cwd);
  assert.ok(
    transition >= 0 &&
      stop > transition &&
      release > stop &&
      settings > release &&
      start > settings &&
      ready > start &&
      pid > ready &&
      cwd > pid &&
      verified > cwd,
  );
});

test(
  "Oracle rollback keeps the service stopped after every partial or unverified restore",
  { timeout: 30_000 },
  async () => {
    const [source, releaseDigest] = await Promise.all([
      readFile(restorePath, "utf8"),
      readFile(releaseDigestPath, "utf8"),
    ]);
    const harnessDirectory = await mkdtemp(
      resolve(tmpdir(), "clearra-oracle-rollback-contract-"),
    );
    try {
      const harnessRestore = instrumentRestoreForSandbox(source);
      await Promise.all([
        writeFile(resolve(harnessDirectory, "restore-oracle-release"), harnessRestore),
        writeFile(resolve(harnessDirectory, "release-tree-digest.mjs"), releaseDigest),
        writeFile(resolve(harnessDirectory, "run-restore-contract.sh"), executableHarness),
      ]);

      const shellDirectory = bashPath(harnessDirectory);
      for (const scenario of [
        "success",
        "settings-failure",
        "start-failure",
        "is-active-failure",
        "pid-failure",
        "cwd-failure",
      ]) {
        const result = spawnSync(
          "bash",
          [resolveShell(shellDirectory, "run-restore-contract.sh"), shellDirectory, scenario],
          {
            encoding: "utf8",
            timeout: 20_000,
            windowsHide: true,
          },
        );
        assert.equal(
          result.status,
          0,
          [
            `rollback executable contract failed for ${scenario}`,
            result.error?.stack ?? "",
            result.stdout,
            result.stderr,
          ].filter(Boolean).join("\n"),
        );
      }
    } finally {
      await rm(harnessDirectory, { recursive: true, force: true });
    }
  },
);

function instrumentRestoreForSandbox(source) {
  const replacements = [
    ["release_root=/opt/clearra/releases", 'release_root="$CLEARRA_TEST_ROOT/opt/clearra/releases"'],
    ["current_link=/opt/clearra/current", 'current_link="$CLEARRA_TEST_ROOT/opt/clearra/current"'],
    ["settings_path=/etc/clearra-gateway/settings", 'settings_path="$CLEARRA_TEST_ROOT/etc/clearra-gateway/settings"'],
    ["systemctl_path=/usr/bin/systemctl", 'systemctl_path="$CLEARRA_TEST_ROOT/bin/systemctl"'],
    [
      "  /etc/clearra-gateway/settings.pre-*) ;;",
      '  "$CLEARRA_TEST_ROOT"/etc/clearra-gateway/settings.pre-*) ;;',
    ],
    [
      "temporary_directory=$(mktemp -d /opt/clearra/.oracle-rollback.XXXXXX)",
      'temporary_directory=$(mktemp -d "$CLEARRA_TEST_ROOT/opt/clearra/.oracle-rollback.XXXXXX")',
    ],
    [
      "temporary_settings=$(mktemp /etc/clearra-gateway/.settings.rollback.XXXXXX)",
      'temporary_settings=$(mktemp "$CLEARRA_TEST_ROOT/etc/clearra-gateway/.settings.rollback.XXXXXX")',
    ],
  ];
  let instrumented = source;
  for (const [before, after] of replacements) {
    assert.equal(instrumented.includes(before), true, `missing harness marker: ${before}`);
    instrumented = instrumented.replace(before, after);
  }
  return instrumented;
}

function bashPath(path) {
  if (process.platform !== "win32") return path;
  const match = /^([A-Za-z]):[\\/](.*)$/u.exec(path);
  if (!match) throw new Error(`cannot map Windows path into WSL: ${path}`);
  return `/mnt/${match[1].toLowerCase()}/${match[2].replaceAll("\\", "/")}`;
}

function resolveShell(directory, name) {
  return `${directory.replace(/\/$/u, "")}/${name}`;
}

function escapeRegex(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

const executableHarness = String.raw`#!/bin/sh
set -eu

harness_directory=$1
scenario=$2
test_directory=$(mktemp -d)
trap 'rm -rf -- "$test_directory"' EXIT HUP INT TERM

fixture_root="$test_directory/fixture"
bin_directory="$fixture_root/bin"
state_directory="$fixture_root/state"
release_root="$fixture_root/opt/clearra/releases"
prior_release="$release_root/prior-release"
candidate_release="$release_root/candidate-release"
settings_directory="$fixture_root/etc/clearra-gateway"
settings_path="$settings_directory/settings"
settings_backup="$settings_directory/settings.pre-v0.7.5-rollback"

mkdir -p \
  "$bin_directory" \
  "$state_directory" \
  "$prior_release/apps/clearra-discord-bot/src/admin" \
  "$candidate_release/apps/clearra-discord-bot/src/admin" \
  "$fixture_root/opt/clearra" \
  "$settings_directory"
printf '%s\n' 'export const restored = true;' > \
  "$prior_release/apps/clearra-discord-bot/src/admin/main.mjs"
printf '%s\n' 'export const candidate = true;' > \
  "$candidate_release/apps/clearra-discord-bot/src/admin/main.mjs"
ln -s -- "$candidate_release" "$fixture_root/opt/clearra/current"
printf '%s\n' 'CURRENT=old' > "$settings_path"
printf '%s\n' 'CLEARRA_JOB_URL=https://prior.example.run.app/jobs' > "$settings_backup"

cat > "$bin_directory/id" <<'FAKE_ID'
#!/bin/sh
if [ "$1" = "-u" ]; then
  printf '%s\n' 0
  exit 0
fi
exec /usr/bin/id "$@"
FAKE_ID

cat > "$bin_directory/stat" <<'FAKE_STAT'
#!/bin/sh
if [ "$1" = "-c" ] && [ "$2" = "%u" ]; then
  printf '%s\n' 0
  exit 0
fi
exec /usr/bin/stat "$@"
FAKE_STAT

cat > "$bin_directory/install" <<'FAKE_INSTALL'
#!/bin/sh
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o|-g|-m) shift 2 ;;
    --) shift; break ;;
    *) break ;;
  esac
done
exec /usr/bin/install -m 0644 "$1" "$2"
FAKE_INSTALL

cat > "$bin_directory/mv" <<'FAKE_MV'
#!/bin/sh
count=0
if [ -f "$CLEARRA_TEST_MV_COUNT" ]; then
  count=$(cat "$CLEARRA_TEST_MV_COUNT")
fi
count=$((count + 1))
printf '%s\n' "$count" > "$CLEARRA_TEST_MV_COUNT"
if [ "$CLEARRA_TEST_SCENARIO" = "settings-failure" ] && [ "$count" -eq 2 ]; then
  exit 73
fi
exec /usr/bin/mv "$@"
FAKE_MV

cat > "$bin_directory/readlink" <<'FAKE_READLINK'
#!/bin/sh
last_argument=
for argument do
  last_argument=$argument
done
case "$last_argument" in
  /proc/*/cwd)
    if [ "$CLEARRA_TEST_SCENARIO" = "cwd-failure" ]; then
      printf '%s\n' "$CLEARRA_TEST_ROOT/wrong-process-directory"
    else
      printf '%s\n' "$CLEARRA_TEST_PRIOR_RELEASE/apps/clearra-discord-bot"
    fi
    exit 0
    ;;
esac
exec /usr/bin/readlink "$@"
FAKE_READLINK

cat > "$bin_directory/systemctl" <<'FAKE_SYSTEMCTL'
#!/bin/sh
command=$1
printf '%s\n' "$command" >> "$CLEARRA_TEST_LOG"
case "$command" in
  stop)
    rm -f -- "$CLEARRA_TEST_ACTIVE"
    ;;
  start)
    : > "$CLEARRA_TEST_ACTIVE"
    if [ "$CLEARRA_TEST_SCENARIO" = "start-failure" ]; then
      exit 74
    fi
    ;;
  is-active)
    if [ "$CLEARRA_TEST_SCENARIO" = "is-active-failure" ]; then
      exit 75
    fi
    [ -f "$CLEARRA_TEST_ACTIVE" ]
    ;;
  show)
    if [ "$CLEARRA_TEST_SCENARIO" = "pid-failure" ]; then
      printf '%s\n' 1
    else
      printf '%s\n' 4242
    fi
    ;;
  *) exit 76 ;;
esac
FAKE_SYSTEMCTL

chmod +x "$bin_directory"/*
export CLEARRA_TEST_ROOT="$fixture_root"
export CLEARRA_TEST_LOG="$state_directory/systemctl.log"
export CLEARRA_TEST_ACTIVE="$state_directory/active"
export CLEARRA_TEST_MV_COUNT="$state_directory/mv-count"
export CLEARRA_TEST_SCENARIO="$scenario"
export CLEARRA_TEST_PRIOR_RELEASE="$prior_release"
PATH="$bin_directory:$PATH"
export PATH

release_sha=$(node "$harness_directory/release-tree-digest.mjs" "$prior_release")
settings_sha=$(sha256sum -- "$settings_backup" | awk '{print $1}')
set +e
/bin/sh "$harness_directory/restore-oracle-release" \
  "$prior_release" \
  "$release_sha" \
  "$settings_backup" \
  "$settings_sha" \
  > "$state_directory/stdout" \
  2> "$state_directory/stderr"
restore_status=$?
set -e

fail_contract() {
  printf '%s\n' "scenario=$scenario status=$restore_status" >&2
  cat "$state_directory/stdout" >&2 || true
  cat "$state_directory/stderr" >&2 || true
  cat "$CLEARRA_TEST_LOG" >&2 || true
  exit 1
}

[ -f "$CLEARRA_TEST_LOG" ] || fail_contract
if [ "$scenario" = "success" ]; then
  [ "$restore_status" -eq 0 ] || fail_contract
  [ -f "$CLEARRA_TEST_ACTIVE" ] || fail_contract
  grep -Fx 'oracle_restore=passed' "$state_directory/stdout" >/dev/null || fail_contract
  [ "$(tail -n 1 "$CLEARRA_TEST_LOG")" = "show" ] || fail_contract
  [ "$(readlink -f -- "$fixture_root/opt/clearra/current")" = "$prior_release" ] || fail_contract
  cmp -s "$settings_path" "$settings_backup" || fail_contract
else
  [ "$restore_status" -ne 0 ] || fail_contract
  [ ! -e "$CLEARRA_TEST_ACTIVE" ] || fail_contract
  [ "$(tail -n 1 "$CLEARRA_TEST_LOG")" = "stop" ] || fail_contract
  [ "$(grep -c '^stop$' "$CLEARRA_TEST_LOG")" -ge 2 ] || fail_contract
  [ "$(readlink -f -- "$fixture_root/opt/clearra/current")" = "$prior_release" ] || fail_contract
  if [ "$scenario" = "settings-failure" ]; then
    ! grep -Fx 'start' "$CLEARRA_TEST_LOG" >/dev/null || fail_contract
    grep -Fx 'CURRENT=old' "$settings_path" >/dev/null || fail_contract
  else
    grep -Fx 'start' "$CLEARRA_TEST_LOG" >/dev/null || fail_contract
  fi
fi

printf '%s\n' "scenario=$scenario passed"
`;
