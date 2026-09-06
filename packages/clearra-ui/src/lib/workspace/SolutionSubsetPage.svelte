<script lang="ts">
  import SolutionCopyFormatControl from './SolutionCopyFormatControl.svelte';
  import type { SolutionCopyFormat } from './solutionExport';
  import type { SolutionExportKeySource } from './solutionExportAsync';
  import SolutionGallery from './SolutionGallery.svelte';
  import type { WorkspaceLanguage } from './workspaceI18n';

  export let solutionKeys: string[] = [];
  export let exportSolutionKeys: string[] | null = null;
  export let exportKeySource: SolutionExportKeySource | null = null;
  export let exportSetIdentity = '';
  export let solutionCaptions: string[] = [];
  export let solutionSetIdentity = '';
  export let solutionOrdinalBase = '0';
  export let targetLines = 4;
  export let language: WorkspaceLanguage;
  export let copyFormat: SolutionCopyFormat = 'ctk';
</script>

<div class="solution-subset-page">
  {#key exportSetIdentity || solutionSetIdentity}
    <SolutionCopyFormatControl
      bind:value={copyFormat}
      {language}
      solutionKeys={exportSolutionKeys ?? solutionKeys}
      keySource={exportKeySource}
    />
  {/key}
  <SolutionGallery
    {solutionKeys}
    solutionCount={solutionKeys.length}
    solutionSetHash={solutionSetIdentity}
    {solutionOrdinalBase}
    {solutionCaptions}
    {targetLines}
    {language}
    {copyFormat}
  />
</div>

<style>
  .solution-subset-page {
    display: grid;
    gap: 12px;
    padding: 12px 15px;
  }
</style>
