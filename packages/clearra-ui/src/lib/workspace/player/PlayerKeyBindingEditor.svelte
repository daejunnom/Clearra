<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  import {
    PLAYER_BINDING_DEFINITIONS,
    playerKeyboardCodeLabel,
    type PlayerBindingAction,
    type PlayerKeyBindings
  } from './playerUiModel';
  import {
    workspaceMessage,
    type WorkspaceLanguage,
    type WorkspaceMessageKey
  } from '../workspaceI18n';

  export let language: WorkspaceLanguage;
  export let bindings: PlayerKeyBindings;
  export let capturingAction: PlayerBindingAction | null = null;
  export let disabled = false;

  const dispatch = createEventDispatcher<{
    capturebinding: {
      action: PlayerBindingAction;
      currentCode: string;
      bindings: PlayerKeyBindings;
    };
    cancelbinding: void;
  }>();
  let bindingList: HTMLDivElement;
  let cancelRequested = false;

  $: label = (key: WorkspaceMessageKey) => workspaceMessage(language, key);
  $: if (!capturingAction) cancelRequested = false;

  function requestCapture(action: PlayerBindingAction) {
    if (disabled) return;
    cancelRequested = false;
    dispatch('capturebinding', {
      action,
      currentCode: bindings[action],
      bindings: { ...bindings }
    });
  }

  function handleFocusOut(event: FocusEvent) {
    if (!capturingAction) return;
    const next = event.relatedTarget;
    if (next instanceof Node && bindingList.contains(next)) return;
    requestCancel();
  }

  function handleWindowPointerDown(event: PointerEvent) {
    if (!capturingAction || event.composedPath().includes(bindingList)) return;
    requestCancel();
  }

  function requestCancel() {
    if (!capturingAction || cancelRequested) return;
    cancelRequested = true;
    dispatch('cancelbinding');
  }
</script>

<svelte:window on:pointerdown={handleWindowPointerDown} />

<div
  bind:this={bindingList}
  class="binding-list"
  role="group"
  aria-label={label('playerKeys')}
  on:focusout={handleFocusOut}
>
  <span class="capture-announcement" aria-live="polite">
    {capturingAction ? label('playerPressKey') : ''}
  </span>
  {#each PLAYER_BINDING_DEFINITIONS as definition}
    <div class="binding-row">
      <span>{label(definition.labelKey)}</span>
      <button
        type="button"
        class:capturing={capturingAction === definition.action}
        aria-pressed={capturingAction === definition.action}
        aria-label={`${label(definition.labelKey)}: ${
          capturingAction === definition.action
            ? label('playerPressKey')
            : playerKeyboardCodeLabel(bindings[definition.action])
        }`}
        {disabled}
        on:click={() => requestCapture(definition.action)}
      >
        {capturingAction === definition.action
          ? label('playerPressKey')
          : playerKeyboardCodeLabel(bindings[definition.action])}
      </button>
    </div>
  {/each}
</div>

<style>
  .binding-list {
    display: grid;
    gap: 6px;
  }

  .capture-announcement {
    height: 1px;
    margin: -1px;
    overflow: hidden;
    padding: 0;
    position: absolute;
    width: 1px;
  }

  .binding-row {
    align-items: center;
    border-bottom: 1px solid #e4e9e6;
    display: grid;
    gap: 12px;
    grid-template-columns: minmax(0, 1fr) minmax(112px, auto);
    min-height: 42px;
    padding: 4px 0;
  }

  .binding-row:last-child {
    border-bottom: 0;
  }

  span {
    color: #394640;
    font-size: 11px;
    line-height: 1.35;
  }

  button {
    background: #f5f7f6;
    border: 1px solid #cbd4cf;
    border-radius: 5px;
    color: #26332e;
    cursor: pointer;
    font: 700 11px ui-monospace, SFMono-Regular, Consolas, monospace;
    min-height: 32px;
    padding: 5px 9px;
    white-space: nowrap;
  }

  button:hover:not(:disabled) {
    background: #eaf3f0;
    border-color: #88ada5;
  }

  button:focus-visible {
    outline: 2px solid #16877d;
    outline-offset: 2px;
  }

  button.capturing {
    background: #0d7168;
    border-color: #0d7168;
    color: #fff;
  }

  button:disabled {
    cursor: default;
    opacity: .55;
  }

  @media (max-width: 420px) {
    .binding-row {
      grid-template-columns: minmax(0, 1fr) minmax(96px, auto);
    }
  }
</style>
