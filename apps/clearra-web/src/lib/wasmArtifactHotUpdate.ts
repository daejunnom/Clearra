import {
  announceWasmArtifactGeneration,
  type ClearraWasmArtifactGeneration
} from '@clearra/ui/wasm-artifact-generation';

export const CLEARRA_WASM_ARTIFACT_UPDATE_EVENT =
  'clearra:wasm-artifact-updated' as const;
export const CLEARRA_WASM_ARTIFACT_SYNC_EVENT =
  'clearra:wasm-artifact-sync' as const;

const VITE_WEBSOCKET_CONNECT_EVENT = 'vite:ws:connect' as const;

type ClearraViteHotContext = Readonly<{
  on: {
    (
      event: typeof CLEARRA_WASM_ARTIFACT_UPDATE_EVENT,
      listener: (payload: unknown) => void
    ): void;
    (event: typeof VITE_WEBSOCKET_CONNECT_EVENT, listener: () => void): void;
  };
  off: {
    (
      event: typeof CLEARRA_WASM_ARTIFACT_UPDATE_EVENT,
      listener: (payload: unknown) => void
    ): void;
    (event: typeof VITE_WEBSOCKET_CONNECT_EVENT, listener: () => void): void;
  };
  send: (event: typeof CLEARRA_WASM_ARTIFACT_SYNC_EVENT) => void;
  dispose: (listener: () => void) => void;
}>;

export function installWasmArtifactHotUpdate(
  hot: ClearraViteHotContext | undefined,
  mode = ''
): () => void {
  if (mode === 'local-recovery' || mode === 'local-audit') {
    return installWasmArtifactPolling(async (signal) => {
      const response = await fetch('/__clearra/wasm-generation', {
        cache: 'no-store', signal
      });
      return response.ok ? response.json() : null;
    });
  }
  if (!hot) return () => undefined;
  let installed = true;
  const receive = (payload: unknown) => {
    const generation = parseGeneration(payload);
    if (generation) announceWasmArtifactGeneration(generation);
  };
  const synchronize = () => hot.send(CLEARRA_WASM_ARTIFACT_SYNC_EVENT);
  const remove = () => {
    if (!installed) return;
    installed = false;
    hot.off(CLEARRA_WASM_ARTIFACT_UPDATE_EVENT, receive);
    hot.off(VITE_WEBSOCKET_CONNECT_EVENT, synchronize);
  };
  hot.on(CLEARRA_WASM_ARTIFACT_UPDATE_EVENT, receive);
  hot.on(VITE_WEBSOCKET_CONNECT_EVENT, synchronize);
  hot.dispose(remove);
  // Vite buffers client events sent before the initial WebSocket connection.
  // Repeating this handshake after reconnect closes the only gap in a
  // broadcast-only protocol: an artifact published while the tab was asleep
  // or disconnected must still invalidate its retained worker generation.
  synchronize();
  return remove;
}

/** The audit page never reloads: a verified update affects only the next job. */
export function installWasmArtifactPolling(
  fetchGeneration: (signal: AbortSignal) => Promise<unknown>,
  schedule: (callback: () => void) => () => void = (callback) => {
    const timer = setTimeout(callback, 1_000);
    return () => clearTimeout(timer);
  }
): () => void {
  let installed = true;
  let cancelTimer: (() => void) | undefined;
  const controller = new AbortController();
  const synchronize = async () => {
    try {
      const generation = parseGeneration(await fetchGeneration(controller.signal));
      if (installed && generation) announceWasmArtifactGeneration(generation);
    } catch {
      // A stopped/restarting dev server is not a request to destroy user state.
    } finally {
      if (installed) cancelTimer = schedule(() => void synchronize());
    }
  };
  void synchronize();
  return () => {
    installed = false;
    cancelTimer?.();
    controller.abort();
  };
}

export function parseGeneration(
  payload: unknown
): ClearraWasmArtifactGeneration | null {
  if (!payload || typeof payload !== 'object') return null;
  const candidate = payload as Record<string, unknown>;
  if (
    typeof candidate.sourceSha256 !== 'string' ||
    typeof candidate.bindingsSha256 !== 'string' ||
    typeof candidate.wasmSha256 !== 'string' ||
    ![candidate.sourceSha256, candidate.bindingsSha256, candidate.wasmSha256]
      .every((value) => /^[0-9a-f]{64}$/.test(value))
  ) {
    return null;
  }
  return {
    sourceSha256: candidate.sourceSha256,
    bindingsSha256: candidate.bindingsSha256,
    wasmSha256: candidate.wasmSha256
  };
}
