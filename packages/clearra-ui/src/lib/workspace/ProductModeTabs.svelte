<script lang="ts">
  import { goto } from '$app/navigation';
  import { Blocks, FileStack, Flame, Gamepad2, GitBranch, Grid3X3, Image, Layers3, ListOrdered, Palette, Scale, RotateCw } from '@lucide/svelte';

  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';
  import type { WorkspaceMode } from './workspaceMode';

  export let active: WorkspaceMode;
  export let language: WorkspaceLanguage;
  export let busy = false;

  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);

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
  <a
    href="?tool=pc"
    class:active={active === 'pc'}
    class:busy
    aria-disabled={busy}
    aria-current={active === 'pc' ? 'page' : undefined}
    on:click={preventBusyNavigation}
  >
    <Grid3X3 size={16} strokeWidth={1.8} />{label('pcSearch')}
  </a>
  <a
    href="?tool=setup"
    class:active={active === 'setup'}
    class:busy
    aria-disabled={busy}
    aria-current={active === 'setup' ? 'page' : undefined}
    on:click={preventBusyNavigation}
  >
    <Layers3 size={16} strokeWidth={1.8} />{label('setupFinder')}
  </a>
  <a
    href="?tool=setup-score"
    class:active={active === 'setup-score'}
    class:busy
    aria-disabled={busy}
    aria-current={active === 'setup-score' ? 'page' : undefined}
    on:click={preventBusyNavigation}
  >
    <Layers3 size={16} strokeWidth={1.8} />{language === 'ko' ? 'Setup 점수' : 'Setup score'}
  </a>
  <a
    href="?tool=spin-structure"
    class:active={active === 'spin-structure'}
    class:busy
    aria-disabled={busy}
    aria-current={active === 'spin-structure' ? 'page' : undefined}
    on:click={preventBusyNavigation}
  >
    <RotateCw size={16} strokeWidth={1.8} />{language === 'ko' ? 'Spin 구조' : 'Spin structure'}
  </a>
  <a
    href="?tool=build"
    class:active={active === 'build'}
    class:busy
    aria-disabled={busy}
    aria-current={active === 'build' ? 'page' : undefined}
    on:click={preventBusyNavigation}
  >
    <Blocks size={16} strokeWidth={1.8} />{language === 'ko' ? 'Build 도구' : 'Build tools'}
  </a>
  <a
    href="?tool=build-probability"
    class:active={active === 'build-probability'}
    class:busy
    aria-disabled={busy}
    aria-current={active === 'build-probability' ? 'page' : undefined}
    on:click={preventBusyNavigation}
  >
    <Blocks size={16} strokeWidth={1.8} />{label('buildProbability')}
  </a>
  <a
    href="?tool=sequence"
    class:active={active === 'sequence'}
    class:busy
    aria-disabled={busy}
    aria-current={active === 'sequence' ? 'page' : undefined}
    on:click={preventBusyNavigation}
  >
    <ListOrdered size={16} strokeWidth={1.8} />{label('operationSequence')}
  </a>
  <a
    href="?tool=sequence-dependencies"
    class:active={active === 'sequence-dependencies'}
    class:busy
    aria-disabled={busy}
    aria-current={active === 'sequence-dependencies' ? 'page' : undefined}
    on:click={preventBusyNavigation}
  >
    <GitBranch size={16} strokeWidth={1.8} />{label('sequenceDependencies')}
  </a>
  <a
    href="?tool=parity"
    class:active={active === 'parity'}
    class:busy
    aria-disabled={busy}
    aria-current={active === 'parity' ? 'page' : undefined}
    on:click={preventBusyNavigation}
  >
    <Scale size={16} strokeWidth={1.8} />{label('utilityParity')}
  </a>
  <a
    href="?tool=fumen"
    class:active={active === 'fumen'}
    class:busy
    aria-disabled={busy}
    aria-current={active === 'fumen' ? 'page' : undefined}
    on:click={preventBusyNavigation}
  >
    <FileStack size={16} strokeWidth={1.8} />{label('utilityFumen')}
  </a>
  <a
    href="?tool=render"
    class:active={active === 'render'}
    class:busy
    aria-disabled={busy}
    aria-current={active === 'render' ? 'page' : undefined}
    on:click={preventBusyNavigation}
  >
    <Image size={16} strokeWidth={1.8} />{label('utilityRender')}
  </a>
  <a
    href="?tool=to-gray"
    class:active={active === 'to-gray'}
    class:busy
    aria-disabled={busy}
    aria-current={active === 'to-gray' ? 'page' : undefined}
    on:click={preventBusyNavigation}
  >
    <Palette size={16} strokeWidth={1.8} />{label('utilityToGray')}
  </a>
  <a
    href="?tool=mirror"
    class:active={active === 'mirror'}
    class:busy
    aria-disabled={busy}
    aria-current={active === 'mirror' ? 'page' : undefined}
    on:click={preventBusyNavigation}
  >
    <RotateCw size={16} strokeWidth={1.8} />{label('utilityMirror')}
  </a>
  <a
    href="?tool=damage"
    class:active={active === 'damage'}
    class:busy
    aria-disabled={busy}
    aria-current={active === 'damage' ? 'page' : undefined}
    on:click={preventBusyNavigation}
  >
    <Flame size={16} strokeWidth={1.8} />{label('maximumDamage')}
  </a>
  <a
    href="?tool=spin-finder"
    class:active={active === 'spin-finder'}
    class:busy
    aria-disabled={busy}
    aria-current={active === 'spin-finder' ? 'page' : undefined}
    on:click={preventBusyNavigation}
  >
    <RotateCw size={16} strokeWidth={1.8} />{label('spinFinder')}
  </a>
  <a
    href="?tool=ren"
    class:active={active === 'ren'}
    class:busy
    aria-disabled={busy}
    aria-current={active === 'ren' ? 'page' : undefined}
    on:click={preventBusyNavigation}
  >
    <Layers3 size={16} strokeWidth={1.8} />{label('maximumRen')}
  </a>
  <a
    href="?tool=ctk"
    class:active={active === 'ctk'}
    class:busy
    aria-disabled={busy}
    aria-current={active === 'ctk' ? 'page' : undefined}
    on:click={preventBusyNavigation}
  >
    <Palette size={16} strokeWidth={1.8} />{label('ctkDrawer')}
  </a>
  <a
    href="?tool=player"
    class:active={active === 'player'}
    class:busy
    aria-disabled={busy}
    aria-current={active === 'player' ? 'page' : undefined}
    on:click={preventBusyNavigation}
  >
    <Gamepad2 size={16} strokeWidth={1.8} />{label('player')}
  </a>
</nav>

<div class="product-mode-select">
  <label>
    <span>{label('workspaceMode')}</span>
    <select aria-label={label('workspaceMode')} value={`?tool=${active}`} disabled={busy} on:change={changeMode}>
      <option value="?tool=pc">{label('pcSearch')}</option>
      <option value="?tool=setup">{label('setupFinder')}</option>
      <option value="?tool=setup-score">{language === 'ko' ? 'Setup 점수' : 'Setup score'}</option>
      <option value="?tool=spin-structure">{language === 'ko' ? 'Spin 구조' : 'Spin structure'}</option>
      <option value="?tool=build">{language === 'ko' ? 'Build 도구' : 'Build tools'}</option>
      <option value="?tool=build-probability">{label('buildProbability')}</option>
      <option value="?tool=sequence">{label('operationSequence')}</option>
      <option value="?tool=sequence-dependencies">{label('sequenceDependencies')}</option>
      <option value="?tool=parity">{label('utilityParity')}</option>
      <option value="?tool=fumen">{label('utilityFumen')}</option>
      <option value="?tool=render">{label('utilityRender')}</option>
      <option value="?tool=to-gray">{label('utilityToGray')}</option>
      <option value="?tool=mirror">{label('utilityMirror')}</option>
      <option value="?tool=damage">{label('maximumDamage')}</option>
      <option value="?tool=spin-finder">{label('spinFinder')}</option>
      <option value="?tool=ren">{label('maximumRen')}</option>
      <option value="?tool=ctk">{label('ctkDrawer')}</option>
      <option value="?tool=player">{label('player')}</option>
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
    padding: 0 max(24px, calc((100vw - 1460px) / 2));
    overflow-x: auto;
  }

  a {
    align-items: center;
    border-bottom: 2px solid transparent;
    color: #596560;
    display: inline-flex;
    font-size: 13px;
    font-weight: 720;
    gap: 7px;
    height: 49px;
    padding: 0 14px;
    text-decoration: none;
  }

  a:hover {
    color: #075f58;
  }

  a.active {
    border-bottom-color: #16877d;
    color: #075f58;
  }

  a.busy {
    cursor: not-allowed;
    opacity: .55;
  }

  .product-mode-select {
    background: #fff;
    border-bottom: 1px solid #d7ded9;
    display: none;
    padding: 10px 16px;
  }

  .product-mode-select label {
    display: grid;
    gap: 5px;
    min-width: 0;
  }

  .product-mode-select span {
    color: #65716c;
    font-size: 11px;
    font-weight: 700;
  }

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
    .product-tabs {
      display: none;
    }

    .product-mode-select {
      display: block;
    }
  }
</style>
