/** Keep bounded compute quanta fair to host task sources. */
export function createWorkerHostYield(lane: 'mixed' | 'timer' = 'mixed'): () => Promise<void> {
  const scheduler = (globalThis as typeof globalThis & {
    scheduler?: { yield?: () => Promise<void> };
  }).scheduler;
  const schedulerYield = scheduler?.yield?.bind(scheduler);
  // A coordinator also services IDB and durable offer/start acknowledgements.
  // Its continuation must not repeatedly refill the posted-message lane while
  // those callbacks wait. scheduler.yield admits those task sources without a
  // background-tab timer clamp; old engines retain the timer-only fallback.
  // Callers still coalesce cheap slices within one quantum.
  if (lane === 'timer') {
    return schedulerYield ?? (() => new Promise<void>((resolve) => setTimeout(resolve, 0)));
  }
  const channel = new MessageChannel();
  const pending: Array<() => void> = [];
  let channelQuanta = 0;
  channel.port1.onmessage = () => pending.shift()?.();
  (channel.port1 as MessagePort & { unref?: () => void }).unref?.();
  (channel.port2 as MessagePort & { unref?: () => void }).unref?.();
  return () => {
    // A self-refilling MessageChannel can monopolize its task source. Give
    // progress/cancellation a turn occasionally. Chrome can heavily clamp a
    // worker timer when the page is backgrounded; scheduler.yield preserves a
    // fair continuation without turning an active 11-worker search into long
    // periods of near-zero CPU. Older engines retain the bounded timer fallback.
    if (++channelQuanta >= 8) {
      channelQuanta = 0;
      if (schedulerYield) return schedulerYield();
      return new Promise<void>((resolve) => setTimeout(resolve, 0));
    }
    return new Promise<void>((resolve) => {
      pending.push(resolve);
      channel.port2.postMessage(undefined);
    });
  };
}
