export async function writeClipboardText(
  value: string,
  signal?: AbortSignal
): Promise<void> {
  throwIfAborted(signal);
  const clipboard = globalThis.navigator?.clipboard;
  if (!clipboard) throw new Error('clipboard-unavailable');

  const ClipboardItemConstructor = (
    globalThis as unknown as {
      ClipboardItem?: new (
        items: Record<string, Blob>
      ) => ClipboardItem;
    }
  ).ClipboardItem;
  if (ClipboardItemConstructor && typeof clipboard.write === 'function') {
    try {
      throwIfAborted(signal);
      const item = new ClipboardItemConstructor({
        'text/plain': new Blob([value], {
          type: 'text/plain;charset=utf-8'
        })
      });
      await clipboard.write([item]);
      throwIfAborted(signal);
      return;
    } catch (error) {
      if (signal?.aborted || isAbortError(error)) throw abortError(signal);
      // Some embedded browsers expose ClipboardItem without allowing write().
    }
  }

  if (typeof clipboard.writeText !== 'function') {
    throw new Error('clipboard-write-unavailable');
  }
  throwIfAborted(signal);
  await clipboard.writeText(value);
  throwIfAborted(signal);
}

function throwIfAborted(signal: AbortSignal | undefined): void {
  if (signal?.aborted) throw abortError(signal);
}

function isAbortError(error: unknown): boolean {
  return error instanceof Error && error.name === 'AbortError';
}

function abortError(signal: AbortSignal | undefined): Error {
  if (signal?.reason instanceof Error) return signal.reason;
  const error = new Error('Clipboard write was aborted.');
  error.name = 'AbortError';
  return error;
}
