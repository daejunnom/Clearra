<script lang="ts">
  import { Database, Gauge } from '@lucide/svelte';
  import { createEventDispatcher } from 'svelte';

  import {
    type BuildProbabilityRequest,
    type BuildProbabilityValidationCode
  } from './buildProbabilityModel';
  import QueueTextInput from '../components/QueueTextInput.svelte';
  import QueuePatternHelp from './QueuePatternHelp.svelte';
  import WorkspaceControlPanel from './WorkspaceControlPanel.svelte';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';

  export let request: BuildProbabilityRequest;
  export let language: WorkspaceLanguage;
  export let validationCodes: BuildProbabilityValidationCode[] = [];

  const dispatch = createEventDispatcher<{ change: BuildProbabilityRequest }>();
  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);

  function patch(change: Partial<BuildProbabilityRequest>) {
    dispatch('change', { ...request, ...change });
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
    <QueuePatternHelp {language} explainInitialHold />

    <div class="workspace-switch-row">
      <label class="workspace-switch-label">
        <input type="checkbox" checked={request.holdEnabled} on:change={(event) => patch({ holdEnabled: (event.currentTarget as HTMLInputElement).checked })} />
        <span class="workspace-switch" aria-hidden="true"></span><span>{label('hold')}</span>
      </label>
      {#if request.holdEnabled}
        <label class="workspace-field workspace-inline-select">
          <span>{label('holdPiece')}</span>
          <select value={request.holdPiece} on:change={(event) => patch({ holdPiece: (event.currentTarget as HTMLSelectElement).value as BuildProbabilityRequest['holdPiece'] })}>
            <option value="empty">{label('empty')}</option>
            {#each ['I', 'O', 'T', 'S', 'Z', 'J', 'L'] as piece}<option value={piece}>{piece}</option>{/each}
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
        <select
          value={request.aggregation}
          on:change={(event) => patch({ aggregation: (event.currentTarget as HTMLSelectElement).value as BuildProbabilityRequest['aggregation'] })}
        >
          <option value="buildability">{label('buildProbability')}</option>
          <option value="spin">{label('spinSearch')}</option>
        </select>
      </label>
      <label class="workspace-field">
        <span>{label('rule')}</span>
        <select value={request.rule} on:change={(event) => patch({ rule: (event.currentTarget as HTMLSelectElement).value as BuildProbabilityRequest['rule'] })}>
          <option value="srs-plus">SRS+</option>
          <option value="srs">SRS</option>
          <option value="srs-x">SRS-X</option>
        </select>
      </label>
      <label class="workspace-field">
        <span>{label('spinProfile')}</span>
        <select
          value={request.spinProfile}
          disabled={request.aggregation === 'buildability' && !request.preserveB2B}
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
    </div>
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
  </section>

  {#if validationCodes.length}
    <ul class="workspace-validation" aria-live="polite">
      {#each validationCodes as code}<li>{label(code)}</li>{/each}
    </ul>
  {/if}
</WorkspaceControlPanel>

<style>
  .b2b-preservation-control { display: grid; gap: 5px; margin-top: 14px; }
</style>
