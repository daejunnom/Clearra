<script lang="ts">
  import {
    ArrowLeft,
    Check,
    Languages,
    Link2,
    Play,
    Square,
    TriangleAlert
  } from '@lucide/svelte';
  import { getContext, onDestroy, onMount } from 'svelte';

  import {
    DEFAULT_RUNTIME_WARMUP_POLICY,
    HOST_CAPABILITY_SNAPSHOT_CONTEXT,
    automaticWorkerAuthority,
    sharedBrowserHostCapabilitySnapshot,
    updateWasmCommandText,
    wasmWorkerState,
    WasmTerminalWorkerController,
    type HostCapabilitySnapshot
  } from '../wasm';
  import QueueTextInput from '../components/QueueTextInput.svelte';
  import BoardEditor from './BoardEditor.svelte';
  import { writeClipboardText } from './clipboardText';
  import {
    decodePcSolverPath,
    DEFAULT_PC_SOLVER_LINK_STATE,
    encodePcSolverPath,
    type PcSolverLinkState
  } from './pcSolverLinkState';
  import QueuePatternHelp from './QueuePatternHelp.svelte';
  import PcSolverResult from './PcSolverResult.svelte';
  import WorkerAuthorityStatus from './WorkerAuthorityStatus.svelte';
  import {
    automaticPcTargetLines,
    buildWorkspaceCommand,
    clearCompletedRows,
    createDefaultWorkspaceRequest,
    trimBoardMask,
    workspaceValidationCodes,
    type RuleProfile,
    type SolverWorkspaceRequest,
    type WorkspaceValidationCode
  } from './solverWorkspaceModel';
  import {
    preferredWorkspaceLanguage,
    workspaceMessage,
    type WorkspaceLanguage
  } from './workspaceI18n';
  import {
    workspaceViewFromWasm,
    type WorkspaceRuntimeStatus,
    type WorkspaceRuntimeView
  } from './workspaceRuntime';

  export let workerFactory: (() => Worker) | null = null;
  export let initialPathState = '';
  export let basePath = '/pc-solver';
  export let homeHref = '/';

  const decodedLinkState = initialPathState ? decodePcSolverPath(initialPathState) : null;
  const hostCapabilitySnapshot =
    getContext<HostCapabilitySnapshot>(HOST_CAPABILITY_SNAPSHOT_CONTEXT) ??
    sharedBrowserHostCapabilitySnapshot();
  const workerController = new WasmTerminalWorkerController(
    workerFactory,
    hostCapabilitySnapshot
  );
  const ruleOptions: Array<{ value: RuleProfile; label: string }> = [
    { value: 'srs-plus', label: 'SRS+' },
    { value: 'srs', label: 'SRS' },
    { value: 'srs-x', label: 'SRS-X' },
    { value: 'jstris-180', label: 'Jstris 180' }
  ];
  const MAX_PC_SOLVER_LINES = 4;

  let request = withAutomaticTarget(requestFromLinkState(decodedLinkState ?? DEFAULT_PC_SOLVER_LINK_STATE));
  let language: WorkspaceLanguage = 'en';
  let mounted = false;
  let hasRun = false;
  let elapsedMs = 0;
  let resultTargetLines = request.lines;
  let runStartedAt = 0;
  let elapsedTimer: ReturnType<typeof setInterval> | null = null;
  let pathTimer: ReturnType<typeof setTimeout> | null = null;
  let copiedTimer: ReturnType<typeof setTimeout> | null = null;
  let lastPathState = '';
  let shareStatus: 'idle' | 'copied' | 'failed' = 'idle';
  let completedRowsWarning = 0;
  let invalidSharedLink = Boolean(initialPathState && !decodedLinkState);

  $: workerController.setWorkerFactory(workerFactory);
  $: workerAuthority = automaticWorkerAuthority(
    hostCapabilitySnapshot,
    request.useAllLogicalProcessors
  );
  $: workerView = workspaceViewFromWasm($wasmWorkerState);
  $: runtimeView = hasRun ? workerView : idleRuntimeView(workerView);
  $: validationCodes = standaloneValidationCodes(request);
  $: active = runtimeView.status === 'running' || runtimeView.status === 'cancelling';
  $: encodedPathState = encodePcSolverPath(linkStateFromRequest(request));
  $: label = (
    key: Parameters<typeof workspaceMessage>[1],
    values: Record<string, string | number> = {}
  ) => workspaceMessage(language, key, values);
  $: if (mounted && encodedPathState !== lastPathState) schedulePathUpdate(encodedPathState);
  $: if (isTerminal(runtimeView.status) && elapsedTimer !== null) stopElapsedTimer();

  onMount(() => {
    mounted = true;
    language = preferredWorkspaceLanguage(
      localStorage.getItem('clearra-language') ?? navigator.language
    );
    request = withAutomaticTarget({
      ...request,
      workers: automaticWorkerAuthority(hostCapabilitySnapshot).workersEffective
    });
    workerController.prewarm(
      request.workers,
      false,
      DEFAULT_RUNTIME_WARMUP_POLICY,
      automaticWorkerAuthority(
        hostCapabilitySnapshot,
        request.useAllLogicalProcessors
      )
    );
    replacePath(encodePcSolverPath(linkStateFromRequest(request)));
    const handlePageHide = () => dispose();
    window.addEventListener('pagehide', handlePageHide);
    return () => window.removeEventListener('pagehide', handlePageHide);
  });

  onDestroy(dispose);

  function requestFromLinkState(state: PcSolverLinkState): SolverWorkspaceRequest {
    return {
      ...createDefaultWorkspaceRequest(),
      lines: state.lines,
      boardMask: state.boardMask,
      queue: state.queue,
      holdEnabled: state.holdEnabled,
      rule: state.rule,
      scoreMode: 'off',
      backend: 'auto',
      gpuDevice: 'auto',
      tablebaseEnabled: false,
      precomputeBuildDependencies: false
    };
  }

  function linkStateFromRequest(value: SolverWorkspaceRequest): PcSolverLinkState {
    return {
      lines: value.lines,
      boardMask: value.boardMask,
      queue: value.queue,
      holdEnabled: value.holdEnabled,
      rule: value.rule
    };
  }

  function updateRequest(next: SolverWorkspaceRequest) {
    invalidSharedLink = false;
    const scoreCpuOnly =
      next.scoreMode === 'summary' ||
      next.scoreMode === 'score-finder' ||
      next.scoreMode === 'score-minimals';
    const requestedUseAll = next.useAllLogicalProcessors;
    const useAllChanged = requestedUseAll !== request.useAllLogicalProcessors;
    const scoreModeChanged = next.scoreMode !== request.scoreMode;
    request = withAutomaticTarget({
      ...next,
      workers: useAllChanged
        ? automaticWorkerAuthority(
            hostCapabilitySnapshot,
            requestedUseAll
          ).workersEffective
        : next.workers,
      useAllLogicalProcessors: requestedUseAll,
      backend: scoreCpuOnly ? 'cpu' : 'auto',
      gpuDevice: 'auto',
      tablebaseEnabled: false,
      precomputeBuildDependencies: false
    });
    if (useAllChanged || scoreModeChanged) {
      workerController.prewarm(
        request.workers,
        false,
        DEFAULT_RUNTIME_WARMUP_POLICY,
        automaticWorkerAuthority(
          hostCapabilitySnapshot,
          request.useAllLogicalProcessors
        )
      );
    }
  }

  function withAutomaticTarget(next: SolverWorkspaceRequest): SolverWorkspaceRequest {
    return {
      ...next,
      lines: automaticPcTargetLines(next.boardMask, next.queue, MAX_PC_SOLVER_LINES)
        ?? MAX_PC_SOLVER_LINES,
      boardMask: trimBoardMask(next.boardMask, MAX_PC_SOLVER_LINES)
    };
  }

  function standaloneValidationCodes(value: SolverWorkspaceRequest): WorkspaceValidationCode[] {
    const normalized = clearCompletedRows(value.boardMask, MAX_PC_SOLVER_LINES);
    const inferredLines = automaticPcTargetLines(
      normalized.boardMask,
      value.queue,
      MAX_PC_SOLVER_LINES
    );
    const codes = workspaceValidationCodes(
      {
        ...value,
        lines: inferredLines ?? MAX_PC_SOLVER_LINES,
        boardMask: normalized.boardMask
      },
      'web'
    );
    if (inferredLines === null && value.queue.trim() && !codes.includes('queue_invalid')) {
      codes.push('scenario_supply_mismatch');
    }
    return [...new Set(codes)];
  }

  function setBoardMask(boardMask: bigint) {
    completedRowsWarning = 0;
    updateRequest({
      ...request,
      boardMask: trimBoardMask(boardMask, MAX_PC_SOLVER_LINES)
    });
  }

  function setLanguage(next: WorkspaceLanguage) {
    language = next;
    localStorage.setItem('clearra-language', next);
  }

  function run() {
    if (active || validationCodes.length) return;
    const normalized = clearCompletedRows(request.boardMask, MAX_PC_SOLVER_LINES);
    const targetLines = automaticPcTargetLines(
      normalized.boardMask,
      request.queue,
      MAX_PC_SOLVER_LINES
    );
    if (targetLines === null) return;
    const executionRequest = {
      ...request,
      lines: targetLines,
      boardMask: normalized.boardMask
    };
    completedRowsWarning = normalized.clearedRows;
    if (executionRequest !== request) updateRequest(executionRequest);
    resultTargetLines = executionRequest.lines;
    updateWasmCommandText(buildWorkspaceCommand(executionRequest));
    if (!workerController.run()) return;
    hasRun = true;
    startElapsedTimer();
  }

  function cancel() {
    if (active) workerController.cancel();
  }

  async function copyShareLink() {
    clearCopiedTimer();
    try {
      const path = pathForState(encodePcSolverPath(linkStateFromRequest(request)));
      await writeClipboardText(new URL(path, window.location.origin).href);
      shareStatus = 'copied';
    } catch {
      shareStatus = 'failed';
    }
    copiedTimer = setTimeout(() => (shareStatus = 'idle'), 1800);
  }

  function schedulePathUpdate(state: string) {
    if (pathTimer !== null) clearTimeout(pathTimer);
    pathTimer = setTimeout(() => replacePath(state), 120);
  }

  function replacePath(state: string) {
    if (!mounted) return;
    lastPathState = state;
    window.history.replaceState(window.history.state, '', pathForState(state));
  }

  function pathForState(state: string): string {
    return `${basePath.replace(/\/$/, '')}/${state}`;
  }

  function startElapsedTimer() {
    stopElapsedTimer();
    elapsedMs = 0;
    runStartedAt = performance.now();
    elapsedTimer = setInterval(() => {
      elapsedMs = performance.now() - runStartedAt;
    }, 100);
  }

  function stopElapsedTimer() {
    if (elapsedTimer !== null) {
      clearInterval(elapsedTimer);
      elapsedTimer = null;
    }
    if (runStartedAt > 0) elapsedMs = performance.now() - runStartedAt;
  }

  function clearCopiedTimer() {
    if (copiedTimer !== null) clearTimeout(copiedTimer);
    copiedTimer = null;
  }

  function dispose() {
    mounted = false;
    stopElapsedTimer();
    clearCopiedTimer();
    if (pathTimer !== null) clearTimeout(pathTimer);
    pathTimer = null;
    workerController.dispose();
  }

  function isTerminal(status: WorkspaceRuntimeStatus): boolean {
    return status === 'completed' || status === 'failed' || status === 'cancelled' || status === 'terminated';
  }

  function idleRuntimeView(view: WorkspaceRuntimeView): WorkspaceRuntimeView {
    return {
      ...view,
      status: 'idle',
      jobId: null,
      progressLabel: '',
      progressDone: 0,
      progressTotal: 0,
      progressTelemetry: null,
      publicFailures: [],
      developerDiagnostics: [],
      response: null,
      searchReport: null,
      webgpuReport: null,
      backendReport: null,
      resourceReport: null,
      developerError: null
    };
  }
</script>

<svelte:head>
  <title>Clearra PC Solver</title>
  <meta name="description" content="Exact browser-based perfect clear solver" />
</svelte:head>

<main class="solver-page">
  <header class="solver-header">
    <a class="brand" href={homeHref} aria-label={label('backToClearra')}>
      <span class="brand-mark" aria-hidden="true"><i></i><i></i><i></i><i></i></span>
      <strong>Clearra <span>PC Solver</span></strong>
    </a>
    <div class="header-actions">
      <a class="back-link" href={homeHref}><ArrowLeft size={15} strokeWidth={1.8} />{label('backToClearra')}</a>
      <button class="share-link" type="button" on:click={copyShareLink}>
        {#if shareStatus === 'copied'}<Check size={15} strokeWidth={2} />{:else}<Link2 size={15} strokeWidth={1.8} />{/if}
        {label(shareStatus === 'copied' ? 'solverLinkCopied' : shareStatus === 'failed' ? 'solverLinkFailed' : 'copySolverLink')}
      </button>
      <div class="language-control" aria-label={label('language')}>
        <Languages size={15} strokeWidth={1.8} />
        <button type="button" class:active={language === 'en'} on:click={() => setLanguage('en')}>EN</button>
        <button type="button" class:active={language === 'ko'} on:click={() => setLanguage('ko')}>KO</button>
      </div>
    </div>
  </header>

  <section class="solver-band">
    {#if invalidSharedLink}
      <div class="notice warning" role="alert"><TriangleAlert size={16} strokeWidth={1.9} />{label('solverLinkInvalid')}</div>
    {/if}
    {#if completedRowsWarning > 0}
      <div class="notice" role="status"><TriangleAlert size={16} strokeWidth={1.9} />{label('completedRowsCleared', { count: completedRowsWarning })}</div>
    {/if}

    <div class="solver-grid">
      <div class="field-panel">
        <BoardEditor
          {request}
          {language}
          displayHeight={MAX_PC_SOLVER_LINES}
          showImport={false}
          showStats={false}
          showToolbar={false}
          on:change={(event) => setBoardMask(event.detail)}
        />
      </div>

      <aside class="control-panel" aria-label={label('search')}>
        <div class="control-heading">
          <span>{label('pcSolver')}</span>
          <strong>{label('search')}</strong>
        </div>

        <label class="queue-field">
          <span>{label('queuePattern')}</span>
          <QueueTextInput
            class="workspace-queue-input"
            value={request.queue}
            placeholder={label('queuePlaceholder')}
            spellcheck="false"
            aria-invalid={request.queue.length > 0 && validationCodes.includes('queue_invalid')}
            on:value={(event) => updateRequest({ ...request, queue: event.detail })}
            on:keydown={(event) => event.key === 'Enter' && run()}
          />
        </label>
        <QueuePatternHelp {language} />

        <fieldset class="result-control">
          <legend>{label('scoreMode')}</legend>
          <div class="result-chips">
            <button
              type="button"
              class:active={request.scoreMode === 'path'}
              aria-pressed={request.scoreMode === 'path'}
              on:click={() => updateRequest({ ...request, scoreMode: 'path' })}
            >{label('pathFamily')}</button>
            <button
              type="button"
              class:active={request.scoreMode === 'off'}
              aria-pressed={request.scoreMode === 'off'}
              on:click={() => updateRequest({ ...request, scoreMode: 'off' })}
            >{label('scoreOff')}</button>
            <button
              type="button"
              class:active={request.scoreMode === 'minimum-cover'}
              aria-pressed={request.scoreMode === 'minimum-cover'}
              on:click={() => updateRequest({ ...request, scoreMode: 'minimum-cover' })}
            >{label('minimumSolutions')}</button>
            <button
              type="button"
              class:active={request.scoreMode === 'summary'}
              aria-pressed={request.scoreMode === 'summary'}
              on:click={() => updateRequest({ ...request, scoreMode: 'summary' })}
            >{label('scoreSummary')}</button>
            <button
              type="button"
              class:active={request.scoreMode === 'score-finder'}
              aria-pressed={request.scoreMode === 'score-finder'}
              on:click={() => updateRequest({ ...request, scoreMode: 'score-finder' })}
            >{label('scoreFinder')}</button>
            <button
              type="button"
              class:active={request.scoreMode === 'score-minimals'}
              aria-pressed={request.scoreMode === 'score-minimals'}
              on:click={() => updateRequest({ ...request, scoreMode: 'score-minimals' })}
            >{label('scoreMinimals')}</button>
          </div>
        </fieldset>

        <div class="option-row">
          <span>{label('hold')}</span>
          <button
            class="switch"
            class:active={request.holdEnabled}
            type="button"
            role="switch"
            aria-label={label('hold')}
            aria-checked={request.holdEnabled}
            on:click={() => updateRequest({ ...request, holdEnabled: !request.holdEnabled })}
          ><i></i></button>
        </div>
        <WorkerAuthorityStatus authority={workerAuthority} {language} />

        <div class="option-row">
          <span>{label('useAllThreads')}</span>
          <button
            class="switch"
            class:active={request.useAllLogicalProcessors}
            type="button"
            role="switch"
            aria-label={label('useAllThreads')}
            aria-checked={request.useAllLogicalProcessors}
            on:click={() => updateRequest({
              ...request,
              useAllLogicalProcessors: !request.useAllLogicalProcessors
            })}
          ><i></i></button>
        </div>

        <fieldset class="rule-control">
          <legend>{label('rule')}</legend>
          <div class="rule-chips">
            {#each ruleOptions as option}
              <button
                type="button"
                class:active={request.rule === option.value}
                aria-pressed={request.rule === option.value}
                on:click={() => updateRequest({ ...request, rule: option.value })}
              >{option.label}</button>
            {/each}
          </div>
        </fieldset>

        {#if validationCodes.length && request.queue.trim()}
          <div class="validation" role="alert">
            {#each validationCodes as code}<p>{label(code)}</p>{/each}
          </div>
        {/if}

        <div class="run-actions">
          <button class="cancel" type="button" disabled={!active} on:click={cancel}>
            <Square size={14} fill="currentColor" />{label('cancel')}
          </button>
          <button class="run" type="button" disabled={active || validationCodes.length > 0} on:click={run}>
            <Play size={15} fill="currentColor" />{label('run')}
          </button>
        </div>
      </aside>
    </div>
  </section>

  <PcSolverResult
    view={runtimeView}
    {language}
    {elapsedMs}
    targetLines={resultTargetLines}
    loadSolutionPage={(offset, limit, signal) =>
      workerController.loadSolutionPage(offset, limit, signal)}
    loadNextProductPage={(signal) => workerController.loadNextProductPage(signal)}
    loadProductMemberPage={(outerPageNumber, memberPageNumber, signal) =>
      workerController.loadProductMemberPage(outerPageNumber, memberPageNumber, signal)}
    releaseProductPages={() => workerController.releaseProductPages()}
  />
</main>

<style>
  :global(*) { box-sizing: border-box; }
  :global(html) { background: #fff; font-family: Inter, "Noto Sans KR", ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
  :global(body) { margin: 0; min-width: 320px; }
  :global(body), :global(button), :global(input), :global(select), :global(textarea) { font-family: inherit; letter-spacing: 0; }
  :global(button:focus-visible), :global(a:focus-visible), :global(input:focus-visible) { outline: 2px solid #16877d; outline-offset: 2px; }
  .solver-page { background: #fff; color: #17211e; min-height: 100vh; }
  .solver-header { align-items: center; background: #fff; border-bottom: 1px solid #d7ded9; display: flex; gap: 24px; justify-content: space-between; min-height: 64px; padding: 11px max(22px, calc((100vw - 1180px) / 2)); }
  .brand { align-items: center; color: #17211e; display: inline-flex; gap: 11px; min-width: 0; text-decoration: none; }
  .brand > strong { font-size: 19px; white-space: nowrap; }
  .brand > strong span { color: #63706b; font-size: 13px; font-weight: 700; margin-left: 5px; }
  .brand-mark { display: grid; flex: 0 0 auto; gap: 2px; grid-template-columns: repeat(2, 9px); grid-template-rows: repeat(2, 9px); }
  .brand-mark i { background: #16877d; border-radius: 2px; }
  .brand-mark i:nth-child(2) { background: #e0ac36; }
  .brand-mark i:nth-child(3) { background: #d96c4b; }
  .brand-mark i:nth-child(4) { background: #334e77; }
  .header-actions, .back-link, .share-link, .language-control { align-items: center; display: flex; }
  .header-actions { gap: 7px; }
  .back-link, .share-link { background: #fff; border: 1px solid #cbd3ce; border-radius: 5px; color: #46534e; cursor: pointer; font-size: 11px; font-weight: 750; gap: 6px; height: 34px; padding: 0 10px; text-decoration: none; }
  .share-link { min-width: 112px; justify-content: center; }
  .language-control { border: 1px solid #cfd7d2; border-radius: 5px; gap: 2px; height: 34px; padding: 0 3px 0 7px; }
  .language-control > :global(svg) { color: #68736f; margin-right: 2px; }
  .language-control button { background: transparent; border: 0; border-radius: 3px; color: #6d7873; cursor: pointer; font-size: 10px; font-weight: 800; height: 26px; padding: 0 7px; }
  .language-control button.active { background: #dcece7; color: #075f58; }
  .solver-band { margin: 0 auto; max-width: 1060px; padding: 18px 24px 24px; }
  .solver-grid { display: grid; grid-template-columns: minmax(380px, 1fr) minmax(320px, .82fr); padding: 6px 0 2px; }
  .field-panel { min-width: 0; padding-right: 24px; }
  .control-panel { border-left: 1px solid #d9dfdb; min-width: 0; padding-left: 24px; }
  .control-heading { border-bottom: 1px solid #d9dfdb; display: grid; gap: 3px; margin-bottom: 18px; padding-bottom: 13px; }
  .control-heading span { color: #68736f; font-size: 10px; font-weight: 750; text-transform: uppercase; }
  .control-heading strong { font-size: 17px; }
  .queue-field { display: grid; gap: 7px; }
  .queue-field span, .option-row > span, .rule-control legend, .result-control legend { color: #53605b; font-size: 11px; font-weight: 750; }
  .queue-field :global(.workspace-queue-input) { border: 1px solid #bfc9c4; border-radius: 5px; color: #1d544f; font-family: ui-monospace, SFMono-Regular, Consolas, monospace; font-size: 14px; font-weight: 750; height: 42px; min-width: 0; padding: 0 11px; width: 100%; }
  .queue-field :global(.workspace-queue-input:focus) { border-color: #16877d; box-shadow: 0 0 0 3px #16877d1f; outline: 0; }
  .queue-field :global(.workspace-queue-input[aria-invalid='true']) { border-color: #c45635; }
  .option-row { align-items: center; border-bottom: 1px solid #e0e5e2; border-top: 1px solid #e0e5e2; display: flex; justify-content: space-between; margin-top: 18px; padding: 14px 0; }
  .switch { background: #bcc7c2; border: 0; border-radius: 999px; cursor: pointer; height: 24px; padding: 3px; transition: background 120ms ease; width: 42px; }
  .switch i { background: #fff; border-radius: 50%; display: block; height: 18px; transform: translateX(0); transition: transform 120ms ease; width: 18px; }
  .switch.active { background: #16877d; }
  .switch.active i { transform: translateX(18px); }
  .rule-control, .result-control { border: 0; margin: 17px 0 0; padding: 0; }
  .rule-control legend, .result-control legend { margin-bottom: 8px; padding: 0; }
  .rule-chips, .result-chips { display: grid; gap: 6px; grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .result-chips { grid-template-columns: repeat(3, minmax(0, 1fr)); }
  .rule-chips button, .result-chips button { background: #f6f8f6; border: 1px solid #cbd3ce; border-radius: 5px; color: #53605b; cursor: pointer; font-size: 11px; font-weight: 750; min-height: 34px; padding: 0 8px; }
  .rule-chips button.active, .result-chips button.active { background: #dcece7; border-color: #16877d; color: #075f58; }
  .validation, .notice { align-items: flex-start; background: #fff2ec; border: 1px solid #e0a28d; border-radius: 5px; color: #833c26; display: flex; font-size: 11px; gap: 8px; margin: 12px 0; padding: 9px 11px; }
  .validation { display: grid; }
  .validation p { margin: 0; }
  .notice { background: #fff7df; border-color: #dfbd68; color: #654a0e; }
  .notice + .notice { margin-top: -5px; }
  .run-actions { display: grid; gap: 8px; grid-template-columns: minmax(0, .7fr) minmax(0, 1.3fr); margin-top: 20px; }
  .run-actions button { align-items: center; border-radius: 5px; cursor: pointer; display: inline-flex; font-size: 12px; font-weight: 780; gap: 7px; height: 40px; justify-content: center; }
  .run-actions button:disabled { cursor: default; opacity: .4; }
  .run { background: #16877d; border: 1px solid #0e746b; color: #fff; }
  .cancel { background: #fff; border: 1px solid #bfc9c4; color: #4c5954; }

  @media (max-width: 860px) {
    .solver-grid { grid-template-columns: 1fr; }
    .field-panel { padding-right: 0; }
    .control-panel { border-left: 0; border-top: 1px solid #d9dfdb; margin-top: 24px; padding-left: 0; padding-top: 22px; }
  }
  @media (max-width: 620px) {
    .solver-header { align-items: flex-start; padding: 11px 15px; }
    .header-actions { justify-content: flex-end; }
    .back-link { display: none; }
    .share-link { min-width: 34px; padding: 0 8px; }
    .share-link :global(svg) { flex: 0 0 auto; }
    .brand > strong span { display: block; margin: 3px 0 0; }
    .solver-band { padding: 16px 0 22px; }
    .notice { margin-left: 16px; margin-right: 16px; }
    .solver-grid { padding: 18px 16px; }
  }
  @media (max-width: 430px) {
    .language-control > :global(svg) { display: none; }
    .language-control { padding-left: 3px; }
    .share-link { font-size: 0; min-width: 34px; }
  }
  @media (prefers-reduced-motion: reduce) {
    .switch, .switch i { transition: none; }
  }
</style>
