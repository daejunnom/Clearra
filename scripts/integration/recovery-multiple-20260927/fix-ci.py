"""Correct reviewed test/build boundaries, without changing main or deployment."""
import json
import os
import pathlib
import subprocess

BASE = '51084c78c3a06f355a352b4c013314c151c47709'
PARENT = '7384392f661fdc7b2c87d797e99466981296fb04'
BRANCH = 'codex/recovery-multiple-early-20260927'
PATHS = ['Cargo.lock', 'crates/clearra-cli-command/Cargo.toml',
         'crates/clearra-forward-search/src/boundary_recovery_multi_tests.rs',
         'packages/clearra-ui/src/lib/workspace/BoundaryRecoveryControls.svelte']

def git(*args):
    return subprocess.check_output(['git', *args], text=True, timeout=120).strip()

def refs():
    for ref, sha in [('refs/heads/main', BASE), ('refs/heads/' + BRANCH, PARENT)]:
        if git('ls-remote', 'origin', ref).split() != [sha, ref]:
            raise RuntimeError('Reviewed source moved; reconcile before applying')

refs()
git('checkout', '--detach', PARENT)
if git('rev-parse', 'HEAD^{tree}') != '17707b2863ec163f19f53e9d8baefeef4a6e759d':
    raise RuntimeError('Reviewed source tree mismatch')
path = pathlib.Path(PATHS[3])
text = path.read_text()
old = '<select value={request.maxEarlyPlacements} aria-invalid='
assert text.count(old) == 1
text = text.replace(old, "<select aria-label={label('recoveryMaxEarlyPlacements')} aria-description={label('recoveryEarlyCountHelp')}\n        value={request.maxEarlyPlacements} aria-invalid=")
path.write_text(text)
path = pathlib.Path(PATHS[1])
text = path.read_text()
assert text.count('\n[lib]\n') == 1
path.write_text(text.replace('\n[lib]\n', '\n[dev-dependencies]\nclearra-host-contract = { path = "../clearra-host-contract" }\n\n[lib]\n'))
path = pathlib.Path(PATHS[0])
text = path.read_text()
start = text.index('name = "clearra-cli-command"')
end = text.index('[[package]]', start)
package = text[start:end]
assert package.count(' "clearra-fumen",\n') == 1
package = package.replace(' "clearra-fumen",\n', ' "clearra-fumen",\n "clearra-host-contract",\n')
path.write_text(text[:start] + package + text[end:])
path = pathlib.Path(PATHS[2])
extra = '''
#[test]
fn early_supports_can_turn_an_isolated_b2b_break_into_a_real_tetris_save() {
    let mut query = tower(2);
    let i_vertical = (0..4).fold(0_u64, |value, row| value | (1_u64 << (10 * row)));
    let lower_o = 0xc03_u64 << 11;
    let upper_o = 0xc03_u64 << 31;
    query.height = 6;
    query.initial_board = mask(0x3fe | (1..4).fold(0, |value, row| value | (0x3f8_u64 << (10 * row))));
    query.stage_one_target = Board256Mask::EMPTY;
    query.final_board = mask(6);
    query.queue = vec![PieceKind::I, PieceKind::O, PieceKind::O, PieceKind::T];
    query.placement_role_masks = vec![mask(i_vertical), mask(lower_o), mask(upper_o)];
    query.hold_enabled = true;
    query.preserve_b2b_by_stage = [true, true];
    // An isolated first-stage I clears just one row and leaves garbage behind.
    // That old evidence must not prune the geometry of the combined replay.
    let (isolated, _, lines) = place_and_clear(10, query.height,
        ForwardBoard::from_mask(query.initial_board).union_for_height(ForwardBoard::from_mask(mask(i_vertical)), query.height));
    assert_eq!(lines, 1);
    assert!(!isolated.is_empty());
    let result = query.search(&ExecutionControl::default()).unwrap();
    assert_witness(&query, &result);
    assert_eq!(result.borrowed_stage_two_count, 2);
    assert!(result.steps.iter().all(|step| step.b2b_active_after));
    let late = result.steps.last().unwrap();
    assert_eq!(late.placement_role_index, 0);
    assert_eq!(late.cleared_lines, 4);
    assert!(late.b2b_active_after);
}
'''
path.write_text(path.read_text() + extra)
git('add', '--', *PATHS)
if git('write-tree') != '6d9f0dfc9a29c4fb85fa8f6da91deb6ea55dfe53':
    raise RuntimeError('Reviewed edited tree mismatch')
subprocess.run(['cargo', '+1.98.1', 'fmt', '--all'], check=True, timeout=120)
if git('diff', 'HEAD', '--name-only').splitlines() != PATHS:
    raise RuntimeError('Unexpected formatting scope')
git('add', '--', *PATHS)
git('diff', '--cached', '--check')
subprocess.run(['cargo', '+1.98.1', 'fmt', '--all', '--check'], check=True, timeout=120)
tree = git('write-tree')
git('config', 'user.name', 'github-actions[bot]')
git('config', 'user.email', '41898282+github-actions[bot]@users.noreply.github.com')
git('commit', '-m', 'fix: separate quota label from help and prove interleaved B2B saves')
candidate = git('rev-parse', 'HEAD')
refs()
git('push', 'origin', candidate + ':refs/heads/' + BRANCH)
if git('ls-remote', 'origin', 'refs/heads/' + BRANCH).split() != [candidate, 'refs/heads/' + BRANCH]:
    raise RuntimeError('Candidate readback mismatch')
out = pathlib.Path(os.environ['RUNNER_TEMP']) / 'multiple-recovery-source'
out.mkdir(exist_ok=True)
(out / 'complete.patch').write_text(git('diff', '--binary', BASE, candidate) + '\n')
receipt = {'source_commit': candidate, 'parent': PARENT, 'base': BASE, 'tree': tree,
           'branch': BRANCH, 'files': git('diff', '--name-only', BASE, candidate).splitlines(),
           'main_written': False, 'published': False}
(out / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt), flush=True)
with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
    output.write('candidate=' + candidate + '\ntree=' + tree + '\n')
