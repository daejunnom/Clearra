// SRP rationale: this module has one change reason: expose one validated
// portfolio alternative as the ordinary solution-export key source.
import type {
  ClearraCoveragePortfolioRuntimePage
} from '../wasm/wasmCommandClient';
import type { SolutionExportKeySource } from './solutionExportAsync';
import {
  PRODUCT_MEMBER_PAGE_SIZE,
  coveragePortfolioPageReference,
  loadCoveragePortfolioExactPage,
  validateCoveragePortfolioRuntimePage,
  type ProductMemberPageLoader
} from './productResultPager';

const MAX_EXPORTABLE_PORTFOLIO_MEMBERS = 1_000_000;

export type CoveragePortfolioExportSourceOptions = {
  initialPage: ClearraCoveragePortfolioRuntimePage;
  loadMemberPage: ProductMemberPageLoader | null;
  isCurrent: () => boolean;
};

export type CoveragePortfolioExportSourceActivation = Readonly<
  | { keySource: SolutionExportKeySource | null; error: null }
  | { keySource: null; error: string }
>;

/**
 * Converts validation or supported-bound failures into the same explicit error
 * state used by the product pager. A rejected full-set export must never fall
 * back to copying only the currently rendered member page.
 */
export function tryCreateCoveragePortfolioExportKeySource(
  options: CoveragePortfolioExportSourceOptions
): CoveragePortfolioExportSourceActivation {
  try {
    return {
      keySource: createCoveragePortfolioExportKeySource(options),
      error: null
    };
  } catch (error) {
    return {
      keySource: null,
      error: error instanceof Error
        ? error.message
        : 'portfolio solution export is unavailable'
    };
  }
}

/**
 * Keeps rendering and export paging independent: the gallery may show any one
 * member page, while Copy all/Download always reads the complete selected
 * outer alternative in canonical candidate order.
 */
export function createCoveragePortfolioExportKeySource(
  options: CoveragePortfolioExportSourceOptions
): SolutionExportKeySource | null {
  const initialPage = options.initialPage;
  const initialValidationError = validateCoveragePortfolioRuntimePage(initialPage, {
    setIdentitySha256: initialPage.set_identity_sha256,
    candidateMapSha256: initialPage.candidate_map_sha256,
    alternativeIndex: initialPage.alternative_index,
    memberPageNumber: '1'
  });
  if (initialValidationError) throw new Error(initialValidationError);
  if (initialPage.member_page_number !== '1') {
    throw new Error('portfolio export source must start from member page 1');
  }

  const cardinality = BigInt(initialPage.optimal_cardinality);
  if (cardinality === 0n) return null;
  if (cardinality > BigInt(MAX_EXPORTABLE_PORTFOLIO_MEMBERS)) {
    throw new RangeError('portfolio solution export exceeds the supported page bound');
  }
  const keyCount = Number(cardinality);
  const totalMemberPages = Number(BigInt(initialPage.total_member_pages));
  const referencePage = coveragePortfolioPageReference(initialPage);
  let materializedKeys: readonly string[] | null = null;
  let materialization:
    | { signal: AbortSignal | undefined; promise: Promise<readonly string[]> }
    | null = null;

  return {
    keyCount,
    async readKeys(start, count, signal) {
      assertKeyRange(start, count, keyCount);
      throwIfAborted(signal);
      assertCurrent(options.isCurrent);
      const keys = materializedKeys ?? await materializeOnce(signal);
      throwIfAborted(signal);
      assertCurrent(options.isCurrent);
      return keys.slice(start, start + count);
    }
  };

  async function materializeOnce(signal?: AbortSignal): Promise<readonly string[]> {
    if (materializedKeys) return materializedKeys;
    if (!materialization || materialization.signal !== signal) {
      const promise = materializeAllPages(signal)
        .then((keys) => {
          materializedKeys = Object.freeze(keys);
          return materializedKeys;
        })
        .finally(() => {
          if (materialization?.promise === promise) materialization = null;
        });
      materialization = { signal, promise };
    }
    return materialization.promise;
  }

  async function materializeAllPages(signal?: AbortSignal): Promise<string[]> {
    const pages: ClearraCoveragePortfolioRuntimePage[] = [initialPage];
    for (let pageNumber = 2; pageNumber <= totalMemberPages; pageNumber += 1) {
      throwIfAborted(signal);
      assertCurrent(options.isCurrent);
      if (!options.loadMemberPage) {
        throw new Error('the selected portfolio has no member-page loader');
      }
      const expectedPageNumber = pageNumber.toString();
      const loadedPage = await loadCoveragePortfolioExactPage({
        loadMemberPage: options.loadMemberPage,
        alternativeIndex: initialPage.alternative_index,
        memberPageNumber: expectedPageNumber,
        signal,
        isCurrent: options.isCurrent,
        expectation: {
          setIdentitySha256: referencePage.set_identity_sha256,
          candidateMapSha256: referencePage.candidate_map_sha256,
          alternativeIndex: initialPage.alternative_index,
          memberPageNumber: expectedPageNumber,
          referencePage,
          requireSameAlternativeMetadata: true
        }
      });
      throwIfAborted(signal);
      assertCurrent(options.isCurrent);
      if (!loadedPage) {
        throw new Error('portfolio solution export page replay was cancelled');
      }
      pages.push(loadedPage);
    }

    const keys: string[] = [];
    const observedCandidateIds = new Set<string>();
    const observedKeys = new Set<string>();
    let previousCandidateId = 0n;
    for (const page of pages) {
      for (const member of page.members) {
        const candidateId = BigInt(member.candidate_id);
        if (
          candidateId <= previousCandidateId ||
          observedCandidateIds.has(member.candidate_id) ||
          observedKeys.has(member.normalized_solution_key)
        ) {
          throw new Error('portfolio export members are duplicated or out of canonical order');
        }
        previousCandidateId = candidateId;
        observedCandidateIds.add(member.candidate_id);
        observedKeys.add(member.normalized_solution_key);
        keys.push(member.normalized_solution_key);
      }
    }
    if (keys.length !== keyCount) {
      throw new Error('portfolio export ended before the declared member count');
    }
    return keys;
  }
}

function assertKeyRange(start: number, count: number, keyCount: number): void {
  if (
    !Number.isSafeInteger(start) ||
    !Number.isSafeInteger(count) ||
    start < 0 ||
    count < 0 ||
    start > keyCount - count
  ) {
    throw new RangeError('portfolio export key range is invalid');
  }
}

function assertCurrent(isCurrent: () => boolean): void {
  if (!isCurrent()) {
    throw new Error('portfolio export was replaced by another result or alternative');
  }
}

function throwIfAborted(signal: AbortSignal | undefined): void {
  if (!signal?.aborted) return;
  if (signal.reason instanceof Error) throw signal.reason;
  const error = new Error('portfolio solution export was aborted');
  error.name = 'AbortError';
  throw error;
}
