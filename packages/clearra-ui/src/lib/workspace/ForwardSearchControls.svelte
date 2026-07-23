<script lang="ts">
  import { CircleHelp, Database, Flame, RotateCw, ShieldCheck } from '@lucide/svelte';
  import { createEventDispatcher } from 'svelte';

  import {
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
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';

  export let request: ForwardSearchRequest;
  export let language: WorkspaceLanguage;
  export let validationCodes: ForwardSearchValidationCode[];

  const dispatch = createEventDispatcher<{ change: ForwardSearchRequest }>();
  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);
  $: categoryOptions = spinCategoryOptions(request.spinProfile);
  $: if (!categoryOptions.includes(request.spinCategory)) update({ spinCategory: 'any' });

  function update(change: Partial<ForwardSearchRequest>) {
    dispatch('change', { ...request, ...change });
  }
</script>

<WorkspaceControlPanel ariaLabel={label('forwardContract')}>
  <section class="workspace-control-section">
    <h2 class="workspace-control-heading"><Database size={16} strokeWidth={1.8} />{label('source')}</h2>
    <div class="workspace-field queue-field">
    <div class="queue-label">
      <label class="workspace-field-label" for="forward-queue-input">{label(request.tool === 'damage' ? 'fixedQueue' : 'queuePattern')}</label>
      {#if request.tool === 'spin-finder'}
        <button
          type="button"
          class="tooltip-trigger"
          aria-label={label('queuePatternHelp')}
          aria-describedby="spin-queue-tooltip"
        >
          <CircleHelp size={14} strokeWidth={1.8} />
          <span id="spin-queue-tooltip" role="tooltip">{label('spinQueueTooltip')}</span>
        </button>
      {/if}
    </div>
    <QueueTextInput
      class="workspace-queue-input"
      id="forward-queue-input"
      value={request.queue}
      placeholder={request.tool === 'damage' ? 'TZOISLJ' : 'P4 / [^T]4'}
      spellcheck="false"
      aria-describedby="forward-queue-help"
      aria-invalid={validationCodes.includes('forward_queue_invalid')}
      on:value={(event) => update({ queue: event.detail })}
    />
    <small class="workspace-field-help" id="forward-queue-help">{label(request.tool === 'damage' ? 'fixedQueueHelp' : 'spinPatternHelp')}</small>
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
    <div class="workspace-contract-band">
      <ShieldCheck size={15} strokeWidth={1.8} />
      <span>{label('forwardSearchContract')}</span>
      <b>SRS+ · {label('forwardDirection')}</b>
    </div>

    <div class="workspace-field-grid">
    <label class="workspace-field">
      <span>{label('rule')}</span>
      <select value={request.rule} on:change={(event) => update({ rule: (event.currentTarget as HTMLSelectElement).value as ForwardSearchRequest['rule'] })}>
        <option value="srs-plus">SRS+</option>
        <option value="srs">SRS</option>
        <option value="srs-x">SRS-X</option>
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
        <small class="workspace-field-help">{label('minimumDamageHelp')}</small>
      </label>
    {/if}
    <div class="workspace-field-grid">
      <label class="workspace-field">
        <span>{label('initialCombo')}</span>
        <input type="number" min="0" step="1" value={request.initialCombo} on:input={(event) => update({ initialCombo: Number((event.currentTarget as HTMLInputElement).value) })} />
      </label>
      <label class="workspace-field">
        <span>{label('initialB2B')}</span>
        <input type="number" min="0" step="1" value={request.initialB2B} on:input={(event) => update({ initialB2B: Number((event.currentTarget as HTMLInputElement).value) })} />
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
  .queue-label { align-items: center; color: #66716d; display: flex; font-size: 11px; font-weight: 700; gap: 6px; }
  .queue-label label { display: inline; }
  .damage-mode { margin-top: 12px; }
  .tooltip-trigger { background: transparent; border: 0; color: #52605b; cursor: help; display: inline-flex; outline: none; padding: 0; position: relative; }
  .tooltip-trigger:focus-visible { border-radius: 2px; outline: 2px solid #16877d; outline-offset: 2px; }
  .tooltip-trigger [role='tooltip'] { background: #17211e; border-radius: 4px; color: #fff; font-size: 10px; font-weight: 500; left: -8px; line-height: 1.5; max-width: min(300px, calc(100vw - 48px)); opacity: 0; padding: 8px 9px; pointer-events: none; position: absolute; top: calc(100% + 7px); transform: translateY(-2px); transition: opacity 120ms ease, transform 120ms ease; visibility: hidden; width: max-content; z-index: 20; }
  .tooltip-trigger:hover [role='tooltip'], .tooltip-trigger:focus [role='tooltip'] { opacity: 1; transform: translateY(0); visibility: visible; }
  .queue-field :global(input[aria-invalid='true']) { border-color: #bd5a3d; }
</style>
