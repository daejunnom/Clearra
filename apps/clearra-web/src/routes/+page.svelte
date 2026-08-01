<script lang="ts">
  import { goto } from '$app/navigation';
  import { base } from '$app/paths';
  import { page } from '$app/stores';
  import { BuildProbabilityWorkspace, CtkDrawerWorkspace, ForwardSearchWorkspace, SetupFinderWorkspace, SolverWorkspace } from '@clearra/ui/workspace';
  import { onMount } from 'svelte';
  import { resolveCtkViewerQuery } from '../lib/ctkViewerQuery';

  function workerFactory() {
    return new Worker(new URL('../workers/clearraWorker.ts', import.meta.url), {
      type: 'module'
    });
  }

  $: ctkViewer = resolveCtkViewerQuery($page.url);
  $: selectedTool =
    $page.url.searchParams.get('tool') ?? (ctkViewer.document ? 'ctk' : null);

  onMount(() => {
    if (!['pc', 'setup', 'build-probability', 'damage', 'spin-finder', 'ctk'].includes(selectedTool ?? '')) {
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
{:else if selectedTool === 'damage' || selectedTool === 'spin-finder'}
  <ForwardSearchWorkspace tool={selectedTool} {workerFactory} />
{:else}
  <SolverWorkspace runtime="web" {workerFactory} />
{/if}
