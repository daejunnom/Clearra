<script lang="ts">
  import { Database, Gauge, Zap } from '@lucide/svelte';
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

  const dispatch = createEventDispatcher<{ change: SolverWorkspaceRequest }>();
  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);

  function patch(change: Partial<SolverWorkspaceRequest>) {
    dispatch('change', { ...request, ...change });
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
    <QueuePatternHelp {language} explainInitialHold />

    <div class="workspace-switch-row">
      <label class="workspace-switch-label">
        <input type="checkbox" checked={request.holdEnabled} on:change={(event) => patch({ holdEnabled: (event.currentTarget as HTMLInputElement).checked })} />
        <span class="workspace-switch" aria-hidden="true"></span>
        <span>{label('hold')}</span>
      </label>
      {#if request.holdEnabled}
        <label class="workspace-field workspace-inline-select">
          <span>{label('holdPiece')}</span>
          <select value={request.holdPiece} on:change={(event) => patch({ holdPiece: (event.currentTarget as HTMLSelectElement).value as SolverWorkspaceRequest['holdPiece'] })}>
            <option value="empty">{label('empty')}</option>
            {#each ['I', 'O', 'T', 'S', 'Z', 'J', 'L'] as piece}
              <option value={piece}>{piece}</option>
            {/each}
          </select>
        </label>
      {/if}
    </div>
  </section>

  <section class="workspace-control-section">
    <h2 class="workspace-control-heading"><Gauge size={16} strokeWidth={1.8} />{label('search')}</h2>
    <div class="workspace-field-grid">
      <label class="workspace-field">
        <span>{label('scoreMode')}</span>
        <select value={request.scoreMode} on:change={(event) => patch({ scoreMode: (event.currentTarget as HTMLSelectElement).value as SolverWorkspaceRequest['scoreMode'] })}>
          <option value="off">{label('scoreOff')}</option>
          <option value="minimum-cover">{label('minimumSolutions')}</option>
          <option value="summary">{label('scoreSummary')}</option>
        </select>
      </label>
      <label class="workspace-field">
        <span>{label('rule')}</span>
        <select value={request.rule} on:change={(event) => patch({ rule: (event.currentTarget as HTMLSelectElement).value as SolverWorkspaceRequest['rule'] })}>
          <option value="srs-plus">SRS+</option>
          <option value="srs">SRS</option>
          <option value="srs-x">SRS-X</option>
        </select>
      </label>
      <label class="workspace-field">
        <span>{label('initialB2B')}</span>
        <input
          type="number"
          min="0"
          step="1"
          value={request.initialB2B}
          disabled={request.scoreMode !== 'summary'}
          on:input={(event) => patch({ initialB2B: Number((event.currentTarget as HTMLInputElement).value) })}
        />
      </label>
      <label class="workspace-field">
        <span>{label('scoreProfile')}</span>
        <select
          value={request.scoreProfile}
          disabled={request.scoreMode !== 'summary'}
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
          disabled={request.scoreMode !== 'summary' && !request.preserveB2B}
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
            on:change={(event) => patch({ preserveB2B: (event.currentTarget as HTMLInputElement).checked })}
          />
          <span class="workspace-switch" aria-hidden="true"></span>
          <span>{label('preserveB2B')}</span>
        </label>
        <small class="workspace-field-help">{label('preserveB2BHelp')}</small>
      </div>
      <label class="workspace-switch-label">
        <input
          type="checkbox"
          checked={request.solutionProbabilities}
          on:change={(event) => patch({ solutionProbabilities: (event.currentTarget as HTMLInputElement).checked })}
        />
        <span class="workspace-switch" aria-hidden="true"></span>
        <span>{label('solutionProbabilities')}</span>
      </label>
    </div>
  </section>

  <section class="workspace-control-section">
    <h2 class="workspace-control-heading"><Zap size={16} strokeWidth={1.8} />{label('backend')}</h2>
    <div class="workspace-segmented four" role="group" aria-label={label('backend')}>
      {#each ['auto', 'cpu', 'gpu', 'hybrid'] as backend}
        <button type="button" class:active={request.backend === backend} on:click={() => patch({ backend: backend as SolverWorkspaceRequest['backend'] })}>{backend.toUpperCase()}</button>
      {/each}
    </div>
    <label class="workspace-field wide">
      <span>{label('gpuDevice')}</span>
      <input
        value={request.gpuDevice}
        disabled={request.backend === 'cpu'}
        inputmode="numeric"
        on:input={(event) => patch({ gpuDevice: (event.currentTarget as HTMLInputElement).value.trim() || 'auto' })}
      />
    </label>
    <div class="workspace-toggle-grid policy-toggle">
      <label class="workspace-switch-label"><input type="checkbox" checked={false} disabled /><span class="workspace-switch" aria-hidden="true"></span><span>{label('useAllThreads')}</span></label>
    </div>
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
</style>
