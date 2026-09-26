<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import { componentMessage } from '../i18n/componentCatalog';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';
  export let count = 0;
  export let language: WorkspaceLanguage;
  export let disabled = false;
  const dispatch = createEventDispatcher<{ run: void; clear: void }>();
</script>
{#if count > 0}
  <section class="mandatory-summary" aria-label={componentMessage(language, 'mandatorySolutions')}>
    <div class="selection-copy">
      <strong>{componentMessage(language, 'mandatorySolutions')}: {count}</strong>
      <span>{componentMessage(language, 'runPinnedMinimum')}</span>
    </div>
    <div class="selection-buttons">
      <button type="button" {disabled} on:click={() => dispatch('run')}>{workspaceMessage(language, 'minimumSolutions')}</button>
      <button type="button" {disabled} on:click={() => dispatch('clear')}>{componentMessage(language, 'clearMandatorySelection')}</button>
    </div>
  </section>
{/if}
<style>
  .mandatory-summary { align-items: center; background: #f1f4f2; display: flex; flex-wrap: wrap; gap: 8px 12px; min-width: 0; max-width: 100%; padding: 10px 12px; }
  .selection-copy { display: grid; gap: 3px; min-width: 0; max-width: 26em; }
  strong { color: #33423d; font-size: 11px; }
  span { color: #6c7873; font-size: 10px; line-height: 1.4; }
  .selection-buttons { display: flex; flex-wrap: wrap; gap: 7px; min-width: 0; }
  button { background: #fff; border: 1px solid #aebbb5; border-radius: 5px; color: #53615c; cursor: pointer; font: inherit; font-size: 10px; font-weight: 750; min-height: 32px; padding: 6px 10px; white-space: nowrap; word-break: keep-all; }
  button:disabled { opacity: .5; cursor: default; }
  @media (pointer: coarse) { button { min-height: 44px; } }
</style>
