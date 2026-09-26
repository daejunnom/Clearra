<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import type { SpinProfile } from './solverWorkspaceModel';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';
  export let value: SpinProfile;
  export let language: WorkspaceLanguage;
  export let disabled = false;
  const dispatch = createEventDispatcher<{ change: SpinProfile }>();
</script>
<label class="workspace-field">
  <span>{workspaceMessage(language, 'spinProfile')}</span>
  <select {value} {disabled} on:change={(event) => dispatch('change', (event.currentTarget as HTMLSelectElement).value as SpinProfile)}>
    <option value="t-spins">T-Spins</option>
    <option value="t-spins-plus">T-Spins+</option>
    <option value="all-spin">All-Spin</option>
    <option value="all-spin-plus">All-Spin+</option>
    <option value="all-mini">All-Mini</option>
    <option value="all-mini-plus">All-Mini+</option>
  </select>
</label>
