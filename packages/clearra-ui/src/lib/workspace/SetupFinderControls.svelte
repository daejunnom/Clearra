<script lang="ts">
  import { Database, Layers3, ShieldCheck } from '@lucide/svelte';
  import { createEventDispatcher } from 'svelte';

  import QueueTextInput from '../components/QueueTextInput.svelte';
  import {
    explicitSetupHold,
    setupCycle,
    type SetupFinderRequest,
    type SetupFinderValidationCode
  } from './setupFinderModel';
  import WorkspaceControlPanel from './WorkspaceControlPanel.svelte';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';

  export let request: SetupFinderRequest;
  export let language: WorkspaceLanguage;
  export let validationCodes: SetupFinderValidationCode[] = [];

  const dispatch = createEventDispatcher<{ change: SetupFinderRequest }>();
  $: label = (
    key: Parameters<typeof workspaceMessage>[1],
    values: Record<string, string | number> = {}
  ) => workspaceMessage(language, key, values);
  $: cycle = setupCycle(request.remaining);
  $: explicitHold = explicitSetupHold(request.remaining);

  function update(change: Partial<SetupFinderRequest>) {
    dispatch('change', { ...request, ...change });
  }
</script>

<WorkspaceControlPanel ariaLabel={label('setupFinder')}>
  <section class="workspace-control-section">
    <h2 class="workspace-control-heading">
      <Database size={16} strokeWidth={1.8} />{label('setupResidue')}
    </h2>
    <label class="workspace-field wide">
      <span>{label('remainingPieces')}</span>
      <QueueTextInput
        class="workspace-queue-input"
        value={request.remaining}
        maxlength="16"
        placeholder="IOTSZJL"
        spellcheck="false"
        aria-invalid={validationCodes.length > 0}
        on:value={(event) => update({ remaining: event.detail })}
      />
      <small class="workspace-field-help">{label('setupResidueHelp')}</small>
    </label>

    <div class="residue-facts">
      <span>{label('pcCycle')}</span><strong>{cycle ? label('cycleNumber', { cycle }) : '—'}</strong>
      <span>{label('initialHold')}</span>
      <strong>{explicitHold ?? label('holdConditionsSeparated')}</strong>
    </div>
  </section>

  <section class="workspace-control-section">
    <h2 class="workspace-control-heading">
      <Layers3 size={16} strokeWidth={1.8} />{label('setupSearchContract')}
    </h2>
    <div class="workspace-contract-band">
      <ShieldCheck size={15} strokeWidth={1.8} />
      <span>{label('pcTarget')}</span>
      <b>10×4 · SRS+</b>
    </div>
    {#if cycle === 7}
      <div class="workspace-switch-row">
        <label class="workspace-switch-label">
          <input
            type="checkbox"
            checked={request.allowPostCycleBorrow}
            on:change={(event) => update({
              allowPostCycleBorrow: (event.currentTarget as HTMLInputElement).checked
            })}
          />
          <span class="workspace-switch" aria-hidden="true"></span>
          <span>{label('allowPostCycleBorrow')}</span>
        </label>
      </div>
      <small class="workspace-field-help boundary-help">{label('postCycleBorrowHelp')}</small>
    {/if}
  </section>

  {#if validationCodes.length}
    <ul class="workspace-validation">
      {#each validationCodes as code}<li>{label(code)}</li>{/each}
    </ul>
  {/if}
</WorkspaceControlPanel>

<style>
  .residue-facts {
    background: #f0f3f1;
    display: grid;
    font-size: 11px;
    gap: 1px;
    grid-template-columns: 1fr auto;
    margin-top: 14px;
  }
  .residue-facts span,
  .residue-facts strong { padding: 9px 10px; }
  .residue-facts span { color: #68736f; }
  .residue-facts strong { color: #173f3a; text-align: right; }
  .boundary-help { display: block; margin-top: 8px; }
</style>
