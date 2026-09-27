"""Copy the exact uploaded review source into an isolated branch; never run it."""
from __future__ import annotations

import base64
import hashlib
import json
import lzma
import os
from pathlib import Path, PurePosixPath
import subprocess

BASE = '7d7fc390b2f65d52ebf8362dff65dcbfcb6d481b'
BASE_TREE = 'decc00271a61fb3c30408d451777aa7acbf17b6f'
REVIEW_TREE = '7a1ac7441aa830577ff015043d15f20201c5ab62'
BRANCH = 'codex/review-recovery-two-supplies-exchange-20260927'
DATA_ROOT = Path('scripts/integration/recovery-review-upload-20260927')
CHUNKS = [
    'c51925b517ef3aba07a497989f475c4d508459b3',
    'a1d36fb95970fd7edf84c73ec54242ab828fdd07',
    '84fb098c5e6f58d536cb2b364cbce9113a3ea4f0',
    '5b362dcf6ff05c32dae1ebeadcfccd2100d5b4f6',
]
DATA_SHA256 = '519d93e4ba5f40eb8d74d97f0dcd2234c8f27f0e2eb593ec4728f1da2a15627d'


def git(*args: str) -> str:
    return subprocess.check_output(['git', *args], text=True, timeout=120).strip()


def blob(data: bytes) -> str:
    return hashlib.sha1(b'blob ' + str(len(data)).encode() + b'\0' + data).hexdigest()


def load_records() -> list[dict]:
    encoded = []
    for index, expected in enumerate(CHUNKS, 1):
        data = (DATA_ROOT / f'changes-{index:02}.xz.b64').read_bytes()
        if blob(data) != expected:
            raise RuntimeError(f'Upload chunk identity mismatch: {index}')
        encoded.append(data.strip())
    compressed = base64.b64decode(b''.join(encoded), validate=True)
    if len(compressed) != 38112:
        raise RuntimeError('Unexpected compressed source length')
    decoder = lzma.LZMADecompressor(memlimit=256 * 1024 * 1024)
    decoded = decoder.decompress(compressed, max_length=161319)
    if (len(decoded) != 161318 or not decoder.eof or decoder.unused_data
            or hashlib.sha256(decoded).hexdigest() != DATA_SHA256):
        raise RuntimeError('Reviewed source transport checksum mismatch')
    records = json.loads(decoded)
    if not isinstance(records, list) or len(records) != 51:
        raise RuntimeError('Exactly 51 reviewed files are required')
    paths = [record['path'] for record in records]
    if paths != sorted(set(paths)):
        raise RuntimeError('Duplicate or unordered file paths')
    for name in paths:
        path = PurePosixPath(name)
        if (path.is_absolute() or '..' in path.parts or '\\' in name
                or path.parts[0] not in {'crates', 'packages', 'scripts', 'tests'}):
            raise RuntimeError('Unexpected product path: ' + name)
    return records


def replace_ranges(original: bytes, record: dict) -> bytes:
    if record['before'] is not None and blob(original) != record['before']:
        raise RuntimeError('Base blob mismatch: ' + record['path'])
    text = original.decode('utf-8')
    result = []
    cursor = 0
    for start, end, replacement in record['edits']:
        if (type(start) is not int or type(end) is not int or not isinstance(replacement, str)
                or not cursor <= start <= end <= len(text)):
            raise RuntimeError('Invalid reviewed edit range: ' + record['path'])
        result.extend((text[cursor:start], replacement))
        cursor = end
    result.append(text[cursor:])
    data = ''.join(result).encode('utf-8')
    if blob(data) != record['after']:
        raise RuntimeError('Result blob mismatch: ' + record['path'])
    return data


def main() -> None:
    control = git('rev-parse', 'HEAD')
    records = load_records()
    ref = 'refs/heads/' + BRANCH
    if git('ls-remote', 'origin', 'refs/heads/main').split() != [BASE, 'refs/heads/main']:
        raise RuntimeError('Main moved; review the baseline before importing')
    if git('ls-remote', 'origin', ref):
        raise RuntimeError('Review branch already exists; never overwrite it')
    if git('status', '--porcelain'):
        raise RuntimeError('The source checkout must be clean')
    git('checkout', '--detach', BASE)
    if git('rev-parse', 'HEAD^{tree}') != BASE_TREE:
        raise RuntimeError('Base source tree mismatch')
    outputs = {}
    for record in records:
        path = Path(record['path'])
        if path.is_symlink():
            raise RuntimeError('Symbolic link is not a reviewed source file')
        if record['before'] is None:
            if path.exists():
                raise RuntimeError('A new source path already exists: ' + str(path))
            original = b''
        else:
            original = path.read_bytes()
        outputs[path] = replace_ranges(original, record)
    for path, data in outputs.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    paths = [record['path'] for record in records]
    git('add', '--', *paths)
    if git('diff', '--cached', '--name-only').splitlines() != paths:
        raise RuntimeError('Changed file inventory differs from the review')
    git('diff', '--cached', '--check')
    if git('write-tree') != REVIEW_TREE:
        raise RuntimeError('Final source is not byte-identical to the uploaded review')
    git('config', 'user.name', 'github-actions[bot]')
    git('config', 'user.email', '41898282+github-actions[bot]@users.noreply.github.com')
    git('commit', '-m', 'feat: stage two-supply recovery and piece-exchange review implementation',
        '-m', 'Import the 51 reviewed files without modification. Native Rust compilation, real WASM integration, and deployment gates are pending. Review only; do not promote or publish this snapshot as verified.')
    candidate = git('rev-parse', 'HEAD')
    if git('show', '-s', '--format=%P', candidate) != BASE:
        raise RuntimeError('Unexpected review parent')
    if (git('ls-remote', 'origin', 'refs/heads/main').split() != [BASE, 'refs/heads/main']
            or git('ls-remote', 'origin', ref)):
        raise RuntimeError('Remote changed before upload; refusing to overwrite')
    # Ordinary non-force push of a new review branch only. No merge or dispatch.
    git('push', 'origin', candidate + ':' + ref)
    if git('ls-remote', 'origin', ref).split() != [candidate, ref]:
        raise RuntimeError('Review branch readback mismatch')
    receipt = {
        'repository': 'daejunnom/Clearra', 'branch': BRANCH,
        'source_commit': candidate, 'parent': BASE, 'tree': REVIEW_TREE,
        'control_commit': control, 'file_count': len(paths),
        'source_review_archive': 'Clearra_recovery_two_supplies_exchange_REVIEW_ONLY_20260927.zip',
        'files': [{'path': str(path), 'blob_sha': blob(data), 'sha256': hashlib.sha256(data).hexdigest()}
                  for path, data in outputs.items()],
        'byte_identical_to_review': True, 'main_written': False,
        'native_tests_run': False, 'deployment_gate_started': False,
        'published': False, 'status': 'REVIEW_UPLOADED_NATIVE_UNVERIFIED',
        'observed_main_after_upload': git('ls-remote', 'origin', 'refs/heads/main').split()[0],
    }
    out = Path(os.environ['RUNNER_TEMP']) / 'recovery-review-upload.json'
    out.write_text(json.dumps(receipt, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(json.dumps(receipt, ensure_ascii=False), flush=True)
    with open(os.environ['GITHUB_STEP_SUMMARY'], 'a', encoding='utf-8') as summary:
        summary.write(f'## Review code uploaded, not accepted for deployment\n\nBranch: `{BRANCH}`\n\nCommit: `{candidate}`\n\n51 files match the uploaded review tree `{REVIEW_TREE}`. No compilation, product execution, main update, tag, or deployment.\n')


if __name__ == '__main__':
    main()
