<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { Plus, Minus } from '@lucide/svelte';
  import WorkspaceBoardEditor from './WorkspaceBoardEditor.svelte';
  import WorkspaceControlPanel from './WorkspaceControlPanel.svelte';
  import type { RecoveryBuildRequest } from './recoveryBuildModel';
  import { recoveryBuildMessage, type RecoveryBuildMessage } from './recoveryBuildI18n';
  import { overwriteRecoveryField, recoveryFieldReferences, type RecoveryField } from './recoveryFieldReferences';
  import { recoveryMiddleStages, appendRecoveryMiddle, removeRecoveryMiddle, MAX_RECOVERY_MIDDLES,
    recoveryCanRemoveMiddle, captureRecoveryEditor, restoreRecoveryEditor, recoveryEditorKey,
    type RecoveryEditorSnapshot, type RecoveryEditorSelection } from './recoveryStageModel';
  import type { WorkspaceLanguage } from './workspaceI18n';

  export let request: RecoveryBuildRequest;
  export let language: WorkspaceLanguage;
  const dispatch = createEventDispatcher<{ change: RecoveryBuildRequest }>();
  const fields: Array<{field:RecoveryField;tone:'dark'|'medium'|'light';label:RecoveryBuildMessage}> = [
    {field:'startMask',tone:'dark',label:'start'},
    {field:'middleMask',tone:'medium',label:'middle'},
    {field:'resultMask',tone:'light',label:'result'}
  ];
  let selected:RecoveryField='startMask';
  let middleId=recoveryMiddleStages(request)[0].id;
  type HistoryEntry={snapshot:RecoveryEditorSnapshot;selection:RecoveryEditorSelection};
  let undoStack:HistoryEntry[]=[];
  let redoStack:HistoryEntry[]=[];
  let lastKey=recoveryEditorKey(captureRecoveryEditor(request));
  let editError=false;
  $: label=(key:RecoveryBuildMessage)=>recoveryBuildMessage(language,key);
  $: middles=recoveryMiddleStages(request);
  $: if(!middles.some(s=>s.id===middleId)) middleId=middles[0].id;
  $: observedKey=recoveryEditorKey(captureRecoveryEditor(request));
  // Changing supplies/height externally invalidates structural undo rather than
  // accidentally restoring stale supplies. Pure display/rule options are kept.
  $: if(observedKey!==lastKey){undoStack=[];redoStack=[];lastKey=observedKey;}
  $: middleTitle=(index:number)=>middles.length===1?label('middle'):`${label('middle')} ${index+1}`;
  $: references=recoveryFieldReferences(request,selected,true,middleId).map(reference=>({
    ...reference,label:reference.middleIndex===undefined?label(reference.label):middleTitle(reference.middleIndex)
  }));
  $: selectedMask=selected==='middleMask'?middles.find(s=>s.id===middleId)!.mask:request[selected];
  $: entry=fields.find(f=>f.field===selected)!;
  $: editorTitle=selected==='middleMask'?middleTitle(middles.findIndex(s=>s.id===middleId)):label(entry.label);
  function selection():RecoveryEditorSelection{return {field:selected,middleId};}
  function remember(){undoStack=[...undoStack.slice(-63),{snapshot:captureRecoveryEditor(request),selection:selection()}];redoStack=[];}
  function apply(next:RecoveryBuildRequest, active=selection()){
    editError=false;lastKey=recoveryEditorKey(captureRecoveryEditor(next));
    selected=active.field;middleId=active.middleId;dispatch('change',next);
  }
  function change(mask:bigint,height=request.height){
    try{apply(overwriteRecoveryField(request,selected,mask,Math.max(request.height,height),middleId));}
    catch{editError=true;}
  }
  function add(){
    try{const next=appendRecoveryMiddle(request);remember();apply(next.request,{...next.selection,field:selected});}
    catch{editError=true;}
  }
  function remove(){
    try{const next=removeRecoveryMiddle(request,selection());remember();apply(next.request,next.selection);}
    catch{editError=true;}
  }
  function undo(){
    const prior=undoStack.at(-1);if(!prior)return;
    redoStack=[...redoStack,{snapshot:captureRecoveryEditor(request),selection:selection()}];
    undoStack=undoStack.slice(0,-1);apply(restoreRecoveryEditor(request,prior.snapshot),prior.selection);
  }
  function redo(){
    const next=redoStack.at(-1);if(!next)return;
    undoStack=[...undoStack,{snapshot:captureRecoveryEditor(request),selection:selection()}];
    redoStack=redoStack.slice(0,-1);apply(restoreRecoveryEditor(request,next.snapshot),next.selection);
  }
</script>

<div class="recovery-field-editor">
  <WorkspaceControlPanel ariaLabel={label('start')}>
    <div class="workspace-segmented field-palette" role="group" aria-label={label('start')}>
      {#each fields as item}
        <button type="button" class:active={selected===item.field} aria-pressed={selected===item.field}
          on:click={()=>selected=item.field}>
          <i class={item.tone} aria-hidden="true"></i><span>{label(item.label)}</span>
        </button>
      {/each}
    </div>
    {#if selected==='middleMask' && middles.length>1}
      <div class="workspace-segmented field-palette middle-palette" role="group" aria-label={label('middle')}>
        {#each middles as stage,index (stage.id)}
          <button type="button" class:active={middleId===stage.id} aria-pressed={middleId===stage.id}
            on:click={()=>middleId=stage.id}>
            <i class="medium" aria-hidden="true"></i><span>{middleTitle(index)}</span>
          </button>
        {/each}
      </div>
    {/if}
    <p class="workspace-field-help">{label('fieldsHelp')}</p>
    {#if editError}<p class="workspace-validation" role="alert">{label('frameError')}</p>{/if}
  </WorkspaceControlPanel>
  {#key `${selected}:${middleId}`}
    <WorkspaceBoardEditor mode="forward" height={request.height} existingMask={selectedMask}
      targetMask={0n} piecesNeeded={null} showStats={false} {language} occupiedTone={entry.tone}
      referenceLayers={references} labelOverride={editorTitle} enableGlobalPaste={false}
      externalHistory={{canUndo:undoStack.length>0,canRedo:redoStack.length>0,undo,redo}}
      on:history={remember}
      on:change={(event)=>change(event.detail.existingMask)}
      on:import={(event)=>change(event.detail.existingMask,event.detail.height)}>
      <svelte:fragment slot="afterUndo">
        <button type="button" class="recovery-stage-action" title={label('addMiddle')} aria-label={label('addMiddle')}
          disabled={middles.length>=MAX_RECOVERY_MIDDLES} on:click={add}><Plus size={16}/></button>
        <button type="button" class="recovery-stage-action" title={label('removeMiddleHelp')} aria-label={label('removeMiddle')}
          disabled={!recoveryCanRemoveMiddle(request)} on:click={remove}><Minus size={16}/></button>
      </svelte:fragment>
    </WorkspaceBoardEditor>
  {/key}
</div>

<style>
  .recovery-field-editor { display: grid; gap: 14px; min-width: 0; }
  .field-palette { grid-template-columns: repeat(3, minmax(0, 1fr)); }
  .field-palette button { display: flex; align-items: center; justify-content: center; gap: 7px; padding: 8px !important; }
  .field-palette i { width: 14px; height: 14px; border: 1px solid #444; border-radius: 2px; flex: 0 0 auto; }
  .dark { background: #606060; } .medium { background: #a0a0a0; } .light { background: #dedede; }
  .middle-palette { display: flex; flex-wrap: wrap; gap: 4px; }
  .middle-palette button { flex: 1 1 80px; }
  .recovery-stage-action { align-items: center; background: #fff; border: 1px solid #cbd3ce; border-radius: 5px; color: #34403c; cursor: pointer; display: inline-flex; height: 32px; justify-content: center; padding: 0; width: 32px; }
  .recovery-stage-action:hover:not(:disabled) { background: #e4f1ee; border-color: #36847c; }
  .recovery-stage-action:disabled { cursor: default; opacity: .35; }
  .field-palette span { word-break: keep-all; overflow-wrap: normal; }
  @media (pointer: coarse) { .field-palette button { min-height: 44px !important; } }
  @media (max-width: 380px) { .field-palette button { flex-direction: column; } }
</style>
