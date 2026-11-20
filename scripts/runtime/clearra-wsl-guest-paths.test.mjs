import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

// Exercise the real helpers with no WSL, systemd, builds, or host writes.
const guest = readFileSync(new URL('./clearra-wsl-guest.sh', import.meta.url), 'utf8')
  .replaceAll('\r\n', '\n');
const bash = process.platform === 'win32'
  ? join(process.env.ProgramFiles || 'C:/Program Files', 'Git', 'bin', 'bash.exe')
  : 'bash';

function helper(name) {
  const start = guest.indexOf(`\n${name}() {\n`) + 1;
  assert.ok(start > 0, `missing Bash helper: ${name}`);
  const end = guest.indexOf('\n}', start);
  assert.ok(end > start, `invalid Bash helper: ${name}`);
  return guest.slice(start, end + 2);
}

function check(t, code) {
  const result = spawnSync(bash, ['--noprofile', '--norc', '-s'], {
    input: code, encoding: 'utf8', windowsHide: true,
  });
  if (result.error?.code === 'ENOENT') {
    t.skip('a native Bash is needed for the no-WSL path mocks');
    return;
  }
  assert.ifError(result.error);
  assert.equal(result.status, 0, result.stderr || result.stdout);
}

test('guest archive digest must describe the copied bytes, including on mismatch', t => {
  check(t, `
set -euo pipefail
${helper('copy_verified_source_archive')}
copied=0
cp() {
    [[ "$*" == '-- /mnt/c/host.tar.gz /guest/private.tar.gz' ]] || return 19
    copied=1
}
sha256sum() {
    [[ "$copied" == 1 && "$*" == '-- /guest/private.tar.gz' ]] || return 23
    printf '%s  /guest/private.tar.gz\\n' verified
}
copy_verified_source_archive /mnt/c/host.tar.gz /guest/private.tar.gz verified
if copy_verified_source_archive /mnt/c/host.tar.gz /guest/private.tar.gz invalid; then
    exit 37
fi
cp() { return 21; }
if copy_verified_source_archive /mnt/c/host.tar.gz /guest/private.tar.gz verified; then
    exit 39
fi
`);
});

test('guest executes only with bounded and matching kernel controllers', t => {
  check(t, `
set -euo pipefail
${helper('finite_cgroup_limits_match')}
finite_cgroup_limits_match 536870912 0 128 536870912 128
finite_cgroup_limits_match 536866816 0 128 536870912 128
if finite_cgroup_limits_match max 0 128 536870912 128; then exit 31; fi
if finite_cgroup_limits_match 536870912 max 128 536870912 128; then exit 32; fi
if finite_cgroup_limits_match 536870912 0 max 536870912 128; then exit 33; fi
if finite_cgroup_limits_match 536870912 0 64 536870912 128; then exit 34; fi
if finite_cgroup_limits_match 536875008 0 128 536870912 128; then exit 35; fi
if finite_cgroup_limits_match 534773760 0 128 536870912 128; then exit 36; fi
`);
});

test('guest benchmark output rejects occupied or noncanonical destinations', t => {
  check(t, `
set -euo pipefail
${helper('require_fresh_or_empty_directory')}
${helper('require_empty_benchmark_output')}
if require_fresh_or_empty_directory /; then exit 42; fi
if require_fresh_or_empty_directory /bin/bash; then exit 43; fi
if require_empty_benchmark_output /unexpected/output; then exit 44; fi
if require_empty_benchmark_output /mnt/c/../c/unexpected; then exit 45; fi
require_empty_benchmark_output /mnt/z/clearra-test-missing-directory-98df0aa8
`);
});

test('sync receipt counts archive members, not the lines or bytes in their paths', t => {
  const countProgram = helper('count_source_archive_files').split('\n').slice(2, -2).join('\n');
  const result = spawnSync(process.platform === 'win32' ? 'python' : 'python3', ['-'], {
    encoding: 'utf8',
    input: `
__name__ = "source_receipt_fixture"
${countProgram}

from io import BytesIO

def archived(members):
    result = BytesIO()
    with tarfile.open(fileobj=result, mode="w:gz") as archive:
        for name, kind in members:
            entry = tarfile.TarInfo(name)
            entry.type = kind
            archive.addfile(entry)
    result.seek(0)
    return result

files = ["space in.c", "line\\nbreak.rs", "x" * 120 + ".c"]
assert count_files(archived((path, tarfile.REGTYPE) for path in files)) == 3
invalid_members = [
    [],
    [("same.c", tarfile.REGTYPE), ("./same.c", tarfile.REGTYPE)],
    [("/outside.c", tarfile.REGTYPE)],
    [("path/../outside.c", tarfile.REGTYPE)],
    [("directory", tarfile.DIRTYPE)],
    [("link.c", tarfile.SYMTYPE)],
]
for members in invalid_members:
    try:
        count_files(archived(members))
    except ValueError:
        pass
    else:
        raise AssertionError("invalid archive was counted")
`,
    windowsHide: true,
  });
  if (result.error?.code === 'ENOENT') {
    t.skip('Python is needed to run the guest archive metadata parser');
    return;
  }
  assert.ifError(result.error);
  assert.equal(result.status, 0, result.stderr || result.stdout);
});
