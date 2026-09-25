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
    <strong>{componentMessage(language, 'mandatorySolutions')}: {count}</strong>
    <p>{componentMessage(language, 'runPinnedMinimum')}</p>
    <div>
      <button type="button" {disabled} on:click={() => dispatch('run')}>{workspaceMessage(language, 'minimumSolutions')}</button>
      <button type="button" {disabled} on:click={() => dispatch('clear')}>{componentMessage(language, 'clearMandatorySelection')}</button>
    </div>
  </section>
{/if}
<style>
  .mandatory-summary { border: 1px solid #cbd3ce; border-radius: 6px; margin: 12px 0; padding: 12px; font-size: 12px; }
  p { margin: 8px 0; }
  div { display: flex; flex-wrap: wrap; gap: 8px; }
  button { background: #fff; border: 1px solid #aebbb5; border-radius: 5px; cursor: pointer; min-height: 44px; padding: 8px 12px; }
  button:disabled { opacity: .5; cursor: default; }
</style>
