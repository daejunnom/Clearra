<script lang="ts">
  import BoundaryRecoveryWorkspace from './BoundaryRecoveryWorkspace.svelte';
  import BuildProbabilityWorkspace from './BuildProbabilityWorkspace.svelte';
  import BuildV2Workspace from './BuildV2Workspace.svelte';
  import CtkDrawerWorkspace from './CtkDrawerWorkspace.svelte';
  import DocumentUtilityWorkspace from './DocumentUtilityWorkspace.svelte';
  import ForwardSearchWorkspace from './ForwardSearchWorkspace.svelte';
  import OperationSequenceWorkspace from './OperationSequenceWorkspace.svelte';
  import PlayerWorkspace from './PlayerWorkspace.svelte';
  import SequenceDependenciesWorkspace from './SequenceDependenciesWorkspace.svelte';
  import SetupFinderWorkspace from './SetupFinderWorkspace.svelte';
  import SetupScoreWorkspace from './SetupScoreWorkspace.svelte';
  import SolverWorkspace from './SolverWorkspace.svelte';
  import SpinStructureWorkspace from './SpinStructureWorkspace.svelte';
  import { workspaceToolRoute, type WorkspaceMode } from './workspaceMode';
  import type { WorkspaceRuntimeKind } from './workspaceRuntime';

  export let selectedTool: WorkspaceMode | null;
  export let runtime: WorkspaceRuntimeKind;
  export let workerFactory: (() => Worker) | null = null;
  export let ctkDocument: string | undefined = undefined;
  export let ctkViewerMode = false;

  $: route = workspaceToolRoute(selectedTool);
</script>

{#if route.kind === 'build'}
  <BuildV2Workspace {runtime} {workerFactory} />
{:else if route.kind === 'build-probability'}
  <BuildProbabilityWorkspace {runtime} {workerFactory} />
{:else if route.kind === 'recovery'}
  <BoundaryRecoveryWorkspace {runtime} {workerFactory} />
{:else if route.kind === 'sequence-dependencies'}
  <SequenceDependenciesWorkspace {runtime} {workerFactory} />
{:else if route.kind === 'sequence'}
  <OperationSequenceWorkspace {runtime} {workerFactory} />
{:else if route.kind === 'document-utility'}
  {#key route.tool}
    <DocumentUtilityWorkspace tool={route.tool} {runtime} {workerFactory} />
  {/key}
{:else if route.kind === 'setup'}
  <SetupFinderWorkspace {runtime} {workerFactory} />
{:else if route.kind === 'setup-score'}
  <SetupScoreWorkspace {runtime} {workerFactory} />
{:else if route.kind === 'spin-structure'}
  <SpinStructureWorkspace {runtime} {workerFactory} />
{:else if route.kind === 'ctk'}
  <CtkDrawerWorkspace initialDocument={ctkDocument} viewerMode={ctkViewerMode} {workerFactory} />
{:else if route.kind === 'player'}
  <PlayerWorkspace {runtime} {workerFactory} />
{:else if route.kind === 'forward-search'}
  {#key route.tool}
    <ForwardSearchWorkspace tool={route.tool} {runtime} {workerFactory} />
  {/key}
{:else}
  <SolverWorkspace {runtime} {workerFactory} />
{/if}
