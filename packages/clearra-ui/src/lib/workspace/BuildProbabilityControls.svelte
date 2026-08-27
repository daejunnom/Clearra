<script lang="ts">
  import { Database, Gauge } from '@lucide/svelte';
  import { createEventDispatcher } from 'svelte';

  import {
    BUILD_SOURCE_PIECES_MAX,
    BUILD_SOURCE_PIECES_MIN,
    updateBuildProbabilityDraft,
    type BuildProbabilityRequest,
    type BuildProbabilityValidationCode
  } from './buildProbabilityModel';
  import QueueTextInput from '../components/QueueTextInput.svelte';
  import QueuePatternHelp from './QueuePatternHelp.svelte';
  import WorkspaceControlPanel from './WorkspaceControlPanel.svelte';
  import WorkerAuthorityStatus from './WorkerAuthorityStatus.svelte';
  import type { WorkerAuthorityReport } from '../wasm';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';

  export let request: BuildProbabilityRequest;
  export let language: WorkspaceLanguage;
  export let validationCodes: BuildProbabilityValidationCode[] = [];
  export let workerAuthority: WorkerAuthorityReport;

  const dispatch = createEventDispatcher<{ change: BuildProbabilityRequest }>();
  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);

  function patch(change: Partial<BuildProbabilityRequest>) {
    dispatch('change', updateBuildProbabilityDraft(request, change));
  }

  function setAggregation(aggregation: BuildProbabilityRequest['aggregation']) {
    patch({ aggregation });
  }

  function setSourcePieces(input: HTMLInputElement) {
    patch({ sourcePieces: input.value === '' ? null : input.valueAsNumber });
  }
</script>

<WorkspaceControlPanel ariaLabel={label('buildProbability')}>
  <section class="workspace-control-section">
    <h2 class="workspace-control-heading"><Database size={16} strokeWidth={1.8} />{label('source')}</h2>
    <label class="workspace-field">
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

    <label class="workspace-field">
      <span>{label('sourcePieces')}</span>
      <input
        type="number"
        min={BUILD_SOURCE_PIECES_MIN}
        max={BUILD_SOURCE_PIECES_MAX}
        step="1"
        value={request.sourcePieces ?? ''}
        placeholder={label('sourcePiecesAutomatic')}
        aria-describedby="build-source-pieces-help"
        on:input={(event) => setSourcePieces(event.currentTarget as HTMLInputElement)}
      />
      <small id="build-source-pieces-help" class="workspace-field-help">{label('sourcePiecesHelp')}</small>
    </label>

    <div class="workspace-switch-row">
      <label class="workspace-switch-label">
        <input type="checkbox" checked={request.holdEnabled} on:change={(event) => patch({ holdEnabled: (event.currentTarget as HTMLInputElement).checked })} />
        <span class="workspace-switch" aria-hidden="true"></span><span>{label('hold')}</span>
      </label>
    </div>
  </section>

  <section class="workspace-control-section">
    <h2 class="workspace-control-heading"><Gauge size={16} strokeWidth={1.8} />{label('search')}</h2>
    <div class="workspace-field-grid">
      <label class="workspace-field">
        <span>{label('scoreMode')}</span>
        <select
          value={request.aggregation}
          on:change={(event) => setAggregation((event.currentTarget as HTMLSelectElement).value as BuildProbabilityRequest['aggregation'])}
        >
          <option value="tiling">{label('tilingOnly')}</option>
          <option value="buildability">{label('buildProbability')}</option>
          <option value="spin">{label('spinSearch')}</option>
        </select>
      </label>
      <label class="workspace-field">
        <span>{label('rule')}</span>
        <select
          value={request.rule}
          disabled={request.aggregation === 'tiling'}
          on:change={(event) => patch({ rule: (event.currentTarget as HTMLSelectElement).value as BuildProbabilityRequest['rule'] })}
        >
          <option value="srs-plus">SRS+</option>
          <option value="srs">SRS</option>
          <option value="srs-x">SRS-X</option>
          <option value="jstris-180">Jstris 180</option>
        </select>
      </label>
      <label class="workspace-field">
        <span>{label('spinProfile')}</span>
        <select
          value={request.spinProfile}
          disabled={request.aggregation === 'tiling' || (request.aggregation === 'buildability' && !request.preserveB2B)}
          on:change={(event) => patch({ spinProfile: (event.currentTarget as HTMLSelectElement).value as BuildProbabilityRequest['spinProfile'] })}
        >
          <option value="t-spins">T-Spins</option>
          <option value="t-spins-plus">T-Spins+</option>
          <option value="all-spin">All-Spin</option>
          <option value="all-spin-plus">All-Spin+</option>
          <option value="all-mini">All-Mini</option>
          <option value="all-mini-plus">All-Mini+</option>
        </select>
      </label>
      <label class="workspace-field">
        <span>{label('finesseCalculation')}</span>
        <select
          value={request.finesse}
          disabled={request.aggregation === 'tiling'}
          on:change={(event) => patch({ finesse: (event.currentTarget as HTMLSelectElement).value as BuildProbabilityRequest['finesse'] })}
        >
          <option value="off">{label('finesseOff')}</option>
          <option value="inputs">{label('finesseInputs')}</option>
        </select>
      </label>
      <label class="workspace-field">
        <span>{label('finessePatternKnowledge')}</span>
        <select
          value={request.patternKnowledge}
          disabled={request.aggregation === 'tiling' || request.finesse === 'off'}
          on:change={(event) => patch({ patternKnowledge: (event.currentTarget as HTMLSelectElement).value as BuildProbabilityRequest['patternKnowledge'] })}
        >
          <option value="both">{label('finessePatternBoth')}</option>
          <option value="oracle">{label('finessePatternOracle')}</option>
          <option value="visible-7">{label('finessePatternVisibleSeven')}</option>
        </select>
        <small class="workspace-field-help">{label('finessePatternKnowledgeHelp')}</small>
      </label>
    </div>
    <div class="b2b-preservation-control">
      <label class="workspace-switch-label">
        <input
          type="checkbox"
          checked={request.preserveB2B}
          disabled={request.aggregation === 'tiling'}
          on:change={(event) => patch({ preserveB2B: (event.currentTarget as HTMLInputElement).checked })}
        />
        <span class="workspace-switch" aria-hidden="true"></span>
        <span>{label('preserveB2B')}</span>
      </label>
    </div>
    <div class="solution-probabilities-control">
      <label class="workspace-switch-label">
        <input
          type="checkbox"
          checked={request.solutionProbabilities}
          disabled={request.aggregation === 'tiling'}
          on:change={(event) => patch({
            solutionProbabilities: (event.currentTarget as HTMLInputElement).checked
          })}
        />
        <span class="workspace-switch" aria-hidden="true"></span>
        <span>{label('solutionProbabilities')}</span>
      </label>
    </div>
    <div class="worker-policy-control">
      <label class="workspace-switch-label">
        <input
          type="checkbox"
          checked={request.useAllLogicalProcessors}
          on:change={(event) => patch({
            useAllLogicalProcessors: (event.currentTarget as HTMLInputElement).checked
          })}
        />
        <span class="workspace-switch" aria-hidden="true"></span>
        <span>{label('useAllThreads')}</span>
      </label>
      <WorkerAuthorityStatus authority={workerAuthority} {language} />
    </div>
    <div class="dependency-analysis-control">
      <label class="workspace-switch-label">
        <input
          type="checkbox"
          checked={request.precomputeBuildDependencies}
          disabled={request.aggregation === 'tiling'}
          on:change={(event) => patch({
            precomputeBuildDependencies: (event.currentTarget as HTMLInputElement).checked
          })}
        />
        <span class="workspace-switch" aria-hidden="true"></span>
        <span>{label('precomputeBuildDependencies')}</span>
      </label>
      <small class="workspace-field-help">{label('precomputeBuildDependenciesHelp')}</small>
    </div>
  </section>

  {#if validationCodes.length}
    <ul class="workspace-validation" aria-live="polite">
      {#each validationCodes as code}<li>{label(code)}</li>{/each}
    </ul>
  {/if}
</WorkspaceControlPanel>

<style>
  .b2b-preservation-control { display: grid; gap: 5px; margin-top: 14px; }
  .solution-probabilities-control { display: grid; gap: 5px; margin-top: 14px; }
  .worker-policy-control { display: grid; gap: 5px; margin-top: 14px; }
  .dependency-analysis-control { display: grid; gap: 5px; margin-top: 14px; }
</style>
