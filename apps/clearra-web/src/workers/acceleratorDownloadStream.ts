// SRP: exact-length bounded transfer and response-body ownership. This module
// neither downloads a catalog nor validates/installs an accelerator generation.
const MAX_TRANSFER_BYTES = 64 * 1024 * 1024;

function requireActive(signal: AbortSignal) {
  if (signal.aborted) throw new Error('accelerator_download_cancelled');
}

async function rejectResponse(response: Response, code: string): Promise<never> {
  await response.body?.cancel(code).catch(() => {});
  throw new Error(code);
}

export async function readAcceleratorDownloadResponse(
  response: Response,
  expectedBytes: number,
  signal: AbortSignal,
  progress: (transferred: number, total: number) => void
): Promise<Uint8Array<ArrayBuffer>> {
  if (!Number.isSafeInteger(expectedBytes) || expectedBytes < 1 || expectedBytes > MAX_TRANSFER_BYTES) {
    return rejectResponse(response, 'accelerator_download_size_mismatch');
  }
  if (!response.ok || !response.body) {
    return rejectResponse(response, 'accelerator_download_response_invalid');
  }
  if (signal.aborted) return rejectResponse(response, 'accelerator_download_cancelled');
  const length = response.headers.get('content-length');
  if (length !== null && Number(length) !== expectedBytes) {
    return rejectResponse(response, 'accelerator_download_size_mismatch');
  }
  const reader = response.body.getReader();
  let cancellation: Promise<void> | null = null;
  const cancel = (reason: unknown) => {
    // One stream owner releases the underlying fetch/body on every rejection.
    // Cleanup errors must not replace the original validation/cancel reason.
    cancellation ??= reader.cancel(reason).catch(() => {});
    return cancellation;
  };
  const onAbort = () => { void cancel('accelerator_download_cancelled'); };
  signal.addEventListener('abort', onAbort, { once: true });
  try {
    requireActive(signal);
    const bytes = new Uint8Array(expectedBytes);
    let offset = 0;
    while (true) {
      requireActive(signal);
      const { done, value } = await reader.read();
      requireActive(signal);
      if (done) break;
      if (value.byteLength > bytes.byteLength - offset) {
        throw new Error('accelerator_download_size_mismatch');
      }
      bytes.set(value, offset);
      offset += value.byteLength;
      progress(offset, bytes.byteLength);
    }
    if (offset !== bytes.byteLength) throw new Error('accelerator_download_size_mismatch');
    return bytes;
  } catch (error) {
    await cancel(error);
    throw error;
  } finally {
    signal.removeEventListener('abort', onAbort);
    if (cancellation) await cancellation;
    reader.releaseLock();
  }
}
