<script lang="ts">
  import { goto } from '$app/navigation';
  import { base } from '$app/paths';
  import { page } from '$app/stores';
  import { isWorkspaceMode, WorkspaceToolRouter } from '@clearra/ui/workspace';
  import { onMount } from 'svelte';

  $: selectedTool = $page.url.searchParams.get('tool') ?? 'pc';
  $: workspaceMode = isWorkspaceMode(selectedTool) ? selectedTool : null;

  onMount(() => {
    if (workspaceMode === null) {
      void goto(`${base}/?tool=pc`, { replaceState: true, noScroll: true, keepFocus: true });
    }
  });
</script>

<WorkspaceToolRouter runtime="desktop" selectedTool={workspaceMode} />
