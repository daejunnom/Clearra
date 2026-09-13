// SRP: read-only access to one already leased OPFS file. No download, graph
// interpretation or generation ownership. Never open an exclusive/write mode.
type ReadAccess = {
  readonly mode: string;
  getSize(): number;
  read(buffer: Uint8Array, options: { at: number }): number;
  close(): void;
};
type SyncCapableFile = {
  createSyncAccessHandle?: (options: { mode: 'read-only' }) => Promise<ReadAccess>;
};
const fail = (code: string): never => { throw Object.assign(new Error(code), { code }); };
const checkSignal = (signal?: AbortSignal) => { if (signal?.aborted) fail('pc4_online_cancelled'); };

export async function openPc4LocalFileSource(fileHandle: FileSystemFileHandle, size: number, signal?: AbortSignal) {
  checkSignal(signal);
  let access: ReadAccess | undefined;
  try {
    // The mode attribute is the multiple-readers capability, not merely the
    // older exclusive sync API. That API is exposed in dedicated workers only.
    // Do not probe an old implementation by taking a write/exclusive lock.
    const prototype = (globalThis as { FileSystemSyncAccessHandle?: { prototype: object } })
      .FileSystemSyncAccessHandle?.prototype;
    const syncFile = fileHandle as unknown as SyncCapableFile;
    if (prototype && 'mode' in prototype && typeof syncFile.createSyncAccessHandle === 'function') {
      try { access = await syncFile.createSyncAccessHandle({ mode: 'read-only' }); }
      catch (error) {
        // Unsupported optional API keeps the existing local Blob path. Real
        // I/O, permission and lock failures are not hidden or retried over HTTP.
        if (!(error instanceof TypeError) && !(error instanceof DOMException && error.name === 'NotSupportedError')) throw error;
      }
      if (access && access.mode !== 'read-only') {
        access.close(); access = undefined;
      }
    }
    checkSignal(signal);
    const file = access ? null : await fileHandle.getFile();
    checkSignal(signal);
    if ((access ? access.getSize() : file!.size) !== size) fail('pc4_download_local_size_mismatch');
    const backend = access ? 'sync-access-handle' as const : 'blob-slice' as const;
    let closed = false, closing: Promise<void> | undefined;
    const pending = new Set<Promise<Uint8Array>>();
    const checkOpen = () => { checkSignal(signal); if (closed) fail('pc4_online_cancelled'); };
    const read = (offset: number, length: number): Promise<Uint8Array> => {
      const work = (async () => {
        checkOpen();
        if (!Number.isSafeInteger(offset) || offset < 0 || !Number.isSafeInteger(length) || length < 1 ||
            length > 65536 || offset > size - length) fail('pc4_online_range_invalid');
        if (access) {
          const bytes = new Uint8Array(length);
          let readBytes = 0;
          while (readBytes < length) {
            const count = access.read(bytes.subarray(readBytes), { at: offset + readBytes });
            if (!Number.isSafeInteger(count) || count <= 0 || count > length - readBytes) fail('pc4_online_truncated_range');
            readBytes += count;
          }
          return bytes;
        }
        const bytes = new Uint8Array(await file!.slice(offset, offset + length).arrayBuffer());
        checkOpen();
        if (bytes.length !== length) fail('pc4_online_truncated_range');
        return bytes;
      })();
      pending.add(work);
      // Both handlers resolve: cleanup itself must not create an unhandled rejection.
      void work.then(() => pending.delete(work), () => pending.delete(work));
      return work;
    };
    const close = (): Promise<void> => {
      if (!closing) {
        closed = true;
        closing = (async () => {
          // Keep the outer generation lease until even a pending Blob slice
          // has settled. Update/delete must not race a discarded reader.
          await Promise.allSettled([...pending]);
          access?.close();
        })();
      }
      return closing;
    };
    return { backend, read, close };
  } catch (error) {
    access?.close();
    throw error;
  }
}
