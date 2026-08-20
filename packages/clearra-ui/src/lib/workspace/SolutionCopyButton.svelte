<script lang="ts">
  import { AlertTriangle, Check, Copy } from '@lucide/svelte';

  import { writeClipboardText } from './clipboardText';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';
  import {
    encodeFinesseWitnessCtk,
    encodeSolution,
    SolutionExportError,
    type FinesseWitnessExport,
    type SolutionCopyFormat,
    type SolutionExportPage
  } from './solutionExport';

  export let page: SolutionExportPage | null = null;
  export let finesseWitness: FinesseWitnessExport | null = null;
  export let format: SolutionCopyFormat = 'ctk';
  export let language: WorkspaceLanguage;

  let state: 'idle' | 'copied' | 'failed' = 'idle';
  let failureKey:
    | 'solutionCopyFailed'
    | 'fumenCopyHeightUnsupported'
    | 'fumenCommentTooLong'
    | 'invalidFumenComment' = 'solutionCopyFailed';
  let timer = 0;

  $: label = (
    key: Parameters<typeof workspaceMessage>[1],
    values: Record<string, string | number> = {}
  ) => workspaceMessage(language, key, values);
  $: title = state === 'failed'
    ? label(failureKey)
    : label('copySolution');

  async function copySolution() {
    if (!page) return;
    try {
      const source = format === 'ctk' && finesseWitness
        ? encodeFinesseWitnessCtk(finesseWitness)
        : encodeSolution(page, format);
      await writeClipboardText(source);
      failureKey = 'solutionCopyFailed';
      setState('copied');
    } catch (error) {
      failureKey = solutionCopyFailureKey(error);
      setState('failed');
    }
  }

  function solutionCopyFailureKey(error: unknown): typeof failureKey {
    const code = error instanceof SolutionExportError
      ? error.code
      : error instanceof Error
        ? error.message
        : '';
    if (code === 'fumen-height-unsupported') return 'fumenCopyHeightUnsupported';
    if (code === 'fumen-comment-too-long') return 'fumenCommentTooLong';
    if (code === 'invalid-fumen-comment') return 'invalidFumenComment';
    return 'solutionCopyFailed';
  }

  function setState(next: typeof state) {
    state = next;
    window.clearTimeout(timer);
    timer = window.setTimeout(() => {
      state = 'idle';
    }, 1400);
  }
</script>

<button
  type="button"
  disabled={!page}
  {title}
  aria-label={title}
  on:click={copySolution}
>
  {#if state === 'copied'}
    <Check size={14} />
  {:else if state === 'failed'}
    <AlertTriangle size={14} />
  {:else}
    <Copy size={14} />
  {/if}
</button>

<style>
  button {
    align-items: center;
    background: transparent;
    border: 0;
    color: #50605a;
    cursor: pointer;
    display: inline-flex;
    flex: 0 0 auto;
    min-height: 28px;
    justify-content: center;
    padding: 0;
    min-width: 28px;
  }

  button:disabled {
    cursor: default;
    opacity: .35;
  }

  @media (pointer: coarse) {
    button {
      min-height: 44px;
      min-width: 44px;
    }
  }
</style>
