<script lang="ts">
  import { getContext, type ComponentType, type SvelteComponent } from 'svelte';
  import type { WorkspaceLanguage } from './workspaceI18n';
  export let language: WorkspaceLanguage;
  // Optional host facility; shared controls never import browser storage/I/O.
  const control = getContext<ComponentType<SvelteComponent<{ language: WorkspaceLanguage }>> | undefined>('clearra.pc4-download-control.v1');
</script>

{#if control}<svelte:component this={control} {language} />{/if}
