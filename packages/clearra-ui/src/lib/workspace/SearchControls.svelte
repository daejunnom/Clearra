<script lang="ts">
  import { Database, Gauge } from '@lucide/svelte';
  import { createEventDispatcher } from 'svelte';

  import {
    type SolverWorkspaceRequest,
    type WorkspaceValidationCode
  } from './solverWorkspaceModel';
  import QueueTextInput from '../components/QueueTextInput.svelte';
  import QueuePatternHelp from './QueuePatternHelp.svelte';
  import WorkspaceControlPanel from './WorkspaceControlPanel.svelte';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';

  export let request: SolverWorkspaceRequest;
  export let language: WorkspaceLanguage;
  export let validationCodes: WorkspaceValidationCode[] = [];
  export let tablebaseControlAvailable = false;
  export let dependencyDagControlAvailable = false;
  export let tablebaseStatus: 'disabled' | 'loading' | 'ready' | 'unavailable' = 'disabled';
  export let tablebaseByteLength = 0;

  const dispatch = createEventDispatcher<{ change: SolverWorkspaceRequest }>();
  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);
  $: tilingOnly = request.scoreMode === 'tiling';
  $: failedQueueOnly = request.scoreMode === 'failed-queue';

  function patch(change: Partial<SolverWorkspaceRequest>) {
    dispatch('change', { ...request, ...change });
  }

  $: tablebaseStatusLabel = tablebaseMessage(
    tablebaseStatus,
    tablebaseByteLength,
    language
  );

  function tablebaseMessage(
    status: typeof tablebaseStatus,
    byteLength: number,
    currentLanguage: WorkspaceLanguage
  ): string {
    if (status === 'ready') {
      return workspaceMessage(currentLanguage, 'tablebaseReady', {
        size: tablebaseSize(byteLength)
      });
    }
    if (status === 'loading') {
      const loading = workspaceMessage(currentLanguage, 'tablebaseLoading');
      return byteLength === 0 ? loading : `${loading} · ${tablebaseSize(byteLength)}`;
    }
    if (status === 'unavailable') {
      return workspaceMessage(currentLanguage, 'tablebaseUnavailable');
    }
    return workspaceMessage(currentLanguage, 'tablebaseDisabled');
  }

  function tablebaseSize(byteLength: number): string {
    return byteLength < 1024 * 1024
      ? `${(byteLength / 1024).toFixed(1)} KiB`
      : `${(byteLength / (1024 * 1024)).toFixed(1)} MiB`;
  }

</script>

<WorkspaceControlPanel ariaLabel={label('search')}>
  <section class="workspace-control-section">
    <h2 class="workspace-control-heading"><Database size={16} strokeWidth={1.8} />{label('source')}</h2>
    <label class="workspace-field wide">
      <span>{label('queuePattern')}</span>
      <QueueTextInput
        class="workspace-queue-input"
        value={request.queue}
        maxlength="64"
        placeholder={label('queuePlaceholder')}
        spellcheck="false"
        on:value={(event) => patch({ queue: event.detail })}
      />
    </label>
    <QueuePatternHelp {language} />

    <div class="workspace-switch-row">
      <label class="workspace-switch-label">
        <input type="checkbox" checked={request.holdEnabled} on:change={(event) => patch({ holdEnabled: (event.currentTarget as HTMLInputElement).checked })} />
        <span class="workspace-switch" aria-hidden="true"></span>
        <span>{label('hold')}</span>
      </label>
    </div>
    <label class="workspace-field wide">
      <span>{label('queueKnowledge')}</span>
      <select
        value={request.queueKnowledge}
        disabled={tilingOnly}
        on:change={(event) => patch({
          queueKnowledge: (event.currentTarget as HTMLSelectElement).value as SolverWorkspaceRequest['queueKnowledge']
        })}
      >
        <option value="oracle">{label('queueKnowledgeOracle')}</option>
        <option value="visible-7">{label('queueKnowledgeVisibleSeven')}</option>
      </select>
    </label>
  </section>

  <section class="workspace-control-section">
    <h2 class="workspace-control-heading"><Gauge size={16} strokeWidth={1.8} />{label('search')}</h2>
    <div class="workspace-field-grid">
      <label class="workspace-field">
        <span>{label('scoreMode')}</span>
        <select value={request.scoreMode} on:change={(event) => patch({ scoreMode: (event.currentTarget as HTMLSelectElement).value as SolverWorkspaceRequest['scoreMode'] })}>
          <option value="tiling">{label('tilingOnly')}</option>
          <option value="off">{label('scoreOff')}</option>
          <option value="minimum-cover">{label('minimumSolutions')}</option>
          <option value="summary">{label('scoreSummary')}</option>
          <option value="failed-queue">{label('failedQueues')}</option>
        </select>
      </label>
      <label class="workspace-field">
        <span>{label('rule')}</span>
        <select value={request.rule} disabled={tilingOnly} on:change={(event) => patch({ rule: (event.currentTarget as HTMLSelectElement).value as SolverWorkspaceRequest['rule'] })}>
          <option value="srs-plus">SRS+</option>
          <option value="srs">SRS</option>
          <option value="srs-x">SRS-X</option>
          <option value="jstris-180">Jstris 180</option>
        </select>
      </label>
      <label class="workspace-field">
        <span>{label('initialB2B')}</span>
        <input
          type="number"
          min="0"
          step="1"
          value={request.initialB2B}
          disabled={tilingOnly || failedQueueOnly || request.scoreMode !== 'summary'}
          on:input={(event) => patch({ initialB2B: Number((event.currentTarget as HTMLInputElement).value) })}
        />
      </label>
      <label class="workspace-field">
        <span>{label('scoreProfile')}</span>
        <select
          value={request.scoreProfile}
          disabled={tilingOnly || failedQueueOnly || request.scoreMode !== 'summary'}
          on:change={(event) => patch({ scoreProfile: (event.currentTarget as HTMLSelectElement).value as SolverWorkspaceRequest['scoreProfile'] })}
        >
          <option value="tetrio">{label('scoreProfileTetrio')}</option>
          <option value="guideline">{label('scoreProfileGuideline')}</option>
          <option value="jstris-ultra">{label('scoreProfileJstrisUltra')}</option>
        </select>
      </label>
      <label class="workspace-field">
        <span>{label('spinProfile')}</span>
        <select
          value={request.spinProfile}
          disabled={tilingOnly || (request.scoreMode !== 'summary' && !request.preserveB2B)}
          on:change={(event) => patch({ spinProfile: (event.currentTarget as HTMLSelectElement).value as SolverWorkspaceRequest['spinProfile'] })}
        >
          <option value="t-spins">T-Spins</option>
          <option value="t-spins-plus">T-Spins+</option>
          <option value="all-spin">All-Spin</option>
          <option value="all-spin-plus">All-Spin+</option>
          <option value="all-mini">All-Mini</option>
          <option value="all-mini-plus">All-Mini+</option>
        </select>
      </label>
    </div>
    <div class="workspace-toggle-grid policy-toggle">
      <div class="b2b-preservation-control">
        <label class="workspace-switch-label">
          <input
            type="checkbox"
            checked={request.preserveB2B}
            disabled={tilingOnly}
            on:change={(event) => patch({ preserveB2B: (event.currentTarget as HTMLInputElement).checked })}
          />
          <span class="workspace-switch" aria-hidden="true"></span>
          <span>{label('preserveB2B')}</span>
        </label>
      </div>
      <label class="workspace-switch-label">
        <input
          type="checkbox"
          checked={request.solutionProbabilities}
          disabled={tilingOnly || failedQueueOnly}
          on:change={(event) => patch({ solutionProbabilities: (event.currentTarget as HTMLInputElement).checked })}
        />
        <span class="workspace-switch" aria-hidden="true"></span>
        <span>{label('solutionProbabilities')}</span>
      </label>
    </div>
    {#if tablebaseControlAvailable}
      <div class="tablebase-control">
        <label class="workspace-switch-label">
          <input
            type="checkbox"
            checked={request.tablebaseEnabled}
            disabled={tilingOnly}
            on:change={(event) => patch({
              tablebaseEnabled: (event.currentTarget as HTMLInputElement).checked
            })}
          />
          <span class="workspace-switch" aria-hidden="true"></span>
          <span>{label('tablebase')}</span>
        </label>
        <small class="workspace-field-help">{label('tablebaseHelp')}</small>
        <span class="tablebase-status" aria-live="polite">{tablebaseStatusLabel}</span>
      </div>
    {/if}
    {#if dependencyDagControlAvailable}
      <div class="dependency-dag-control">
        <label class="workspace-switch-label">
          <input
            type="checkbox"
            checked={request.precomputeBuildDependencies}
            disabled={tilingOnly}
            on:change={(event) => patch({
              precomputeBuildDependencies: (event.currentTarget as HTMLInputElement).checked
            })}
          />
          <span class="workspace-switch" aria-hidden="true"></span>
          <span>{label('precomputeBuildDependencies')}</span>
        </label>
        <small class="workspace-field-help">{label('precomputeBuildDependenciesHelp')}</small>
      </div>
    {/if}
  </section>

  {#if validationCodes.length}
    <ul class="workspace-validation" aria-live="polite">
      {#each validationCodes as code}
        <li>{label(code)}</li>
      {/each}
    </ul>
  {/if}
</WorkspaceControlPanel>

<style>
  .policy-toggle { grid-template-columns: 1fr; }
  .b2b-preservation-control { display: grid; gap: 5px; }
  .tablebase-control, .dependency-dag-control {
    align-content: start;
    display: grid;
    gap: 5px;
    margin-top: 14px;
    min-width: 0;
  }
  .tablebase-control + .dependency-dag-control { margin-top: 18px; }
  .tablebase-control :global(.workspace-field-help),
  .dependency-dag-control :global(.workspace-field-help) {
    display: block;
    overflow-wrap: anywhere;
  }
  .tablebase-status { color: #3f5c57; font-size: 11px; font-weight: 700; }
</style>
