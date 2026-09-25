"""One reviewed linear promotion. Existing workflows own publication authority."""
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
CANDIDATE = '952280950aeb24528279cbff685fe8acec892223'
CANDIDATE_BRANCH = 'codex/converge-reviewed-linear-20260925'
REVIEWED = 'a18751a5bd9199dd58438df91ef0cdb68fd07f96'
TREE = 'ddb0eeb91b3202f6ae89c270bceb3c33ee435756'
GATE_RUN = '36148996653'
GATE_CONTROL = 'bfecc1f579d6332dd855c8351f05317a197ab643'
MANAGEMENT_RUN = '36148886040'
EXPECTED_JOBS = {
    'prepare', 'contracts', 'boundary-and-worker-regressions', 'ready',
    'acceptance (FoundationNoProductDebt)',
    'acceptance (FoundationAdversarialCorrectness)',
    'acceptance (FoundationDesktopHost)', 'acceptance (Sanitizer)',
    'acceptance (RustExact)', 'acceptance (RustProduct)', 'acceptance (Pages)',
}
RUNS = {
    GATE_RUN: (GATE_CONTROL, 'codex/clearra-safe-integration-20260925',
               '.github/workflows/clearra-reviewed-integration.yml', EXPECTED_JOBS),
    MANAGEMENT_RUN: (CANDIDATE, CANDIDATE_BRANCH,
                     '.github/workflows/management-policy.yml',
                     {'Clearra management Windows runtime', 'Clearra management policy'}),
}
ROOT = pathlib.Path(os.environ['RUNNER_TEMP']) / 'clearra-approved-promotion'
ROOT.mkdir(exist_ok=True)
REPORT = {'schema': 'clearra.reviewed-promotion-receipt.v1', 'main_before': MAIN,
          'candidate': CANDIDATE, 'reviewed_tree_source': REVIEWED, 'tree': TREE,
          'gate_run_id': GATE_RUN, 'management_run_id': MANAGEMENT_RUN,
          'main_updated': False, 'publication_status': 'not-dispatched'}

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
                             ('/git/ref/heads/' + CANDIDATE_BRANCH, CANDIDATE)]:
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

def verify_run(run_id, run):
    sha, branch, path, _ = RUNS[run_id]
    if (str(run.get('id')) != run_id or run.get('head_sha') != sha or
        run.get('head_branch') != branch or run.get('path') != path or
        run.get('event') != 'push' or run.get('run_attempt') != 1 or
        run.get('repository', {}).get('full_name') != REPOSITORY):
        raise RuntimeError('Required workflow identity differs from its reviewed first attempt')

def await_gates():
    deadline = time.monotonic() + 110 * 60
    last = {}
    while time.monotonic() < deadline:
        require_refs()
        complete = True
        for run_id in RUNS:
            run = api('GET', f'/actions/runs/{run_id}')
            verify_run(run_id, run)
            state = (run.get('status'), run.get('conclusion'))
            if state != last.get(run_id):
                record(f'Required run {run_id}: {state[0]} / {state[1]}')
                last[run_id] = state
            if state[0] == 'completed':
                if state[1] != 'success':
                    raise RuntimeError(f'Required run {run_id} did not succeed; no promotion')
            elif state[0] in {'queued', 'in_progress', 'waiting', 'pending', 'requested'} and state[1] is None:
                complete = False
            else:
                raise RuntimeError('Unexpected required workflow state')
        if complete:
            return
        time.sleep(30)
    raise RuntimeError('Bounded gate wait expired; no promotion performed')

def verify_completed_gates():
    for run_id, (sha, _, _, expected_jobs) in RUNS.items():
        run = api('GET', f'/actions/runs/{run_id}')
        verify_run(run_id, run)
        if (run['status'], run['conclusion']) != ('completed', 'success'):
            raise RuntimeError('A required run no longer has successful completed evidence')
        jobs = all_pages(f'/actions/runs/{run_id}/jobs?filter=latest', 'jobs')
        if len(jobs) != len(expected_jobs) or {j['name'] for j in jobs} != expected_jobs:
            raise RuntimeError('Required job inventory differs from its actual workflow')
        for job in jobs:
            if job['status'] != 'completed' or job['conclusion'] != 'success' or job['head_sha'] != sha:
                raise RuntimeError('A required job did not pass on its exact source')
    artifacts = all_pages(f'/actions/runs/{GATE_RUN}/artifacts', 'artifacts')
    matches = [a for a in artifacts if a['name'] == f'reviewed-integration-receipt-{GATE_RUN}']
    if len(matches) != 1 or matches[0]['expired'] or matches[0]['size_in_bytes'] > 1024 * 1024:
        raise RuntimeError('The exact-source integration input receipt is missing or invalid')
    artifact = matches[0]
    payload = api('GET', f"/actions/artifacts/{artifact['id']}/zip", binary=True)
    if len(payload) > 1024 * 1024 or 'sha256:' + hashlib.sha256(payload).hexdigest() != artifact.get('digest'):
        raise RuntimeError('Integration receipt archive length or digest mismatch')
    with zipfile.ZipFile(io.BytesIO(payload)) as archive:
        if archive.namelist() != ['receipt.json'] or archive.getinfo('receipt.json').file_size > 65536:
            raise RuntimeError('Unexpected integration receipt archive contents')
        receipt = json.loads(archive.read('receipt.json'))
    expected = {'schema': 'clearra.nonpublishing-integration-input.v1', 'main': MAIN,
                'reviewed_candidate': REVIEWED, 'candidate': CANDIDATE, 'tree': TREE,
                'run_id': GATE_RUN, 'run_attempt': '1',
                'main_updated': False, 'publication_authority': False}
    if receipt != expected:
        raise RuntimeError('Integration receipt does not attest the exact linear candidate')
    commit = api('GET', f'/git/commits/{CANDIDATE}')
    original = api('GET', f'/git/commits/{REVIEWED}')
    if (commit['tree']['sha'] != TREE or original['tree']['sha'] != TREE or
        [p['sha'] for p in commit['parents']] != [MAIN]):
        raise RuntimeError('Linear candidate changed the verified tree or its single parent')
    comparison = api('GET', f'/compare/{MAIN}...{CANDIDATE}')
    if comparison['status'] != 'ahead' or comparison['behind_by'] != 0 or comparison['ahead_by'] != 1:
        raise RuntimeError('Promotion is not a one-commit linear fast-forward')
    REPORT['gate_receipt_artifact_id'] = str(artifact['id'])
    REPORT['gate_receipt_digest'] = artifact['digest']
    REPORT['passed_jobs'] = sorted(EXPECTED_JOBS)
    REPORT['management_policy_passed'] = True

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
    await_gates()
    verify_completed_gates()
    require_refs()
    existing = all_pages('/actions/workflows/release-cli.yml/runs?branch=main&event=workflow_dispatch', 'workflow_runs')
    if any(r['head_sha'] == CANDIDATE for r in existing):
        raise RuntimeError('An exact-source canonical run already exists; reconcile instead of redispatching')
    REPORT['pending_main_update'] = True
    record('Both real workflows passed. Moving main by non-force linear fast-forward only.')
    updated = api('PATCH', '/git/refs/heads/main', {'sha': CANDIDATE, 'force': False})
    if updated.get('object', {}).get('sha') != CANDIDATE:
        raise RuntimeError('main update result is uncertain; do not dispatch')
    REPORT['main_updated'] = True
    REPORT.pop('pending_main_update', None)
    record(f'main fast-forward complete: {CANDIDATE}')
    canonical = dispatch('release-cli.yml', {})
    dispatch('queue-pages-publication.yml', {'acceptance_run_id': canonical, 'snapshot_sha': MAIN})
    REPORT['publication_status'] = 'canonical-and-conditional-pages-requested'
    record('Canonical release acceptance is running. The existing Pages queue owns its success wait, rollback capture and Pages dispatch. No tag or release proof was manufactured.')

if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        REPORT['error'] = str(error)
        record('Promotion sequence stopped: ' + str(error))
        raise
