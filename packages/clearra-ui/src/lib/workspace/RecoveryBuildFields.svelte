<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { Plus, Minus } from '@lucide/svelte';
  import { recoveryStageEntries, recoveryFieldMask, recoveryMiddleCount, addRecoveryMiddle, removeRecoveryMiddle, canRemoveRecoveryMiddle } from './recoveryStages';
  import WorkspaceBoardEditor from './WorkspaceBoardEditor.svelte';
  import WorkspaceControlPanel from './WorkspaceControlPanel.svelte';
  import { type RecoveryBuildRequest } from './recoveryBuildModel';
  import { recoveryBuildMessage, type RecoveryBuildMessage } from './recoveryBuildI18n';
  import { overwriteRecoveryField, recoveryFieldReferences, type RecoveryField } from './recoveryFieldReferences';
  import type { WorkspaceLanguage } from './workspaceI18n';

  export let request: RecoveryBuildRequest;
  export let language: WorkspaceLanguage;
  const dispatch = createEventDispatcher<{ change: RecoveryBuildRequest }>();
  const fields: Array<{ field: 'startMask' | 'middleMask' | 'resultMask'; tone: 'dark' | 'medium' | 'light'; label: RecoveryBuildMessage }> = [
    { field: 'startMask', tone: 'dark', label: 'start' },
    { field: 'middleMask', tone: 'medium', label: 'middle' },
    { field: 'resultMask', tone: 'light', label: 'result' }
  ];
  let selected: 'startMask' | 'middleMask' | 'resultMask' = 'startMask';
  let middleSelection: RecoveryField = 'middleMask';
  $: middles = recoveryStageEntries(request).slice(0,-1);
  $: if (!middles.some(stage=>stage.field===middleSelection)) middleSelection='middleMask';
  $: editing = selected === 'middleMask' ? middleSelection : selected;
  function addStage() { const next=addRecoveryMiddle(request); dispatch('change',next); selected='middleMask'; middleSelection=recoveryStageEntries(next).at(-2)!.field; }
  function removeStage() { dispatch('change',removeRecoveryMiddle(request)); }

  $: label = (key: RecoveryBuildMessage) => recoveryBuildMessage(language, key);
  $: references = recoveryFieldReferences(request, editing)
    .map((reference) => ({ ...reference, label: label(reference.label) + (reference.label==='middle' && middles.length>1 ? ` ${reference.stageNumber}` : '') }));
  function change(field: RecoveryField, mask: bigint, height = request.height) {
    dispatch('change', overwriteRecoveryField(request, field, mask, Math.max(request.height, height)));
  }

</script>

<div class="recovery-field-editor">
  <WorkspaceControlPanel ariaLabel={label('start')}>
    <div class="workspace-segmented field-palette" role="group" aria-label={label('start')}>
      {#each fields as entry}
        <button type="button" class:active={selected === entry.field} aria-pressed={selected === entry.field}
          on:click={() => selected = entry.field}>
          <i class={entry.tone} aria-hidden="true"></i><span>{label(entry.label)}</span>
        </button>
      {/each}
    </div>
    {#if selected === 'middleMask' && middles.length > 1}
      <div class="workspace-segmented middle-palette" role="group" aria-label={label('middleStages')}>
        {#each middles as stage, index (stage.field)}
          <button type="button" class:active={middleSelection===stage.field} aria-pressed={middleSelection===stage.field}
            on:click={()=>middleSelection=stage.field}>{label('middle')} {index+1}</button>
        {/each}
      </div>
    {/if}
    <p class="workspace-field-help">{label('fieldsHelp')}</p>

  </WorkspaceControlPanel>
  {#each fields.filter((entry) => entry.field === selected) as entry (editing)}
    <WorkspaceBoardEditor mode="forward" height={request.height} existingMask={recoveryFieldMask(request,editing)}
      targetMask={0n} piecesNeeded={null} showStats={false} {language} occupiedTone={entry.tone}
      referenceLayers={references} labelOverride={label(entry.label)} enableGlobalPaste={false}
      on:change={(event) => change(editing, event.detail.existingMask)}
      on:import={(event) => change(editing, event.detail.existingMask, event.detail.height)}>
      <svelte:fragment slot="after-undo">
        <button type="button" class="tool-button" aria-label={label('addMiddle')} title={label('addMiddle')}
          disabled={recoveryMiddleCount(request)>=59} on:click={addStage}><Plus size={16}/></button>
        <button type="button" class="tool-button" aria-label={label('removeMiddle')} title={label('removeMiddle')}
          disabled={!canRemoveRecoveryMiddle(request)} on:click={removeStage}><Minus size={16}/></button>
      </svelte:fragment>
    </WorkspaceBoardEditor>
  {/each}
</div>

<style>
  .recovery-field-editor { display: grid; gap: 14px; min-width: 0; }
  .middle-palette { display: flex; flex-wrap: wrap; }
  .middle-palette button { flex: 1 1 80px; min-width: 0; }
  .tool-button { display: inline-flex; align-items:center; justify-content:center; min-width:32px; min-height:32px; }
  .field-palette { grid-template-columns: repeat(3, minmax(0, 1fr)); }
  .field-palette button { display: flex; align-items: center; justify-content: center; gap: 7px; padding: 8px !important; }
  .field-palette i { width: 14px; height: 14px; border: 1px solid #444; border-radius: 2px; flex: 0 0 auto; }
  .dark { background: #606060; } .medium { background: #a0a0a0; } .light { background: #dedede; }
  .field-palette span { word-break: keep-all; overflow-wrap: normal; }
  @media (pointer: coarse) { .field-palette button { min-height: 44px !important; } }
  @media (max-width: 380px) { .field-palette button { flex-direction: column; } }
</style>
