#!/usr/bin/env python3
"""Source-bound reconciliation; no publication or history rewrites.
All deletions require ancestry proof and an exact old-OID lease. The one-shot
runner is excluded from the product tree; its history is retained for audit.
"""
from __future__ import annotations
import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

REPOSITORY = 'daejunnom/Clearra'
EXPECTED_MAIN = '3e06bce344da89212b002d0cba96063703088074'
MAINTENANCE = 'codex/release-integrate-pinned-boundary-20260925'
CANDIDATE = 'codex/candidate-pinned-boundary-release-20260925'
PINNED = 'codex/pinned-minimals-boundary-recovery-20260923'
EXPECTED_PINNED = '980806508bd88ab436023d08023cd306fb1904b8'
RELEASE_GROUPS = {
    'parallel-workers': [
        'pc-all-worker-scheduling-20260912',
        'v0.8.1-parallelism-range-20260912',
        'v0.8.1-parallel-abba-20260920',
        'v0.8.1-worker-tail-audit-20260912',
    ],
    'gui-output-replay-i18n': [
        'japanese-i18n-prep', 'japanese-translation-20260910',
        'next-gui-render-watchdog', 'pc-replay-dp-20260906',
        'v0.8.1-copy-contract-20260912', 'v0.8.1-minimum-lazy-contract-20260912',
        'v0.8.1-completion-audit-20260912', 'v0.8.1-product-gap-audit-20260912',
        'v0.8.1-product-parity-round2-20260912',
        'v081-product-parity-closure-20260912',
    ],
    'ci-build-release': [
        'actions-tail-parallelism', 'ci-independent-failure-collection-next',
        'deploy-approval-flow-20260907', 'fix-pages-bootstrap',
        'fix-release-publish', 'gate-independent-failure-collection',
        'next-fast-deploy', 'nonbuild-release-tail', 'pages-legacy-contract-fix',
        'release-build-architecture-ab', 'release-pages',
        'release-rust-shard-contention', 'release-rust-shard-contention-ci',
        'release-rust-shard-contention-ci-v2', 'release-rust-shard-contention-ci-v3',
        'v0.8.1-strict-clippy-20260912',
    ],
}
RETAIN = {
    'codex/converge-v081-v090-release-20260923': 'accelerator/product exposure needs tests and is not authorized for this release',
    'codex/converge-v081-v090-performance-20260921': 'ABBA did not establish a product performance gain; retain research',
    'codex/converge-legal-board-cleanup-20260923': 'non-shipping management change; FF-only and isolation required',
    'codex/preserved-converge-clearra-management-20260921-20260922-91097f35': 'preserved management history; no automatic promotion',
    'codex/v0.9.0-stacked-on-v0.8.1-20260912': 'development PC4 preservation; no product activation',
}
EXPERIMENT_WORDS = ('legal-board', 'legal_board', 'conditioned-reach', 'conditioned_reach',
                    'pc4', 'tablebase', 'lagrangian', 'findminhs', 'column-mod-four',
                    'column_mod_four')
CORE_ALGORITHM_PREFIXES = (
    'core-c/src/', 'core-c/include/', 'crates/clearra-coverage/src/cover/',
    'crates/clearra-core-executor/src/backend/wasm_cpu/',
    'crates/clearra-build-coverage/src/', 'crates/clearra-geometry/src/',
    'crates/clearra-forward-search/src/',
)
OWNED_PATHS = {
    '.github/workflows/user-branch-reconciliation-20260925.yml',
    'scripts/maintenance/user-branch-reconciliation-20260925.py',
}


def git(*args: str, check: bool = True, timeout: int = 180) -> subprocess.CompletedProcess:
    p = subprocess.run(['git', *args], text=True, capture_output=True, timeout=timeout)
    if check and p.returncode:
        raise RuntimeError(f'git {args[0]} failed ({p.returncode}): {p.stderr[:1200]}')
    return p


def ancestor(a: str, b: str) -> bool:
    p = git('merge-base', '--is-ancestor', a, b, check=False)
    if p.returncode not in (0, 1):
        raise RuntimeError('ancestry lookup failed')
    return p.returncode == 0


def remote_head(branch: str) -> str | None:
    result = git('ls-remote', '--heads', 'origin', f'refs/heads/{branch}').stdout.splitlines()
    if not result:
        return None
    if len(result) != 1:
        raise RuntimeError('ambiguous ref')
    oid, ref = result[0].split('\t')
    if ref != f'refs/heads/{branch}' or not re.fullmatch('[0-9a-f]{40}', oid):
        raise RuntimeError('invalid remote ref')
    return oid


def snapshot() -> dict[str, str]:
    git('fetch', '--no-tags', 'origin', '+refs/heads/*:refs/remotes/origin/*', timeout=1200)
    result = {}
    for line in git('for-each-ref', '--format=%(refname)\t%(objectname)', 'refs/remotes/origin/').stdout.splitlines():
        name, oid = line.split('\t')
        name = name.removeprefix('refs/remotes/origin/')
        if name != 'HEAD' and re.fullmatch('[0-9a-f]{40}', oid):
            result[name] = oid
    return result


def write_report(root: Path, report: dict) -> None:
    root.mkdir(parents=True, exist_ok=True)
    (root / 'reconciliation.json').write_text(json.dumps(report, indent=2, ensure_ascii=False) + '\n')
    summary = ['# Clearra branch reconciliation', '', f"Main snapshot: `{report.get('main')}`", '',
               'The candidate is not promoted by the preparation job.', '',
               '| Branch | Result | Reason |', '|---|---|---|']
    for item in report.get('integration', []):
        reason = str(item.get('reason', '')).replace('|', '/').replace('\n', ' ')
        summary.append(f"| `{item['branch']}` | {item['status']} | {reason} |")
    summary += ['', '| Branch | Cleanup |', '|---|---|']
    for item in report.get('cleanup', []):
        summary.append(f"| `{item['branch']}` | {item['status']} |")
    (root / 'reconciliation.md').write_text('\n'.join(summary) + '\n')


def suspicious_paths(current: str, tree: str, group: str) -> list[str]:
    paths = git('diff', '--name-only', current, tree).stdout.splitlines()
    blocked = []
    for path in paths:
        lower = path.lower()
        if path.startswith(('docs/research/', 'docs/history/')):
            continue
        if path.startswith(('_local/', 'build/', 'dist/')) or lower.endswith(('.bin', '.wasm')):
            blocked.append(path)
        elif any(word in lower for word in EXPERIMENT_WORDS):
            blocked.append(path)
        elif path.startswith(CORE_ALGORITHM_PREFIXES):
            if group != 'parallel-workers' or not re.search(r'(worker|schedul|parallel|thread)', lower):
                blocked.append(path)
    patch = git('diff', '--unified=0', current, tree).stdout
    active_path = ''
    needles = ('legalboard', 'legal_board', 'legal-board', 'conditionedreachability',
               'conditioned_reach', 'conditioned-reach', 'pc4', 'tablebase',
               'lagrangian', 'findminhs', 'column_mod_four', 'column-mod-four')
    for line in patch.splitlines():
        if line.startswith('+++ b/'):
            active_path = line[6:]
        elif line.startswith('+') and not line.startswith('+++'):
            if active_path.startswith(('docs/research/', 'docs/history/')):
                continue
            if any(word in line.lower() for word in needles):
                blocked.append(active_path)
    return sorted(set(blocked))


def resolve_reviewed_dependency_conflict(current: str, head: str, output: str) -> str | None:
    """Resolve only the reviewed ^5.9.3 vs 5.9.3 declaration, not code conflicts."""
    if current != EXPECTED_MAIN or head != EXPECTED_PINNED:
        return None
    paths = {line.split('\t', 1)[1] for line in output.splitlines()
             if re.match(r'^100644 [0-9a-f]{40} [123]\t', line)}
    expected = {'apps/clearra-web/package.json', 'pnpm-lock.yaml'}
    if paths != expected:
        return None
    package_path = 'apps/clearra-web/package.json'
    ours = json.loads(git('show', current + ':' + package_path).stdout)
    theirs = json.loads(git('show', head + ':' + package_path).stdout)
    if ours.get('devDependencies', {}).get('typescript') != '^5.9.3':
        return None
    if theirs.get('devDependencies', {}).get('typescript') != '5.9.3':
        return None
    theirs['devDependencies']['typescript'] = '^5.9.3'
    if ours != theirs:
        return None
    ours_lock = git('show', current + ':pnpm-lock.yaml').stdout
    theirs_lock = git('show', head + ':pnpm-lock.yaml').stdout
    old = '      typescript:\n        specifier: 5.9.3\n        version: 5.9.3\n'
    new = '      typescript:\n        specifier: ^5.9.3\n        version: 5.9.3\n'
    if theirs_lock.count(old) != 1 or theirs_lock.replace(old, new, 1) != ours_lock:
        return None
    tree = output.splitlines()[0]
    if not re.fullmatch('[0-9a-f]{40}', tree):
        return None
    temp_root = Path('build/branch-reconciliation')
    temp_root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='reviewed-merge-', dir=temp_root) as temp:
        env = dict(os.environ, GIT_INDEX_FILE=str((Path(temp) / 'index').resolve()))
        def index_git(*args):
            result = subprocess.run(['git', *args], text=True, capture_output=True, env=env, timeout=60)
            if result.returncode:
                raise RuntimeError('reviewed dependency merge index operation failed')
            return result.stdout.strip()
        index_git('read-tree', tree)
        for path in sorted(expected):
            blob = git('rev-parse', current + ':' + path).stdout.strip()
            index_git('update-index', '--add', '--cacheinfo', '100644,' + blob + ',' + path)
        return index_git('write-tree')


def merge_candidate(current: str, head: str, branch: str, group: str) -> tuple[str, dict]:
    row = {'branch': branch, 'head': head, 'group': group}
    if ancestor(head, current):
        return current, dict(row, status='already_in_candidate')
    p = git('merge-tree', '--write-tree', current, head, check=False, timeout=600)
    lines = p.stdout.splitlines()
    if p.returncode != 0:
        reviewed = resolve_reviewed_dependency_conflict(current, head, p.stdout) if group == 'pinned-and-boundary' else None
        if reviewed is None:
            return current, dict(row, status='retained_conflict', reason='clean merge unavailable', conflict_report=p.stdout[-20000:])
        tree = reviewed
        row['reviewed_resolution'] = 'Keep main TypeScript ^5.9.3 declaration and the identical frozen 5.9.3 dependency graph'
    else:
        if not lines or not re.fullmatch('[0-9a-f]{40}', lines[0]):
            raise RuntimeError('merge-tree did not produce a valid tree')
        tree = lines[0]
    if group != 'pinned-and-boundary':
        blocked = suspicious_paths(current, tree, group)
        if blocked:
            return current, dict(row, status='retained_scope_review', reason='net merge modifies protected algorithm/research/asset paths', blocked_paths=blocked)
    message = f"Merge {branch} for the user-approved release integration\n\nPreserve both histories. Research activation is excluded. Any dependency conflict resolution is recorded in the source-bound audit."
    new = git('commit-tree', tree, '-p', current, '-p', head, '-m', message).stdout.strip()
    return new, dict(row, status='integrated_candidate', merge_commit=new,
                     changed_files=git('diff', '--name-only', current, tree).stdout.splitlines())


def delete_merged(snapshot_refs: dict[str, str], main: str, report: dict, root: Path, *, include_owned: bool = False) -> None:
    for branch, sha in sorted(snapshot_refs.items()):
        if not branch.startswith('codex/') or (not include_owned and branch in (MAINTENANCE, CANDIDATE)):
            continue
        if not ancestor(sha, main):
            continue
        item = {'branch': branch, 'expected_head': sha, 'main_proof': main}
        try:
            observed = remote_head(branch)
            if observed is None:
                item['status'] = 'already_absent'
            elif observed != sha:
                item['status'] = 'kept_head_changed'
            else:
                result = git('push', '--porcelain', f'--force-with-lease=refs/heads/{branch}:{sha}',
                             'origin', f':refs/heads/{branch}', check=False)
                if result.returncode:
                    item.update(status='delete_rejected', reason=result.stderr[-1500:])
                elif remote_head(branch) is None:
                    item['status'] = 'deleted_verified'
                else:
                    item['status'] = 'delete_not_verified'
        except Exception as exc:
            item.update(status='delete_error', reason=str(exc))
        report.setdefault('cleanup', []).append(item)
        write_report(root, report)


def prepare(root: Path) -> None:
    if os.environ.get('GITHUB_REPOSITORY') != REPOSITORY:
        raise RuntimeError('repository boundary mismatch')
    if os.environ.get('GITHUB_REF') != 'refs/heads/' + MAINTENANCE:
        raise RuntimeError('maintenance branch boundary mismatch')
    refs = snapshot()
    main = refs.get('main')
    if main != EXPECTED_MAIN or remote_head('main') != EXPECTED_MAIN:
        raise RuntimeError('main advanced; stop instead of reconciling a stale plan')
    if refs.get(PINNED) != EXPECTED_PINNED:
        raise RuntimeError('pinned source advanced; source review required')
    git('config', 'user.name', 'github-actions[bot]')
    git('config', 'user.email', '41898282+github-actions[bot]@users.noreply.github.com')
    report = {'schema': 'clearra.branch-reconciliation.v1',
              'timestamp': datetime.now(timezone.utc).isoformat(), 'main': main,
              'snapshot': refs, 'integration': [], 'cleanup': [], 'retained': []}
    current, result = merge_candidate(main, EXPECTED_PINNED, PINNED, 'pinned-and-boundary')
    report['integration'].append(result)
    write_report(root, report)
    if result['status'] not in ('integrated_candidate', 'already_in_candidate'):
        delete_merged(refs, main, report, root)
        raise RuntimeError('pinned/boundary merge requires conflict resolution; no candidate published')
    for group, names in RELEASE_GROUPS.items():
        for suffix in names:
            branch = 'codex/' + suffix
            if branch not in refs:
                report['integration'].append({'branch': branch, 'status': 'absent', 'group': group})
                continue
            current, item = merge_candidate(current, refs[branch], branch, group)
            report['integration'].append(item)
            write_report(root, report)
    selected = {PINNED, MAINTENANCE, CANDIDATE, *('codex/' + n for names in RELEASE_GROUPS.values() for n in names)}
    for branch, sha in sorted(refs.items()):
        if branch == 'main' or branch in selected or ancestor(sha, current):
            continue
        report['retained'].append({'branch': branch, 'head': sha,
                                  'reason': RETAIN.get(branch, 'not in the exact release allowlist; research/development or classification review'),
                                  'ff_from_main': ancestor(main, sha)})
    maintenance_sha = os.environ['GITHUB_SHA']
    if not ancestor(main, maintenance_sha):
        raise RuntimeError('maintenance is not descended from the frozen main')
    if set(git('diff', '--name-only', main, maintenance_sha).stdout.splitlines()) != OWNED_PATHS:
        raise RuntimeError('unexpected user changes on the maintenance ref')
    product_tree = git('rev-parse', current + '^{tree}').stdout.strip()
    current = git('commit-tree', product_tree, '-p', current, '-p', maintenance_sha, '-m',
                  'Record branch-reconciliation provenance without shipping its one-shot runner').stdout.strip()
    if remote_head('main') != main:
        raise RuntimeError('main changed during preparation; do not publish candidate')
    if remote_head(CANDIDATE) is not None:
        raise RuntimeError('candidate branch already exists; never overwrite it')
    git('push', 'origin', f'{current}:refs/heads/{CANDIDATE}')
    if remote_head(CANDIDATE) != current:
        raise RuntimeError('candidate push readback mismatch')
    report.update(candidate_branch=CANDIDATE, candidate_sha=current,
                  candidate_tree=git('rev-parse', current + '^{tree}').stdout.strip())
    for path in OWNED_PATHS:
        if git('cat-file', '-e', current + ':' + path, check=False).returncode == 0:
            raise RuntimeError('maintenance code leaked into release candidate')
    report['maintenance_excluded_from_candidate'] = True
    write_report(root, report)
    if os.environ.get('GITHUB_OUTPUT'):
        with open(os.environ['GITHUB_OUTPUT'], 'a') as f:
            f.write(f'candidate_sha={current}\ncandidate_branch={CANDIDATE}\nmain_snapshot={main}\n')
    delete_merged(refs, main, report, root)
    report['complete'] = True
    write_report(root, report)
    print(json.dumps({'main': main, 'candidate': current,
                      'integrated': sum(x['status'] == 'integrated_candidate' for x in report['integration']),
                      'retained_release': sum(x['status'].startswith('retained_') for x in report['integration']),
                      'deleted': sum(x['status'] == 'deleted_verified' for x in report['cleanup'])}))


def finalize(root: Path, candidate: str) -> None:
    if os.environ.get('GITHUB_REPOSITORY') != REPOSITORY:
        raise RuntimeError('repository boundary mismatch')
    if os.environ.get('GITHUB_REF') != 'refs/heads/' + MAINTENANCE:
        raise RuntimeError('workflow source mismatch')
    report = json.loads((root / 'reconciliation.json').read_text())
    if report['candidate_sha'] != candidate or not re.fullmatch('[0-9a-f]{40}', candidate):
        raise RuntimeError('candidate identity mismatch')
    snapshot()
    if remote_head('main') != candidate:
        raise RuntimeError('main is not the successfully gated candidate')
    refs = dict(report['snapshot'])
    refs[CANDIDATE] = candidate
    report['promoted_main'] = candidate
    delete_merged(refs, candidate, report, root, include_owned=True)
    report['cleanup_finalized'] = True
    write_report(root, report)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--phase', choices=['prepare', 'finalize'], default='prepare')
    parser.add_argument('--candidate-sha')
    parser.add_argument('--report-dir', required=True)
    args = parser.parse_args()
    try:
        if args.phase == 'prepare':
            prepare(Path(args.report_dir))
        else:
            finalize(Path(args.report_dir), args.candidate_sha or '')
    except Exception as exc:
        print(f'RECONCILIATION STOPPED: {exc}', file=sys.stderr)
        sys.exit(1)
