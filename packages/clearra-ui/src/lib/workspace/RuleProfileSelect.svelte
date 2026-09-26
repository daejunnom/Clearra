<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { RuleProfile } from './solverWorkspaceModel';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';
  export let value: RuleProfile;
  export let language: WorkspaceLanguage;
  export let disabled = false;
  const dispatch = createEventDispatcher<{ change: RuleProfile }>();
</script>
<label class="workspace-field">
  <span>{workspaceMessage(language, 'rule')}</span>
  <select {value} {disabled} on:change={(event) => dispatch('change', (event.currentTarget as HTMLSelectElement).value as RuleProfile)}>
    <option value="srs-plus">SRS+</option>
    <option value="srs">SRS</option>
    <option value="srs-x">SRS-X</option>
    <option value="jstris-180">Jstris 180</option>
  </select>
</label>
