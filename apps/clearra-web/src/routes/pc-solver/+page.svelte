<script lang="ts">
  import { base } from '$app/paths';
  import { PcSolverStandalone } from '@clearra/ui/workspace';
  import {
    HOST_CAPABILITY_SNAPSHOT_CONTEXT,
    sharedBrowserHostCapabilitySnapshot
  } from '@clearra/ui/wasm';
  import { setContext } from 'svelte';

  setContext(
    HOST_CAPABILITY_SNAPSHOT_CONTEXT,
    sharedBrowserHostCapabilitySnapshot()
  );

  function workerFactory() {
    return new Worker(new URL('../../workers/clearraWorker.ts', import.meta.url), {
      type: 'module'
    });
  }
</script>

<PcSolverStandalone
  {workerFactory}
  basePath={`${base}/pc-solver`}
  homeHref={`${base}/?tool=pc`}
/>
