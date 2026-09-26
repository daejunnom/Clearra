"""Apply the pinned source documents; never modify main or bypass protections."""
import hashlib
import json
import os
import pathlib
import subprocess

BASE = '521fee125b478a5a39a90a808d6c471c430c8494'
PARENT = '246e1c5ee9a743ab9a24ba731af55ccc6b805ddd'
BRANCH = 'codex/recovery-gray-fields-20260926'
RAW_TREE = '0028b7b2b173c21e3a18eb58371de555b17d4088'


def git(*args):
    return subprocess.check_output(['git', *args], text=True, timeout=120).strip()


def blob(text):
    data = text.encode('utf-8')
    return hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()


def refs():
    for ref, sha in [('refs/heads/main', BASE), ('refs/heads/' + BRANCH, PARENT)]:
        if git('ls-remote', 'origin', ref).split() != [sha, ref]:
            raise RuntimeError('Reviewed ref moved; reconcile before applying')


spec_root = pathlib.Path('scripts/integration/ui-polish-20260927')
documents = [json.loads((spec_root / name).read_text(encoding='utf-8')) for name in
             ['core-edits.json', 'ui-edits.json', 'completion-1.json', 'completion-2.json', 'completion-3.json']]
# Correct only recovered digest metadata; the complete reviewed raw tree below
# remains immutable and proves the exact intended English help replacement.
english = next(x for x in documents[0] if x['path'] == 'crates/clearra-cli/src/args/cli_parser.rs')
if english['after'] != 'ba5822cb62503c19e2c077f72bab0f42e80c7863':
    raise RuntimeError('Recovered help manifest changed')
english['after'] = '5b53655bee6fa1c49fd5d88e68f8a40a60408a42'
expected = sorted({x['path'] for group in documents for x in group})
if len(expected) != 37:
    raise RuntimeError('Unexpected reviewed scope')
for path in expected:
    p = pathlib.PurePosixPath(path)
    if p.is_absolute() or '..' in p.parts or p.parts[0] not in {'crates', 'packages', 'scripts'}:
        raise RuntimeError('Invalid source path')
refs()
git('checkout', '--detach', PARENT)
if git('rev-parse', 'HEAD^{tree}') != '1d40c2161033065fdb7036c2ae7b535e50a64691':
    raise RuntimeError('Reviewed parent tree mismatch')
git('merge-base', '--is-ancestor', BASE, PARENT)
for group in documents:
    for item in group:
        path = pathlib.Path(item['path'])
        if path.is_symlink():
            raise RuntimeError('Source path is a link')
        old = path.read_text(encoding='utf-8') if path.exists() else None
        if (blob(old) if old is not None else None) != item['before']:
            raise RuntimeError('Before digest mismatch: ' + str(path))
        text = old or ''
        end = 0
        for first, last, value in item['edits']:
            if not (isinstance(first, int) and isinstance(last, int) and isinstance(value, str)
                    and end <= first <= last <= len(text)):
                raise RuntimeError('Invalid nonoverlapping edit')
            end = last
        for first, last, value in reversed(item['edits']):
            text = text[:first] + value + text[last:]
        if blob(text) != item['after']:
            raise RuntimeError('After digest mismatch: ' + str(path))
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding='utf-8')
git('add', '--', *expected)
if git('write-tree') != RAW_TREE:
    raise RuntimeError('Complete reviewed source tree mismatch')
subprocess.run(['cargo', '+1.98.1', 'fmt', '--all'], check=True, timeout=120)
if git('diff', 'HEAD', '--name-only').splitlines() != expected:
    raise RuntimeError('Formatter changed unreviewed files')
git('add', '--', *expected)
git('diff', '--cached', '--check')
subprocess.run(['cargo', '+1.98.1', 'fmt', '--all', '--check'], check=True, timeout=120)
tree = git('write-tree')
output = pathlib.Path(os.environ['RUNNER_TEMP']) / 'ui-polish-source'
output.mkdir()
(output / 'complete.patch').write_text(git('diff', '--cached', '--binary') + '\n', encoding='utf-8')
git('config', 'user.name', 'github-actions[bot]')
git('config', 'user.email', '41898282+github-actions[bot]@users.noreply.github.com')
git('commit', '-m', 'feat: streamline mandatory solution controls and infer recovery placement counts')
candidate = git('rev-parse', 'HEAD')
if git('show', '-s', '--format=%P', candidate) != PARENT:
    raise RuntimeError('Candidate parent mismatch')
refs()
git('push', 'origin', candidate + ':refs/heads/' + BRANCH)
if git('ls-remote', 'origin', 'refs/heads/' + BRANCH).split()[0] != candidate:
    raise RuntimeError('Candidate readback mismatch')
receipt = {'source_commit': candidate, 'parent': PARENT, 'base': BASE, 'tree': tree,
           'files': expected, 'main_written': False}
(output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')
print(json.dumps(receipt), flush=True)
subprocess.run(['gh', 'workflow', 'run', 'management-policy.yml', '--ref', BRANCH], check=True, timeout=120)
with open(os.environ['GITHUB_OUTPUT'], 'a') as f:
    f.write('candidate=' + candidate + '\ntree=' + tree + '\n')
