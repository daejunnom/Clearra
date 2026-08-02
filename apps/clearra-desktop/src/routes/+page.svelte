<script lang="ts">
  import { goto } from '$app/navigation';
  import { base } from '$app/paths';
  import { page } from '$app/stores';
  import {
    BuildProbabilityWorkspace,
    CtkDrawerWorkspace,
    ForwardSearchWorkspace,
    SetupFinderWorkspace,
    SolverWorkspace
  } from '@clearra/ui/workspace';
  import { onMount } from 'svelte';

  const tools = ['pc', 'setup', 'build-probability', 'damage', 'spin-finder', 'ctk'] as const;

  $: selectedTool = $page.url.searchParams.get('tool') ?? 'pc';

  onMount(() => {
    if (!tools.includes(selectedTool as (typeof tools)[number])) {
      void goto(`${base}/?tool=pc`, { replaceState: true, noScroll: true, keepFocus: true });
    }
  });
</script>

{#if selectedTool === 'build-probability'}
  <BuildProbabilityWorkspace runtime="desktop" />
{:else if selectedTool === 'setup'}
  <SetupFinderWorkspace runtime="desktop" />
{:else if selectedTool === 'ctk'}
  <CtkDrawerWorkspace />
{:else if selectedTool === 'damage' || selectedTool === 'spin-finder'}
  {#key selectedTool}
    <ForwardSearchWorkspace tool={selectedTool} runtime="desktop" />
  {/key}
{:else}
  <SolverWorkspace runtime="desktop" />
{/if}
