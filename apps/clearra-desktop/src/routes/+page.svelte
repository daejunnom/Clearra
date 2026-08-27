<script lang="ts">
  import { goto } from '$app/navigation';
  import { base } from '$app/paths';
  import { page } from '$app/stores';
  import {
    BuildProbabilityWorkspace,
    BuildV2Workspace,
    CtkDrawerWorkspace,
    DocumentUtilityWorkspace,
    ForwardSearchWorkspace,
    OperationSequenceWorkspace,
    PlayerWorkspace,
    SequenceDependenciesWorkspace,
    SetupFinderWorkspace,
    SetupScoreWorkspace,
    SolverWorkspace,
    SpinStructureWorkspace
  } from '@clearra/ui/workspace';
  import { onMount } from 'svelte';

  const tools = ['pc', 'setup', 'setup-score', 'spin-structure', 'build', 'build-probability', 'sequence', 'sequence-dependencies', 'parity', 'fumen', 'render', 'to-gray', 'mirror', 'damage', 'spin-finder', 'ren', 'ctk', 'player'] as const;

  $: selectedTool = $page.url.searchParams.get('tool') ?? 'pc';

  onMount(() => {
    if (!tools.includes(selectedTool as (typeof tools)[number])) {
      void goto(`${base}/?tool=pc`, { replaceState: true, noScroll: true, keepFocus: true });
    }
  });
</script>

{#if selectedTool === 'build'}
  <BuildV2Workspace runtime="desktop" />
{:else if selectedTool === 'build-probability'}
  <BuildProbabilityWorkspace runtime="desktop" />
{:else if selectedTool === 'sequence-dependencies'}
  <SequenceDependenciesWorkspace runtime="desktop" />
{:else if selectedTool === 'sequence'}
  <OperationSequenceWorkspace runtime="desktop" />
{:else if selectedTool === 'parity' || selectedTool === 'fumen' || selectedTool === 'render' || selectedTool === 'to-gray' || selectedTool === 'mirror'}
  {#key selectedTool}
    <DocumentUtilityWorkspace tool={selectedTool} runtime="desktop" />
  {/key}
{:else if selectedTool === 'setup'}
  <SetupFinderWorkspace runtime="desktop" />
{:else if selectedTool === 'setup-score'}
  <SetupScoreWorkspace runtime="desktop" />
{:else if selectedTool === 'spin-structure'}
  <SpinStructureWorkspace runtime="desktop" />
{:else if selectedTool === 'ctk'}
  <CtkDrawerWorkspace />
{:else if selectedTool === 'player'}
  <PlayerWorkspace runtime="desktop" />
{:else if selectedTool === 'damage' || selectedTool === 'spin-finder' || selectedTool === 'ren'}
  {#key selectedTool}
    <ForwardSearchWorkspace tool={selectedTool} runtime="desktop" />
  {/key}
{:else}
  <SolverWorkspace runtime="desktop" />
{/if}
