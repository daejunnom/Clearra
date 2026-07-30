<script lang="ts">
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';
  import SolutionCopyAllButton from './SolutionCopyAllButton.svelte';
  import type {
    SolutionCopyFormat,
    SolutionExportPage
  } from './solutionExport';

  export let value: SolutionCopyFormat = 'fumen';
  export let language: WorkspaceLanguage;
  export let solutionKeys: string[] = [];
  export let loadPages:
    | ((signal?: AbortSignal) => Promise<SolutionExportPage[]> | SolutionExportPage[])
    | null = null;

  $: label = (
    key: Parameters<typeof workspaceMessage>[1],
    values: Record<string, string | number> = {}
  ) => workspaceMessage(language, key, values);
</script>

<div class="copy-format">
  <div>
    <strong>{label('solutionCopyFormat')}</strong>
    <span>{label('solutionCopyFormatHelp')}</span>
  </div>
  <div class="copy-actions">
    <div class="segments" role="group" aria-label={label('solutionCopyFormat')}>
      <button
        type="button"
        class:active={value === 'fumen'}
        aria-pressed={value === 'fumen'}
        on:click={() => (value = 'fumen')}
      >Fumen</button>
      <button
        type="button"
        class:active={value === 'ctk'}
        aria-pressed={value === 'ctk'}
        on:click={() => (value = 'ctk')}
      >CTK3</button>
    </div>
    <SolutionCopyAllButton
      format={value}
      {language}
      {solutionKeys}
      {loadPages}
    />
  </div>
</div>

<style>
  .copy-format {
    align-items: center;
    background: #f1f4f2;
    display: flex;
    gap: 20px;
    justify-content: space-between;
    min-width: 0;
    padding: 10px 12px;
  }

  .copy-format > div:first-child {
    display: grid;
    gap: 3px;
    min-width: 0;
  }

  strong {
    color: #33423d;
    font-size: 11px;
  }

  span {
    color: #6c7873;
    font-size: 10px;
    line-height: 1.4;
  }

  .segments {
    border: 1px solid #aebbb5;
    border-radius: 5px;
    display: grid;
    flex: 0 0 auto;
    grid-template-columns: repeat(2, minmax(58px, 1fr));
    overflow: hidden;
  }

  .copy-actions {
    align-items: center;
    display: flex;
    flex: 0 0 auto;
    gap: 7px;
  }

  button {
    background: #fff;
    border: 0;
    color: #53615c;
    cursor: pointer;
    font: inherit;
    font-size: 10px;
    font-weight: 750;
    min-height: 30px;
    padding: 0 10px;
  }

  button + button {
    border-left: 1px solid #aebbb5;
  }

  button.active {
    background: #16877d;
    color: #fff;
  }

  @media (max-width: 520px) {
    .copy-format {
      align-items: stretch;
      flex-direction: column;
      gap: 8px;
    }

    .copy-actions {
      align-items: stretch;
      display: grid;
      grid-template-columns: minmax(0, 1fr) auto;
    }
  }
</style>
