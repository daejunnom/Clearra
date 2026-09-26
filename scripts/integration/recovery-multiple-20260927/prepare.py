"""Apply a checksummed source delta; never update main or publish artifacts."""
import base64
import gzip
import hashlib
import json
import os
import pathlib
import subprocess

BASE = '51084c78c3a06f355a352b4c013314c151c47709'
BASE_TREE = '1de6091e27351282dc4a4fe9e4aacc455beecef0'
EDITED_TREE = '3fdfbee2d596e283350f8193eb11e619138630af'
BRANCH = 'codex/recovery-multiple-early-20260927'
ROOT = pathlib.Path('scripts/integration/recovery-multiple-20260927')


def git(*args):
    return subprocess.check_output(['git', *args], text=True, timeout=120).strip()


def blob(data):
    return hashlib.sha1(f'blob {len(data)}\0'.encode() + data).hexdigest()


def require_main():
    if git('ls-remote', 'origin', 'refs/heads/main').split() != [BASE, 'refs/heads/main']:
        raise RuntimeError('Main moved; reconcile instead of applying an old source delta')


require_main()
records = []
for name, count, digest in [
    ('core', 3, 'f518b0202270b9ee7c2b2ffa92d3f369a8cafd5d754324e5b8bfb70a4235a8c7'),
    ('surface', 2, '965ec9e096e4c11a2b7f3088a73bcc3134b6f1ea693a5f93023f617d5bd74206'),
]:
    encoded = ''.join((ROOT / f'{name}.{i}.b64').read_text().strip() for i in range(count))
    raw = gzip.decompress(base64.b64decode(encoded, validate=True))
    if hashlib.sha256(raw).hexdigest() != digest:
        raise RuntimeError('Reviewed delta checksum mismatch: ' + name)
    records.extend(json.loads(raw))
paths = sorted(record['path'] for record in records)
if len(paths) != 28 or len(set(paths)) != len(paths):
    raise RuntimeError('Unexpected reviewed change inventory')
if any(pathlib.PurePosixPath(path).is_absolute() or '..' in pathlib.PurePosixPath(path).parts
       or not path.startswith(('crates/', 'packages/clearra-ui/', 'apps/clearra-discord-bot/', 'scripts/tools/'))
       for path in paths):
    raise RuntimeError('Unreviewed source path')
existing = git('ls-remote', 'origin', 'refs/heads/' + BRANCH).splitlines()
if existing:
    raise RuntimeError('Candidate branch already exists; read its exact commit before retrying')
git('checkout', '--detach', BASE)
if git('rev-parse', 'HEAD^{tree}') != BASE_TREE:
    raise RuntimeError('Base source tree mismatch')
for record in records:
    path = pathlib.Path(record['path'])
    current = path.read_bytes() if path.exists() else b''
    if (blob(current) if path.exists() else None) != record['before']:
        raise RuntimeError('Reviewed input blob mismatch: ' + str(path))
    text = current.decode('utf-8')
    previous = len(text) + 1
    for start, end, replacement in reversed(record['edits']):
        if not 0 <= start <= end < previous:
            raise RuntimeError('Invalid or overlapping source edit')
        text = text[:start] + replacement + text[end:]
        previous = start
    output = text.encode('utf-8')
    if blob(output) != record['after']:
        raise RuntimeError('Reviewed output blob mismatch: ' + str(path))
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(output)
git('add', '--', *paths)
if git('write-tree') != EDITED_TREE:
    raise RuntimeError('Reviewed edited tree mismatch')
subprocess.run(['cargo', '+1.98.1', 'fmt', '--all'], check=True, timeout=120)
if git('diff', 'HEAD', '--name-only').splitlines() != paths:
    raise RuntimeError('Formatter changed an unreviewed file')
git('add', '--', *paths)
git('diff', '--cached', '--check')
subprocess.run(['cargo', '+1.98.1', 'fmt', '--all', '--check'], check=True, timeout=120)
tree = git('write-tree')
git('config', 'user.name', 'github-actions[bot]')
git('config', 'user.email', '41898282+github-actions[bot]@users.noreply.github.com')
git('commit', '-m', 'feat: search multiple early recovery roles with exact quotas and replay B2B policy')
candidate = git('rev-parse', 'HEAD')
require_main()
git('push', 'origin', candidate + ':refs/heads/' + BRANCH)
if git('ls-remote', 'origin', 'refs/heads/' + BRANCH).split() != [candidate, 'refs/heads/' + BRANCH]:
    raise RuntimeError('Candidate readback mismatch')
out = pathlib.Path(os.environ['RUNNER_TEMP']) / 'multiple-recovery-source'
out.mkdir(exist_ok=True)
(out / 'complete.patch').write_text(git('diff', '--binary', BASE, candidate) + '\n')
receipt = {'source_commit': candidate, 'parent': BASE, 'tree': tree, 'branch': BRANCH,
           'files': paths, 'main_written': False, 'published': False}
(out / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt), flush=True)
with open(os.environ['GITHUB_OUTPUT'], 'a') as output:
    output.write('candidate=' + candidate + '\ntree=' + tree + '\n')
