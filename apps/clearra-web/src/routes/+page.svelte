<script lang="ts">
  import { goto } from '$app/navigation';
  import { base } from '$app/paths';
  import { page } from '$app/stores';
  import { BuildProbabilityWorkspace, CtkDrawerWorkspace, ForwardSearchWorkspace, PC_SOLVER_HREF_CONTEXT, PlayerWorkspace, SetupFinderWorkspace, SolverWorkspace } from '@clearra/ui/workspace';
  import {
    HOST_CAPABILITY_SNAPSHOT_CONTEXT,
    sharedBrowserHostCapabilitySnapshot
  } from '@clearra/ui/wasm';
  import { onMount, setContext } from 'svelte';
  import { resolveCtkViewerQuery } from '../lib/ctkViewerQuery';

  function workerFactory() {
    return new Worker(new URL('../workers/clearraWorker.ts', import.meta.url), {
      type: 'module'
    });
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
    if (!['pc', 'setup', 'build-probability', 'damage', 'spin-finder', 'ctk', 'player'].includes(selectedTool ?? '')) {
      void goto(`${base}/?tool=pc`, { replaceState: true, noScroll: true, keepFocus: true });
    }
  });
</script>

{#if selectedTool === 'build-probability'}
  <BuildProbabilityWorkspace {workerFactory} />
{:else if selectedTool === 'setup'}
  <SetupFinderWorkspace {workerFactory} />
{:else if selectedTool === 'ctk'}
  <CtkDrawerWorkspace
    initialDocument={ctkViewer.document ?? undefined}
    viewerMode={ctkViewer.viewer}
  />
{:else if selectedTool === 'player'}
  <PlayerWorkspace {workerFactory} />
{:else if selectedTool === 'damage' || selectedTool === 'spin-finder'}
  {#key selectedTool}
    <ForwardSearchWorkspace tool={selectedTool} {workerFactory} />
  {/key}
{:else}
  <SolverWorkspace runtime="web" {workerFactory} />
{/if}
