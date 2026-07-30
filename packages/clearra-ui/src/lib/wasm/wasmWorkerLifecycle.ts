const WASM_OWNER_LIFECYCLE_CHANNEL = 'clearra-wasm-owner-lifecycle-v1';

export type ClearraWasmForcedTerminationReason =
  | 'cancel-timeout'
  | 'owner-disposed'
  | 'worker-failure';

type ClearraWasmOwnerTerminationMessage = {
  type: 'terminate-owner';
  ownerId: string;
  reason: ClearraWasmForcedTerminationReason;
};

let fallbackOwnerSequence = 0;
const workerOwnerIds = new WeakMap<Worker, string>();

export function createWasmWorkerOwnerId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  fallbackOwnerSequence += 1;
  return `clearra-wasm-owner-${Date.now().toString(36)}-${fallbackOwnerSequence.toString(36)}`;
}

export function ensureWasmWorkerOwnerId(worker: Worker): string {
  const existing = workerOwnerIds.get(worker);
  if (existing) return existing;
  const ownerId = createWasmWorkerOwnerId();
  workerOwnerIds.set(worker, ownerId);
  return ownerId;
}

export function terminateOwnedWasmWorker(
  worker: Worker,
  reason: ClearraWasmForcedTerminationReason
) {
  const ownerId = workerOwnerIds.get(worker);
  if (ownerId) signalWasmOwnerTermination(ownerId, reason);
  workerOwnerIds.delete(worker);
  worker.terminate();
}

export function signalWasmOwnerTermination(
  ownerId: string,
  reason: ClearraWasmForcedTerminationReason
) {
  if (!ownerId || typeof BroadcastChannel !== 'function') return;
  const channel = new BroadcastChannel(WASM_OWNER_LIFECYCLE_CHANNEL);
  channel.postMessage({
    type: 'terminate-owner',
    ownerId,
    reason
  } satisfies ClearraWasmOwnerTerminationMessage);
  setTimeout(() => channel.close(), 0);
}

export function listenForWasmOwnerTermination(
  ownerId: string,
  onTerminate: (reason: ClearraWasmForcedTerminationReason) => void
): () => void {
  if (!ownerId || typeof BroadcastChannel !== 'function') return () => undefined;
  const channel = new BroadcastChannel(WASM_OWNER_LIFECYCLE_CHANNEL);
  channel.onmessage = (event: MessageEvent<unknown>) => {
    if (!isOwnerTerminationMessage(event.data) || event.data.ownerId !== ownerId) return;
    onTerminate(event.data.reason);
  };
  return () => channel.close();
}

function isOwnerTerminationMessage(value: unknown): value is ClearraWasmOwnerTerminationMessage {
  if (typeof value !== 'object' || value === null) return false;
  const candidate = value as Partial<ClearraWasmOwnerTerminationMessage>;
  return (
    candidate.type === 'terminate-owner' &&
    typeof candidate.ownerId === 'string' &&
    (candidate.reason === 'cancel-timeout' ||
      candidate.reason === 'owner-disposed' ||
      candidate.reason === 'worker-failure')
  );
}
