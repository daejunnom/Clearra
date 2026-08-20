import type { SolutionExportKeySource } from './solutionExportAsync';

export type SolutionPageResponse = {
  keys: string[];
  total: number;
};

export type SolutionPageLoader = (
  offset: number,
  limit: number,
  signal?: AbortSignal
) => Promise<SolutionPageResponse>;

export type BoundSolutionPageOptions = {
  keyCount: number;
  loadPage: SolutionPageLoader;
  resultIdentity: string;
  currentResultIdentity: () => string;
};

export type PagedSolutionExportKeySourceOptions = {
  keyCount: number;
  loadPage: SolutionPageLoader;
  commentForKey?: (key: string) => string | undefined;
  pageSize?: number;
};

const DEFAULT_EXPORT_PAGE_SIZE = 1_000;

/**
 * Binds the process-global WASM page store to one completed result. The store
 * is replaced by subsequent searches, so callers must never reuse a loader
 * after the result identity changes.
 */
export function bindSolutionPageLoader(
  options: BoundSolutionPageOptions
): SolutionPageLoader {
  const { keyCount, loadPage, resultIdentity, currentResultIdentity } = options;
  assertKeyCount(keyCount);
  if (!resultIdentity) {
    throw new RangeError('Solution page result identity is required.');
  }

  return async (offset, limit, signal) => {
    assertPageRange(offset, limit, keyCount);
    throwIfAborted(signal);
    assertCurrentResult(resultIdentity, currentResultIdentity);
    const response = await loadPage(offset, limit, signal);
    throwIfAborted(signal);
    assertCurrentResult(resultIdentity, currentResultIdentity);
    assertPageResponse(response, offset, limit, keyCount);
    return response;
  };
}

export function createPagedSolutionExportKeySource(
  options: PagedSolutionExportKeySourceOptions
): SolutionExportKeySource {
  const pageSize = options.pageSize ?? DEFAULT_EXPORT_PAGE_SIZE;
  if (!Number.isSafeInteger(pageSize) || pageSize < 1) {
    throw new RangeError('Solution export page size is out of range.');
  }
  assertKeyCount(options.keyCount);
  const loadPage = options.loadPage;
  const source: SolutionExportKeySource = {
    keyCount: options.keyCount,
    async readKeys(start, count, signal) {
      assertPageRange(start, count, options.keyCount);
      const keys: string[] = [];
      while (keys.length < count) {
        throwIfAborted(signal);
        const offset = start + keys.length;
        const response = await loadPage(
          offset,
          Math.min(pageSize, count - keys.length),
          signal
        );
        keys.push(...response.keys);
      }
      throwIfAborted(signal);
      return keys;
    }
  };
  if (options.commentForKey) source.commentForKey = options.commentForKey;
  return source;
}

export function solutionPageResultIdentity(
  solutionSetHash: string | null | undefined,
  keyCount: number | null,
  materializedKeys: readonly string[]
): string {
  const hash = solutionSetHash?.trim();
  if (hash && hash !== 'not-calculated') {
    return `hash:${hash}:count:${keyCount ?? 'unknown'}`;
  }
  return [
    'fallback',
    keyCount ?? 'unknown',
    materializedKeys.length,
    ...materializedKeys.map((key) => `${key.length}:${key}`)
  ].join(':');
}

function assertKeyCount(keyCount: number): void {
  if (!Number.isSafeInteger(keyCount) || keyCount < 1) {
    throw new RangeError('Solution page key count is out of range.');
  }
}

function assertPageRange(offset: number, limit: number, keyCount: number): void {
  assertKeyCount(keyCount);
  if (
    !Number.isSafeInteger(offset) ||
    !Number.isSafeInteger(limit) ||
    offset < 0 ||
    limit < 0 ||
    limit > keyCount ||
    offset > keyCount - limit
  ) {
    throw new RangeError('Solution page range is invalid.');
  }
}

function assertPageResponse(
  response: SolutionPageResponse,
  offset: number,
  limit: number,
  keyCount: number
): void {
  if (
    !response ||
    !Array.isArray(response.keys) ||
    !Number.isSafeInteger(response.total) ||
    response.total !== keyCount ||
    response.keys.length > limit ||
    response.keys.some((key) => typeof key !== 'string')
  ) {
    throw new Error('Solution page response does not match the completed result.');
  }
  if (limit > 0 && response.keys.length === 0) {
    throw new Error('Solution page store ended before the reported total.');
  }
  if (offset + response.keys.length > keyCount) {
    throw new Error('Solution page response exceeds the completed result.');
  }
}

function assertCurrentResult(
  expected: string,
  currentResultIdentity: () => string
): void {
  if (currentResultIdentity() !== expected) {
    throw new Error('Solution page result was replaced by a newer search.');
  }
}

function throwIfAborted(signal: AbortSignal | undefined): void {
  if (!signal?.aborted) return;
  if (signal.reason instanceof Error) throw signal.reason;
  const error = new Error('Solution page load was aborted.');
  error.name = 'AbortError';
  throw error;
}
