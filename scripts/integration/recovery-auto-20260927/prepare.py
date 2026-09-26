"""Apply reviewed source deltas, pinned formatting, and a non-force candidate push."""
import hashlib
import json
import os
import pathlib
import subprocess

BASE = '51084c78c3a06f355a352b4c013314c151c47709'
PARENT = '2495578ebd276170fd3450727176be090c39f30e'
PARENT_TREE = '87c3e14116f18474780511dda552cf1d9f7f7039'
EDITED_TREE = 'c0ecc4a91df6c0ad1c85803a2604b81c281d2295'
BRANCH = 'codex/converge-recovery-auto-20260927'
ROOT = pathlib.Path('scripts/integration/recovery-auto-20260927')
ASSETS = {'core-edits.json': '6843eb4fa3ec941a5a48c60d8be3d0b3f645c93deebf972be6c75e2fbb75c558',
          'surfaces-edits.json': '4db9e1892dce31ceab7e7e42a728c67600c078ab9910dcfdd5e40a284f77c5fa'}

def git(*args):
    return subprocess.check_output(['git', *args], text=True, timeout=120).strip()

def require_refs():
    for ref, sha in [('refs/heads/main', BASE), ('refs/heads/' + BRANCH, PARENT)]:
        if git('ls-remote', 'origin', ref).split() != [sha, ref]:
            raise RuntimeError('Reviewed source moved; reconcile before applying: ' + ref)

def blob(content):
    return hashlib.sha1(b'blob ' + str(len(content)).encode() + b'\0' + content).hexdigest()

edits = []
for name, expected in ASSETS.items():
    data = (ROOT / name).read_bytes()
    if hashlib.sha256(data).hexdigest() != expected:
        raise RuntimeError('Reviewed asset checksum mismatch: ' + name)
    edits.extend(json.loads(data))
names = sorted(edit['path'] for edit in edits)
if len(names) != 28 or len(set(names)) != len(names):
    raise RuntimeError('Reviewed file inventory mismatch')
require_refs()
git('checkout', '--detach', PARENT)
if git('rev-parse', 'HEAD^{tree}') != PARENT_TREE:
    raise RuntimeError('Parent tree mismatch')
git('merge-base', '--is-ancestor', BASE, PARENT)
for edit in edits:
    path = pathlib.Path(edit['path'])
    if path.is_absolute() or '..' in path.parts or path.parts[0] not in ['apps', 'crates', 'packages', 'scripts']:
        raise RuntimeError('Unexpected edited source path')
    if edit['before'] is None:
        if path.exists():
            raise RuntimeError('New source already exists: ' + str(path))
        text = ''
    else:
        data = path.read_bytes()
        if blob(data) != edit['before']:
            raise RuntimeError('Source blob mismatch: ' + str(path))
        text = data.decode('utf-8')
    end = len(text)
    for start, stop, replacement in reversed(edit['edits']):
        if not 0 <= start <= stop <= end:
            raise RuntimeError('Invalid or overlapping reviewed edits')
        text = text[:start] + replacement + text[stop:]
        end = start
    data = text.encode('utf-8')
    if blob(data) != edit['after']:
        raise RuntimeError('Edited content mismatch: ' + str(path))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
git('add', '--', *names)
if git('write-tree') != EDITED_TREE:
    raise RuntimeError('Reviewed edited tree mismatch')
# The formatter may only change reviewed files. No algorithm or expectation
# changes are generated in CI; the exact reviewed deltas above own all behavior.
subprocess.run(['cargo', '+1.98.1', 'fmt', '--all'], check=True, timeout=120)
if git('diff', 'HEAD', '--name-only').splitlines() != names:
    raise RuntimeError('Formatter changed unreviewed files')
git('add', '--', *names)
git('diff', '--cached', '--check')
subprocess.run(['cargo', '+1.98.1', 'fmt', '--all', '--check'], check=True, timeout=120)
tree = git('write-tree')
out = pathlib.Path(os.environ['RUNNER_TEMP']) / 'automatic-recovery-source'
out.mkdir()
(out / 'change.patch').write_text(git('diff', '--cached', '--binary') + '\n', encoding='utf-8')
git('config', 'user.name', 'github-actions[bot]')
git('config', 'user.email', '41898282+github-actions[bot]@users.noreply.github.com')
git('commit', '-m', 'feat: preserve automatic early quotas and unbounded pattern traversal across hosts')
candidate = git('rev-parse', 'HEAD')
require_refs()
git('push', 'origin', candidate + ':refs/heads/' + BRANCH)
if git('ls-remote', 'origin', 'refs/heads/' + BRANCH).split() != [candidate, 'refs/heads/' + BRANCH]:
    raise RuntimeError('Candidate readback mismatch')
receipt = {'source_commit': candidate, 'parent': PARENT, 'base': BASE, 'tree': tree,
           'branch': BRANCH, 'files': names, 'main_written': False, 'published': False,
           'scope': 'existing declared-role search; not the two-Build-catalog cross-stage pipeline'}
(out / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt), flush=True)
with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
    output.write('candidate=' + candidate + '\ntree=' + tree + '\n')
