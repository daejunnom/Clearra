<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { componentMessage } from '../i18n/componentCatalog';
  import ResultWorkspaceFrame from './ResultWorkspaceFrame.svelte';
  import SolutionToolbar from './SolutionToolbar.svelte';
  import MandatorySelectionSummary from './MandatorySelectionSummary.svelte';
  import SolutionCopyFormatControl from './SolutionCopyFormatControl.svelte';
  import RecoveryPng from './RecoveryPng.svelte';
  import WorkspaceToggle from './WorkspaceToggle.svelte';
  import PcPathReplayGif from './PcPathReplayGif.svelte';
  import { PRODUCT_MEMBER_PAGE_SIZE } from './productResultPager';
  import { recoveryBuildMessage } from './recoveryBuildI18n';
  import { recoveryBuildExamplePages, recoveryBuildExportPages, recoveryBuildWitness, recoveryBuildTerminalMask, validateRecoveryBuildPayload } from './recoveryBuildPresentation';
  import type { RecoveryBuildPayload } from './recoveryBuildPayloadTypes';
  import type { RecoveryMinimumSelection } from './recoveryBuildModel';
  import type { WorkspaceRuntimeView } from './workspaceRuntime';
  import type { SolutionCopyFormat } from './solutionExport';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';
  export let view: WorkspaceRuntimeView;
  export let language: WorkspaceLanguage;
  export let elapsedMs = 0;
  export let pngRender = false;
  export let showProbabilities = false;
  export let minimumDisabled = true;
  const dispatch = createEventDispatcher<{minimum: RecoveryMinimumSelection}>();
  let resultOnly = false;
  let copyFormat: SolutionCopyFormat = 'ctk';
  let page = 0;
  let pins: string[] = [];
  let previous: RecoveryBuildPayload | null = null;
  $: label = (key: Parameters<typeof recoveryBuildMessage>[1]) => recoveryBuildMessage(language,key);
  $: standard = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language,key);
  $: product = view.response?.product_result_payload;
  $: candidate = product?.content.payload_kind === 'recovery-build' ? product.content.payload : null;
  $: payload = candidate && product?.contract === 'recovery-build.v2' && validateRecoveryBuildPayload(candidate) ? candidate : null;
  $: invalid = view.status === 'completed' && !payload;
  $: if (payload !== previous) {
    if (payload?.input_identity !== previous?.input_identity) pins = payload?.required_solution_keys?.slice() ?? [];
    previous = payload;
    page = 0;
  }
  $: selected = payload?.minimum_proven ? new Set(payload.selected_solution_keys) : null;
  $: entries = payload?.solutions_complete
    ? (payload.solutions ?? []).filter(s=>!selected || selected.has(s.key))
    : (payload?.examples ?? []).map(example=>({key:example.status,example,probability:'',covered_count:''}));
  $: pages = Math.max(1,Math.ceil(entries.length / PRODUCT_MEMBER_PAGE_SIZE));
  $: if (page >= pages) page = pages - 1;
  $: visible = entries.slice(page * PRODUCT_MEMBER_PAGE_SIZE,(page+1) * PRODUCT_MEMBER_PAGE_SIZE);
  function percent(value: string): string { return `${(Number(value)*100).toLocaleString(language,{maximumFractionDigits:4})}%`; }
  function exportPages(report: RecoveryBuildPayload) { return () => recoveryBuildExportPages(report, resultOnly); }
  function togglePin(key: string) { pins = pins.includes(key) ? pins.filter(p=>p!==key) : [...pins,key]; }
  function minimum(keys: string[]) {
    if (!minimumDisabled && payload?.solutions_complete) dispatch('minimum',{sourceIdentity:payload.input_identity,keys});
  }
</script>
<ResultWorkspaceFrame ariaLabel={standard('results')} status={view.status} statusLabel={standard(view.status)}
  elapsedLabel={standard('elapsed')} elapsedText={`${(elapsedMs/1000).toFixed(1)}s`} progressProfile="recovery-build" {language}
  progressLabel={view.progressLabel || standard(view.status)} progressDone={view.progressDone} progressTotal={view.progressTotal}
  progressTelemetry={view.progressTelemetry} publicFailures={view.publicFailures}>
  {#if invalid}<p class="invalid-evidence" role="alert">{label('invalidResult')}</p>
  {:else if payload}
    <div class="recovery-metrics">
      <article><span>{label('normal')}</span><strong>{percent(payload.normal_probability)}</strong><small>{payload.normal_count}</small></article>
      <article><span>{label('recovery')}</span><strong>{percent(payload.recovery_probability)}</strong><small>{payload.recovery_count}</small></article>
      <article><span>{label('unavailable')}</span><strong>{percent(payload.no_path_probability)}</strong><small>{payload.no_path_count}</small></article>
    </div>
    {#if entries.length}
      <SolutionToolbar {language}>
        <svelte:fragment slot="actions">
          <span class="solution-count">{label(payload.solutions_complete ? (payload.minimum_proven ? 'provenMinimum' : 'completeSolutions') : 'examples')}: {entries.length}</span>
          {#if payload.solutions_complete}<button type="button" disabled={minimumDisabled} on:click={() => minimum([])}>{standard('minimumSolutions')}</button>{/if}
        </svelte:fragment>
        <svelte:fragment slot="copy"><WorkspaceToggle label={label('resultOnly')} checked={resultOnly} on:change={(event) => resultOnly = event.detail} /><SolutionCopyFormatControl bind:value={copyFormat} {language} loadPages={exportPages(payload)} /></svelte:fragment>
      </SolutionToolbar>
      {#if payload.solutions_complete}
        <MandatorySelectionSummary count={pins.length} {language} disabled={minimumDisabled} on:run={() => minimum(pins)} on:clear={() => pins=[]} />
        <p class="coverage-scope">{label('galleryHelp')}</p>
      {/if}
      {#if pages > 1}
        <nav class="recovery-pagination" aria-label={label('solutionPage')}>
          <button type="button" disabled={page===0} on:click={()=>page--}>{label('previousPage')}</button>
          <span>{page+1} / {pages}</span>
          <button type="button" disabled={page+1>=pages} on:click={()=>page++}>{label('nextPage')}</button>
        </nav>
      {/if}
      <ul class="recovery-path-gallery solution-gallery">
        {#each visible as entry, index (payload.input_identity + ':' + entry.key)}
          <li data-recovery-path={entry.example.status} data-solution-key={entry.key}>
            <h3>{label('solutions')} {page * PRODUCT_MEMBER_PAGE_SIZE + index + 1}</h3>
            {#if payload.solutions_complete}
              <label class="mandatory-choice"><input type="checkbox" checked={pins.includes(entry.key)} disabled={minimumDisabled}
                on:change={() => togglePin(entry.key)} />{componentMessage(language,'mandatorySolutions')}</label>
            {/if}
            {#if pngRender}
              <RecoveryPng page={recoveryBuildExamplePages(payload,entry.example,true)[0]} ariaLabel={label('result')} invalidLabel={label('invalidResult')} />
            {:else}
              <PcPathReplayGif witness={recoveryBuildWitness(payload,entry.example)} {language} targetLines={payload.height} showFrameCount={false}
                expectedTerminalBoardMask={recoveryBuildTerminalMask(entry.example)} ariaLabel={label('result')} invalidLabel={label('invalidResult')} />
            {/if}
            {#if showProbabilities && payload.solutions_complete}<p class="solution-probability">{standard('solutionProbability')}: {percent(entry.probability)}</p>{/if}
          </li>
        {/each}
      </ul>
    {:else}<p class="empty-state">{label('noPath')}</p>{/if}
  {:else if view.status==='idle'}<p class="empty-state">{label('notReady')}</p>{/if}
</ResultWorkspaceFrame>
<style>
  .recovery-metrics { display:grid;grid-template-columns:repeat(3,minmax(0,1fr));background:#f1f4f2;border-radius:8px; }
  .recovery-metrics article { display:grid;gap:8px;padding:16px;border-right:1px solid white;min-width:0; }
  .recovery-metrics span,.coverage-scope { color:#68736f;font-size:12px;line-height:1.5; }
  .recovery-metrics strong { font-size:24px;overflow-wrap:anywhere; }
  .recovery-metrics small { font-variant-numeric:tabular-nums;overflow-wrap:anywhere; }
  .recovery-path-gallery { display:grid;grid-template-columns:repeat(auto-fit,minmax(min(260px,100%),1fr));gap:14px;list-style:none;margin:0;padding:0; }
  .recovery-path-gallery>li { background:#f1f4f2;border:1px solid #d7ded9;border-radius:8px;padding:14px;min-width:0; }
  h3 { font-size:14px;margin:0 0 12px; }
  .recovery-path-gallery p,.recovery-path-gallery li { font-size:12px; }
  .solution-count,.empty-state { font-size:12px;color:#68736f; }
  .mandatory-choice,.recovery-pagination { display:flex;align-items:center;gap:8px;margin:8px 0; }
  .recovery-pagination { justify-content:center; }
  button { border:1px solid #cfd8d3;border-radius:5px;padding:6px 10px;background:#fff;color:#35443f;cursor:pointer; }
  button:disabled { opacity:.5;cursor:default; }
  .invalid-evidence { color:#8d3026; } @media(max-width:520px) { .recovery-metrics{grid-template-columns:1fr;} }
</style>
