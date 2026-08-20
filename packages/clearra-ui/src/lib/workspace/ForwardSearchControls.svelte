<script lang="ts">
  import { Database, Flame, RotateCw } from '@lucide/svelte';
  import { createEventDispatcher } from 'svelte';

  import {
    MAX_FORWARD_CHAIN,
    spinCategoryOptions,
    type ForwardDamageAggregation,
    type ForwardSearchRequest,
    type ForwardSpinCategory,
    type ForwardSpinLines,
    type ForwardSearchValidationCode
  } from './forwardSearchModel';
  import QueueTextInput from '../components/QueueTextInput.svelte';
  import QueuePatternHelp from './QueuePatternHelp.svelte';
  import WorkspaceControlPanel from './WorkspaceControlPanel.svelte';
  import WorkerAuthorityStatus from './WorkerAuthorityStatus.svelte';
  import type { WorkerAuthorityReport } from '../wasm';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';

  export let request: ForwardSearchRequest;
  export let language: WorkspaceLanguage;
  export let validationCodes: ForwardSearchValidationCode[];
  export let workerAuthority: WorkerAuthorityReport;

  const dispatch = createEventDispatcher<{ change: ForwardSearchRequest }>();
  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);
  $: categoryOptions = spinCategoryOptions(request.spinProfile);
  $: if (!categoryOptions.includes(request.spinCategory)) update({ spinCategory: 'any' });

  function update(change: Partial<ForwardSearchRequest>) {
    dispatch('change', { ...request, ...change });
  }
</script>

<WorkspaceControlPanel ariaLabel={label(request.tool === 'damage' ? 'maximumDamage' : 'spinFinder')}>
  <section class="workspace-control-section">
    <h2 class="workspace-control-heading"><Database size={16} strokeWidth={1.8} />{label('source')}</h2>
    <div class="workspace-field queue-field">
    <label class="workspace-field-label" for="forward-queue-input">{label(request.tool === 'damage' ? 'fixedQueue' : 'queuePattern')}</label>
    <QueueTextInput
      class="workspace-queue-input"
      id="forward-queue-input"
      value={request.queue}
      placeholder={request.tool === 'damage' ? 'TZOISLJ' : 'P4 / [^T]4'}
      spellcheck="false"
      aria-invalid={validationCodes.includes('forward_queue_invalid')}
      on:value={(event) => update({ queue: event.detail })}
    />
  </div>
  {#if request.tool === 'spin-finder'}<QueuePatternHelp {language} />{/if}

    <div class="workspace-switch-row">
      <label class="workspace-switch-label">
        <input
          type="checkbox"
          checked={request.holdEnabled}
          on:change={(event) => update({ holdEnabled: (event.currentTarget as HTMLInputElement).checked })}
        />
        <span class="workspace-switch" aria-hidden="true"></span>
        <span>{label('hold')}</span>
      </label>
    </div>
  </section>

  <section class="workspace-control-section">
    <h2 class="workspace-control-heading">
      {#if request.tool === 'damage'}<Flame size={16} strokeWidth={1.8} />{:else}<RotateCw size={16} strokeWidth={1.8} />{/if}
      {label(request.tool === 'damage' ? 'maximumDamage' : 'spinFinder')}
    </h2>
    <div class="workspace-field-grid">
    <label class="workspace-field">
      <span>{label('rule')}</span>
      <select value={request.rule} on:change={(event) => update({ rule: (event.currentTarget as HTMLSelectElement).value as ForwardSearchRequest['rule'] })}>
        <option value="srs-plus">SRS+</option>
        <option value="srs">SRS</option>
        <option value="srs-x">SRS-X</option>
        <option value="jstris-180">Jstris 180</option>
      </select>
    </label>
    <label class="workspace-field">
      <span>{label('spinProfile')}</span>
      <select value={request.spinProfile} on:change={(event) => update({ spinProfile: (event.currentTarget as HTMLSelectElement).value as ForwardSearchRequest['spinProfile'] })}>
        <option value="t-spins">T-Spins</option>
        <option value="t-spins-plus">T-Spins+</option>
        <option value="all-mini">All-Mini</option>
        <option value="all-mini-plus">All-Mini+</option>
        <option value="all-spin">All-Spin</option>
        <option value="all-spin-plus">All-Spin+</option>
      </select>
    </label>
    </div>

    <div class="b2b-preservation-control">
      <label class="workspace-switch-label">
        <input
          type="checkbox"
          checked={request.preserveB2B}
          on:change={(event) => update({ preserveB2B: (event.currentTarget as HTMLInputElement).checked })}
        />
        <span class="workspace-switch" aria-hidden="true"></span>
        <span>{label('preserveB2B')}</span>
      </label>
    </div>
    <div class="worker-policy-control">
      <label class="workspace-switch-label">
        <input
          type="checkbox"
          checked={request.useAllLogicalProcessors}
          on:change={(event) => update({
            useAllLogicalProcessors: (event.currentTarget as HTMLInputElement).checked
          })}
        />
        <span class="workspace-switch" aria-hidden="true"></span>
        <span>{label('useAllThreads')}</span>
      </label>
      <WorkerAuthorityStatus authority={workerAuthority} {language} />
    </div>

  {#if request.tool === 'damage'}
    <label class="workspace-field wide damage-mode">
      <span>{label('damageResultMode')}</span>
      <div class="workspace-segmented two" role="group" aria-label={label('damageResultMode')}>
        {#each ['maximum', 'at-least'] as mode}
          <button
            type="button"
            class:active={request.damageAggregation === mode}
            on:click={() => update({ damageAggregation: mode as ForwardDamageAggregation })}
          >{label(mode === 'maximum' ? 'maximumDamageOnly' : 'damageAtLeast')}</button>
        {/each}
      </div>
    </label>
    {#if request.damageAggregation === 'at-least'}
      <label class="workspace-field wide">
        <span>{label('minimumDamage')}</span>
        <input
          type="number"
          min="0"
          max="4294967295"
          step="1"
          value={request.minimumDamage}
          on:input={(event) => update({ minimumDamage: Number((event.currentTarget as HTMLInputElement).value) })}
        />
      </label>
    {/if}
    <div class="workspace-field-grid">
      <label class="workspace-field">
        <span>{label('initialCombo')}</span>
        <input type="number" min="0" max={MAX_FORWARD_CHAIN} step="1" value={request.initialCombo} on:input={(event) => update({ initialCombo: Number((event.currentTarget as HTMLInputElement).value) })} />
      </label>
      <label class="workspace-field">
        <span>{label('initialB2B')}</span>
        <input type="number" min="0" max={MAX_FORWARD_CHAIN} step="1" value={request.initialB2B} on:input={(event) => update({ initialB2B: Number((event.currentTarget as HTMLInputElement).value) })} />
      </label>
    </div>
  {:else}
    <div class="workspace-field-grid">
      <label class="workspace-field">
        <span>{label('spinLines')}</span>
        <select value={request.spinLines} on:change={(event) => {
          const value = (event.currentTarget as HTMLSelectElement).value;
          update({ spinLines: value as ForwardSpinLines });
        }}>
          <option value="any">{label('any')}</option>
          <option value="0">0</option><option value="1">1</option><option value="2">2</option><option value="3">3</option><option value="4">4</option>
          <option value="1+">1 {label('orMoreLines')}</option>
          <option value="2+">2 {label('orMoreLines')}</option>
          <option value="3+">3 {label('orMoreLines')}</option>
          <option value="4+">4 {label('orMoreLines')}</option>
        </select>
      </label>
      {#if categoryOptions.length > 1}
        <label class="workspace-field">
          <span>{label('spinPieceGroup')}</span>
          <select value={request.spinCategory} on:change={(event) => update({ spinCategory: (event.currentTarget as HTMLSelectElement).value as ForwardSpinCategory })}>
            <option value="any">{label('any')}</option>
            <option value="t">T</option>
            <option value="other">{label('nonTPieces')}</option>
          </select>
        </label>
      {/if}
    </div>
  {/if}
  </section>

  {#if validationCodes.length}
    <div class="workspace-validation" role="alert">
      {#each validationCodes as code}<p>{label(code)}</p>{/each}
    </div>
  {/if}
</WorkspaceControlPanel>

<style>
  .damage-mode { margin-top: 12px; }
  .b2b-preservation-control { display: grid; gap: 5px; margin-top: 14px; }
  .worker-policy-control { display: grid; gap: 5px; margin-top: 14px; }
  .queue-field :global(input[aria-invalid='true']) { border-color: #bd5a3d; }
</style>
