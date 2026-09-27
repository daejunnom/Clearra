<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import WorkspaceBoardEditor from './WorkspaceBoardEditor.svelte';
  import WorkspaceControlPanel from './WorkspaceControlPanel.svelte';
  import { resizeRecoveryBuild, recoveryMiddleBase, type RecoveryBuildRequest } from './recoveryBuildModel';
  import { recoveryBuildMessage, type RecoveryBuildMessage } from './recoveryBuildI18n';
  type RecoveryField = 'startMask' | 'middleMask' | 'resultMask';
  import type { WorkspaceLanguage } from './workspaceI18n';

  export let request: RecoveryBuildRequest;
  export let language: WorkspaceLanguage;
  const dispatch = createEventDispatcher<{ change: RecoveryBuildRequest }>();
  const fields: Array<{ field: RecoveryField; tone: 'dark' | 'medium' | 'light'; label: RecoveryBuildMessage }> = [
    { field: 'startMask', tone: 'dark', label: 'start' },
    { field: 'middleMask', tone: 'medium', label: 'middle' },
    { field: 'resultMask', tone: 'light', label: 'result' }
  ];
  let selected: RecoveryField = 'startMask';
  let showReferences = true;
  $: label = (key: RecoveryBuildMessage) => recoveryBuildMessage(language, key);
  $: activeField = fields.find((entry) => entry.field === selected)!;
  $: references = !showReferences ? [] : selected === 'resultMask'
    ? [{ mask: recoveryMiddleBase(request), tone: 'dark' as const, label: label('middle') }]
    : fields.filter(entry => entry.field !== 'resultMask' && entry.field !== selected)
      .map(entry => ({ mask: request[entry.field], tone: entry.tone, label: label(entry.label) }));
  function change(mask: bigint, height = request.height) {
    const next = resizeRecoveryBuild(request, Math.max(request.height, height));
    const limit = (1n << BigInt(next.height * 10)) - 1n;
    dispatch('change', { ...next, [selected]: mask & limit });
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
    <div class="workspace-switch-row">
      <label class="workspace-switch-label">
        <input type="checkbox" bind:checked={showReferences} />
        <span class="workspace-switch" aria-hidden="true"></span><span>{label('context')}</span>
      </label>
    </div>
    <p class="workspace-field-help">{label('fieldsHelp')}</p>
  </WorkspaceControlPanel>
  {#key selected}
    <WorkspaceBoardEditor mode="forward" height={request.height} existingMask={request[selected]}
      targetMask={0n} piecesNeeded={null} showStats={false} {language} occupiedTone={activeField.tone}
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
