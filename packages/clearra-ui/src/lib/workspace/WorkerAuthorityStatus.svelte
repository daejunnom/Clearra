<script lang="ts">
  import { componentMessage, type ComponentMessageKey } from '../i18n/componentCatalog';
  import type { WorkerAuthorityReport } from '../wasm';
  import type { WorkspaceLanguage } from './workspaceI18n';

  export let authority: WorkerAuthorityReport;
  export let language: WorkspaceLanguage;

  $: reason = reasonLabel(authority.reason, language);

  function reasonLabel(
    value: WorkerAuthorityReport['reason'],
    currentLanguage: WorkspaceLanguage
  ): string {
    const labels: Record<WorkerAuthorityReport['reason'], ComponentMessageKey> = {
      'reserved-main-thread': 'workerReservedMainThread',
      'all-logical-processors': 'workerAllLogicalProcessors',
      'explicit-request': 'workerExplicitRequest',
      'host-cap': 'workerHostCap',
      'invalid-request': 'workerInvalidRequest'
    };
    return componentMessage(currentLanguage, labels[value]);
  }
</script>

<p
  class="worker-authority"
  data-snapshot-id={authority.snapshotId}
  data-workers-requested={authority.workersRequested}
  data-workers-effective={authority.workersEffective}
  data-worker-reason={authority.reason}
  aria-live="polite"
>
  {componentMessage(language, 'workerAuthoritySummary', {
    requested: authority.workersRequested,
    effective: authority.workersEffective,
    reason
  })}
</p>

<style>
  .worker-authority {
    color: #596a64;
    font-size: 10px;
    line-height: 1.45;
    margin: 5px 0 0;
    overflow-wrap: anywhere;
  }
</style>
