<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  import { scenarioPieceWindow, type SolverWorkspaceRequest } from './solverWorkspaceModel';
  import WorkspaceBoardEditor from './WorkspaceBoardEditor.svelte';
  import type { WorkspaceLanguage } from './workspaceI18n';

  export let request: SolverWorkspaceRequest;
  export let language: WorkspaceLanguage;

  const dispatch = createEventDispatcher<{
    change: bigint;
    import: { boardMask: bigint; lines: number };
  }>();
</script>

<WorkspaceBoardEditor
  mode="pc"
  height={request.lines}
  existingMask={request.boardMask}
  targetMask={0n}
  piecesNeeded={scenarioPieceWindow(request)}
  {language}
  on:change={(event) => dispatch('change', event.detail.existingMask)}
  on:import={(event) => dispatch('import', { boardMask: event.detail.existingMask, lines: event.detail.height })}
/>
