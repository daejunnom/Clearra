import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import test from 'node:test';

const guest = readFileSync(new URL('./clearra-wsl-guest.sh', import.meta.url), 'utf8')
  .replaceAll('\r\n', '\n');
const prototype = /(?:^|\n)stable_source_slot_plan\(\) \{\n    python3 - <<'PY'\n([\s\S]*?)\nPY\n\}/u
  .exec(guest);
assert.ok(prototype, 'guest source-slot transaction prototype is available');

test('the stable slot is not reachable from existing guest entrypoints', () => {
  assert.equal(guest.match(/\bstable_source_slot_plan\b/gu)?.length, 1);
});

test('stable slot planning binds verified bytes, atomic outcomes, and the exact retirement target', t => {
  const processResult = spawnSync(process.platform === 'win32' ? 'python' : 'python3', ['-'], {
    encoding: 'utf8',
    input: `
${prototype[1]}

lease = StableSourceLease("c" * 24, "1" * 32, 17, True)
archive_a, archive_b = "a" * 64, "b" * 64
material_a, material_b = "1" * 64, "2" * 64


def tree(name, inode, archive, material, creator=lease.session, checkout=lease.checkout):
    marker = f"clearra.wsl-source-owner.v1\\t{checkout}\\t{archive}\\t{material}\\t{creator}\\n"
    return StableSourceTree(name, lease.device, inode, marker, material, True)


def reject(action):
    try:
        action()
    except ValueError:
        return
    raise AssertionError("an unbound source operation was accepted")


incoming = tree(lease.incoming, 200, archive_a, material_a)
old = tree("source", 100, archive_b, material_b, creator="2" * 32)

# Empty slots use no-replace. A failed publication is not authority to delete.
new = plan_install(lease, archive_a, incoming, None)
assert new.action == "rename_noreplace"
new_current = replace(incoming, name="source")
assert retirement_target(new, lease, new_current, None) is None
reject(lambda: retirement_target(new, lease, None, incoming))

# A changed archive/version requires one physical exchange. Only the old
# owned inode now at the exact incoming sibling may be retired.
update = plan_install(lease, archive_a, incoming, old)
assert update.action == "rename_exchange"
old_incoming = replace(old, name=lease.incoming)
assert retirement_target(update, lease, new_current, old_incoming) == old_incoming
reject(lambda: retirement_target(update, lease, old, incoming))
reject(lambda: retirement_target(update, lease, new_current, old))
reject(lambda: retirement_target(update, lease, new_current, replace(old_incoming, inode=111)))
reject(lambda: retirement_target(update, lease, new_current, replace(old_incoming, marker="")))
reject(lambda: retirement_target(update, replace(lease, session="9" * 32), new_current, old_incoming))
reject(lambda: retirement_target(update, replace(lease, exclusive_and_readers_drained=False),
                                 new_current, old_incoming))

# An identical, independently verified material tree keeps the physical
# source path and inode. A copied marker is insufficient evidence of reuse.
clean = tree("source", 101, archive_a, material_a, creator="3" * 32)
reuse = plan_install(lease, archive_a, incoming, clean)
assert reuse.action == "reuse"
assert retirement_target(reuse, lease, clean, incoming) == incoming
reject(lambda: plan_install(lease, archive_a, incoming, replace(clean, material_sha256=material_b)))
incompatible = tree("source", 101, archive_a, material_b, creator="3" * 32)
reject(lambda: plan_install(lease, archive_a, incoming, incompatible))
reject(lambda: retirement_target(reuse, lease, new_current, incoming))

# Old per-digest paths, bogus archive provenance, linked/non-owned trees,
# foreign creators and unrelated device/file identities stay untouched.
reject(lambda: plan_install(replace(lease, checkout=archive_a), archive_a, incoming, None))
reject(lambda: plan_install(lease, archive_b, incoming, None))
reject(lambda: plan_install(lease, archive_a, incoming, replace(old, owned_directory=False)))
reject(lambda: plan_install(lease, archive_a, incoming, replace(old, device=18)))
reject(lambda: plan_install(lease, archive_a, incoming, replace(old, inode=incoming.inode)))
reject(lambda: plan_install(lease, archive_a, incoming, replace(old, marker=old.marker + old.marker)))
foreign = tree(lease.incoming, 200, archive_a, material_a, creator="2" * 32)
reject(lambda: plan_install(lease, archive_a, foreign, old))
`,
    windowsHide: true,
  });
  if (processResult.error?.code === 'ENOENT') {
    t.skip('Python is needed to exercise the source slot model without WSL or a filesystem fixture');
    return;
  }
  assert.ifError(processResult.error);
  assert.equal(processResult.status, 0, processResult.stderr || processResult.stdout);
});
