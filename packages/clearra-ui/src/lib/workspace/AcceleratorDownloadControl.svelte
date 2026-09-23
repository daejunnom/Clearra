<script lang="ts">
  import { getContext, type ComponentType, type SvelteComponent } from 'svelte';
  import type { WorkspaceLanguage } from './workspaceI18n';
  export let language: WorkspaceLanguage;
  // Optional host facility. Shared UI never imports browser storage or HTTP.
  const control = getContext<ComponentType<SvelteComponent<{ language: WorkspaceLanguage }>> | undefined>('clearra.accelerator-download-control.v1');
</script>

{#if control}<svelte:component this={control} {language} />{/if}
