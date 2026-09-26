"""Keep the actual usize pattern domain, with a cancelled large-universe regression."""
import json
import os
import pathlib
import subprocess

BASE = '51084c78c3a06f355a352b4c013314c151c47709'
PARENT = 'e14adb0f4a0e59a8cfe72b49388092299a43689e'
BRANCH = 'codex/converge-recovery-auto-20260927'
PATH = 'crates/clearra-forward-search/src/boundary_recovery_population.rs'

def git(*args):
    return subprocess.check_output(['git', *args], text=True, timeout=120).strip()

def refs():
    for ref, sha in [('refs/heads/main', BASE), ('refs/heads/' + BRANCH, PARENT)]:
        if git('ls-remote', 'origin', ref).split() != [sha, ref]:
            raise RuntimeError('Reviewed source moved; reconcile instead of overwriting')

refs()
git('checkout', '--detach', PARENT)
if git('rev-parse', 'HEAD^{tree}') != 'a24ac8a8b02a7b9ecab84447ce40a3ac79554302':
    raise RuntimeError('Parent tree mismatch')
if git('hash-object', PATH) != '705fed29f3d63d2eed0dbbd883ec770e1e46fee4':
    raise RuntimeError('Source blob mismatch')
path = pathlib.Path(PATH)
text = path.read_text(encoding='utf-8')
old = 'if pattern_count == 0 || pattern_count > u32::MAX as usize {'
if text.count(old) != 1:
    raise RuntimeError('Expected exactly one redundant cutoff')
text = text.replace(old, 'if pattern_count == 0 {')
position = text.rfind('\n}')
extra = '''

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn native_pattern_indices_are_not_truncated_to_thirty_two_bits() {
        use clearra_supply::{
            pattern_universe::pattern_universe_materializer::PatternUniverseMaterializer,
            queue::queue_pattern_expression::QueuePatternExpression,
        };
        let expression = QueuePatternExpression::parse("P7P7P7", 0).unwrap();
        let universe = PatternUniverseMaterializer::queue_pattern_expression(&expression, 0).unwrap();
        assert_eq!(universe.pattern_count(), 128_024_064_000);
        let cancellation = ExecutionCancellationToken::new();
        cancellation.handle().cancel();
        // No huge evaluation is performed. The typed usize index domain must
        // reach cancellation rather than an unrelated legacy u32 guard.
        let result = search_boundary_recovery_population(
            &universe,
            &ExecutionControl::new(cancellation),
            BoundaryRecoveryPopulationLimits {
                max_pattern_evaluations: None,
                max_total_states: None,
            },
            |_, _| panic!("a cancelled search must not evaluate a pattern"),
        );
        assert_eq!(result.unwrap_err(), BoundaryRecoveryPopulationError::Cancelled);
    }
'''
path.write_text(text[:position] + extra + text[position:], encoding='utf-8')
git('add', '--', PATH)
if git('write-tree') != '12a401418cc788d60f37f867be5f3c18b744f548':
    raise RuntimeError('Reviewed edited tree mismatch')
subprocess.run(['cargo', '+1.98.1', 'fmt', '--all'], check=True, timeout=120)
if git('diff', 'HEAD', '--name-only').splitlines() != [PATH]:
    raise RuntimeError('Unexpected formatting scope')
git('add', '--', PATH)
git('diff', '--cached', '--check')
subprocess.run(['cargo', '+1.98.1', 'fmt', '--all', '--check'], check=True, timeout=120)
tree = git('write-tree')
git('config', 'user.name', 'github-actions[bot]')
git('config', 'user.email', '41898282+github-actions[bot]@users.noreply.github.com')
git('commit', '-m', 'fix: honor native pattern index width without an unrelated recovery cutoff')
candidate = git('rev-parse', 'HEAD')
refs()
git('push', 'origin', candidate + ':refs/heads/' + BRANCH)
if git('ls-remote', 'origin', 'refs/heads/' + BRANCH).split() != [candidate, 'refs/heads/' + BRANCH]:
    raise RuntimeError('Candidate readback mismatch')
out = pathlib.Path(os.environ['RUNNER_TEMP']) / 'automatic-recovery-source'
out.mkdir()
(out / 'change.patch').write_text(git('diff', '--binary', '2495578ebd276170fd3450727176be090c39f30e', candidate) + '\n')
receipt = {'source_commit': candidate, 'parent': PARENT, 'base': BASE, 'tree': tree,
           'branch': BRANCH, 'files': git('diff', '--name-only', '2495578ebd276170fd3450727176be090c39f30e', candidate).splitlines(),
           'main_written': False, 'published': False,
           'scope': 'existing declared-role search; not the two-Build-catalog cross-stage pipeline'}
(out / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt), flush=True)
with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
    output.write('candidate=' + candidate + '\ntree=' + tree + '\n')
