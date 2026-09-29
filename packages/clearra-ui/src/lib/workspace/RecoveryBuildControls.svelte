<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { Database, Gauge } from '@lucide/svelte';
  import WorkspaceControlPanel from './WorkspaceControlPanel.svelte';
  import WorkspaceSupplyInput from './WorkspaceSupplyInput.svelte';
  import QueuePatternHelp from './QueuePatternHelp.svelte';
  import WorkspaceToggle from './WorkspaceToggle.svelte';
  import RuleProfileSelect from './RuleProfileSelect.svelte';
  import SpinProfileSelect from './SpinProfileSelect.svelte';
  import { recoveryEarlyChoices, type RecoveryBuildRequest } from './recoveryBuildModel';
  import { recoveryMiddleStages, setRecoveryStageSupply } from './recoveryStageModel';
  import { recoveryBuildMessage } from './recoveryBuildI18n';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';
  export let request: RecoveryBuildRequest;
  export let language: WorkspaceLanguage;
  export let validation: string[] = [];
  const dispatch = createEventDispatcher<{ change: RecoveryBuildRequest }>();
  $: label = (key: Parameters<typeof recoveryBuildMessage>[1]) => recoveryBuildMessage(language, key);
  $: standard = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);
  $: options = recoveryEarlyChoices(request);
  $: middles = recoveryMiddleStages(request);
  $: destinations = [...middles,{id:'result',mask:request.resultMask,supply:request.secondSupply}];
  $: middleTitle = (index:number) => middles.length===1 ? label('middle') : `${label('middle')} ${index+1}`;
  $: supplyTitle = (index:number) => `${index===0?label('start'):middleTitle(index-1)} → ${index===middles.length?label('result'):middleTitle(index)}`;
  function supply(id:string,value:string){dispatch('change',setRecoveryStageSupply(request,id,value));}
  function patch(value: Partial<RecoveryBuildRequest>) { dispatch('change', { ...request, ...value }); }
</script>
<WorkspaceControlPanel ariaLabel={standard('source')}>
  <section class="workspace-control-section">
    <h2 class="workspace-control-heading"><Database size={16} />{standard('source')}</h2>
    <div class="recovery-supplies">
      {#each destinations as stage,index (stage.id)}
        <div><WorkspaceSupplyInput value={stage.supply} {language} qualifier={supplyTitle(index)} hint={index===0?label('firstHelp'):label('secondHelp')}
          on:value={(event)=>supply(stage.id,event.detail)}/></div>
      {/each}
    </div>
    <QueuePatternHelp {language} />
    <div class="workspace-switch-row"><WorkspaceToggle label={standard('hold')} checked={request.holdEnabled}
      on:change={(event) => patch({ holdEnabled: event.detail })} /></div>
  </section>
  <section class="workspace-control-section">
    <h2 class="workspace-control-heading"><Gauge size={16} />{standard('search')}</h2>
    <label class="workspace-field"><span>{label('early')}</span>
      <select value={request.maxEarly} aria-label={label('early')}
        on:change={(event) => patch({ maxEarly: event.currentTarget.value === 'auto' ? 'auto' : Number(event.currentTarget.value) })}>
        <option value="auto">{label('auto')}</option>
        {#if request.maxEarly !== 'auto' && !options.includes(request.maxEarly)}<option value={request.maxEarly}>{request.maxEarly}</option>{/if}
        {#each options as value}<option {value}>{value}</option>{/each}
      </select>
    </label>
    <p class="workspace-field-help">{label('earlyHelp')}</p>
    <WorkspaceToggle label={standard('useAllThreads')} checked={request.useAllLogicalProcessors}
      on:change={(event) => patch({ useAllLogicalProcessors: event.detail })} />
    <WorkspaceToggle label={label('exchange')} checked={request.allowPieceExchange}
      on:change={(event) => patch({ allowPieceExchange: event.detail })} />
    <p class="workspace-field-help">{label('exchangeHelp')}</p>
    <WorkspaceToggle label={standard('preserveB2B')} checked={request.preserveB2B}
      on:change={(event) => patch({ preserveB2B: event.detail })} />
    <p class="workspace-field-help">{label('b2bHelp')}</p>
    <WorkspaceToggle label={standard('minimumSolutions')} checked={request.minimumSolutions ?? false}
      on:change={(event) => patch({ minimumSolutions: event.detail })} />
    <WorkspaceToggle label={label('perSolutionProbability')} checked={request.solutionProbabilities ?? false}
      on:change={(event) => patch({ solutionProbabilities: event.detail })} />
    <WorkspaceToggle label={label('pngRender')} checked={request.pngRender ?? false}
      on:change={(event) => patch({ pngRender: event.detail })} />
    <div class="workspace-field-grid">
      <RuleProfileSelect value={request.rule} {language} on:change={(event) => patch({ rule: event.detail })} />
      <SpinProfileSelect value={request.spinProfile} {language} on:change={(event) => patch({ spinProfile: event.detail })} />
    </div>
  </section>
  {#if validation.includes('multistage-engine-pending')}
    <p class="workspace-validation" role="alert">{label('stageDraft')}</p>
  {:else if validation.length > 0}<p class="workspace-validation" role="alert">{label('invalid')}</p>{/if}
</WorkspaceControlPanel>
<style>
  .recovery-supplies { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 11rem), 1fr)); gap: 12px; }
  .recovery-supplies > div { min-width: 0; }
</style>
