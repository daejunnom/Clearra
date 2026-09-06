<script lang="ts">
  import { readWorkspaceLanguage, persistWorkspaceLanguage } from './workspaceLanguagePreference';
  import { getContext, onDestroy, onMount } from 'svelte';
  import { get } from 'svelte/store';

  import {
    cancelDesktopJob,
    clearDesktopTerminalResult,
    desktopJobState,
    disposeDesktopJobPolling,
    resumeDesktopJobPolling,
    startDesktopJob,
    updateDesktopRequest
  } from '../stores';
  import {
    CPU_ONLY_RUNTIME_WARMUP_POLICY,
    HOST_CAPABILITY_SNAPSHOT_CONTEXT,
    clearWasmTerminalResult,
    sharedBrowserHostCapabilitySnapshot,
    updateWasmCommandText,
    wasmWorkerState,
    WasmTerminalWorkerController,
    type HostCapabilitySnapshot
  } from '../wasm';
  import WorkspaceShell from './WorkspaceShell.svelte';
  import WorkspaceFailureNotice from './WorkspaceFailureNotice.svelte';
  import {
    buildOperationDocumentCommand,
    operationDocumentRequestForDesktop,
    type OperationDocumentCommandInput
  } from './operationDocumentCommandModel';
  import {
    workspaceMessage,
    type WorkspaceLanguage
  } from './workspaceI18n';
  import { workspaceViewFromDesktop, workspaceViewFromWasm } from './workspaceRuntime';

  export let workerFactory: (() => Worker) | null = null;
  export let runtime: 'web' | 'desktop' = 'web';

  const profiles = ['srs-plus', 'srs', 'srs-x', 'jstris-180', 'no-kick'] as const;
  const hostCapabilitySnapshot =
    getContext<HostCapabilitySnapshot>(HOST_CAPABILITY_SNAPSHOT_CONTEXT) ??
    sharedBrowserHostCapabilitySnapshot();
  const workerController = new WasmTerminalWorkerController(
    workerFactory,
    hostCapabilitySnapshot
  );

  let language: WorkspaceLanguage = 'en';
  let document = '';
  let ruleProfile = 'srs-plus';
  let kickProfile = 'srs-plus';
  let timeoutSeconds = 900;
  let disposed = false;

  $: workerController.setWorkerFactory(workerFactory);
  $: runtimeView = runtime === 'web'
    ? workspaceViewFromWasm($wasmWorkerState)
    : workspaceViewFromDesktop($desktopJobState);
  $: active = runtimeView.status === 'running' || runtimeView.status === 'cancelling';
  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);
  $: normalizedDocument = document.trim();
  $: validDocument = /^(?:ctk3(?:b_|_|@)|(?:v115|[Ddm]115)@)[^\s]+$/u.test(normalizedDocument);
  $: validTimeout = Number.isInteger(timeoutSeconds) && timeoutSeconds >= 1 && timeoutSeconds <= 900;
  $: reportFields = publicOperationReportFields(
    runtimeView.searchReport?.summary_fields ?? [],
    language
  );

  onMount(() => {
    language = readWorkspaceLanguage();
    if (runtime === 'web') {
      clearWasmTerminalResult();
      workerController.prewarm(1, false, CPU_ONLY_RUNTIME_WARMUP_POLICY);
    } else {
      clearDesktopTerminalResult();
      resumeDesktopJobPolling();
    }
  });

  onDestroy(disposeWorkspace);

  function disposeWorkspace() {
    if (disposed) return;
    disposed = true;
    if (runtime === 'web') {
      workerController.dispose();
      clearWasmTerminalResult();
      return;
    }
    const state = get(desktopJobState);
    if (state.jobId !== null || state.status === 'running' || state.status === 'cancelling') {
      void cancelDesktopJob();
    } else {
      disposeDesktopJobPolling();
      clearDesktopTerminalResult();
    }
  }

  function setLanguage(next: WorkspaceLanguage) {
    language = next;
    persistWorkspaceLanguage(next);
  }

  async function run() {
    if (active || !validDocument || !validTimeout) return;
    const commandInput: OperationDocumentCommandInput = {
      capability: 'sequence',
      document: normalizedDocument,
      ruleProfile,
      kickProfile,
      timeoutSeconds
    };
    if (runtime === 'web') {
      updateWasmCommandText(buildOperationDocumentCommand(commandInput));
      workerController.run();
      return;
    }
    updateDesktopRequest(operationDocumentRequestForDesktop(commandInput, language));
    await startDesktopJob();
  }

  async function cancel() {
    if (!active) return;
    if (runtime === 'web') workerController.cancel();
    else await cancelDesktopJob();
  }

  function publicOperationReportFields(
    fields: Array<[string, string]>,
    selectedLanguage: WorkspaceLanguage
  ): Array<[string, string]> {
    const labels: Record<string, readonly [string, string]> = {
      operation_count: ['Operations', '배치 수'],
      cleared_line_count: ['Cleared lines', '삭제한 줄'],
      rule_profile: ['Rule profile', '규칙 프로필'],
      kick_profile: ['Kick profile', '킥 프로필']
    };
    return fields.flatMap(([key, value]) => {
      const publicLabel = labels[key];
      return publicLabel
        ? [[publicLabel[selectedLanguage === 'ko' ? 1 : 0], value] as [string, string]]
        : [];
    });
  }
</script>

<svelte:head>
  <title>{label('operationSequence')} · Clearra</title>
  <meta name="description" content="Lossless operation trace normalization and replay validation" />
</svelte:head>

<WorkspaceShell
  activeMode="sequence"
  {language}
  {active}
  statusLabel={label(runtimeView.status)}
  workspaceLabel={label('operationSequence')}
  dimensionLabel=""
  dimensionValue={1}
  showDimension={false}
  cancelLabel={label('cancel')}
  runLabel={label('run')}
  runDisabled={!validDocument || !validTimeout}
  singlePanel
  on:language={(event) => setLanguage(event.detail)}
  on:cancel={cancel}
  on:run={run}
>
  <div slot="controls" class="controls">
    <label class="document-field">
      <span>{language === 'ko' ? 'Operation trace 문서' : 'Operation trace document'}</span>
      <textarea
        rows="7"
        bind:value={document}
        disabled={active}
        placeholder="ctk3_… or v115@…"
        aria-invalid={document.length > 0 && !validDocument}
      ></textarea>
      <small>
        {language === 'ko'
          ? '문서에 기록된 operation 순서와 좌표를 그대로 정규화하고 재생 검증합니다. 큐나 홀드를 추론하지 않습니다.'
          : 'The recorded operation order and coordinates are normalized and replay-validated as supplied. Queue and hold are never inferred.'}
      </small>
    </label>
    <div class="profile-grid">
      <label>
        <span>{language === 'ko' ? '규칙 프로필' : 'Rule profile'}</span>
        <select bind:value={ruleProfile} disabled={active}>
          {#each profiles as profile}
            <option value={profile}>{profile}</option>
          {/each}
        </select>
      </label>
      <label>
        <span>{language === 'ko' ? '킥 프로필' : 'Kick profile'}</span>
        <select bind:value={kickProfile} disabled={active}>
          {#each profiles as profile}
            <option value={profile}>{profile}</option>
          {/each}
        </select>
      </label>
      <label>
        <span>{language === 'ko' ? '제한시간 (초)' : 'Timeout (seconds)'}</span>
        <input type="number" min="1" max="900" step="1" bind:value={timeoutSeconds} disabled={active} />
      </label>
    </div>
  </div>

  <section slot="result" class="result" aria-live="polite">
    <h2>{language === 'ko' ? '정규화된 재생 보고서' : 'Normalized replay report'}</h2>
    {#if reportFields.length > 0}
      <dl>
        {#each reportFields as [key, value]}
          <div>
            <dt>{key}</dt>
            <dd>{value}</dd>
          </div>
        {/each}
      </dl>
    {:else if runtimeView.publicFailures.length}
      <WorkspaceFailureNotice failures={runtimeView.publicFailures} {language} compact />
    {:else}
      <p class="empty">
        {language === 'ko'
          ? '검증을 실행하면 배치 수, 줄 삭제와 적용한 규칙·킥 프로필이 여기에 표시됩니다.'
          : 'Run validation to see the placement count, line clears, and applied rule and kick profiles.'}
      </p>
    {/if}
  </section>
</WorkspaceShell>

<style>
  .controls { display: grid; gap: 20px; }
  label { display: grid; gap: 6px; }
  label > span { color: #4c5954; font-size: 12px; font-weight: 750; }
  textarea, select, input {
    background: #fff;
    border: 1px solid #cbd3ce;
    border-radius: 5px;
    color: #26322e;
    font: inherit;
    padding: 10px;
  }
  textarea { min-height: 150px; resize: vertical; word-break: break-all; }
  textarea[aria-invalid='true'] { border-color: #b84a4a; }
  small { color: #65716c; line-height: 1.5; }
  .profile-grid { display: grid; gap: 12px; grid-template-columns: repeat(3, minmax(0, 1fr)); }
  .result { margin: 0 auto; max-width: 1460px; padding: 8px 24px 40px; }
  .result h2 { font-size: 17px; margin: 0 0 14px; }
  dl { background: #fff; border: 1px solid #d5dcd7; border-radius: 7px; margin: 0; padding: 8px 18px; }
  dl div { display: grid; gap: 16px; grid-template-columns: minmax(190px, .35fr) minmax(0, 1fr); padding: 10px 0; }
  dl div + div { border-top: 1px solid #e4e9e6; }
  dt { color: #596560; font-size: 12px; font-weight: 750; }
  dd { font-family: ui-monospace, SFMono-Regular, Consolas, monospace; margin: 0; overflow-wrap: anywhere; }
  .empty { background: #fff; border: 1px solid #d5dcd7; border-radius: 7px; margin: 0; padding: 18px; }
  @media (max-width: 720px) {
    .profile-grid { grid-template-columns: 1fr; }
    .result { padding-left: 16px; padding-right: 16px; }
    dl div { grid-template-columns: 1fr; gap: 4px; }
  }
</style>
