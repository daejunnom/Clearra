<script lang="ts">
  import { Play, Square } from '@lucide/svelte';
  import { createEventDispatcher } from 'svelte';

  import WorkspaceHeader from './WorkspaceHeader.svelte';
  import type { WorkspaceLanguage } from './workspaceI18n';

  export let activeMode: 'pc' | 'build-probability' | 'damage' | 'spin-finder';
  export let language: WorkspaceLanguage;
  export let active = false;
  export let statusLabel: string;
  export let runtimeLabel: string;
  export let workspaceLabel: string;
  export let dimensionLabel: string;
  export let dimensionValue: number;
  export let dimensionMin = 1;
  export let dimensionMax = 24;
  export let cancelLabel: string;
  export let runLabel: string;
  export let runDisabled = false;

  const dispatch = createEventDispatcher<{
    language: WorkspaceLanguage;
    dimension: number;
    cancel: void;
    run: void;
  }>();
  let workspaceRegion: HTMLElement | null = null;

  export function scrollWorkspaceIntoView() {
    workspaceRegion?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }
</script>

<main class="app-shell">
  <WorkspaceHeader
    {activeMode}
    {language}
    {active}
    {statusLabel}
    {runtimeLabel}
    on:language={(event) => dispatch('language', event.detail)}
  />

  <section class="workspace-nav" aria-label={workspaceLabel}>
    <label class="dimension-field">
      <span>{dimensionLabel}</span>
      <input
        type="number"
        min={dimensionMin}
        max={dimensionMax}
        step="1"
        value={dimensionValue}
        on:input={(event) => dispatch('dimension', Number((event.currentTarget as HTMLInputElement).value))}
      />
    </label>
    <div class="run-actions">
      <button class="cancel" type="button" disabled={!active} on:click={() => dispatch('cancel')}>
        <Square size={14} fill="currentColor" />{cancelLabel}
      </button>
      <button class="run" type="button" disabled={active || runDisabled} on:click={() => dispatch('run')}>
        <Play size={16} fill="currentColor" />{runLabel}
      </button>
    </div>
  </section>

  <section class="workspace-band" bind:this={workspaceRegion}>
    <slot name="notice" />
    <div class="workspace-grid">
      <div class="workspace-editor"><slot name="editor" /></div>
      <div class="workspace-controls"><slot name="controls" /></div>
    </div>
  </section>

  <slot name="result" />
</main>

<style>
  :global(*) { box-sizing: border-box; }
  :global(html) { background: #eef1ed; font-family: Inter, "Noto Sans KR", ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
  :global(body) { margin: 0; min-width: 320px; }
  :global(body), :global(button), :global(input), :global(select), :global(textarea) {
    font-family: Inter, "Noto Sans KR", ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    letter-spacing: 0;
  }
  :global(button:focus-visible), :global(a:focus-visible), :global(summary:focus-visible) {
    outline: 2px solid #16877d;
    outline-offset: 2px;
  }
  .app-shell { background: #eef1ed; color: #17211e; min-height: 100vh; }
  .workspace-nav { align-items: end; display: flex; gap: 18px; margin: 0 auto; max-width: 1460px; padding: 18px 24px 4px; }
  .dimension-field { display: grid; gap: 5px; margin-right: auto; }
  .dimension-field span { color: #65716c; font-size: 11px; font-weight: 700; }
  .dimension-field input { background: #fff; border: 1px solid #cbd3ce; border-radius: 5px; color: #26322e; font-size: 13px; height: 38px; padding: 0 10px; width: 130px; }
  .dimension-field input:focus { border-color: #16877d; box-shadow: 0 0 0 3px #16877d1f; outline: 0; }
  .run-actions { display: flex; gap: 8px; }
  .run-actions button { align-items: center; border-radius: 5px; cursor: pointer; display: inline-flex; font-size: 12px; font-weight: 750; gap: 7px; height: 38px; padding: 0 15px; }
  .run-actions button:disabled { cursor: default; opacity: .4; }
  .run { background: #16877d; border: 1px solid #0e746b; color: #fff; }
  .cancel { background: #fff; border: 1px solid #bfc9c4; color: #4c5954; }
  .workspace-band { padding: 16px max(24px, calc((100vw - 1460px) / 2)) 24px; scroll-margin-top: 122px; }
  .workspace-grid { background: #fff; border: 1px solid #d5dcd7; border-radius: 7px; display: grid; gap: 0; grid-template-columns: minmax(380px, 1.1fr) minmax(420px, .9fr); padding: 24px; }
  .workspace-editor, .workspace-controls { min-width: 0; }
  .workspace-editor { padding-right: 24px; }
  .workspace-controls { border-left: 1px solid #d9dfdb; padding-left: 24px; }

  @media (max-width: 980px) {
    .workspace-grid { grid-template-columns: 1fr; }
    .workspace-editor { padding-right: 0; }
    .workspace-controls { border-left: 0; border-top: 1px solid #d9dfdb; margin-top: 24px; padding-left: 0; padding-top: 24px; }
  }
  @media (max-width: 560px) {
    .workspace-nav { align-items: stretch; flex-wrap: wrap; padding: 16px 16px 2px; }
    .dimension-field { flex: 1 1 100%; }
    .dimension-field input { width: 100%; }
    .run-actions { display: grid; flex: 1 1 100%; grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .run-actions button { justify-content: center; min-width: 0; padding: 0 11px; width: 100%; }
    .workspace-band { padding-left: 16px; padding-right: 16px; }
    .workspace-grid { border-left: 0; border-radius: 0; border-right: 0; margin-left: -16px; margin-right: -16px; padding: 18px 16px; }
  }
</style>
