/** Keep bounded compute quanta fair to host task sources. */
export function createWorkerHostYield(lane: 'mixed' | 'timer' = 'mixed'): () => Promise<void> {
  // A coordinator also services IDB and durable offer/start acknowledgements.
  // Its continuation must not repeatedly refill the posted-message lane while
  // those callbacks wait. Callers still coalesce cheap slices within one quantum.
  if (lane === 'timer') return () => new Promise<void>((resolve) => setTimeout(resolve, 0));
  const channel = new MessageChannel();
  const pending: Array<() => void> = [];
  let channelQuanta = 0;
  channel.port1.onmessage = () => pending.shift()?.();
  (channel.port1 as MessagePort & { unref?: () => void }).unref?.();
  (channel.port2 as MessagePort & { unref?: () => void }).unref?.();
  return () => {
    // A self-refilling MessageChannel can monopolize its task source. Give
    // timer-based progress/cancellation a turn occasionally, without paying
    // a clamped timeout after every cheap solver slice.
    if (++channelQuanta >= 8) {
      channelQuanta = 0;
      return new Promise<void>((resolve) => setTimeout(resolve, 0));
    }
    return new Promise<void>((resolve) => {
      pending.push(resolve);
      channel.port2.postMessage(undefined);
    });
  };
}
