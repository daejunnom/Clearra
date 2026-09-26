<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import WorkspaceBoardEditor from './WorkspaceBoardEditor.svelte';
  import WorkspaceControlPanel from './WorkspaceControlPanel.svelte';
  import { componentMessage, type ComponentMessageKey } from '../i18n/componentCatalog';
  import { updateRecoveryField, type BoundaryRecoveryRequest, type RecoveryField } from './boundaryRecoveryModel';
  import type { WorkspaceLanguage } from './workspaceI18n';

  export let request: BoundaryRecoveryRequest;
  export let language: WorkspaceLanguage;
  const dispatch = createEventDispatcher<{ change: BoundaryRecoveryRequest }>();
  const fields: Array<{ field: RecoveryField; tone: 'dark' | 'medium' | 'light'; label: ComponentMessageKey }> = [
    { field: 'initialBoardMask', tone: 'dark', label: 'recoveryInitialField' },
    { field: 'stageOneBoardMask', tone: 'medium', label: 'recoveryStageOneField' },
    { field: 'targetBoardMask', tone: 'light', label: 'recoveryTargetField' }
  ];
  let selected: RecoveryField = 'initialBoardMask';
  let showReferences = true;
  $: label = (key: ComponentMessageKey) => componentMessage(language, key);
  $: activeField = fields.find((entry) => entry.field === selected)!;
  $: references = showReferences ? fields.filter((entry) => entry.field !== selected)
    .map((entry) => ({ mask: request[entry.field], tone: entry.tone, label: label(entry.label) })) : [];

  function change(mask: bigint, height = request.height) {
    dispatch('change', updateRecoveryField(request, selected, mask, height));
  }
</script>

<div class="recovery-field-editor">
  <WorkspaceControlPanel ariaLabel={label('recoveryFieldLayer')}>
    <div class="workspace-segmented field-palette" role="group" aria-label={label('recoveryFieldLayer')}>
      {#each fields as entry}
        <button type="button" class:active={selected === entry.field} aria-pressed={selected === entry.field}
          on:click={() => selected = entry.field}>
          <i class={entry.tone} aria-hidden="true"></i><span>{label(entry.label)}</span>
        </button>
      {/each}
    </div>
    <div class="workspace-switch-row">
      <label class="workspace-switch-label">
        <input type="checkbox" bind:checked={showReferences} />
        <span class="workspace-switch" aria-hidden="true"></span><span>{label('recoveryFieldContext')}</span>
      </label>
    </div>
    <p class="workspace-field-help">{label('recoveryFieldHelp')}</p>
  </WorkspaceControlPanel>
  {#key selected}
    <WorkspaceBoardEditor mode="forward" height={request.height} existingMask={request[selected]}
      targetMask={0n} piecesNeeded={null} {language} occupiedTone={activeField.tone}
      referenceLayers={references} labelOverride={label(activeField.label)} enableGlobalPaste={false}
      on:change={(event) => change(event.detail.existingMask)}
      on:import={(event) => change(event.detail.existingMask, event.detail.height)} />
  {/key}
</div>

<style>
  .recovery-field-editor { display: grid; gap: 14px; min-width: 0; }
  .field-palette { grid-template-columns: repeat(3, minmax(0, 1fr)); }
  .field-palette button { display: flex; align-items: center; justify-content: center; gap: 7px; padding: 8px !important; }
  .field-palette i { width: 14px; height: 14px; border: 1px solid #444; border-radius: 2px; flex: 0 0 auto; }
  .dark { background: #606060; } .medium { background: #a0a0a0; } .light { background: #dedede; }
  .field-palette span { word-break: keep-all; overflow-wrap: normal; }
  @media (pointer: coarse) { .field-palette button { min-height: 44px !important; } }
  @media (max-width: 380px) { .field-palette button { flex-direction: column; } }
</style>
