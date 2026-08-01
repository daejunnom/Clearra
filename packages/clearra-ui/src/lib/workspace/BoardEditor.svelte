<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  import { scenarioPieceWindow, type SolverWorkspaceRequest } from './solverWorkspaceModel';
  import WorkspaceBoardEditor from './WorkspaceBoardEditor.svelte';
  import type { WorkspaceLanguage } from './workspaceI18n';

  export let request: SolverWorkspaceRequest;
  export let language: WorkspaceLanguage;
  export let showImport = true;
  export let showStats = true;
  export let showToolbar = true;
  export let displayHeight: number | null = null;

  $: editorHeight = displayHeight ?? request.lines;

  const dispatch = createEventDispatcher<{
    change: bigint;
    import: { boardMask: bigint; lines: number };
  }>();
</script>

<WorkspaceBoardEditor
  mode="pc"
  height={editorHeight}
  existingMask={request.boardMask}
  targetMask={0n}
  piecesNeeded={scenarioPieceWindow(request)}
  {language}
  {showImport}
  {showStats}
  {showToolbar}
  on:change={(event) => dispatch('change', event.detail.existingMask)}
  on:import={(event) => dispatch('import', { boardMask: event.detail.existingMask, lines: event.detail.height })}
/>
