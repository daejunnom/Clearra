import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const paths = [
  '../build-core-c.sh',
  './wsl-native-cargo.sh',
  './wsl-core-c-tests.sh',
  './package-release-cli.sh',
];
const sources = new Map(await Promise.all(paths.map(async path => [
  path, (await readFile(new URL(path, import.meta.url), 'utf8')).replaceAll('\r\n', '\n'),
])));

for (const [path, source] of sources) {
  test(`${path} enters one source-bound build owner before any build output`, () => {
    assert.match(source, /if \[\[ -z "\$\{CLEARRA_BUILD_SESSION_ID:-\}" \]\]; then/u);
    assert.match(source, /exec node "\$(?:ROOT|ROOT_DIR|AUTHORITY_ROOT)\/scripts\/tools\/invoke-clearra-build\.mjs"/u);
    assert.match(source, /--source-root "\$(?:ROOT|ROOT_DIR)" --purpose/u);
    assert.match(source, /clearra-build-paths\.mjs" --source-root "\$(?:ROOT|ROOT_DIR)" --field transaction/u);
    assert.match(source, /clearra-build-paths\.mjs" --source-root "\$(?:ROOT|ROOT_DIR)" --field cargo-target/u);
    assert.match(source, /export CARGO_TARGET_DIR="\$MANAGED_CARGO_TARGET"/u);
    const validation = source.indexOf(' --field transaction)');
    assert.ok(validation >= 0, 'transaction validator must be present');
    const output = source.search(/^(?:mkdir -p|cmake -S|cargo build|[ \t]*gcc "\$)/mu);
    assert.ok(output > validation, 'build output must follow marker/lease validation');
    assert.doesNotMatch(source, /XDG_CACHE_HOME|RUNNER_TEMP|\/tmp\/clearra-native-c-core|DEFAULT_BUILD_ROOT/u);
  });

  test(`${path} rejects independent roots and leaves generation retirement to its owner`, () => {
    for (const key of ['CLEARRA_WSL_NATIVE_BUILD_ROOT', 'CLEARRA_CORE_C_BUILD_DIR', 'CLEARRA_RELEASE_BUILD_ROOT']) {
      assert.ok(source.indexOf(key) < source.indexOf('exec node'), `${key} must be rejected before a new owner can create output`);
    }
    assert.match(source, /CLEARRA_WSL_CARGO_TARGET_DIR/u);
    assert.doesNotMatch(source, /\brm\s+(?:-[a-zA-Z]+\s+)*|CACHE_LAYOUT_MARKER|LEGACY_.*BUILD_ROOT/u);
  });
}

test('manual C/WSL entries default to experiment while release packaging defaults to product', () => {
  for (const path of paths.slice(0, 3)) assert.match(sources.get(path), /CLEARRA_BUILD_PURPOSE:-experiment/u);
  const release = sources.get('./package-release-cli.sh');
  assert.match(release, /CLEARRA_BUILD_PURPOSE:-product/u);
  assert.match(release, /OUTPUT_DIR="\$\(realpath -m -- "\$OUTPUT_DIR"\)"/u);
  assert.match(release, /-- bash .*package-release-cli\.sh" "\$OUTPUT_DIR" "\$VERSION" "\$TARGET_TRIPLE"/u);
  assert.match(release, /RELEASE_BINARY="\$OUTPUT_DIR\/Clearra-CLI-v/u);
  assert.match(release, /BINARY="\$CARGO_TARGET_DIR\/release\/clearra"/u);
});

test('WSL keeps Linux source requirements but no separate ext4 build cache', () => {
  for (const path of ['./wsl-native-cargo.sh', './wsl-core-c-tests.sh']) {
    const source = sources.get(path);
    assert.match(source, /9p \| v9fs \| drvfs \| fuseblk/u);
    assert.match(source, /\/mnt\/\*/u);
    assert.match(source, /BUILD_ROOT="\$BUILD_TRANSACTION\/core-c-/u);
    assert.match(source, /ROOT="\$\{CLEARRA_WSL_WORKSPACE:-\$AUTHORITY_ROOT\}"/u);
    assert.match(source, /node "\$AUTHORITY_ROOT\/scripts\/tools\/clearra-build-paths\.mjs" --source-root "\$ROOT"/u);
    assert.match(source, /-- bash "\$AUTHORITY_ROOT\/scripts\/tools\/wsl-/u);
    assert.doesNotMatch(source, /\$HOME\/\.cache|native-c-core-variants\/|\$CACHE_ROOT/u);
  }
});

test('native Cargo rejects output and config switches before entering the build owner', () => {
  const source = sources.get('./wsl-native-cargo.sh');
  for (const argument of ['--target-dir', '--build-dir', '--artifact-dir', '--out-dir', '--config']) {
    assert.ok(source.indexOf(`${argument}|${argument}=*`) < source.indexOf('exec node'), argument);
  }
});

test('WSL runtime batch consumes only the active owner exact benchmark binary', async () => {
  const source = await readFile(new URL('./wsl-pc-runtime-batch.sh', import.meta.url), 'utf8');
  assert.match(source, /--source-root "\$SOURCE_ROOT" --field transaction/u);
  assert.match(source, /--source-root "\$SOURCE_ROOT" --field cargo-target/u);
  assert.match(source, /EXPECTED_BINARY="\$MANAGED_CARGO_TARGET\/release\/clearra-pc-artifact"/u);
  assert.match(source, /\[\[ "\$BINARY" == "\$EXPECTED_BINARY" \]\]/u);
  assert.match(source, /p\.assertNoBuildLinks\(process\.argv\[2\]\)/u);
  assert.doesNotMatch(source, /\/home\/\*\/\.cache\/Clearra\/build|invoke-clearra-build|cargo build/u);
});

test('standalone NoPrepare batch refuses before artifact execution or report mutation', async () => {
  const source = await readFile(new URL('./wsl-pc-runtime-batch.sh', import.meta.url), 'utf8');
  assert.match(source, /\[\[ -n "\$\{CLEARRA_BUILD_SESSION_ID:-\}" \]\]/u);
  assert.match(source, /Standalone\/NoPrepare execution cannot borrow a cached binary/u);
  const ownerCheck = source.indexOf('--field transaction)');
  assert.ok(ownerCheck > 0);
  assert.ok(ownerCheck < source.indexOf('rm -rf -- "$REPORT_ROOT"'));
  assert.ok(ownerCheck < source.indexOf('"$BINARY" --list-gpu-devices'));
});
