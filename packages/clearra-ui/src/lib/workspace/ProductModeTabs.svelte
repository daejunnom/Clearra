<script lang="ts">
  import { Blocks, Flame, Grid3X3, RotateCw } from '@lucide/svelte';

  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';

  export let active: 'pc' | 'build-probability' | 'damage' | 'spin-finder';
  export let language: WorkspaceLanguage;

  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);
</script>

<nav class="product-tabs" aria-label={label('workspaceMode')}>
  <a href="?tool=pc" class:active={active === 'pc'} aria-current={active === 'pc' ? 'page' : undefined}>
    <Grid3X3 size={16} strokeWidth={1.8} />{label('pcSearch')}
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
</nav>

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
</style>
