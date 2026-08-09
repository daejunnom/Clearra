<script lang="ts">
  import { AlertTriangle, Check, Copy } from '@lucide/svelte';

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
  let failureKey: 'solutionCopyFailed' | 'fumenCopyHeightUnsupported' =
    'solutionCopyFailed';
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
      await navigator.clipboard.writeText(source);
      failureKey = 'solutionCopyFailed';
      setState('copied');
    } catch (error) {
      failureKey =
        error instanceof SolutionExportError && error.code === 'fumen-height-unsupported'
          ? 'fumenCopyHeightUnsupported'
          : 'solutionCopyFailed';
      setState('failed');
    }
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
    height: 28px;
    justify-content: center;
    padding: 0;
    width: 28px;
  }

  button:disabled {
    cursor: default;
    opacity: .35;
  }
</style>
