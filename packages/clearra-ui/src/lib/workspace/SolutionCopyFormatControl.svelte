<script lang="ts">
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';
  import SolutionCopyAllButton from './SolutionCopyAllButton.svelte';
  import SolutionDownloadButton from './SolutionDownloadButton.svelte';
  import type {
    SolutionCopyFormat,
    SolutionExportPage
  } from './solutionExport';
  import type { SolutionExportKeySource } from './solutionExportAsync';

  export let value: SolutionCopyFormat = 'ctk';
  export let language: WorkspaceLanguage;
  export let compact = false;
  export let solutionKeys: string[] = [];
  export let loadPages:
    | ((signal?: AbortSignal) => Promise<SolutionExportPage[]> | SolutionExportPage[])
    | null = null;
  export let keySource: SolutionExportKeySource | null = null;

  $: label = (
    key: Parameters<typeof workspaceMessage>[1],
    values: Record<string, string | number> = {}
  ) => workspaceMessage(language, key, values);
</script>

<div class="copy-format" class:compact>
  {#if !compact}
    <div>
      <strong>{label('solutionCopyFormat')}</strong>
      <span>{label('solutionCopyFormatHelp')}</span>
    </div>
  {/if}
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
    <div class="copy-primary-action">
      <SolutionCopyAllButton
        format={value}
        {language}
        {solutionKeys}
        {loadPages}
        {keySource}
      />
    </div>
    {#if value === 'ctk'}
      <div class="download-action">
        <SolutionDownloadButton
          {language}
          {solutionKeys}
          {loadPages}
          {keySource}
        />
      </div>
    {/if}
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
    max-width: 100%;
    padding: 10px 12px;
  }
  .copy-format.compact { background: transparent; justify-content: flex-end; padding: 0; }

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
    flex-wrap: wrap;
    max-width: 100%;
    min-width: 0;
  }

  .copy-primary-action,
  .download-action {
    min-width: 0;
  }

  .copy-primary-action :global(button),
  .download-action :global(button) {
    max-width: 100%;
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
      grid-template-columns: repeat(2, minmax(0, 1fr));
      width: 100%;
    }

    .segments {
      grid-column: 1 / -1;
      width: 100%;
    }

    .copy-primary-action :global(button),
    .download-action :global(button) {
      justify-content: center;
      min-width: 0;
      overflow-wrap: anywhere;
      padding-inline: 7px;
      width: 100%;
    }
  }

  @media (max-width: 360px) {
    .copy-actions { grid-template-columns: minmax(0, 1fr); }
    .segments { grid-column: 1; }
  }

  @media (pointer: coarse) {
    button { min-height: 44px; }
  }
</style>
