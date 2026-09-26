<script lang="ts">
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';
  export let language: WorkspaceLanguage;
</script>
<div class="solution-toolbar">
  <h3>{workspaceMessage(language, 'solutions')}</h3>
  <div class="solution-actions"><slot name="actions" /></div>
  <div class="solution-copy"><slot name="copy" /></div>
</div>
<style>
  .solution-toolbar { align-items: center; display: flex; flex-wrap: wrap; gap: 12px 16px; min-width: 0; margin-bottom: 14px; }
  h3 { color: #253d35; font-size: 14px; font-weight: 750; flex: 0 0 auto; margin: 0; white-space: nowrap; }
  .solution-actions { flex: 1 1 240px; min-width: 0; max-width: 100%; }
  .solution-copy { flex: 0 1 auto; min-width: 0; max-width: 100%; margin-left: auto; }
  .solution-actions:empty, .solution-copy:empty { display: none; }
  @media (max-width: 980px) {
    .solution-toolbar { align-items: stretch; }
    h3 { flex-basis: 100%; }
    .solution-actions, .solution-copy { flex: 1 1 100%; margin-left: 0; }
  }
</style>
