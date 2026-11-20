<script lang="ts">
  import { AlertTriangle, Check, Download, LoaderCircle } from '@lucide/svelte';
  import { getContext, onDestroy, onMount } from 'svelte';

  import {
    HOST_CAPABILITY_SNAPSHOT_CONTEXT,
    sharedBrowserHostCapabilitySnapshot,
    type HostCapabilitySnapshot
  } from '../wasm/hostCapabilitySnapshot';
  import { saveCtk3Source } from './ctk3File';
  import type { SolutionExportPage } from './solutionExport';
  import {
    encodeSolutionKeySource,
    encodeSolutionKeysForClipboard,
    encodeSolutionPagesForClipboard,
    type SolutionExportKeySource
  } from './solutionExportAsync';
  import { createSolutionExportOperation } from './solutionExportOperation';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';

  export let language: WorkspaceLanguage;
  export let solutionKeys: string[] = [];
  export let loadPages:
    | ((signal?: AbortSignal) => Promise<SolutionExportPage[]> | SolutionExportPage[])
    | null = null;
  export let keySource: SolutionExportKeySource | null = null;

  const hostCapabilitySnapshot =
    getContext<HostCapabilitySnapshot>(HOST_CAPABILITY_SNAPSHOT_CONTEXT) ??
    sharedBrowserHostCapabilitySnapshot();

  let state: 'idle' | 'loading' | 'saved' | 'failed' = 'idle';
  let failureKey:
    | 'solutionDownloadFailed'
    | 'fumenCommentTooLong'
    | 'invalidFumenComment' = 'solutionDownloadFailed';
  const operation = createSolutionExportOperation({
    success: 'saved',
    abortMessage: 'Solution download was aborted.',
    onState(next) {
      if (next === 'saved') failureKey = 'solutionDownloadFailed';
      state = next;
    },
    onFailure(error) {
      failureKey = solutionDownloadFailureKey(error);
    }
  });

  $: available = solutionKeys.length > 0 || loadPages !== null || keySource !== null;
  $: label = (
    key: Parameters<typeof workspaceMessage>[1],
    values: Record<string, string | number> = {}
  ) => workspaceMessage(language, key, values);

  onMount(() => operation.bindPageHide(window));
  onDestroy(operation.destroy);

  function downloadAll() {
    return operation.run(available, async (signal, checkAbort) => {
      let encoded: string;
      if (keySource) {
        encoded = await encodeSolutionKeySource(keySource, 'ctk', {
          signal,
          hostCapabilitySnapshot
        });
      } else if (loadPages) {
        const pages = await loadPages(signal);
        checkAbort();
        encoded = await encodeSolutionPagesForClipboard(pages, 'ctk', {
          signal,
          hostCapabilitySnapshot
        });
      } else {
        encoded = await encodeSolutionKeysForClipboard(solutionKeys, 'ctk', {
          signal,
          hostCapabilitySnapshot
        });
      }
      checkAbort();
      saveCtk3Source(encoded, 'clearra-solutions.ctk3');
    });
  }

  function solutionDownloadFailureKey(error: unknown): typeof failureKey {
    const code = error instanceof Error ? error.message : '';
    if (code === 'fumen-comment-too-long') return 'fumenCommentTooLong';
    if (code === 'invalid-fumen-comment') return 'invalidFumenComment';
    return 'solutionDownloadFailed';
  }
</script>

<button
  type="button"
  disabled={!available || state === 'loading'}
  aria-busy={state === 'loading'}
  title={state === 'failed' ? label(failureKey) : label('downloadCtk3File')}
  on:click={downloadAll}
>
  {#if state === 'loading'}
    <span class="spinner"><LoaderCircle size={14} /></span>
  {:else if state === 'saved'}
    <Check size={14} />
  {:else if state === 'failed'}
    <AlertTriangle size={14} />
  {:else}
    <Download size={14} />
  {/if}
  <span>{label(state === 'failed' ? 'solutionDownloadFailed' : 'downloadCtk3File')}</span>
</button>

<style>
  button {
    align-items: center;
    background: #fff;
    border: 1px solid #aebbb5;
    border-radius: 5px;
    color: #3e4d48;
    cursor: pointer;
    display: inline-flex;
    flex: 0 0 auto;
    font: inherit;
    font-size: 10px;
    font-weight: 750;
    gap: 6px;
    min-height: 32px;
    min-width: 0;
    padding: 0 10px;
    overflow-wrap: anywhere;
  }

  button:disabled { cursor: default; opacity: .45; }
  .spinner { animation: spin .8s linear infinite; display: inline-flex; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (pointer: coarse) { button { min-height: 44px; } }
</style>
