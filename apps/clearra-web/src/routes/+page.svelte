<script lang="ts">
  import { goto } from '$app/navigation';
  import { base } from '$app/paths';
  import { page } from '$app/stores';
  import { BoundaryRecoveryWorkspace, BuildProbabilityWorkspace, BuildV2Workspace, CtkDrawerWorkspace, DocumentUtilityWorkspace, ForwardSearchWorkspace, OperationSequenceWorkspace, PC_SOLVER_HREF_CONTEXT, PlayerWorkspace, SequenceDependenciesWorkspace, SetupFinderWorkspace, SetupScoreWorkspace, SolverWorkspace, SpinStructureWorkspace, WORKSPACE_MODE_VISIBILITY_CONTEXT, type WorkspaceMode } from '@clearra/ui/workspace';
  import {
    HOST_CAPABILITY_SNAPSHOT_CONTEXT,
    sharedBrowserHostCapabilitySnapshot
  } from '@clearra/ui/wasm';
  import { onMount, setContext } from 'svelte';
  import { resolveCtkViewerQuery } from '../lib/ctkViewerQuery';
  import { installWasmArtifactHotUpdate } from '../lib/wasmArtifactHotUpdate';
  import { isLocalSearchProfileMode, localSearchProfileText } from '../lib/localSearchProfile';
  import Pc4DownloadPanel from '../lib/Pc4DownloadPanel.svelte';
  setContext('clearra.pc4-download-control.v1', Pc4DownloadPanel);

  // Pages publishes this product menu, not every tool implemented by the shared
  // library. Keep direct tool links and the desktop menu independent of it.
  const pagesModes = [
    'pc', 'setup', 'build-probability', 'recovery',
    'damage', 'spin-finder', 'ctk', 'player'
  ] as const satisfies readonly WorkspaceMode[];
  setContext(WORKSPACE_MODE_VISIBILITY_CONTEXT, pagesModes);

  const showLocalProfile = isLocalSearchProfileMode(import.meta.env.MODE);
  let localProfile = '';

  function workerFactory() {
    const worker = new Worker(new URL('../workers/clearraWorker.ts', import.meta.url), {
      type: 'module'
    });
    if (showLocalProfile) worker.addEventListener('message', ({ data }) => {
      const profile = localSearchProfileText(data);
      if (profile !== null) localProfile = profile;
    });
    return worker;
  }

  setContext(PC_SOLVER_HREF_CONTEXT, `${base}/pc-solver`);
  setContext(
    HOST_CAPABILITY_SNAPSHOT_CONTEXT,
    sharedBrowserHostCapabilitySnapshot()
  );

  $: ctkViewer = resolveCtkViewerQuery($page.url);
  $: selectedTool =
    $page.url.searchParams.get('tool') ?? (ctkViewer.document ? 'ctk' : null);

  onMount(() => {
    const removeWasmArtifactHotUpdate = installWasmArtifactHotUpdate(import.meta.hot, import.meta.env.MODE);
    if (!['pc', 'setup', 'setup-score', 'spin-structure', 'build', 'build-probability', 'recovery', 'sequence', 'sequence-dependencies', 'parity', 'fumen', 'render', 'to-gray', 'mirror', 'damage', 'spin-finder', 'ren', 'ctk', 'player'].includes(selectedTool ?? '')) {
      void goto(`${base}/?tool=pc`, { replaceState: true, noScroll: true, keepFocus: true });
    }
    return removeWasmArtifactHotUpdate;
  });
</script>

{#if selectedTool === 'build'}
  <BuildV2Workspace {workerFactory} />
{:else if selectedTool === 'build-probability'}
  <BuildProbabilityWorkspace {workerFactory} />
{:else if selectedTool === 'recovery'}
  <BoundaryRecoveryWorkspace {workerFactory} />
{:else if selectedTool === 'sequence-dependencies'}
  <SequenceDependenciesWorkspace {workerFactory} />
{:else if selectedTool === 'sequence'}
  <OperationSequenceWorkspace {workerFactory} />
{:else if selectedTool === 'parity' || selectedTool === 'fumen' || selectedTool === 'render' || selectedTool === 'to-gray' || selectedTool === 'mirror'}
  {#key selectedTool}
    <DocumentUtilityWorkspace tool={selectedTool} {workerFactory} />
  {/key}
{:else if selectedTool === 'setup'}
  <SetupFinderWorkspace {workerFactory} />
{:else if selectedTool === 'setup-score'}
  <SetupScoreWorkspace {workerFactory} />
{:else if selectedTool === 'spin-structure'}
  <SpinStructureWorkspace {workerFactory} />
{:else if selectedTool === 'ctk'}
  <CtkDrawerWorkspace
    initialDocument={ctkViewer.document ?? undefined}
    viewerMode={ctkViewer.viewer}
    {workerFactory}
  />
{:else if selectedTool === 'player'}
  <PlayerWorkspace {workerFactory} />
{:else if selectedTool === 'damage' || selectedTool === 'spin-finder' || selectedTool === 'ren'}
  {#key selectedTool}
    <ForwardSearchWorkspace tool={selectedTool} {workerFactory} />
  {/key}
{:else}
  <SolverWorkspace runtime="web" {workerFactory} />
{/if}

{#if showLocalProfile && localProfile}
  <details class="local-search-profile">
    <summary>로컬 성능 계측 · 최근 완료한 탐색</summary>
    <p>워커별 시간 합계는 서로 겹치므로 전체 경과 시간이 아닙니다. 입력·필드·해법 ID는 포함하지 않습니다.</p>
    <pre>{localProfile}</pre>
  </details>
{/if}

<style>
  .local-search-profile { margin: 1rem; padding: .75rem; border: 1px solid #8886; }
  .local-search-profile pre { overflow: auto; max-height: 32rem; font-size: .75rem; }
</style>
