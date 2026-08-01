<script lang="ts">
  import { base } from '$app/paths';
  import { page } from '$app/stores';
  import { PcSolverStandalone } from '@clearra/ui/workspace';

  function workerFactory() {
    return new Worker(new URL('../../../workers/clearraWorker.ts', import.meta.url), {
      type: 'module'
    });
  }

  $: initialPathState = $page.params.state ?? '';
</script>

<PcSolverStandalone
  {workerFactory}
  {initialPathState}
  basePath={`${base}/pc-solver`}
  homeHref={`${base}/?tool=pc`}
/>
