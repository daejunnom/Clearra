<script lang="ts">
  import { ExternalLink, Languages } from '@lucide/svelte';
  import { createEventDispatcher, getContext, onMount } from 'svelte';

  import ProductModeTabs from './ProductModeTabs.svelte';
  import { PC_SOLVER_HREF_CONTEXT } from './workspaceNavigation';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';
  import type { WorkspaceMode } from './workspaceMode';

  export let activeMode: WorkspaceMode;
  export let language: WorkspaceLanguage;
  export let active = false;
  export let statusLabel: string;

  const dispatch = createEventDispatcher<{ language: WorkspaceLanguage }>();
  const pcSolverHref = getContext<string | null>(PC_SOLVER_HREF_CONTEXT) ?? null;
  let hidden = false;
  let lastScrollY = 0;
  let frame = 0;

  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);

  onMount(() => {
    lastScrollY = window.scrollY;
    return () => cancelAnimationFrame(frame);
  });

  function handleScroll() {
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(() => {
      const current = Math.max(0, window.scrollY);
      const delta = current - lastScrollY;
      if (current <= 12) hidden = false;
      else if (delta > 6) hidden = true;
      else if (delta < -4) hidden = false;
      lastScrollY = current;
    });
  }
</script>

<svelte:window on:scroll={handleScroll} />

<div class:hidden class="header-shell">
  <header class="app-header" class:has-solver-link={Boolean(pcSolverHref)}>
    <div class="brand">
      <div class="brand-mark" aria-hidden="true"><span></span><span></span><span></span><span></span></div>
      <h1>Clearra</h1>
    </div>
    {#if pcSolverHref}
      <a class="pc-solver-link" href={pcSolverHref} title={label('openPcSolver')}>
        <ExternalLink size={15} strokeWidth={1.8} />
        <span>{label('pcSolver')}</span>
      </a>
    {/if}
    <div class="header-status">
      <span class:running={active}><i></i>{statusLabel}</span>
    </div>
    <div class="language-control" aria-label={label('language')}>
      <Languages size={16} strokeWidth={1.8} />
      <button type="button" class:active={language === 'en'} on:click={() => dispatch('language', 'en')}>EN</button>
      <button type="button" class:active={language === 'ko'} on:click={() => dispatch('language', 'ko')}>KO</button>
    </div>
  </header>
  <ProductModeTabs active={activeMode} {language} busy={active} />
</div>

<style>
  .header-shell {
    position: sticky;
    top: 0;
    transform: translateY(0);
    transition: transform 180ms ease;
    z-index: 40;
  }

  .header-shell.hidden {
    transform: translateY(-100%);
  }

  .app-header {
    align-items: center;
    background: #fff;
    border-bottom: 1px solid #d7ded9;
    display: grid;
    gap: 18px;
    grid-template-columns: minmax(230px, 1fr) auto auto;
    min-height: 70px;
    padding: 12px max(24px, calc((100vw - 1460px) / 2));
  }

  .brand { align-items: center; display: flex; gap: 12px; min-height: 34px; min-width: 0; }
  .brand-mark { display: grid; flex: 0 0 auto; gap: 2px; grid-template-columns: repeat(2, 10px); grid-template-rows: repeat(2, 10px); }
  .brand-mark span { background: #16877d; border-radius: 2px; }
  .brand-mark span:nth-child(2) { background: #e0ac36; }
  .brand-mark span:nth-child(3) { background: #d96c4b; }
  .brand-mark span:nth-child(4) { background: #334e77; }
  .app-header.has-solver-link { grid-template-columns: minmax(230px, 1fr) auto auto auto; }
  h1 { color: #17211e; font-size: 22px; line-height: 1; margin: 0; }
  .pc-solver-link { align-items: center; border: 1px solid #cfd7d2; border-radius: 5px; color: #42504b; display: inline-flex; font-size: 11px; font-weight: 750; gap: 6px; height: 34px; padding: 0 10px; text-decoration: none; }
  .pc-solver-link:hover { background: #eef6f3; border-color: #75a9a1; color: #075f58; }
  .header-status, .header-status > span, .language-control { align-items: center; display: flex; }
  .header-status { gap: 8px; }
  .header-status > span { background: #f0f3f1; border: 1px solid #d5ddd8; border-radius: 4px; color: #52605a; font-size: 11px; font-weight: 700; gap: 6px; min-height: 29px; padding: 0 9px; }
  .header-status i { background: #7b8782; border-radius: 50%; height: 7px; width: 7px; }
  .header-status span.running i { animation: pulse 1.2s ease-in-out infinite; background: #d47a2e; }
  .language-control { border: 1px solid #cfd7d2; border-radius: 5px; gap: 2px; height: 34px; padding: 0 3px 0 8px; }
  .language-control > :global(svg) { color: #68736f; margin-right: 3px; }
  .language-control button { background: transparent; border: 0; border-radius: 3px; color: #6d7873; cursor: pointer; font-size: 10px; font-weight: 800; height: 26px; padding: 0 7px; }
  .language-control button.active { background: #dcece7; color: #075f58; }

  @keyframes pulse { 0%, 100% { opacity: .35; } 50% { opacity: 1; } }

  @media (prefers-reduced-motion: reduce) {
    .header-shell { transition: none; }
  }

  @media (max-width: 720px) {
    .app-header { grid-template-columns: 1fr auto; }
    .app-header.has-solver-link { grid-template-columns: 1fr auto auto; }
    .header-status { display: none; }
    .pc-solver-link { justify-content: center; padding: 0; width: 34px; }
    .pc-solver-link span { display: none; }
  }

  @media (max-width: 520px) {
    .app-header { padding: 11px 16px; }
  }
</style>
