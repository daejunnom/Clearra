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
  import BoundaryRecoveryFields from './BoundaryRecoveryFields.svelte';
  import BoundaryRecoveryControls from './BoundaryRecoveryControls.svelte';

  export let workerFactory: (() => Worker) | null = null;
  export let runtime: 'web' | 'desktop' = 'web';

  const hostCapabilitySnapshot = getContext<HostCapabilitySnapshot>(HOST_CAPABILITY_SNAPSHOT_CONTEXT)
    ?? sharedBrowserHostCapabilitySnapshot();
  const workerController = new WasmTerminalWorkerController(workerFactory, hostCapabilitySnapshot);
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
  $: if (Number.isInteger(request.placements) && request.placements >= 2 && request.placements <= 42) {
    selectedRolePosition = Math.min(selectedRolePosition, request.placements);
  }
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
      stageOneBoardMask: trimForwardBoardMask(request.stageOneBoardMask, height),
      targetBoardMask: trimForwardBoardMask(request.targetBoardMask, height),
      borrowPlacementMask: trimForwardBoardMask(request.borrowPlacementMask, height),
      placementRoleMasks: request.placementRoleMasks.map((mask) => trimForwardBoardMask(mask, height))
    };
  }

  function importBorrowPlacement(mask: bigint, height: number) {
    const nextHeight = Math.max(request.height, Math.max(1, Math.min(24, height)));
    request = { ...request, height: nextHeight, borrowPlacementMask: trimForwardBoardMask(mask, nextHeight) };
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

  function holdLabel(decision: string): string {
    const key: Record<string, ComponentMessageKey> = {
      none: 'recoveryHoldNone', swap: 'recoveryHoldSwap', store: 'recoveryHoldStore'
    };
    return key[decision] ? label(key[decision]) : decision;
  }

  function roleScopeLabel(scope: string): string {
    const key: Record<string, ComponentMessageKey> = {
      'occupancy-only': 'recoveryOccupancyScope',
      'exact-lock-time': 'recoveryExactScope',
      'bag-piece-exact-lock-time': 'recoveryBagPieceScope'
    };
    return key[scope] ? label(key[scope]) : scope;
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
    <BoundaryRecoveryFields {request} {language} on:change={(event) => request = event.detail} />
    <details class="placement-constraints" open={request.placementRoleMasks.length > 0 || request.maxEarlyPlacements === 1}>
      <summary>{label('recoveryAdvanced')}</summary>
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
    </details>
  </div>
  <div slot="controls">
    <BoundaryRecoveryControls {request} {language} {validation} {selectedRolePosition}
      on:change={(event) => request = event.detail} on:role={(event) => selectedRolePosition = event.detail} />
  </div>
  <section slot="result" class="recovery-result" aria-live="polite">
    <h2>{label('boundaryRecovery')}</h2>
    {#if payload}
      <p class="outcome">{statusLabel(payload.status)}</p>
      {#if payload.population}
        <p>{label('recoveryEvaluatedPatterns')}: {payload.population.evaluated_pattern_count} / {payload.population.total_possible_pattern_count} · {label('recoverySearchStates')}: {payload.population.state_count} · {label('recoveryFullQueueKnowledge')}</p>
        <p>{label('recoveryNormalProbability')}: {(Number(payload.population.normal_probability) * 100).toFixed(2)}% · {label('recoveryAdditionalProbability')}: {(Number(payload.population.additional_recovery_probability) * 100).toFixed(2)}%</p>
        <p>{label('recoveryTotalResponseProbability')}: {(Number(payload.population.total_response_probability) * 100).toFixed(2)}% · {label('recoveryUnknownProbability')}: {(Number(payload.population.unknown_probability) * 100).toFixed(2)}%</p>
        <p>{label('recoveryPcPreservingProbability')}: {(Number(payload.population.pc_preserving_recovery_probability) * 100).toFixed(2)}% · {label('recoveryNonPcProbability')}: {(Number(payload.population.non_pc_recovery_probability) * 100).toFixed(2)}% · {label('recoveryNoPathProbability')}: {(Number(payload.population.no_path_probability) * 100).toFixed(2)}%</p>
        {#if example}<p>{label('recoveryExampleQueue')}: <code>{example.queue}</code> · {statusLabel(example.status)}</p>{/if}
      {:else}
        <p>{label('recoveryBorrowed')}: {payload.borrowed_stage_two_count} · {label('recoveryCheckpoint')}: {payload.stage_one_checkpoint_step ?? '—'} · {label('recoveryCheckpointPc')}: {payload.checkpoint_is_pc === null ? '—' : label(payload.checkpoint_is_pc ? 'recoveryYes' : 'recoveryNo')}</p>
        <p>{label('recoveryFixedQueue')} · {roleScopeLabel(payload.placement_role_scope)} · {label('recoverySearchStates')}: {payload.normal_states} + {payload.recovery_states}</p>
      {/if}
      {#if displaySteps.length > 0}
        <h3>{label('recoveryTimeline')}</h3>
        <ol>
          {#each displaySteps as step, index}
            <li><strong>{step.piece}</strong> · {label('recoverySourceToken')} #{step.source_queue_index + 1} · {label('recoveryPlacementRole')} #{step.placement_role_index + 1} · {holdLabel(step.hold_decision)} · ({step.x}, {step.y}) · {label('recoveryClearedLines')}: {step.cleared_lines} · B2B {step.b2b_active_after ? '✓' : '—'}{displayCheckpoint === index + 1 ? ` · ${label('recoveryCheckpoint')}` : ''}<code>{step.board_after_mask}</code></li>
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
  .placement-constraints summary { cursor: pointer; color: #34403c; font-size: 13px; font-weight: 700; padding: 12px 0; }
  .recovery-result { max-width: 1460px; margin: 22px auto; padding: 18px 24px; background: white; border-radius: 12px; }
  .recovery-result h2 { margin: 0 0 12px; }
  .recovery-result .outcome { font-size: 18px; font-weight: 750; }
  .recovery-result ol { padding-left: 24px; }
  .recovery-result li { margin: 8px 0; line-height: 1.5; }
  .recovery-result code { display: block; overflow-wrap: anywhere; font-size: 11px; }
</style>
