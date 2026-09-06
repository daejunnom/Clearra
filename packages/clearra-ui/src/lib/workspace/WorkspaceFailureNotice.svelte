<script lang="ts">
  import { TriangleAlert } from '@lucide/svelte';

  import type { WorkspaceLanguage } from './workspaceI18n';
  import {
    workspacePublicFailureMessage,
    type WorkspacePublicFailure
  } from './workspacePublicFailure';

  export let failures: WorkspacePublicFailure[] = [];
  export let language: WorkspaceLanguage;
  export let heading = '';
  export let compact = false;

  $: messages = Array.from(new Map(
    failures.map((failure) => [failure.code, workspacePublicFailureMessage(language, failure)] as const)
  ));
</script>

{#if messages.length}
  <div class:compact class="workspace-failure-notice" role="alert">
    <TriangleAlert size={compact ? 15 : 18} strokeWidth={2} />
    <div>
      {#if heading}<strong>{heading}</strong>{/if}
      {#each messages as [code, message]}<p>{message} <code>{code}</code></p>{/each}
    </div>
  </div>
{/if}

<style>
  .workspace-failure-notice {
    align-items: start;
    background: #fff1ed;
    border: 1px solid #e5b2a4;
    color: #8d3026;
    display: grid;
    gap: 10px;
    grid-template-columns: auto minmax(0, 1fr);
    margin-top: 16px;
    padding: 12px 14px;
  }

  .workspace-failure-notice.compact {
    font-size: 11px;
    margin-top: 8px;
    padding: 8px 10px;
  }

  strong,
  p {
    margin: 0;
  }

  p {
    font-size: 12px;
    margin-top: 4px;
    overflow-wrap: anywhere;
  }

  .compact p {
    font-size: 10px;
  }

  code {
    display: inline-block;
    font-size: 0.9em;
    margin-left: 0.5em;
  }
</style>
