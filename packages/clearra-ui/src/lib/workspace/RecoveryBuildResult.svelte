<script lang="ts">
  import ResultWorkspaceFrame from './ResultWorkspaceFrame.svelte';
  import SolutionToolbar from './SolutionToolbar.svelte';
  import SolutionCopyFormatControl from './SolutionCopyFormatControl.svelte';
  import RecoveryPng from './RecoveryPng.svelte';
  import WorkspaceToggle from './WorkspaceToggle.svelte';
  import PcPathReplayGif from './PcPathReplayGif.svelte';
  import { recoveryBuildMessage } from './recoveryBuildI18n';
  import { recoveryBuildExamplePages, recoveryBuildExportPages, recoveryBuildWitness, recoveryBuildTerminalMask, validateRecoveryBuildPayload } from './recoveryBuildPresentation';
  import type { RecoveryBuildPayload } from './recoveryBuildPayloadTypes';
  import type { WorkspaceRuntimeView } from './workspaceRuntime';
  import type { SolutionCopyFormat } from './solutionExport';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';
  export let view: WorkspaceRuntimeView;
  export let language: WorkspaceLanguage;
  export let elapsedMs = 0;
  export let pngRender = false;
  let resultOnly = false;
  let copyFormat: SolutionCopyFormat = 'ctk';
  $: label = (key: Parameters<typeof recoveryBuildMessage>[1]) => recoveryBuildMessage(language,key);
  $: standard = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language,key);
  $: product = view.response?.product_result_payload;
  $: candidate = product?.content.payload_kind === 'recovery-build' ? product.content.payload : null;
  $: payload = candidate && product?.contract === 'recovery-build.v2' && validateRecoveryBuildPayload(candidate) ? candidate : null;
  $: invalid = view.status === 'completed' && !payload;
  function percent(value: string): string { return `${(Number(value)*100).toLocaleString(language,{maximumFractionDigits:4})}%`; }
  function exportPages(report: RecoveryBuildPayload) { return () => recoveryBuildExportPages(report, resultOnly); }
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
    <p class="coverage-scope">{label('evaluated')}: {payload.evaluated_pattern_count} / {payload.pattern_count}</p>
    {#if payload.examples.length}
      <SolutionToolbar {language}>
        <svelte:fragment slot="actions"><span class="representative-note">{label('examples')}</span></svelte:fragment>
        <svelte:fragment slot="copy"><WorkspaceToggle label={label('resultOnly')} checked={resultOnly} on:change={(event) => resultOnly = event.detail} /><SolutionCopyFormatControl bind:value={copyFormat} {language} loadPages={exportPages(payload)} /></svelte:fragment>
      </SolutionToolbar>
      <p class="coverage-scope">{label('examplesHelp')}</p>
      <ul class="recovery-path-gallery">
        {#each payload.examples as example (payload.input_identity + example.status)}
          <li data-recovery-path={example.status}>
            <h3>{label(example.status === 'normal' ? 'normal' : 'recovery')}</h3>
            {#if pngRender}
              <RecoveryPng page={recoveryBuildExamplePages(payload,example,true)[0]} ariaLabel={label('result')} invalidLabel={label('invalidResult')} />
            {:else}
              <PcPathReplayGif witness={recoveryBuildWitness(payload,example)} {language} targetLines={payload.height} showFrameCount={false}
                expectedTerminalBoardMask={recoveryBuildTerminalMask(example)} ariaLabel={label('result')} invalidLabel={label('invalidResult')} />
            {/if}
            <p>{label('early')}: {example.actual_early} / {example.effective_max_early}</p>

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
  h3 { font-size:14px;margin:0 0 12px; } code { overflow-wrap:anywhere;font-size:11px; }
  .recovery-path-gallery p,.recovery-path-gallery li { font-size:12px; } summary { cursor:pointer;padding:10px 0; }
  .representative-note,.empty-state { font-size:12px;color:#68736f; }
  .invalid-evidence { color:#8d3026; } @media(max-width:520px) { .recovery-metrics{grid-template-columns:1fr;} }
</style>
