<script lang="ts">
  import { AlertTriangle, Check, Copy, LoaderCircle } from '@lucide/svelte';
  import { getContext, onDestroy, onMount } from 'svelte';

  import {
    HOST_CAPABILITY_SNAPSHOT_CONTEXT,
    sharedBrowserHostCapabilitySnapshot,
    type HostCapabilitySnapshot
  } from '../wasm/hostCapabilitySnapshot';
  import { writeClipboardText } from './clipboardText';
  import {
    SolutionExportError,
    type SolutionCopyFormat,
    type SolutionExportPage
  } from './solutionExport';
  import {
    encodeSolutionKeySourceForClipboard,
    encodeSolutionKeysForClipboard,
    encodeSolutionPagesForClipboard,
    type SolutionExportKeySource
  } from './solutionExportAsync';
  import {
    workspaceMessage,
    workspaceSolutionCopyFailureKey,
    type WorkspaceLanguage,
    type WorkspaceSolutionCopyFailureKey
  } from './workspaceI18n';

  export let format: SolutionCopyFormat = 'ctk';
  export let language: WorkspaceLanguage;
  export let solutionKeys: string[] = [];
  export let loadPages:
    | ((signal?: AbortSignal) => Promise<SolutionExportPage[]> | SolutionExportPage[])
    | null = null;
  export let keySource: SolutionExportKeySource | null = null;

  const hostCapabilitySnapshot =
    getContext<HostCapabilitySnapshot>(HOST_CAPABILITY_SNAPSHOT_CONTEXT) ??
    sharedBrowserHostCapabilitySnapshot();

  let state: 'idle' | 'loading' | 'copied' | 'failed' = 'idle';
  let failureKey: WorkspaceSolutionCopyFailureKey = 'solutionCopyFailed';
  let timer = 0;
  let copyController: AbortController | null = null;
  let destroyed = false;

  onMount(() => {
    const handlePageHide = () => abortCopy();
    window.addEventListener('pagehide', handlePageHide);
    return () => window.removeEventListener('pagehide', handlePageHide);
  });

  onDestroy(() => {
    destroyed = true;
    abortCopy();
    clearTimeout(timer);
  });

  $: label = (
    key: Parameters<typeof workspaceMessage>[1],
    values: Record<string, string | number> = {}
  ) => workspaceMessage(language, key, values);
  $: available =
    solutionKeys.length > 0 || loadPages !== null || keySource !== null;
  $: title = state === 'failed'
    ? label(failureKey)
    : state === 'loading'
      ? label('copyAllPending')
      : label('copyAllSolutions');

  async function copyAll() {
    if (!available || state === 'loading') return;
    window.clearTimeout(timer);
    copyController?.abort();
    const controller = new AbortController();
    copyController = controller;
    state = 'loading';
    await nextPaint();
    try {
      throwIfAborted(controller.signal);
      let encoded: string;
      if (keySource) {
        encoded = await encodeSolutionKeySourceForClipboard(keySource, format, {
          signal: controller.signal,
          hostCapabilitySnapshot
        });
      } else if (loadPages) {
        const pages = await loadPages(controller.signal);
        throwIfAborted(controller.signal);
        encoded = await encodeSolutionPagesForClipboard(pages, format, {
          signal: controller.signal,
          hostCapabilitySnapshot
        });
      } else {
        encoded = await encodeSolutionKeysForClipboard(solutionKeys, format, {
          signal: controller.signal,
          hostCapabilitySnapshot
        });
      }
      await writeClipboardText(encoded, controller.signal);
      if (copyController !== controller || destroyed) return;
      failureKey = 'solutionCopyFailed';
      setTerminalState('copied');
    } catch (error) {
      if (controller.signal.aborted || isAbortError(error)) {
        if (!destroyed && copyController === controller) state = 'idle';
        return;
      }
      failureKey = solutionCopyFailureKey(error);
      setTerminalState('failed');
    } finally {
      if (copyController === controller) copyController = null;
    }
  }

  function abortCopy() {
    if (copyController && !copyController.signal.aborted) {
      const error = new Error('Solution copy was aborted.');
      error.name = 'AbortError';
      copyController.abort(error);
    }
  }

  function setTerminalState(next: 'copied' | 'failed') {
    state = next;
    window.clearTimeout(timer);
    timer = window.setTimeout(() => {
      state = 'idle';
    }, next === 'failed' ? 4000 : 1600);
  }

  function nextPaint(): Promise<void> {
    return new Promise((resolve) => {
      if (typeof requestAnimationFrame === 'function') {
        requestAnimationFrame(() => resolve());
      } else {
        setTimeout(resolve, 0);
      }
    });
  }

  function throwIfAborted(signal: AbortSignal) {
    if (!signal.aborted) return;
    if (signal.reason instanceof Error) throw signal.reason;
    const error = new Error('Solution copy was aborted.');
    error.name = 'AbortError';
    throw error;
  }

  function isAbortError(error: unknown): boolean {
    return error instanceof Error && error.name === 'AbortError';
  }

  function solutionCopyFailureKey(error: unknown): typeof failureKey {
    const code = error instanceof SolutionExportError
      ? error.code
      : error instanceof Error
        ? error.message
        : '';
    return workspaceSolutionCopyFailureKey(code);
  }
</script>

<button
  type="button"
  class="copy-all"
  disabled={!available || state === 'loading'}
  aria-busy={state === 'loading'}
  {title}
  on:click={copyAll}
>
  {#if state === 'loading'}
    <span class="spinner"><LoaderCircle size={14} /></span>
  {:else if state === 'copied'}
    <Check size={14} />
  {:else if state === 'failed'}
    <AlertTriangle size={14} />
  {:else}
    <Copy size={14} />
  {/if}
  <span>{label(
    state === 'loading'
      ? 'copyAllPending'
      : state === 'failed'
        ? failureKey
        : 'copyAll'
  )}</span>
</button>

<style>
  .copy-all {
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

  .copy-all:disabled {
    cursor: default;
    opacity: .45;
  }

  .spinner {
    animation: spin .8s linear infinite;
    display: inline-flex;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  @media (pointer: coarse) {
    .copy-all { min-height: 44px; }
  }
</style>
