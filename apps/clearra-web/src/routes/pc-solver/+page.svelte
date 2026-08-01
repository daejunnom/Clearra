<script lang="ts">
  import { base } from '$app/paths';
  import { PcSolverStandalone } from '@clearra/ui/workspace';

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
