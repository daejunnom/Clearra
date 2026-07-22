<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import { BuildProbabilityWorkspace, ForwardSearchWorkspace, SolverWorkspace } from '@clearra/ui/workspace';
  import { onMount } from 'svelte';

  function workerFactory() {
    return new Worker(new URL('../workers/clearraWorker.ts', import.meta.url), {
      type: 'module'
    });
  }

  $: selectedTool = $page.url.searchParams.get('tool');

  onMount(() => {
    if (!['pc', 'build-probability', 'damage', 'spin-finder'].includes(selectedTool ?? '')) {
      void goto('/?tool=pc', { replaceState: true, noScroll: true, keepFocus: true });
    }
  });
</script>

{#if selectedTool === 'build-probability'}
  <BuildProbabilityWorkspace {workerFactory} />
{:else if selectedTool === 'damage' || selectedTool === 'spin-finder'}
  <ForwardSearchWorkspace tool={selectedTool} {workerFactory} />
{:else}
  <SolverWorkspace runtime="web" {workerFactory} />
{/if}
