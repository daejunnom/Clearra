import assert from 'node:assert/strict';
import test from 'node:test';
import {
  CANONICAL_ACCEPTANCE_EVIDENCE_SCHEMA, CANONICAL_ACCEPTANCE_REQUIRED_JOB_NAMES,
} from './canonical-acceptance-evidence.mjs';
import { sealCanonicalReport } from './canonical-release-evidence.mjs';
import { selectAcceptedWindowsDesktopStartupArtifact } from './windows-desktop-startup-attestation.mjs';
import { buildWindowsDesktopTrustedOrigin } from './windows-desktop-trusted-origin.mjs';

const SOURCE = '1'.repeat(40);
const RUN = '23159141141';
const ATTEMPT = '3';
const REPO = 'daejunnom/Clearra';
const WORKFLOW = '.github/workflows/release-cli.yml';
const VERSION = '0.9.0';
const BASE = '/Clearra';
const REPO_ID = 110123403;
const EXE_SHA = '2'.repeat(64);
const GUI_ZIP_SHA = '4'.repeat(64);
const CANONICAL_ZIP_SHA = '5'.repeat(64);
const GUI_NAME = `clearra-windows-gui-v${VERSION}-run-${RUN}-attempt-${ATTEMPT}`;
const CANONICAL_NAME = `canonical-acceptance-evidence-${SOURCE}-run-${RUN}-attempt-${ATTEMPT}`;

function fixture() {
  const expected = {
    repository: REPO, source_commit: SOURCE, run_id: RUN, run_attempt: ATTEMPT,
    release_version: VERSION, pages_base_path: BASE,
  };
  const repository = { id: REPO_ID, full_name: REPO };
  const attempt = {
    id: Number(RUN), run_attempt: Number(ATTEMPT), head_branch: 'main', head_sha: SOURCE,
    repository: { ...repository }, head_repository: { ...repository }, path: WORKFLOW,
    event: 'workflow_dispatch', status: 'completed', conclusion: 'success',
  };
  const rawJobs = [...CANONICAL_ACCEPTANCE_REQUIRED_JOB_NAMES, 'canonical-evidence', 'release-failure-summary'].map(
    (name, index) => ({
      id: 810013200 + index, name, run_id: Number(RUN), run_attempt: Number(ATTEMPT),
      head_sha: SOURCE, status: 'completed', conclusion: 'success',
    }),
  );
  const jobs = { total_count: rawJobs.length, jobs: rawJobs };
  const report = {
    ...expected, schema_id: CANONICAL_ACCEPTANCE_EVIDENCE_SCHEMA, workflow_path: WORKFLOW, status: 'passed',
    jobs: rawJobs.slice(0, CANONICAL_ACCEPTANCE_REQUIRED_JOB_NAMES.length).map(({ name, id }) => ({
      name, job_id: String(id), status: 'passed',
    })),
    accepted_inputs: Object.fromEntries([
      'ctk3_manifest_sha256', 'pages_identity_sha256', 'wasm_build_receipt_sha256', 'gate_index_sha256',
    ].map((key) => [key, EXE_SHA])),
    final_source_fragments: {
      toolchains: { source_commit: SOURCE },
      canonical_gate: { source_commit: SOURCE, status: 'passed', readiness_open_count: 0 },
      surface_reports: ['desktop', 'discord', 'native', 'wasm'].map((surface) => ({
        surface, source_commit: SOURCE, status: 'passed', sha256: EXE_SHA,
      })),
      release_artifacts: [
        ['linux-cli', `Clearra-CLI-v${VERSION}-linux-x86_64`],
        ['windows-cli', `Clearra-CLI-v${VERSION}-windows-x86_64.exe`],
        ['windows-gui', `Clearra-GUI-v${VERSION}-windows-x86_64.exe`],
      ].map(([role, name]) => ({ role, name, sha256: EXE_SHA, size_bytes: 200, source_commit: SOURCE })),
    },
  };
  const acceptedEvidence = sealCanonicalReport(report);
  const rawArtifacts = [
    [892011, GUI_NAME, GUI_ZIP_SHA], [892013, CANONICAL_NAME, CANONICAL_ZIP_SHA],
    [892006, GUI_NAME.replace(/attempt-[0-9]+$/u, 'attempt-2'), '7'.repeat(64)],
  ].map(([id, name, sha256]) => ({
    id, name, digest: 'sha256:' + sha256, expired: false,
    workflow_run: {
      id: Number(RUN), repository_id: REPO_ID, head_repository_id: REPO_ID, head_branch: 'main', head_sha: SOURCE,
    },
  }));
  const artifactCatalog = { total_count: rawArtifacts.length, artifacts: rawArtifacts };
  const downloadedArchives = {
    gui: { artifact_id: '892011', sha256: GUI_ZIP_SHA },
    canonical: { artifact_id: '892013', sha256: CANONICAL_ZIP_SHA },
  };
  return { expected, attempt, jobs, artifactCatalog, acceptedEvidence, downloadedArchives };
}

function replaceEvidence(input, change) {
  const { report_sha256: old, ...remaining } = input.acceptedEvidence;
  const report = structuredClone(remaining);
  change(report);
  input.acceptedEvidence = sealCanonicalReport(report);
}

function rejectWhen(cases) {
  for (const [name, change] of cases) {
    const input = fixture();
    change(input);
    assert.throws(() => buildWindowsDesktopTrustedOrigin(input), undefined, name);
  }
}

const gui = (input) => input.artifactCatalog.artifacts[0];
const canonical = (input) => input.artifactCatalog.artifacts[1];
const producerJob = (input) => input.jobs.jobs.find(({ name }) => name === 'windows-gui');
const evidenceJob = (input) => input.jobs.jobs.find(({ name }) => name === 'canonical-evidence');

test('returns only existing trusted-origin keys and feeds the startup consumer', () => {
  const input = fixture();
  const result = buildWindowsDesktopTrustedOrigin(input);
  assert.deepEqual(result, {
    repository: REPO, workflow_path: WORKFLOW, source_commit: SOURCE, run_id: RUN, run_attempt: ATTEMPT,
    producer_job_id: String(producerJob(input).id), artifact_id: '892011', artifact_name: GUI_NAME,
    artifact_archive_sha256: GUI_ZIP_SHA, canonical_acceptance_sha256: input.acceptedEvidence.report_sha256,
    release_version: VERSION, pages_base_path: BASE,
  });
  assert.notEqual(result.artifact_archive_sha256, CANONICAL_ZIP_SHA);
  assert.notEqual(result.artifact_archive_sha256, EXE_SHA);
  const artifact = selectAcceptedWindowsDesktopStartupArtifact({
    acceptedEvidence: input.acceptedEvidence, trustedActionsOrigin: result,
  });
  assert.equal(artifact.sha256, EXE_SHA);
});

test('exact main/repository/workflow identity and success cannot be substituted', () => {
  rejectWhen([
    ['run ID', (i) => { i.attempt.id++; }],
    ['attempt', (i) => { i.attempt.run_attempt--; }],
    ['source', (i) => { i.attempt.head_sha = '6'.repeat(40); }],
    ['main branch', (i) => { i.attempt.head_branch = 'codex/other'; }],
    ['workflow', (i) => { i.attempt.path = '.github/workflows/other.yml'; }],
    ['event', (i) => { i.attempt.event = 'push'; }],
    ['status', (i) => { i.attempt.status = 'in_progress'; }],
    ['conclusion', (i) => { i.attempt.conclusion = 'failure'; }],
    ['repository display', (i) => { i.attempt.repository.full_name = 'another/Clearra'; }],
    ['head repository ID', (i) => { i.attempt.head_repository.id++; }],
    ['head repository display', (i) => { i.attempt.head_repository.full_name = 'fork/Clearra'; }],
    ['unsafe numeric repository ID', (i) => { i.attempt.repository.id = Number.MAX_SAFE_INTEGER + 1; }],
  ]);
});

test('run-level mixed and truncated job lists cannot stand in for the exact attempt jobs', () => {
  rejectWhen([
    ['missing page', (i) => { i.jobs.total_count++; }],
    ['job attempt', (i) => { producerJob(i).run_attempt--; }],
    ['job run', (i) => { evidenceJob(i).run_id++; }],
    ['job source', (i) => { evidenceJob(i).head_sha = '6'.repeat(40); }],
    ['mixed nonproducer job', (i) => { i.jobs.jobs[0].run_attempt--; }],
    ['missing attempt on job', (i) => { delete producerJob(i).run_attempt; }],
    ['repeat ID', (i) => { i.jobs.jobs[0].id = producerJob(i).id; }],
  ]);
});

test('only one successful GUI producer and canonical evidence job can attest upload', () => {
  rejectWhen([
    ['failed GUI producer', (i) => { producerJob(i).conclusion = 'failure'; }],
    ['failed evidence job', (i) => { evidenceJob(i).conclusion = 'failure'; }],
    ['in-progress evidence job', (i) => { evidenceJob(i).status = 'in_progress'; }],
    ['duplicate producer', (i) => {
      i.jobs.jobs.push({ ...producerJob(i), id: 811199000 });
      i.jobs.total_count++;
    }],
    ['missing evidence job', (i) => {
      i.jobs.jobs = i.jobs.jobs.filter(({ name }) => name !== 'canonical-evidence');
      i.jobs.total_count--;
    }],
  ]);
});

test('canonical acceptance seal must match exact run/attempt and the observed GUI producer job', () => {
  rejectWhen([
    ['broken hash', (i) => { i.acceptedEvidence = { ...i.acceptedEvidence, report_sha256: EXE_SHA }; }],
    ['re-sealed wrong attempt', (i) => { replaceEvidence(i, (e) => { e.run_attempt = '2'; }); }],
    ['re-sealed wrong run', (i) => { replaceEvidence(i, (e) => { e.run_id = RUN + '1'; }); }],
    ['re-sealed wrong GUI job', (i) => {
      replaceEvidence(i, (e) => { e.jobs.find(({ name }) => name === 'windows-gui').job_id = '11111'; });
    }],
    ['re-sealed wrong GUI filename', (i) => {
      replaceEvidence(i, (e) => { e.final_source_fragments.release_artifacts[2].name = 'Unrelated.exe'; });
    }],
  ]);
});

test('artifact catalog refuses truncation, duplicate names/IDs, and wrong attempt names', () => {
  rejectWhen([
    ['missing page', (i) => { i.artifactCatalog.total_count++; }],
    ['wrong attempt name', (i) => { gui(i).name = GUI_NAME.replace(/attempt-[0-9]+$/u, 'attempt-1'); }],
    ['duplicate exact name', (i) => {
      i.artifactCatalog.artifacts.push({ ...canonical(i), id: 38981313 });
      i.artifactCatalog.total_count++;
    }],
    ['repeat ID across pages', (i) => { canonical(i).id = gui(i).id; }],
    ['source omitted from evidence name', (i) => { canonical(i).name = CANONICAL_NAME.replace(SOURCE + '-', ''); }],
  ]);
});

test('expiry and API run/head/repository ownership are checked for both artifacts', () => {
  rejectWhen([
    ['GUI expired', (i) => { gui(i).expired = true; }],
    ['canonical expired', (i) => { canonical(i).expired = true; }],
    ['artifact from another run', (i) => { canonical(i).workflow_run.id++; }],
    ['artifact from another source', (i) => { gui(i).workflow_run.head_sha = '6'.repeat(40); }],
    ['artifact from another repo', (i) => { canonical(i).workflow_run.repository_id++; }],
    ['artifact from a fork', (i) => { canonical(i).workflow_run.head_repository_id++; }],
    ['artifact from a feature branch', (i) => { gui(i).workflow_run.head_branch = 'codex/other'; }],
    ['missing API run binding', (i) => { delete canonical(i).workflow_run; }],
  ]);
});

test('observed raw ZIP hashes and IDs, not executable hashes or API claims alone, bind both files', () => {
  rejectWhen([
    ['replaced GUI bytes', (i) => { i.downloadedArchives.gui.sha256 = EXE_SHA; }],
    ['replaced canonical bytes', (i) => { i.downloadedArchives.canonical.sha256 = GUI_ZIP_SHA; }],
    ['wrong ID with the right hash', (i) => { i.downloadedArchives.gui.artifact_id = '892006'; }],
    ['API digest missing', (i) => { delete gui(i).digest; }],
    ['bad API digest type', (i) => { canonical(i).digest = CANONICAL_ZIP_SHA; }],
    ['same archive renamed by API', (i) => {
      canonical(i).digest = 'sha256:' + GUI_ZIP_SHA;
      i.downloadedArchives.canonical.sha256 = GUI_ZIP_SHA;
    }],
    ['canonical bytes observation missing', (i) => { delete i.downloadedArchives.canonical; }],
  ]);
});
