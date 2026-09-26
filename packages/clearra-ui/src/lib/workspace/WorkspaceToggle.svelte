<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  export let label: string;
  export let checked = false;
  export let disabled = false;
  const dispatch = createEventDispatcher<{ change: boolean }>();
</script>
<label class="workspace-switch-label">
  <input type="checkbox" {checked} {disabled}
    on:change={(event) => dispatch('change', (event.currentTarget as HTMLInputElement).checked)} />
  <span class="workspace-switch" aria-hidden="true"></span>
  <span>{label}</span>
</label>
