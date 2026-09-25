"""One reviewed promotion. Canonical workflows retain all publication authority."""
import hashlib
import io
import json
import os
import pathlib
import subprocess
import time
import zipfile

REPOSITORY = 'daejunnom/Clearra'
MAIN = '3e06bce344da89212b002d0cba96063703088074'
CANDIDATE = 'a18751a5bd9199dd58438df91ef0cdb68fd07f96'
CANDIDATE_REF = 'refs/heads/codex/candidate-pinned-boundary-release-20260925'
GATE_RUN = '36144647114'
GATE_CONTROL = '1e1e6c2786ee449d171d5892765fc4345b92d2cf'
GATE_PATH = '.github/workflows/clearra-reviewed-integration.yml'
EXPECTED_JOBS = {
    'prepare', 'contracts', 'boundary-and-worker-regressions', 'ready',
    'acceptance (FoundationNoProductDebt)',
    'acceptance (FoundationAdversarialCorrectness)',
    'acceptance (FoundationDesktopHost)', 'acceptance (Sanitizer)',
    'acceptance (RustExact)', 'acceptance (RustProduct)', 'acceptance (Pages)',
}
ROOT = pathlib.Path(os.environ['RUNNER_TEMP']) / 'clearra-approved-promotion'
ROOT.mkdir(exist_ok=True)
REPORT = {'schema': 'clearra.reviewed-promotion-receipt.v1', 'main_before': MAIN,
          'candidate': CANDIDATE, 'gate_run_id': GATE_RUN, 'main_updated': False,
          'publication_status': 'not-dispatched'}

def record(message):
    print(message, flush=True)
    with open(os.environ['GITHUB_STEP_SUMMARY'], 'a', encoding='utf8') as f:
        f.write(message + '\n\n')
    (ROOT / 'receipt.json').write_text(json.dumps(REPORT, indent=2) + '\n', encoding='utf8')

def api(method, suffix, body=None, binary=False):
    endpoint = f'repos/{REPOSITORY}' + suffix
    args = ['gh', 'api', '--method', method, '-H', 'X-GitHub-Api-Version: 2026-03-10', endpoint]
    if body is not None:
        args.extend(['--input', '-'])
    result = subprocess.run(args, input=None if body is None else json.dumps(body).encode(),
                            capture_output=True, timeout=60)
    if result.returncode:
        raise RuntimeError(f'{method} {suffix} failed; writes are never retried automatically')
    return result.stdout if binary else json.loads(result.stdout)

def require_refs():
    for suffix, expected in [('/git/ref/heads/main', MAIN),
                             ('/git/ref/heads/codex/candidate-pinned-boundary-release-20260925', CANDIDATE)]:
        value = api('GET', suffix)
        if value.get('object', {}).get('sha') != expected or value['object'].get('type') != 'commit':
            raise RuntimeError('A reviewed source ref moved; no promotion is authorized')

def all_pages(suffix, key):
    rows = []
    for page in range(1, 101):
        separator = '&' if '?' in suffix else '?'
        value = api('GET', suffix + f'{separator}per_page=100&page={page}')
        current = value[key]
        rows.extend(current)
        if len(rows) == value['total_count']:
            return rows
        if not current or len(rows) > value['total_count']:
            raise RuntimeError('Inconsistent paginated GitHub evidence')
    raise RuntimeError('GitHub evidence exceeds the bounded catalog')

def verify_gate_run(run):
    if (str(run.get('id')) != GATE_RUN or run.get('head_sha') != GATE_CONTROL or
        run.get('head_branch') != 'codex/clearra-safe-integration-20260925' or
        run.get('path') != GATE_PATH or run.get('event') != 'push' or
        run.get('run_attempt') != 1 or run.get('repository', {}).get('full_name') != REPOSITORY):
        raise RuntimeError('Integration run identity differs from the reviewed run')

def await_gate():
    deadline = time.monotonic() + 90 * 60
    last = None
    while time.monotonic() < deadline:
        require_refs()
        run = api('GET', f'/actions/runs/{GATE_RUN}')
        verify_gate_run(run)
        state = (run.get('status'), run.get('conclusion'))
        if state != last:
            record(f'Integration gate {GATE_RUN}: {state[0]} / {state[1]}')
            last = state
        if state[0] == 'completed':
            if state[1] != 'success':
                raise RuntimeError('Integration did not succeed; main and deployments remain unchanged')
            return
        if state[0] not in {'queued', 'in_progress', 'waiting', 'pending', 'requested'} or state[1] is not None:
            raise RuntimeError('Unexpected gate state')
        time.sleep(30)
    raise RuntimeError('Bounded gate wait expired; no promotion performed')

def verify_completed_gate():
    run = api('GET', f'/actions/runs/{GATE_RUN}')
    verify_gate_run(run)
    if (run['status'], run['conclusion']) != ('completed', 'success'):
        raise RuntimeError('Gate no longer has successful completed evidence')
    jobs = all_pages(f'/actions/runs/{GATE_RUN}/jobs?filter=latest', 'jobs')
    if len(jobs) != len(EXPECTED_JOBS) or {j['name'] for j in jobs} != EXPECTED_JOBS:
        raise RuntimeError('Acceptance job inventory differs from the reviewed gate')
    for job in jobs:
        if job['status'] != 'completed' or job['conclusion'] != 'success' or job['head_sha'] != GATE_CONTROL:
            raise RuntimeError('An acceptance leaf did not pass on the reviewed control source')
    artifacts = all_pages(f'/actions/runs/{GATE_RUN}/artifacts', 'artifacts')
    matches = [a for a in artifacts if a['name'] == f'reviewed-integration-receipt-{GATE_RUN}']
    if len(matches) != 1 or matches[0]['expired'] or matches[0]['size_in_bytes'] > 1024 * 1024:
        raise RuntimeError('The exact-source integration input receipt is missing or invalid')
    artifact = matches[0]
    payload = api('GET', f"/actions/artifacts/{artifact['id']}/zip", binary=True)
    if 'sha256:' + hashlib.sha256(payload).hexdigest() != artifact.get('digest'):
        raise RuntimeError('Integration receipt archive digest mismatch')
    with zipfile.ZipFile(io.BytesIO(payload)) as archive:
        if archive.namelist() != ['receipt.json'] or archive.getinfo('receipt.json').file_size > 65536:
            raise RuntimeError('Unexpected integration receipt archive contents')
        receipt = json.loads(archive.read('receipt.json'))
    expected = {'schema': 'clearra.nonpublishing-integration-input.v1', 'main': MAIN,
                'reviewed_candidate': '352c42d5536e2ceab0bc55d773c61973c3b96434',
                'candidate': CANDIDATE, 'run_id': GATE_RUN, 'run_attempt': '1',
                'main_updated': False, 'publication_authority': False}
    if any(receipt.get(k) != v for k, v in expected.items()):
        raise RuntimeError('Integration receipt does not attest the exact candidate')
    commit = api('GET', f'/git/commits/{CANDIDATE}')
    if receipt.get('tree') != commit['tree']['sha']:
        raise RuntimeError('Candidate source tree differs from the passed gate')
    comparison = api('GET', f'/compare/{MAIN}...{CANDIDATE}')
    if comparison['status'] != 'ahead' or comparison['behind_by'] != 0:
        raise RuntimeError('Promotion is not a fast-forward')
    REPORT['gate_receipt_artifact_id'] = str(artifact['id'])
    REPORT['gate_receipt_digest'] = artifact['digest']
    REPORT['passed_jobs'] = sorted(EXPECTED_JOBS)

def dispatch(workflow, inputs):
    head = api('GET', '/git/ref/heads/main')
    if head.get('object', {}).get('sha') != CANDIDATE:
        raise RuntimeError('main moved before canonical dispatch')
    REPORT['pending_dispatch'] = workflow
    record(f'Requesting existing {workflow} from exact main {CANDIDATE}; no POST retry')
    response = api('POST', f'/actions/workflows/{workflow}/dispatches', {'ref': 'main', 'inputs': inputs})
    run_id = str(response.get('workflow_run_id', ''))
    if (not run_id.isdecimal() or run_id == '0' or
        response.get('html_url') != f'https://github.com/{REPOSITORY}/actions/runs/{run_id}' or
        response.get('run_url') != f'https://api.github.com/repos/{REPOSITORY}/actions/runs/{run_id}'):
        raise RuntimeError('Uncertain dispatch receipt; no automatic duplicate dispatch')
    REPORT[workflow] = response
    REPORT.pop('pending_dispatch', None)
    record(f'{workflow}: {response["html_url"]}')
    return run_id

def main():
    if os.environ.get('GITHUB_REPOSITORY') != REPOSITORY or os.environ.get('GITHUB_RUN_ATTEMPT') != '1':
        raise RuntimeError('This one-shot promotion requires the original repository and first attempt')
    await_gate()
    verify_completed_gate()
    require_refs()
    existing = all_pages('/actions/workflows/release-cli.yml/runs?branch=main&event=workflow_dispatch', 'workflow_runs')
    if any(r['head_sha'] == CANDIDATE for r in existing):
        raise RuntimeError('An exact-source canonical run already exists; reconcile instead of redispatching')
    REPORT['pending_main_update'] = True
    record('All reviewed leaves passed. Moving main by non-force fast-forward only.')
    updated = api('PATCH', '/git/refs/heads/main', {'sha': CANDIDATE, 'force': False})
    if updated.get('object', {}).get('sha') != CANDIDATE:
        raise RuntimeError('main update result is uncertain; do not dispatch')
    REPORT['main_updated'] = True
    REPORT.pop('pending_main_update', None)
    record(f'main fast-forward complete: {CANDIDATE}')
    canonical = dispatch('release-cli.yml', {})
    queue = dispatch('queue-pages-publication.yml', {'acceptance_run_id': canonical, 'snapshot_sha': MAIN})
    REPORT['publication_status'] = 'canonical-and-conditional-pages-requested'
    record('Canonical release acceptance is running. Existing Pages queue owns the success wait, rollback capture and Pages dispatch. No tag or release receipt was manufactured.')

try:
    main()
except Exception as error:
    REPORT['error'] = str(error)
    record('Promotion sequence stopped: ' + str(error))
    raise
