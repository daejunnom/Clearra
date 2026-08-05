<script lang="ts">
  import { createEventDispatcher, onDestroy, onMount } from 'svelte';

  import {
    drawPlayerFrame,
    type PlayerBoardFrame,
    type PlayerRenderAppearance,
    type PlayerRenderPhase
  } from './playerRenderer';
  import { shouldActivatePlayerBoardFromKey } from './playerInput';

  export let boardLabel: string;
  export let focusHint: string;
  export let activeHint: string;
  export let readyLabel: string;
  export let pausedLabel: string;
  export let gameOverLabel: string;

  const dispatch = createEventDispatcher<{ capture: boolean; activate: void }>();
  let canvas: HTMLCanvasElement;
  let surface: HTMLDivElement;
  let context: CanvasRenderingContext2D | null = null;
  let resizeObserver: ResizeObserver | null = null;
  let currentFrame: PlayerBoardFrame | null = null;
  let currentAppearance: PlayerRenderAppearance = {
    ghostOpacity: 0.5,
    gridOpacity: 0.75
  };
  let phase: PlayerRenderPhase = 'ready';
  let captured = false;
  let lastPixelRatio = 0;

  $: overlayLabel =
    phase === 'ready'
      ? readyLabel
      : phase === 'paused'
        ? pausedLabel
        : phase === 'game-over'
          ? gameOverLabel
          : '';

  onMount(() => {
    context = canvas.getContext('2d', { alpha: false });
    resizeObserver = new ResizeObserver(([entry]) => {
      if (entry) resizeCanvas(entry.contentRect.width, entry.contentRect.height);
      else resizeCanvas();
    });
    resizeObserver.observe(surface);
    window.addEventListener('resize', handleWindowResize);
    resizeCanvas();
  });

  onDestroy(() => {
    resizeObserver?.disconnect();
    if (typeof window !== 'undefined') window.removeEventListener('resize', handleWindowResize);
  });

  export function renderFrame(
    frame: PlayerBoardFrame,
    appearance: PlayerRenderAppearance
  ): void {
    currentFrame = frame;
    currentAppearance = appearance;
    phase = frame.phase;
    if (currentPixelRatio() !== lastPixelRatio) resizeCanvas();
    else draw();
  }

  export function focus(): void {
    surface?.focus({ preventScroll: true });
    if (surface && document.activeElement === surface) setCaptured(true);
  }

  export function release(): void {
    surface?.blur();
  }

  function handleWindowResize() {
    resizeCanvas();
  }

  function handleWindowPointerDown(event: PointerEvent) {
    if (
      !captured ||
      event.defaultPrevented ||
      event.composedPath().includes(surface)
    ) return;
    release();
  }

  function currentPixelRatio() {
    return Math.max(1, Math.min(3, window.devicePixelRatio || 1));
  }

  function resizeCanvas(cssWidth?: number, cssHeight?: number) {
    if (!surface || !canvas) return;
    const bounds = cssWidth === undefined || cssHeight === undefined
      ? surface.getBoundingClientRect()
      : null;
    const ratio = currentPixelRatio();
    const width = Math.max(1, Math.round((cssWidth ?? bounds?.width ?? 0) * ratio));
    const height = Math.max(1, Math.round((cssHeight ?? bounds?.height ?? 0) * ratio));
    lastPixelRatio = ratio;
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }
    draw();
  }

  function draw() {
    if (!context || !currentFrame) return;
    drawPlayerFrame(
      context,
      canvas.width,
      canvas.height,
      currentFrame,
      currentAppearance
    );
  }

  function setCaptured(next: boolean) {
    if (captured === next) return;
    captured = next;
    dispatch('capture', next);
  }

  function handleSurfaceKeyDown(event: KeyboardEvent) {
    if (!shouldActivatePlayerBoardFromKey(event, phase === 'playing')) return;
    event.preventDefault();
    event.stopPropagation();
    activateBoard();
  }

  function activateBoard() {
    focus();
    dispatch('activate');
  }
</script>

<svelte:window on:pointerdown={handleWindowPointerDown} />

<div class="board-wrap">
  <div
    bind:this={surface}
    data-player-board-surface
    class="board-surface"
    class:captured
    role="button"
    tabindex="0"
    aria-label={boardLabel}
    aria-describedby="player-focus-help"
    on:focus={() => setCaptured(true)}
    on:blur={() => setCaptured(false)}
    on:pointerdown={() => focus()}
    on:click={activateBoard}
    on:keydown={handleSurfaceKeyDown}
  >
    <canvas bind:this={canvas} aria-hidden="true"></canvas>
    {#if overlayLabel}
      <div class="state-overlay" aria-live="polite">
        <strong>{overlayLabel}</strong>
        {#if phase === 'ready'}<span>{focusHint}</span>{/if}
      </div>
    {/if}
  </div>
  <p id="player-focus-help" class:active={captured}>
    {captured ? activeHint : focusHint}
  </p>
</div>

<style>
  .board-wrap {
    margin: 0 auto;
    max-width: 440px;
    min-width: 0;
    width: 100%;
  }

  .board-surface {
    aspect-ratio: 1 / 2;
    background: #101817;
    border: 1px solid #253330;
    border-radius: 7px;
    box-shadow: 0 12px 30px rgba(20, 33, 29, .17);
    cursor: pointer;
    max-height: min(68vh, 720px);
    overflow: hidden;
    position: relative;
    touch-action: none;
    width: 100%;
  }

  .board-surface:focus {
    outline: 0;
  }

  .board-surface:focus-visible,
  .board-surface.captured {
    box-shadow:
      0 0 0 3px #eef1ed,
      0 0 0 6px #16877d,
      0 12px 30px rgba(20, 33, 29, .17);
  }

  canvas {
    display: block;
    height: 100%;
    width: 100%;
  }

  .state-overlay {
    align-items: center;
    background: rgba(12, 20, 18, .72);
    color: #f4f8f6;
    display: flex;
    flex-direction: column;
    gap: 8px;
    inset: 0;
    justify-content: center;
    padding: 28px;
    position: absolute;
    text-align: center;
  }

  .state-overlay strong {
    font-size: 21px;
  }

  .state-overlay span {
    color: #d2dfdb;
    font-size: 11px;
    line-height: 1.5;
    max-width: 250px;
  }

  p {
    color: #67736e;
    font-size: 10px;
    line-height: 1.45;
    margin: 10px 0 0;
    text-align: center;
  }

  p.active {
    color: #075f58;
    font-weight: 700;
  }
</style>
