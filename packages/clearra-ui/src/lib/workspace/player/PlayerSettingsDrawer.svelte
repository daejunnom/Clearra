<script lang="ts">
  import { Settings2, X } from '@lucide/svelte';
  import { createEventDispatcher, tick } from 'svelte';

  export let title: string;
  export let openLabel: string;
  export let closeLabel: string;
  export let open = false;

  const dispatch = createEventDispatcher<{ openchange: boolean }>();
  let launcher: HTMLButtonElement;
  let closeButton: HTMLButtonElement;
  let drawer: HTMLElement;
  let previousOpen = open;
  let restoreLauncherFocus = true;

  $: if (open !== previousOpen) {
    const shouldRestoreLauncher = restoreLauncherFocus;
    previousOpen = open;
    void tick().then(() => {
      if (open) closeButton?.focus({ preventScroll: true });
      else if (shouldRestoreLauncher) launcher?.focus({ preventScroll: true });
      restoreLauncherFocus = true;
    });
  }

  function requestOpen(next: boolean, restoreFocus = true) {
    restoreLauncherFocus = restoreFocus;
    if (open !== next) dispatch('openchange', next);
  }

  function handleWindowPointerDown(event: PointerEvent) {
    if (!open) return;
    const path = event.composedPath();
    if (path.includes(drawer) || path.includes(launcher)) return;
    requestOpen(false, false);
  }
</script>

<svelte:window on:pointerdown={handleWindowPointerDown} />

<button
  bind:this={launcher}
  class="settings-launcher"
  type="button"
  aria-controls="player-settings-drawer"
  aria-expanded={open}
  aria-label={openLabel}
  title={openLabel}
  on:click={() => requestOpen(!open)}
>
  <Settings2 size={19} strokeWidth={1.8} />
  <span>{title}</span>
</button>

<aside
  bind:this={drawer}
  id="player-settings-drawer"
  class="settings-drawer"
  class:open
  aria-hidden={!open}
  inert={!open}
  aria-label={title}
>
  <header>
    <div>
      <Settings2 size={18} strokeWidth={1.8} />
      <h2>{title}</h2>
    </div>
    <button
      bind:this={closeButton}
      type="button"
      aria-label={closeLabel}
      title={closeLabel}
      on:click={() => requestOpen(false)}
    >
      <X size={19} strokeWidth={1.8} />
    </button>
  </header>
  <div class="drawer-body"><slot /></div>
</aside>

<style>
  .settings-launcher {
    align-items: center;
    background: #fff;
    border: 1px solid #bfcac5;
    border-radius: 999px;
    box-shadow: 0 9px 24px rgba(24, 39, 34, .14);
    color: #31413b;
    cursor: pointer;
    display: inline-flex;
    font-size: 11px;
    font-weight: 800;
    gap: 7px;
    min-height: 44px;
    padding: 0 13px;
    position: fixed;
    right: 18px;
    top: 82px;
    z-index: 62;
  }

  .settings-launcher:hover {
    background: #e8f3f0;
    border-color: #78a9a1;
    color: #075f58;
  }

  .settings-launcher[aria-expanded='true'] {
    opacity: 0;
    pointer-events: none;
    visibility: hidden;
  }

  .settings-drawer {
    background: #fbfcfb;
    border-left: 1px solid #ccd6d1;
    box-shadow: -18px 0 42px rgba(22, 36, 31, .16);
    height: calc(100dvh - 70px);
    max-width: calc(100vw - 16px);
    overflow: hidden;
    pointer-events: none;
    position: fixed;
    right: 0;
    top: 70px;
    transform: translateX(102%);
    transition: transform 180ms ease, visibility 180ms;
    visibility: hidden;
    width: 420px;
    z-index: 61;
  }

  .settings-drawer.open {
    pointer-events: auto;
    transform: translateX(0);
    visibility: visible;
  }

  header {
    align-items: center;
    background: #fff;
    border-bottom: 1px solid #d8dfdb;
    display: flex;
    justify-content: space-between;
    min-height: 58px;
    padding: 0 14px 0 18px;
  }

  header > div,
  header button {
    align-items: center;
    display: flex;
  }

  header > div { gap: 9px; }

  h2 {
    font-size: 14px;
    margin: 0;
  }

  header button {
    background: transparent;
    border: 0;
    border-radius: 5px;
    color: #53615b;
    cursor: pointer;
    height: 44px;
    justify-content: center;
    width: 44px;
  }

  header button:hover { background: #eef3f1; color: #17211e; }

  .drawer-body {
    height: calc(100% - 58px);
    overflow: auto;
    overscroll-behavior: contain;
    padding: 16px;
  }

  @media (prefers-reduced-motion: reduce) {
    .settings-drawer { transition: none; }
  }

  @media (max-width: 560px) {
    .settings-launcher {
      height: 44px;
      padding: 0;
      right: 12px;
      width: 44px;
    }

    .settings-launcher span { display: none; }
    .settings-drawer { width: min(400px, calc(100vw - 48px)); }
  }
</style>
