<script lang="ts">
  import { base } from '$app/paths';
  import { page } from '$app/stores';
  import { PcSolverStandalone } from '@clearra/ui/workspace';
  import {
    HOST_CAPABILITY_SNAPSHOT_CONTEXT,
    sharedBrowserHostCapabilitySnapshot
  } from '@clearra/ui/wasm';
  import { onMount, setContext } from 'svelte';
  import { installWasmArtifactHotUpdate } from '../../../lib/wasmArtifactHotUpdate';

  setContext(
    HOST_CAPABILITY_SNAPSHOT_CONTEXT,
    sharedBrowserHostCapabilitySnapshot()
  );

  function workerFactory() {
    return new Worker(new URL('../../../workers/clearraWorker.ts', import.meta.url), {
      type: 'module'
    });
  }

  $: initialPathState = $page.params.state ?? '';

  onMount(() => installWasmArtifactHotUpdate(import.meta.hot));
</script>

<PcSolverStandalone
  {workerFactory}
  {initialPathState}
  basePath={`${base}/pc-solver`}
  homeHref={`${base}/?tool=pc`}
/>
