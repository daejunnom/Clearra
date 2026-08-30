<script lang="ts">
  import { goto } from '$app/navigation';
  import {
    Blocks,
    FileStack,
    Flame,
    Gamepad2,
    GitBranch,
    Grid3X3,
    Image,
    Layers3,
    ListOrdered,
    Palette,
    RotateCw,
    Scale
  } from '@lucide/svelte';
  import { getContext } from 'svelte';

  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';
  import type { WorkspaceMode } from './workspaceMode';
  import { WORKSPACE_MODE_VISIBILITY_CONTEXT } from './workspaceNavigation';

  export let active: WorkspaceMode;
  export let language: WorkspaceLanguage;
  export let busy = false;

  const visibleModes = getContext<readonly WorkspaceMode[] | null>(
    WORKSPACE_MODE_VISIBILITY_CONTEXT
  ) ?? null;

  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);
  $: allTabs = [
    { mode: 'pc', icon: Grid3X3, text: label('pcSearch') },
    { mode: 'setup', icon: Layers3, text: label('setupFinder') },
    { mode: 'setup-score', icon: Layers3, text: language === 'ko' ? 'Setup 점수' : 'Setup score' },
    { mode: 'spin-structure', icon: RotateCw, text: language === 'ko' ? 'Spin 구조' : 'Spin structure' },
    { mode: 'build', icon: Blocks, text: language === 'ko' ? 'Build 도구' : 'Build tools' },
    { mode: 'build-probability', icon: Blocks, text: label('buildProbability') },
    { mode: 'sequence', icon: ListOrdered, text: label('operationSequence') },
    { mode: 'sequence-dependencies', icon: GitBranch, text: label('sequenceDependencies') },
    { mode: 'parity', icon: Scale, text: label('utilityParity') },
    { mode: 'fumen', icon: FileStack, text: label('utilityFumen') },
    { mode: 'render', icon: Image, text: label('utilityRender') },
    { mode: 'to-gray', icon: Palette, text: label('utilityToGray') },
    { mode: 'mirror', icon: RotateCw, text: label('utilityMirror') },
    { mode: 'damage', icon: Flame, text: label('maximumDamage') },
    { mode: 'spin-finder', icon: RotateCw, text: label('spinFinder') },
    { mode: 'ren', icon: Layers3, text: label('maximumRen') },
    { mode: 'ctk', icon: Palette, text: label('ctkDrawer') },
    { mode: 'player', icon: Gamepad2, text: label('player') }
  ] satisfies Array<{ mode: WorkspaceMode; icon: typeof Grid3X3; text: string }>;
  $: tabs = visibleModes === null
    ? allTabs
    : allTabs.filter((tab) => visibleModes.includes(tab.mode));

  function changeMode(event: Event) {
    if (busy) return;
    void goto((event.currentTarget as HTMLSelectElement).value, {
      noScroll: true,
      keepFocus: true
    });
  }

  function preventBusyNavigation(event: MouseEvent) {
    if (busy) event.preventDefault();
  }
</script>

<nav class="product-tabs" aria-label={label('workspaceMode')}>
  {#each tabs as tab (tab.mode)}
    <a
      href={`?tool=${tab.mode}`}
      class:active={active === tab.mode}
      class:busy
      aria-disabled={busy}
      aria-current={active === tab.mode ? 'page' : undefined}
      on:click={preventBusyNavigation}
    >
      <svelte:component this={tab.icon} size={16} strokeWidth={1.8} />{tab.text}
    </a>
  {/each}
</nav>

<div class="product-mode-select">
  <label>
    <span>{label('workspaceMode')}</span>
    <select aria-label={label('workspaceMode')} value={`?tool=${active}`} disabled={busy} on:change={changeMode}>
      {#each tabs as tab (tab.mode)}
        <option value={`?tool=${tab.mode}`}>{tab.text}</option>
      {/each}
    </select>
  </label>
</div>

<style>
  .product-tabs {
    align-items: end;
    background: #ffffff;
    border-bottom: 1px solid #d7ded9;
    display: flex;
    gap: 4px;
    min-height: 49px;
    overflow-x: auto;
    padding: 0 max(24px, calc((100vw - 1460px) / 2));
  }

  a {
    align-items: center;
    border-bottom: 2px solid transparent;
    color: #596560;
    display: inline-flex;
    flex: 0 0 auto;
    font-size: 13px;
    font-weight: 720;
    gap: 7px;
    height: 49px;
    padding: 0 14px;
    text-decoration: none;
  }

  a:hover { color: #075f58; }
  a.active { border-bottom-color: #16877d; color: #075f58; }
  a.busy { cursor: not-allowed; opacity: .55; }

  .product-mode-select {
    background: #fff;
    border-bottom: 1px solid #d7ded9;
    display: none;
    padding: 10px 16px;
  }

  .product-mode-select label { display: grid; gap: 5px; min-width: 0; }
  .product-mode-select span { color: #65716c; font-size: 11px; font-weight: 700; }

  .product-mode-select select {
    appearance: auto;
    background: #fff;
    border: 1px solid #cbd3ce;
    border-radius: 5px;
    color: #26322e;
    font-size: 13px;
    font-weight: 720;
    height: 40px;
    min-width: 0;
    padding: 0 10px;
    width: 100%;
  }

  .product-mode-select select:focus {
    border-color: #16877d;
    box-shadow: 0 0 0 3px #16877d1f;
    outline: 0;
  }

  @media (max-width: 720px) {
    .product-tabs { display: none; }
    .product-mode-select { display: block; }
  }
</style>
