<script lang="ts">
  import { AlertTriangle, Check, Download, LoaderCircle } from '@lucide/svelte';
  import { onDestroy, onMount } from 'svelte';

  import { saveCtk3Source } from './ctk3File';
  import type { SolutionExportPage } from './solutionExport';
  import {
    encodeSolutionKeySource,
    encodeSolutionKeysForClipboard,
    encodeSolutionPagesForClipboard,
    type SolutionExportKeySource
  } from './solutionExportAsync';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';

  export let language: WorkspaceLanguage;
  export let solutionKeys: string[] = [];
  export let loadPages:
    | ((signal?: AbortSignal) => Promise<SolutionExportPage[]> | SolutionExportPage[])
    | null = null;
  export let keySource: SolutionExportKeySource | null = null;

  let state: 'idle' | 'loading' | 'saved' | 'failed' = 'idle';
  let controller: AbortController | null = null;
  let timer = 0;
  let destroyed = false;

  $: available = solutionKeys.length > 0 || loadPages !== null || keySource !== null;
  $: label = (
    key: Parameters<typeof workspaceMessage>[1],
    values: Record<string, string | number> = {}
  ) => workspaceMessage(language, key, values);

  onMount(() => {
    const handlePageHide = () => abortDownload();
    window.addEventListener('pagehide', handlePageHide);
    return () => window.removeEventListener('pagehide', handlePageHide);
  });

  onDestroy(() => {
    destroyed = true;
    abortDownload();
    window.clearTimeout(timer);
  });

  async function downloadAll() {
    if (!available || state === 'loading') return;
    window.clearTimeout(timer);
    abortDownload();
    const nextController = new AbortController();
    controller = nextController;
    state = 'loading';
    await nextPaint();
    try {
      let encoded: string;
      if (keySource) {
        encoded = await encodeSolutionKeySource(keySource, 'ctk', {
          signal: nextController.signal
        });
      } else if (loadPages) {
        const pages = await loadPages(nextController.signal);
        throwIfAborted(nextController.signal);
        encoded = await encodeSolutionPagesForClipboard(pages, 'ctk', {
          signal: nextController.signal
        });
      } else {
        encoded = await encodeSolutionKeysForClipboard(solutionKeys, 'ctk', {
          signal: nextController.signal
        });
      }
      throwIfAborted(nextController.signal);
      saveCtk3Source(encoded, 'clearra-solutions.ctk3');
      if (controller !== nextController || destroyed) return;
      setTerminalState('saved');
    } catch (error) {
      if (nextController.signal.aborted || isAbortError(error)) {
        if (!destroyed && controller === nextController) state = 'idle';
      } else {
        setTerminalState('failed');
      }
    } finally {
      if (controller === nextController) controller = null;
    }
  }

  function abortDownload() {
    if (!controller || controller.signal.aborted) return;
    const error = new Error('Solution download was aborted.');
    error.name = 'AbortError';
    controller.abort(error);
  }

  function setTerminalState(next: 'saved' | 'failed') {
    state = next;
    window.clearTimeout(timer);
    timer = window.setTimeout(() => (state = 'idle'), next === 'failed' ? 4000 : 1600);
  }

  function nextPaint(): Promise<void> {
    return new Promise((resolve) => {
      if (typeof requestAnimationFrame === 'function') requestAnimationFrame(() => resolve());
      else setTimeout(resolve, 0);
    });
  }

  function throwIfAborted(signal: AbortSignal) {
    if (!signal.aborted) return;
    if (signal.reason instanceof Error) throw signal.reason;
    const error = new Error('Solution download was aborted.');
    error.name = 'AbortError';
    throw error;
  }

  function isAbortError(error: unknown): boolean {
    return error instanceof Error && error.name === 'AbortError';
  }
</script>

<button
  type="button"
  disabled={!available || state === 'loading'}
  aria-busy={state === 'loading'}
  title={state === 'failed' ? label('solutionDownloadFailed') : label('downloadCtk3File')}
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
    padding: 0 10px;
  }

  button:disabled { cursor: default; opacity: .45; }
  .spinner { animation: spin .8s linear infinite; display: inline-flex; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
