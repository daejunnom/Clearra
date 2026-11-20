<script lang="ts">
  import OperationDocumentWorkspace from './OperationDocumentWorkspace.svelte';

  export let workerFactory: (() => Worker) | null = null;
  export let runtime: 'web' | 'desktop' = 'web';
</script>

<OperationDocumentWorkspace mode="sequence" {runtime} {workerFactory} />
