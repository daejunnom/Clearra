<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  import { buildTargetPieceCount, type BuildProbabilityRequest } from './buildProbabilityModel';
  import WorkspaceBoardEditor from './WorkspaceBoardEditor.svelte';
  import type { WorkspaceLanguage } from './workspaceI18n';

  export let request: BuildProbabilityRequest;
  export let language: WorkspaceLanguage;

  const dispatch = createEventDispatcher<{
    change: { existingMask: bigint; targetMask: bigint };
    import: { existingMask: bigint; height: number };
  }>();
</script>

<WorkspaceBoardEditor
  mode="build-probability"
  height={request.height}
  existingMask={request.existingMask}
  targetMask={request.targetMask}
  piecesNeeded={buildTargetPieceCount(request)}
  {language}
  on:change={(event) => dispatch('change', event.detail)}
  on:import={(event) => dispatch('import', event.detail)}
/>
