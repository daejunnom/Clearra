<script lang="ts">
  import { getContext, onDestroy, onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { componentMessage, type ComponentMessageKey } from '../i18n/componentCatalog';
  import { readWorkspaceLanguage, persistWorkspaceLanguage } from './workspaceLanguagePreference';
  import {
    cancelDesktopJob, clearDesktopTerminalResult, desktopJobState, disposeDesktopJobPolling,
    resumeDesktopJobPolling, startDesktopJob, updateDesktopRequest
  } from '../stores';
  import {
    CPU_ONLY_RUNTIME_WARMUP_POLICY, HOST_CAPABILITY_SNAPSHOT_CONTEXT, automaticWorkerAuthority,
    clearWasmTerminalResult, sharedBrowserHostCapabilitySnapshot, updateWasmCommandText,
    wasmWorkerState, WasmTerminalWorkerController, type HostCapabilitySnapshot
  } from '../wasm';
  import {
    boundaryRecoveryCommand, boundaryRecoveryDesktopRequest, boundaryRecoveryPayload,
    createBoundaryRecoveryRequest, validateBoundaryRecoveryRequest
  } from './boundaryRecoveryModel';
  import { trimForwardBoardMask } from './forwardSearchModel';
  import WorkspaceBoardEditor from './WorkspaceBoardEditor.svelte';
  import WorkspaceFailureNotice from './WorkspaceFailureNotice.svelte';
  import WorkspaceShell from './WorkspaceShell.svelte';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';
  import { workspaceViewFromDesktop, workspaceViewFromWasm } from './workspaceRuntime';
  import type { RuleProfile, SpinProfile } from './solverWorkspaceModel';

  export let workerFactory: (() => Worker) | null = null;
  export let runtime: 'web' | 'desktop' = 'web';

  const hostCapabilitySnapshot = getContext<HostCapabilitySnapshot>(HOST_CAPABILITY_SNAPSHOT_CONTEXT)
    ?? sharedBrowserHostCapabilitySnapshot();
  const workerController = new WasmTerminalWorkerController(workerFactory, hostCapabilitySnapshot);
  const rules: RuleProfile[] = ['srs-plus', 'srs', 'srs-x', 'jstris-180'];
  const spins: SpinProfile[] = ['t-spins', 't-spins-plus', 'all-spin', 'all-spin-plus', 'all-mini', 'all-mini-plus'];
  let request = createBoundaryRecoveryRequest();
  let selectedRolePosition = 1;
  let language: WorkspaceLanguage = 'en';
  let disposed = false;

  $: workerController.setWorkerFactory(workerFactory);
  $: runtimeView = runtime === 'web' ? workspaceViewFromWasm($wasmWorkerState) : workspaceViewFromDesktop($desktopJobState);
  $: payload = boundaryRecoveryPayload(runtimeView.response);
  $: example = payload?.population?.recovery_example ?? payload?.population?.normal_example;
  $: displaySteps = payload?.population ? (example?.steps ?? []) : (payload?.steps ?? []);
  $: displayCheckpoint = payload?.population ? example?.stage_one_checkpoint_step : payload?.stage_one_checkpoint_step;
  $: active = runtimeView.status === 'running' || runtimeView.status === 'cancelling';
  $: validation = validateBoundaryRecoveryRequest(request);
  $: label = (key: ComponentMessageKey) => componentMessage(language, key);
  $: standardLabel = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);

  onMount(() => {
    language = readWorkspaceLanguage();
    if (runtime === 'web') {
      clearWasmTerminalResult();
      workerController.prewarm(1, false, CPU_ONLY_RUNTIME_WARMUP_POLICY,
        automaticWorkerAuthority(hostCapabilitySnapshot, false));
    } else {
      clearDesktopTerminalResult();
      resumeDesktopJobPolling();
    }
    const handlePageHide = () => dispose();
    window.addEventListener('pagehide', handlePageHide);
    return () => window.removeEventListener('pagehide', handlePageHide);
  });
  onDestroy(dispose);

  function dispose() {
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

  function setHeight(value: number) {
    const height = Math.max(1, Math.min(24, Math.trunc(value || 1)));
    request = {
      ...request, height,
      initialBoardMask: trimForwardBoardMask(request.initialBoardMask, height),
      targetBoardMask: trimForwardBoardMask(request.targetBoardMask, height),
      borrowPlacementMask: trimForwardBoardMask(request.borrowPlacementMask, height),
      placementRoleMasks: request.placementRoleMasks.map((mask) => trimForwardBoardMask(mask, height))
    };
  }

  function importBoard(mask: bigint, height: number, target: boolean) {
    const nextHeight = Math.max(request.height, Math.max(1, Math.min(24, height)));
    request = {
      ...request, height: nextHeight,
      [target ? 'targetBoardMask' : 'initialBoardMask']: trimForwardBoardMask(mask, nextHeight)
    };
  }

  function importBorrowPlacement(mask: bigint, height: number) {
    const nextHeight = Math.max(request.height, Math.max(1, Math.min(24, height)));
    request = { ...request, height: nextHeight, borrowPlacementMask: trimForwardBoardMask(mask, nextHeight) };
  }

  function setPlacements(value: number) {
    const placements = Math.max(2, Math.min(42, Math.trunc(value || 2)));
    request = {
      ...request, placements,
      placementRoleMasks: request.placementRoleMasks.length === 0 ? [] :
        Array.from({ length: placements }, (_, index) => request.placementRoleMasks[index] ?? 0n)
    };
    selectedRolePosition = Math.min(selectedRolePosition, placements);
  }

  function setExactRoles(enabled: boolean) {
    request = {
      ...request,
      placementRoleMasks: enabled ? Array.from({ length: request.placements }, () => 0n) : []
    };
  }

  function setRoleMask(position: number, mask: bigint) {
    const next = [...request.placementRoleMasks];
    next[position - 1] = mask;
    request = { ...request, placementRoleMasks: next };
  }

  function importRoleMask(mask: bigint, height: number) {
    const nextHeight = Math.max(request.height, Math.max(1, Math.min(24, height)));
    request = { ...request, height: nextHeight };
    setRoleMask(selectedRolePosition, trimForwardBoardMask(mask, nextHeight));
  }

  async function run() {
    if (active || validation.length > 0) return;
    if (runtime === 'web') {
      updateWasmCommandText(boundaryRecoveryCommand(request));
      workerController.run();
    } else {
      updateDesktopRequest(boundaryRecoveryDesktopRequest(request, language));
      await startDesktopJob();
    }
  }

  async function cancel() {
    if (runtime === 'web') workerController.cancel();
    else await cancelDesktopJob();
  }

  function statusLabel(status: string): string {
    const key: Record<string, ComponentMessageKey> = {
      normal: 'recoveryNormal',
      'pc-preserving-recovery': 'recoveryPcPreserving',
      'non-pc-recovery': 'recoveryNonPc',
      'no-path-within-declared-scope': 'recoveryNoPath',
      incomplete: 'recoveryIncomplete',
      'population-complete': 'recoveryPopulationComplete',
      'population-incomplete': 'recoveryPopulationIncomplete'
    };
    return label(key[status] ?? 'recoveryIncomplete');
  }
</script>

<svelte:head>
  <title>{label('boundaryRecovery')} · Clearra</title>
</svelte:head>

<WorkspaceShell
  activeMode="recovery"
  {language}
  {active}
  statusLabel={standardLabel(runtimeView.status)}
  workspaceLabel={label('boundaryRecovery')}
  dimensionLabel={standardLabel('fieldHeight')}
  dimensionValue={request.height}
  dimensionMin={1}
  dimensionMax={24}
  cancelLabel={standardLabel('cancel')}
  runLabel={standardLabel('run')}
  runDisabled={validation.length > 0}
  on:language={(event) => { language = event.detail; persistWorkspaceLanguage(language); }}
  on:dimension={(event) => setHeight(event.detail)}
  on:cancel={cancel}
  on:run={run}
>
  <div slot="editor" class="recovery-fields">
    <WorkspaceBoardEditor
      mode="forward" height={request.height} existingMask={request.initialBoardMask}
      targetMask={0n} piecesNeeded={request.stageOneCount} {language}
      labelOverride={label('recoveryInitialField')} enableGlobalPaste={false}
      on:change={(event) => request = { ...request, initialBoardMask: event.detail.existingMask }}
      on:import={(event) => importBoard(event.detail.existingMask, event.detail.height, false)}
    />
    <WorkspaceBoardEditor
      mode="forward" height={request.height} existingMask={request.targetBoardMask}
      targetMask={0n} piecesNeeded={request.placements - request.stageOneCount} {language}
      labelOverride={label('recoveryTargetField')} enableGlobalPaste={false}
      on:change={(event) => request = { ...request, targetBoardMask: event.detail.existingMask }}
      on:import={(event) => importBoard(event.detail.existingMask, event.detail.height, true)}
    />
    {#if request.placementRoleMasks.length > 0}
      <WorkspaceBoardEditor
        mode="forward" height={request.height} existingMask={request.placementRoleMasks[selectedRolePosition - 1] ?? 0n}
        targetMask={0n} piecesNeeded={1} {language}
        labelOverride={`${label('recoveryRolePlacement')} ${selectedRolePosition} (${request.queue[selectedRolePosition - 1]?.toUpperCase() ?? '?'})`}
        enableGlobalPaste={false}
        on:change={(event) => setRoleMask(selectedRolePosition, event.detail.existingMask)}
        on:import={(event) => importRoleMask(event.detail.existingMask, event.detail.height)}
      />
    {:else if request.maxEarlyPlacements === 1}
      <WorkspaceBoardEditor
        mode="forward" height={request.height} existingMask={request.borrowPlacementMask}
        targetMask={0n} piecesNeeded={1} {language}
        labelOverride={label('recoveryBorrowPlacement')} enableGlobalPaste={false}
        on:change={(event) => request = { ...request, borrowPlacementMask: event.detail.existingMask }}
        on:import={(event) => importBorrowPlacement(event.detail.existingMask, event.detail.height)}
      />
    {/if}
  </div>
  <section slot="controls" class="recovery-controls" aria-label={label('boundaryRecovery')}>
    <p>{label('recoveryScope')}</p>
    <label class="check"><input type="checkbox" checked={request.placementRoleMasks.length > 0}
      on:change={(event) => setExactRoles((event.currentTarget as HTMLInputElement).checked)} />{label('recoveryExactRoles')}</label>
    {#if request.placementRoleMasks.length > 0}
      <label><span>{label('recoveryRolePosition')}</span>
        <select value={selectedRolePosition} on:change={(event) => selectedRolePosition = Number((event.currentTarget as HTMLSelectElement).value)}>
          {#each Array.from({ length: request.placements }, (_, index) => index + 1) as position}
            <option value={position}>{position} · {request.queue[position - 1]?.toUpperCase() ?? '?'}</option>
          {/each}
        </select>
      </label>
    {/if}
    <label><span>{label('recoveryQueue')}</span>
      <input value={request.queue} placeholder="IOTSZJL" spellcheck="false"
        on:input={(event) => request = { ...request, queue: (event.currentTarget as HTMLInputElement).value }} />
    </label>
    <label><span>{label('recoveryQueuePattern')}</span>
      <input value={request.queuePattern} placeholder="IJLOSTZP7" spellcheck="false"
        on:input={(event) => request = { ...request, queuePattern: (event.currentTarget as HTMLInputElement).value }} />
    </label>
    <label><span>{label('recoveryStageOneCount')}</span>
      <input type="number" min="1" max="41" value={request.stageOneCount}
        on:input={(event) => request = { ...request, stageOneCount: Number((event.currentTarget as HTMLInputElement).value) }} />
    </label>
    <label><span>{label('recoveryPlacements')}</span>
      <input type="number" min="2" max="42" value={request.placements}
        on:input={(event) => setPlacements(Number((event.currentTarget as HTMLInputElement).value))} />
    </label>
    <label><span>{label('recoveryBorrowPosition')}</span>
      <input type="number" min={request.stageOneCount + 1} max={request.placements} value={request.borrowSourcePosition}
        on:input={(event) => request = { ...request, borrowSourcePosition: Number((event.currentTarget as HTMLInputElement).value) }} />
    </label>
    <label><span>{label('recoveryMaxEarlyPlacements')}</span>
      <select value={request.maxEarlyPlacements} on:change={(event) => request = { ...request, maxEarlyPlacements: Number((event.currentTarget as HTMLSelectElement).value) as 0 | 1 }}>
        <option value="0">0</option><option value="1">1</option>
      </select>
    </label>
    <label><span>{label('rule')}</span>
      <select value={request.rule} on:change={(event) => request = { ...request, rule: (event.currentTarget as HTMLSelectElement).value as RuleProfile }}>
        {#each rules as rule}<option value={rule}>{rule}</option>{/each}
      </select>
    </label>
    <label><span>{standardLabel('spinProfile')}</span>
      <select value={request.spinProfile} on:change={(event) => request = { ...request, spinProfile: (event.currentTarget as HTMLSelectElement).value as SpinProfile }}>
        {#each spins as spin}<option value={spin}>{spin}</option>{/each}
      </select>
    </label>
    <label class="check"><input type="checkbox" checked={request.holdEnabled} on:change={(event) => request = { ...request, holdEnabled: (event.currentTarget as HTMLInputElement).checked }} />{label('enableHold')}</label>
    <label class="check"><input type="checkbox" checked={request.initialB2B} on:change={(event) => request = { ...request, initialB2B: (event.currentTarget as HTMLInputElement).checked }} />{label('recoveryInitialB2b')}</label>
    <label class="check"><input type="checkbox" checked={request.preserveB2BStageOne} on:change={(event) => request = { ...request, preserveB2BStageOne: (event.currentTarget as HTMLInputElement).checked }} />{label('recoveryPreserveStageOne')}</label>
    <label class="check"><input type="checkbox" checked={request.preserveB2BStageTwo} on:change={(event) => request = { ...request, preserveB2BStageTwo: (event.currentTarget as HTMLInputElement).checked }} />{label('recoveryPreserveStageTwo')}</label>
    <label><span>{label('recoveryMaxStates')}</span>
      <input type="number" min="1" max="1000000" value={request.maxStates}
        on:input={(event) => request = { ...request, maxStates: Number((event.currentTarget as HTMLInputElement).value) }} />
    </label>
    {#if request.queuePattern.trim()}
      <label><span>{label('recoveryPatternEvaluations')}</span>
        <input type="number" min="1" max="100000" value={request.maxPatternEvaluations}
          on:input={(event) => request = { ...request, maxPatternEvaluations: Number((event.currentTarget as HTMLInputElement).value) }} />
      </label>
      <label><span>{label('recoveryTotalStates')}</span>
        <input type="number" min="1" max="100000000" value={request.maxTotalStates}
          on:input={(event) => request = { ...request, maxTotalStates: Number((event.currentTarget as HTMLInputElement).value) }} />
      </label>
    {/if}
    {#if validation.length > 0}<p role="alert">{label('recoveryInvalid')}</p>{/if}
  </section>
  <section slot="result" class="recovery-result" aria-live="polite">
    <h2>{label('boundaryRecovery')}</h2>
    {#if payload}
      <p class="outcome">{statusLabel(payload.status)}</p>
      {#if payload.population}
        <p>{payload.population.evaluated_pattern_count} / {payload.population.total_possible_pattern_count} · {payload.population.state_count} states · full future queue knowledge</p>
        <p>{label('recoveryNormalProbability')}: {(Number(payload.population.normal_probability) * 100).toFixed(2)}% · {label('recoveryAdditionalProbability')}: {(Number(payload.population.additional_recovery_probability) * 100).toFixed(2)}%</p>
        <p>{label('recoveryTotalResponseProbability')}: {(Number(payload.population.total_response_probability) * 100).toFixed(2)}% · {label('recoveryUnknownProbability')}: {(Number(payload.population.unknown_probability) * 100).toFixed(2)}%</p>
        <p>PC-preserving recovery: {(Number(payload.population.pc_preserving_recovery_probability) * 100).toFixed(2)}% · non-PC recovery: {(Number(payload.population.non_pc_recovery_probability) * 100).toFixed(2)}% · no path: {(Number(payload.population.no_path_probability) * 100).toFixed(2)}%</p>
        {#if example}<p>Example queue: <code>{example.queue}</code> · {statusLabel(example.status)}</p>{/if}
      {:else}
        <p>{label('recoveryBorrowed')}: {payload.borrowed_stage_two_count} · {label('recoveryCheckpoint')}: {payload.stage_one_checkpoint_step ?? '—'} · PC: {payload.checkpoint_is_pc === null ? '—' : payload.checkpoint_is_pc}</p>
        <p>Full fixed queue · {payload.placement_role_scope} · {payload.normal_states} + {payload.recovery_states} states</p>
      {/if}
      {#if displaySteps.length > 0}
        <h3>{label('recoveryTimeline')}</h3>
        <ol>
          {#each displaySteps as step, index}
            <li><strong>{step.piece}</strong> · queue #{step.source_queue_index + 1} · {step.hold_decision} · ({step.x}, {step.y}) · {step.cleared_lines}L · B2B {step.b2b_active_after ? '✓' : '—'}{displayCheckpoint === index + 1 ? ' · checkpoint' : ''}<code>{step.board_after_mask}</code></li>
          {/each}
        </ol>
      {/if}
    {:else if runtimeView.publicFailures.length > 0}
      <WorkspaceFailureNotice failures={runtimeView.publicFailures} {language} />
    {:else if active}
      <p>{standardLabel('running')}</p>
    {:else}
      <p>{label('reviewTheInputAndRunTheCapability')}</p>
    {/if}
  </section>
</WorkspaceShell>

<style>
  .recovery-fields { display: grid; gap: 18px; min-width: 0; }
  .recovery-controls { display: grid; align-content: start; gap: 12px; padding: 18px; background: white; border-radius: 12px; }
  .recovery-controls > p { margin: 0 0 4px; line-height: 1.5; }
  .recovery-controls label:not(.check) { display: grid; gap: 5px; font-size: 13px; font-weight: 650; }
  .recovery-controls input:not([type='checkbox']), .recovery-controls select { width: 100%; padding: 8px; border: 1px solid #bac8be; border-radius: 6px; }
  .recovery-controls .check { display: flex; align-items: center; gap: 8px; font-size: 13px; }
  .recovery-result { max-width: 1460px; margin: 22px auto; padding: 18px 24px; background: white; border-radius: 12px; }
  .recovery-result h2 { margin: 0 0 12px; }
  .recovery-result .outcome { font-size: 18px; font-weight: 750; }
  .recovery-result ol { padding-left: 24px; }
  .recovery-result li { margin: 8px 0; line-height: 1.5; }
  .recovery-result code { display: block; overflow-wrap: anywhere; font-size: 11px; }
</style>
