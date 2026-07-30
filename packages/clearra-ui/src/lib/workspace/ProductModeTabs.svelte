<script lang="ts">
  import { Blocks, Flame, Grid3X3, Layers3, Palette, RotateCw } from '@lucide/svelte';

  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';
  import type { WorkspaceMode } from './workspaceMode';

  export let active: WorkspaceMode;
  export let language: WorkspaceLanguage;

  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);

  function changeMode(event: Event) {
    window.location.assign((event.currentTarget as HTMLSelectElement).value);
  }
</script>

<nav class="product-tabs" aria-label={label('workspaceMode')}>
  <a href="?tool=pc" class:active={active === 'pc'} aria-current={active === 'pc' ? 'page' : undefined}>
    <Grid3X3 size={16} strokeWidth={1.8} />{label('pcSearch')}
  </a>
  <a href="?tool=setup" class:active={active === 'setup'} aria-current={active === 'setup' ? 'page' : undefined}>
    <Layers3 size={16} strokeWidth={1.8} />{label('setupFinder')}
  </a>
  <a
    href="?tool=build-probability"
    class:active={active === 'build-probability'}
    aria-current={active === 'build-probability' ? 'page' : undefined}
  >
    <Blocks size={16} strokeWidth={1.8} />{label('buildProbability')}
  </a>
  <a href="?tool=damage" class:active={active === 'damage'} aria-current={active === 'damage' ? 'page' : undefined}>
    <Flame size={16} strokeWidth={1.8} />{label('maximumDamage')}
  </a>
  <a href="?tool=spin-finder" class:active={active === 'spin-finder'} aria-current={active === 'spin-finder' ? 'page' : undefined}>
    <RotateCw size={16} strokeWidth={1.8} />{label('spinFinder')}
  </a>
  <a href="?tool=ctk" class:active={active === 'ctk'} aria-current={active === 'ctk' ? 'page' : undefined}>
    <Palette size={16} strokeWidth={1.8} />{label('ctkDrawer')}
  </a>
</nav>

<div class="product-mode-select">
  <label>
    <span>{label('workspaceMode')}</span>
    <select aria-label={label('workspaceMode')} value={`?tool=${active}`} on:change={changeMode}>
      <option value="?tool=pc">{label('pcSearch')}</option>
      <option value="?tool=setup">{label('setupFinder')}</option>
      <option value="?tool=build-probability">{label('buildProbability')}</option>
      <option value="?tool=damage">{label('maximumDamage')}</option>
      <option value="?tool=spin-finder">{label('spinFinder')}</option>
      <option value="?tool=ctk">{label('ctkDrawer')}</option>
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
