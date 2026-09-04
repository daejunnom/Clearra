// SRP rationale: this module has one change reason: fail-closed paging models for typed product results.
import type {
  ClearraCoveragePortfolioRuntimePage,
  ClearraProductPageWorkerPayload,
  ClearraProductResultPayload,
  ClearraSolutionSetArtifactPayload
} from '../wasm/wasmCommandClient';

export const PRODUCT_MEMBER_PAGE_SIZE = 100;

export type ProductNextPageLoader = (
  signal?: AbortSignal
) => Promise<ClearraProductPageWorkerPayload>;

export type ProductMemberPageLoader = (
  alternativeIndex: string,
  memberPageNumber: string,
  signal?: AbortSignal
) => Promise<ClearraProductPageWorkerPayload>;

export type ProductPageRelease = () => void | Promise<void>;

const PORTFOLIO_PAGE_CONTRACT = 'portfolio-alternative-page.v1';
const PORTFOLIO_MEMBER_PAGE_CONTRACT = 'portfolio-member-page.v1';
const MAX_RETAINED_PORTFOLIO_PAGES = 2;

export type CoveragePortfolioPageExpectation = {
  setIdentitySha256: string;
  candidateMapSha256?: string;
  alternativeIndex: string;
  memberPageNumber: string;
  referencePage?: CoveragePortfolioPageReference;
  requireSameAlternativeMetadata?: boolean;
};

export type CoveragePortfolioPageReference = Pick<
  ClearraCoveragePortfolioRuntimePage,
  | 'page_contract'
  | 'member_page_contract'
  | 'set_identity_sha256'
  | 'candidate_map_sha256'
  | 'optimal_cardinality'
  | 'known_alternative_count'
  | 'total_alternative_count'
  | 'enumeration_complete'
  | 'total_member_pages'
>;

export function coveragePortfolioPageReference(
  page: ClearraCoveragePortfolioRuntimePage
): CoveragePortfolioPageReference {
  return Object.freeze({
    page_contract: page.page_contract,
    member_page_contract: page.member_page_contract,
    set_identity_sha256: page.set_identity_sha256,
    candidate_map_sha256: page.candidate_map_sha256,
    optimal_cardinality: page.optimal_cardinality,
    known_alternative_count: page.known_alternative_count,
    total_alternative_count: page.total_alternative_count,
    enumeration_complete: page.enumeration_complete,
    total_member_pages: page.total_member_pages
  });
}

export function validateCoveragePortfolioRuntimePage(
  page: ClearraCoveragePortfolioRuntimePage,
  expectation: CoveragePortfolioPageExpectation
): string | null {
  if (
    page.page_contract !== PORTFOLIO_PAGE_CONTRACT ||
    page.member_page_contract !== PORTFOLIO_MEMBER_PAGE_CONTRACT ||
    !/^[0-9a-f]{64}$/u.test(page.set_identity_sha256) ||
    !/^[0-9a-f]{64}$/u.test(page.candidate_map_sha256) ||
    page.set_identity_sha256 !== expectation.setIdentitySha256 ||
    (expectation.candidateMapSha256 !== undefined &&
      page.candidate_map_sha256 !== expectation.candidateMapSha256) ||
    page.alternative_index !== expectation.alternativeIndex ||
    page.member_page_number !== expectation.memberPageNumber ||
    !positiveCanonicalDecimal(page.alternative_index) ||
    !canonicalNonNegativeDecimal(page.optimal_cardinality) ||
    !positiveCanonicalDecimal(page.known_alternative_count) ||
    !positiveCanonicalDecimal(page.member_page_number) ||
    !positiveCanonicalDecimal(page.total_member_pages) ||
    !(
      page.total_alternative_count === null ||
      positiveCanonicalDecimal(page.total_alternative_count)
    ) ||
    typeof page.enumeration_complete !== 'boolean' ||
    !Array.isArray(page.members)
  ) {
    return 'coverage portfolio page contract or identity is invalid';
  }

  const alternativeIndex = BigInt(page.alternative_index);
  const cardinality = BigInt(page.optimal_cardinality);
  const knownAlternativeCount = BigInt(page.known_alternative_count);
  const totalAlternativeCount =
    page.total_alternative_count === null ? null : BigInt(page.total_alternative_count);
  const memberPageNumber = BigInt(page.member_page_number);
  const totalMemberPages = BigInt(page.total_member_pages);
  const pageSize = BigInt(PRODUCT_MEMBER_PAGE_SIZE);
  // An exact cover of an empty required universe has one canonical empty
  // alternative. Core exposes that alternative as member page 1/1 with zero
  // members; it is a successful empty result, not a malformed page.
  const emptyPortfolio = cardinality === 0n;
  const expectedTotalMemberPages = emptyPortfolio
    ? 1n
    : (cardinality + pageSize - 1n) / pageSize;
  const memberStart = (memberPageNumber - 1n) * pageSize;
  const remainingMembers = cardinality - memberStart;
  const expectedMemberCount = emptyPortfolio
    ? 0n
    : remainingMembers < pageSize
      ? remainingMembers
      : pageSize;
  if (
    knownAlternativeCount < alternativeIndex ||
    totalMemberPages !== expectedTotalMemberPages ||
    memberPageNumber > totalMemberPages ||
    (!emptyPortfolio && remainingMembers <= 0n) ||
    (emptyPortfolio &&
      (alternativeIndex !== 1n ||
        knownAlternativeCount !== 1n ||
        totalAlternativeCount !== 1n ||
        !page.enumeration_complete ||
        memberPageNumber !== 1n)) ||
    BigInt(page.members.length) !== expectedMemberCount ||
    (page.enumeration_complete
      ? totalAlternativeCount !== knownAlternativeCount
      : totalAlternativeCount !== null)
  ) {
    return 'coverage portfolio page counts or enumeration metadata are invalid';
  }

  let previousCandidateId = 0n;
  for (const member of page.members) {
    if (
      !positiveCanonicalDecimal(member.candidate_id) ||
      typeof member.normalized_solution_key !== 'string' ||
      member.normalized_solution_key.length === 0
    ) {
      return 'coverage portfolio member identity is invalid';
    }
    const candidateId = BigInt(member.candidate_id);
    if (candidateId <= previousCandidateId) {
      return 'coverage portfolio member IDs are not strictly increasing';
    }
    previousCandidateId = candidateId;
  }

  const reference = expectation.referencePage;
  if (
    reference &&
    (page.page_contract !== reference.page_contract ||
      page.member_page_contract !== reference.member_page_contract ||
      page.set_identity_sha256 !== reference.set_identity_sha256 ||
      page.candidate_map_sha256 !== reference.candidate_map_sha256 ||
      page.optimal_cardinality !== reference.optimal_cardinality ||
      page.total_member_pages !== reference.total_member_pages ||
      (expectation.requireSameAlternativeMetadata === true &&
        (page.known_alternative_count !== reference.known_alternative_count ||
          page.total_alternative_count !== reference.total_alternative_count ||
          page.enumeration_complete !== reference.enumeration_complete)))
  ) {
    return 'coverage portfolio page does not match the active portfolio';
  }
  return null;
}

export type CoveragePortfolioPagerSnapshot = {
  identity: string;
  pages: readonly ClearraCoveragePortfolioRuntimePage[];
  outerPageIndex: number;
  currentPage: ClearraCoveragePortfolioRuntimePage | null;
  prefetchedPage: ClearraCoveragePortfolioRuntimePage | null;
  prefetchInFlight: boolean;
  enumerationSealed: boolean;
  highestMaterializedAlternativeIndex: string | null;
  navigating: boolean;
  error: string;
};

export type CoveragePortfolioPagerControllerOptions = {
  loadNextPage: ProductNextPageLoader | null;
  loadMemberPage: ProductMemberPageLoader | null;
  onChange?: (snapshot: CoveragePortfolioPagerSnapshot) => void;
};

export class CoveragePortfolioPagerController {
  private readonly loadNextPage: ProductNextPageLoader | null;
  private readonly loadMemberPage: ProductMemberPageLoader | null;
  private readonly onChange: ((snapshot: CoveragePortfolioPagerSnapshot) => void) | undefined;
  private identity = '';
  private pages: ClearraCoveragePortfolioRuntimePage[] = [];
  private outerPageIndex = 0;
  private prefetchedPage: ClearraCoveragePortfolioRuntimePage | null = null;
  private prefetchPromise: Promise<void> | null = null;
  private enumerationSealed = false;
  private highestMaterializedAlternativeIndex: string | null = null;
  private navigating = false;
  private error = '';
  private generation = 0;
  private abortController = new AbortController();

  constructor(options: CoveragePortfolioPagerControllerOptions) {
    this.loadNextPage = options.loadNextPage;
    this.loadMemberPage = options.loadMemberPage;
    this.onChange = options.onChange;
  }

  snapshot(): CoveragePortfolioPagerSnapshot {
    return Object.freeze({
      identity: this.identity,
      pages: Object.freeze([...this.pages]),
      outerPageIndex: this.outerPageIndex,
      currentPage: this.pages[this.outerPageIndex] ?? null,
      prefetchedPage: this.prefetchedPage,
      prefetchInFlight: this.prefetchPromise !== null,
      enumerationSealed: this.enumerationSealed,
      highestMaterializedAlternativeIndex: this.highestMaterializedAlternativeIndex,
      navigating: this.navigating,
      error: this.error
    });
  }

  reset(
    identity: string,
    initialPage: ClearraCoveragePortfolioRuntimePage | null,
    { autoPrefetch = true }: { autoPrefetch?: boolean } = {}
  ): void {
    this.abortController.abort();
    this.abortController = new AbortController();
    this.generation += 1;
    this.identity = identity;
    this.pages = [];
    this.outerPageIndex = 0;
    this.prefetchedPage = null;
    this.prefetchPromise = null;
    this.enumerationSealed = false;
    this.highestMaterializedAlternativeIndex = null;
    this.navigating = false;
    this.error = '';
    if (initialPage) {
      const validationError = validateCoveragePortfolioRuntimePage(initialPage, {
        setIdentitySha256: initialPage.set_identity_sha256,
        candidateMapSha256: initialPage.candidate_map_sha256,
        alternativeIndex: initialPage.alternative_index,
        memberPageNumber: initialPage.member_page_number
      });
      if (validationError) {
        this.error = validationError;
      } else {
        this.pages = [initialPage];
        this.highestMaterializedAlternativeIndex = initialPage.alternative_index;
        this.enumerationSealed = initialPage.enumeration_complete;
      }
    }
    this.emit();
    if (autoPrefetch) this.startPrefetch();
  }

  dispose(): void {
    this.abortController.abort();
    this.generation += 1;
    this.prefetchPromise = null;
    this.navigating = false;
  }

  startPrefetch(): void {
    if (
      this.prefetchPromise ||
      this.prefetchedPage ||
      this.enumerationSealed ||
      this.error ||
      !this.loadNextPage ||
      this.pages.length === 0 ||
      this.outerPageIndex !== this.pages.length - 1
    ) {
      return;
    }
    const sourcePage = this.pages[this.outerPageIndex];
    if (sourcePage.alternative_index !== this.highestMaterializedAlternativeIndex) return;

    const expectedAlternativeIndex = incrementCanonicalDecimal(sourcePage.alternative_index);
    const sourceReference = coveragePortfolioPageReference(sourcePage);
    const generation = this.generation;
    const signal = this.abortController.signal;
    const pending = this.prefetchNextExactPage(
      generation,
      signal,
      sourceReference,
      expectedAlternativeIndex
    )
      .then((result) => {
        if (!this.isCurrent(generation, signal)) return;
        if (result.sealed) this.enumerationSealed = true;
        if (result.page) {
          this.highestMaterializedAlternativeIndex = result.page.alternative_index;
          this.prefetchedPage = this.isImmediateSuccessorOfCurrent(result.page)
            ? result.page
            : null;
          if (result.page.enumeration_complete) this.enumerationSealed = true;
        }
        this.emit();
      })
      .catch((reason: unknown) => {
        if (!this.isCurrent(generation, signal)) return;
        this.error = pagerErrorMessage(reason);
        this.emit();
      })
      .finally(() => {
        if (this.generation === generation && this.prefetchPromise === pending) {
          this.prefetchPromise = null;
          this.emit();
        }
      });
    this.prefetchPromise = pending;
    this.emit();
  }

  async next(): Promise<ClearraCoveragePortfolioRuntimePage | null> {
    if (this.navigating || this.error) return null;
    const generation = this.generation;
    this.navigating = true;
    this.emit();
    try {
      return await this.nextUnlocked(generation);
    } catch (reason) {
      if (generation === this.generation) {
        this.error = pagerErrorMessage(reason);
        this.emit();
      }
      return null;
    } finally {
      if (generation === this.generation) {
        this.navigating = false;
        this.emit();
      }
    }
  }

  async previous(): Promise<ClearraCoveragePortfolioRuntimePage | null> {
    if (this.navigating || this.error) return null;
    const generation = this.generation;
    this.navigating = true;
    this.emit();
    try {
      if (this.outerPageIndex > 0) {
        this.outerPageIndex -= 1;
        this.pruneNonAdjacentPrefetchedPage();
        this.emit();
        return this.pages[this.outerPageIndex] ?? null;
      }
      const currentAlternativeIndex = this.pages[this.outerPageIndex]?.alternative_index ?? null;
      if (!currentAlternativeIndex || currentAlternativeIndex === '1') return null;
      return await this.reloadOuterPage(
        decrementCanonicalDecimal(currentAlternativeIndex),
        'previous',
        generation
      );
    } catch (reason) {
      if (generation === this.generation) {
        this.error = pagerErrorMessage(reason);
        this.emit();
      }
      return null;
    } finally {
      if (generation === this.generation) {
        this.navigating = false;
        this.emit();
      }
    }
  }

  private async nextUnlocked(
    generation: number
  ): Promise<ClearraCoveragePortfolioRuntimePage | null> {
    if (this.outerPageIndex + 1 < this.pages.length) {
      this.outerPageIndex += 1;
      this.pruneNonAdjacentPrefetchedPage();
      this.emit();
      this.startPrefetch();
      return this.pages[this.outerPageIndex] ?? null;
    }
    const currentAlternativeIndex = this.pages[this.outerPageIndex]?.alternative_index ?? null;
    if (!currentAlternativeIndex) {
      throw new Error('product alternative index exceeds the supported page range');
    }
    const nextAlternativeIndex = incrementCanonicalDecimal(currentAlternativeIndex);
    const prefetchedAlternativeIndex = this.prefetchedPage?.alternative_index ?? null;
    if (
      this.highestMaterializedAlternativeIndex !== null &&
      compareCanonicalDecimals(nextAlternativeIndex, this.highestMaterializedAlternativeIndex) <=
        0 &&
      prefetchedAlternativeIndex !== nextAlternativeIndex
    ) {
      return await this.reloadOuterPage(nextAlternativeIndex, 'next', generation);
    }

    this.startPrefetch();
    const pending = this.prefetchPromise;
    if (pending) await pending;
    if (generation !== this.generation) return null;
    if (!this.prefetchedPage || this.prefetchedPage.alternative_index !== nextAlternativeIndex) {
      if (
        this.highestMaterializedAlternativeIndex !== null &&
        compareCanonicalDecimals(nextAlternativeIndex, this.highestMaterializedAlternativeIndex) <=
          0
      ) {
        return await this.reloadOuterPage(nextAlternativeIndex, 'next', generation);
      }
      return null;
    }
    const nextPage = this.prefetchedPage;
    this.prefetchedPage = null;
    this.pages =
      this.pages.length >= MAX_RETAINED_PORTFOLIO_PAGES
        ? [...this.pages.slice(1), nextPage]
        : [...this.pages, nextPage];
    this.outerPageIndex = this.pages.length - 1;
    this.emit();
    this.startPrefetch();
    return nextPage;
  }

  private async reloadOuterPage(
    alternativeIndex: string,
    direction: 'previous' | 'next',
    generation: number
  ): Promise<ClearraCoveragePortfolioRuntimePage | null> {
    if (!this.loadMemberPage || !positiveCanonicalDecimal(alternativeIndex)) {
      return null;
    }
    const referencePage = this.pages[0]
      ? coveragePortfolioPageReference(this.pages[0])
      : null;
    if (!referencePage) return null;
    const signal = this.abortController.signal;
    const response = await this.loadMemberPage(alternativeIndex, '1', signal);
    if (!this.isCurrent(generation, signal)) return null;
    const page = requireCoveragePortfolioPageResponse(response, {
      setIdentitySha256: referencePage.set_identity_sha256,
      candidateMapSha256: referencePage.candidate_map_sha256,
      alternativeIndex,
      memberPageNumber: '1',
      referencePage
    });
    this.pages =
      direction === 'previous'
        ? [page, ...this.pages].slice(0, MAX_RETAINED_PORTFOLIO_PAGES)
        : [...this.pages, page].slice(-MAX_RETAINED_PORTFOLIO_PAGES);
    this.outerPageIndex = direction === 'previous' ? 0 : this.pages.length - 1;
    this.pruneNonAdjacentPrefetchedPage();
    if (page.enumeration_complete) this.enumerationSealed = true;
    this.emit();
    this.startPrefetch();
    return page;
  }

  private async prefetchNextExactPage(
    generation: number,
    signal: AbortSignal,
    referencePage: CoveragePortfolioPageReference,
    expectedAlternativeIndex: string
  ): Promise<{
    page: ClearraCoveragePortfolioRuntimePage | null;
    sealed: boolean;
  }> {
    if (!this.loadNextPage) return { page: null, sealed: false };
    while (this.isCurrent(generation, signal)) {
      const response = await this.loadNextPage(signal);
      if (!this.isCurrent(generation, signal)) return { page: null, sealed: false };
      if (
        response.schema_version !== 1 ||
        !['clearra-wasm', 'clearra-desktop'].includes(response.runtime) ||
        response.product_page_kind !== 'coverage-portfolio'
      ) {
        throw new Error('product page kind does not match the active coverage result');
      }
      if (response.state === 'work-budget-exhausted') continue;
      if (response.state === 'sealed' || response.state === 'cancelled') {
        return { page: null, sealed: true };
      }
      return {
        page: requireCoveragePortfolioPageResponse(response, {
          setIdentitySha256: referencePage.set_identity_sha256,
          candidateMapSha256: referencePage.candidate_map_sha256,
          alternativeIndex: expectedAlternativeIndex,
          memberPageNumber: '1',
          referencePage
        }),
        sealed: false
      };
    }
    return { page: null, sealed: false };
  }

  private isCurrent(generation: number, signal: AbortSignal): boolean {
    return generation === this.generation && !signal.aborted;
  }

  private isImmediateSuccessorOfCurrent(page: ClearraCoveragePortfolioRuntimePage): boolean {
    const currentPage = this.pages[this.outerPageIndex];
    return (
      currentPage !== undefined &&
      page.alternative_index === incrementCanonicalDecimal(currentPage.alternative_index)
    );
  }

  private pruneNonAdjacentPrefetchedPage(): void {
    if (this.prefetchedPage && !this.isImmediateSuccessorOfCurrent(this.prefetchedPage)) {
      this.prefetchedPage = null;
    }
  }

  private emit(): void {
    this.onChange?.(this.snapshot());
  }
}

export function requireCoveragePortfolioPageResponse(
  response: ClearraProductPageWorkerPayload,
  expectation: CoveragePortfolioPageExpectation
): ClearraCoveragePortfolioRuntimePage {
  if (
    response.schema_version !== 1 ||
    !['clearra-wasm', 'clearra-desktop'].includes(response.runtime) ||
    response.product_page_kind !== 'coverage-portfolio' ||
    response.state !== 'page'
  ) {
    throw new Error('product page response is not a coverage portfolio page');
  }
  const validationError = validateCoveragePortfolioRuntimePage(response.page, expectation);
  if (validationError) throw new Error(validationError);
  return response.page;
}

function positiveCanonicalDecimal(value: string): boolean {
  return /^(?:[1-9][0-9]*)$/u.test(value);
}

function canonicalNonNegativeDecimal(value: string): boolean {
  return /^(?:0|[1-9][0-9]*)$/u.test(value);
}

export function incrementCanonicalDecimal(value: string): string {
  return (BigInt(value) + 1n).toString();
}

export function decrementCanonicalDecimal(value: string): string {
  return (BigInt(value) - 1n).toString();
}

function pagerErrorMessage(value: unknown): string {
  return value instanceof Error ? value.message : String(value);
}

export function productResultIdentity(payload: ClearraProductResultPayload | null | undefined) {
  if (!payload) return '';
  if (payload.content.payload_kind === 'build-v2') {
    const build = payload.content.payload;
    return [
      build.capability_id,
      build.result_contract,
      build.input_identity_sha256,
      build.evaluation_identity_sha256 ?? '',
      build.page_source_identity_sha256 ?? ''
    ].join(':');
  }
  if (payload.content.payload_kind === 'build-coverage-portfolio-v2') {
    const portfolio = payload.content.payload;
    return [
      payload.contract,
      payload.result_kind,
      portfolio.normalized_solution_set_hash,
      portfolio.page_source_identity_sha256 ?? ''
    ].join(':');
  }
  if (payload.content.payload_kind === 'build-setup-family-v1') {
    const family = payload.content.payload;
    return [
      payload.contract,
      payload.result_kind,
      family.input_identity_sha256,
      family.evaluation_identity_sha256
    ].join(':');
  }
  if (payload.content.payload_kind === 'coverage-portfolio') {
    const page = payload.content.payload;
    return [
      payload.contract,
      payload.result_kind,
      page.set_identity_sha256,
      page.candidate_map_sha256
    ].join(':');
  }
  if (payload.content.payload_kind === 'pc-score-field-summary') {
    const summary = payload.content.payload;
    return [
      payload.contract,
      payload.result_kind,
      summary.pattern_universe_id,
      summary.pattern_weight_model_id,
      summary.materialized_pattern_count,
      summary.overall_score
    ].join(':');
  }
  if (payload.content.payload_kind === 'score-pattern-winner-family') {
    const family = payload.content.payload;
    return [
      payload.contract,
      payload.result_kind,
      family.winner_contract,
      family.winner_count
    ].join(':');
  }
  if (payload.content.payload_kind === 'pc-path-family') {
    const family = payload.content.payload;
    return [
      payload.contract,
      payload.result_kind,
      family.problem_id,
      family.witness_count
    ].join(':');
  }
  if (payload.content.payload_kind === 'setup-ranked-family') {
    const family = payload.content.payload;
    return [
      payload.contract,
      payload.result_kind,
      family.query_identity_sha256,
      family.supply_identity_sha256,
      family.universe_identity_sha256,
      family.product_build
    ].join(':');
  }
  if (payload.content.payload_kind === 'setup-score-ranking') {
    const ranking = payload.content.payload;
    return [
      payload.contract,
      payload.result_kind,
      ranking.input_identity_sha256,
      ranking.evaluation_identity_sha256
    ].join(':');
  }
  if (payload.content.payload_kind === 'spin-structure-family') {
    const family = payload.content.payload;
    return [
      payload.contract,
      payload.result_kind,
      family.query_identity_sha256,
      family.supply_identity_sha256,
      family.universe_identity_sha256,
      family.product_build
    ].join(':');
  }
  if (payload.content.payload_kind === 'parity-report-page') {
    const page = payload.content.payload;
    return [payload.contract, payload.result_kind, page.document_format, page.total_pages].join(':');
  }
  if (payload.content.payload_kind === 'field-document') {
    return [payload.contract, payload.result_kind, payload.content.payload.canonical_sha256].join(':');
  }
  if (payload.content.payload_kind === 'field-document-set') {
    return [
      payload.contract,
      payload.result_kind,
      ...payload.content.payload.documents.map((document) => document.canonical_sha256)
    ].join(':');
  }
  return [payload.contract, payload.result_kind, payload.content.payload.sha256].join(':');
}

export function isCanonicalDecimal(value: string): boolean {
  return /^(0|[1-9][0-9]*)$/.test(value);
}

export function isCanonicalProbability(value: string): boolean {
  if (!/^(?:0|1|0\.[0-9]+|[1-9][0-9]*(?:\.[0-9]+)?e-?[0-9]+)$/u.test(value)) {
    return false;
  }
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed >= 0 && parsed <= 1;
}

export function validateSolutionSetArtifactPayload(
  artifact: ClearraSolutionSetArtifactPayload
): string | null {
  if (
    artifact.contract !== 'solution-set-artifact.v2' ||
    artifact.completeness !== 'complete' ||
    !validArtifactIdentity(artifact.source_result_kind) ||
    !validArtifactIdentity(artifact.source_solution_set_contract) ||
    !validArtifactIdentity(artifact.selection_id) ||
    !validArtifactIdentity(artifact.normalized_key_algorithm) ||
    !validArtifactIdentity(artifact.normalized_set_hash_algorithm) ||
    !validArtifactIdentity(artifact.normalized_set_hash) ||
    (artifact.page_source_identity_sha256 !== null &&
      !/^[0-9a-f]{64}$/u.test(artifact.page_source_identity_sha256)) ||
    !Number.isSafeInteger(artifact.solution_count) ||
    artifact.solution_count <= 0 ||
    artifact.formats.length !== 2 ||
    artifact.formats[0]?.format !== 'ctk3' ||
    artifact.formats[1]?.format !== 'fumen' ||
    !artifact.formats.some((format) => format.state === 'available') ||
    artifact.formats.some((format) => !validSolutionSetArtifactFormat(format))
  ) {
    return 'invalid solution-set artifact payload';
  }
  return null;
}

function validSolutionSetArtifactFormat(
  format: ClearraSolutionSetArtifactPayload['formats'][number]
): boolean {
  if (format.state === 'unavailable') {
    return (
      format.unavailable_reason !== null &&
      [
        'empty-solution-set',
        'unsupported-solution-key',
        'page-limit-exceeded',
        'encoding-failed',
        'transport-byte-limit-exceeded'
      ].includes(format.unavailable_reason) &&
      format.media_type === null &&
      format.filename === null &&
      format.byte_length === null &&
      format.sha256 === null &&
      format.page_count === null &&
      format.document === null
    );
  }
  if (
    format.unavailable_reason !== null ||
    format.media_type === null ||
    format.filename === null ||
    format.byte_length === null ||
    format.sha256 === null ||
    format.page_count === null ||
    format.document === null ||
    !Number.isSafeInteger(format.byte_length) ||
    format.byte_length <= 0 ||
    format.byte_length > 8 * 1024 * 1024 ||
    new TextEncoder().encode(format.document).byteLength !== format.byte_length ||
    !/^[0-9a-f]{64}$/u.test(format.sha256) ||
    !Number.isSafeInteger(format.page_count) ||
    format.page_count <= 0
  ) {
    return false;
  }
  return format.format === 'ctk3'
    ? format.media_type === 'application/vnd.clearra.ctk3' &&
        format.filename.endsWith('.ctk3') &&
        (format.document.startsWith('ctk3_') || format.document.startsWith('ctk3b_'))
    : format.format === 'fumen' &&
        format.media_type === 'text/plain;charset=utf-8' &&
        format.filename.endsWith('.fumen') &&
        format.document.startsWith('v115@');
}

function validArtifactIdentity(value: string): boolean {
  return value.length > 0 && value.length <= 512 && !/[\u0000-\u0020\u007f]/u.test(value);
}

export function validateProductResultPayload(
  payload: ClearraProductResultPayload
): string | null {
  if (payload.content.payload_kind === 'build-v2') {
    return validateBuildV2(payload.contract, payload.result_kind, payload.content.payload);
  }
  if (payload.content.payload_kind === 'build-coverage-portfolio-v2') {
    const portfolio = payload.content.payload;
    const complete = portfolio.completeness;
    return payload.contract === 'build.cover' &&
      payload.result_kind === 'build-coverage-portfolio.v2' &&
      portfolio.contract === 'build-coverage-portfolio.v2' &&
      ['min-cover', 'max-probability-minimum'].includes(portfolio.objective) &&
      [
        portfolio.source_candidate_count,
        portfolio.selected_candidate_count,
        portfolio.pattern_count,
        portfolio.required_pattern_count
      ].every(isCanonicalDecimal) &&
      isCanonicalProbability(portfolio.union_probability) &&
      Boolean(portfolio.probability_basis) &&
      Boolean(portfolio.canonical_first_candidate_id) &&
      /^[0-9a-f]{64}$/u.test(portfolio.normalized_solution_set_hash) &&
      portfolio.page_source_available === true &&
      portfolio.page_source_identity_sha256 !== null &&
      /^[0-9a-f]{64}$/u.test(portfolio.page_source_identity_sha256) &&
      complete.source_universe_complete === true &&
      complete.coverage_rows_complete === true &&
      complete.probability_weights_complete === true &&
      complete.exact_minimum_proven === true &&
      complete.query_bound === true
      ? null
      : 'invalid Build coverage portfolio payload';
  }
  if (payload.content.payload_kind === 'build-setup-family-v1') {
    const family = payload.content.payload;
    const complete = family.completeness;
    return payload.contract === 'build.setup' &&
      payload.result_kind === 'build-target-family.v2' &&
      family.contract === 'build-target-family.v2' &&
      /^[0-9a-f]{64}$/u.test(family.input_identity_sha256) &&
      /^[0-9a-f]{64}$/u.test(family.evaluation_identity_sha256) &&
      ['all', 'unique'].includes(family.objective) &&
      [
        family.source_candidate_count,
        family.reachable_candidate_count,
        family.pattern_count,
        family.covered_pattern_count
      ].every(isCanonicalDecimal) &&
      isCanonicalProbability(family.union_probability) &&
      family.source_candidate_count === family.candidates.length.toString() &&
      canonicalBuildCandidateRows(family.candidates) &&
      complete.input_identity_bound === true &&
      complete.producer_filter_bound === true &&
      complete.buildability_replay_complete === true &&
      complete.coverage_rows_complete === true &&
      complete.probability_weights_complete === true
      ? null
      : 'invalid Build setup family payload';
  }
  if (payload.content.payload_kind === 'setup-ranked-family') {
    return validateSetupRankedFamily(
      payload.contract,
      payload.result_kind,
      payload.content.payload
    );
  }
  if (payload.content.payload_kind === 'setup-score-ranking') {
    return validateSetupScoreRanking(
      payload.contract,
      payload.result_kind,
      payload.content.payload
    );
  }
  if (payload.content.payload_kind === 'spin-structure-family') {
    return validateSpinStructureFamily(
      payload.contract,
      payload.result_kind,
      payload.content.payload
    );
  }
  if (payload.content.payload_kind === 'coverage-portfolio') {
    const page = payload.content.payload;
    const expectedPair =
      (payload.contract === 'pc.minimals' && payload.result_kind === 'pc-minimum-cover.v2') ||
      (payload.contract === 'pc.score-minimals' &&
        payload.result_kind === 'pc-score-portfolio.v2') ||
      (payload.contract === 'spin-structure.cover' &&
        payload.result_kind === 'spin-structure-coverage.v1');
    const pageValidationError = validateCoveragePortfolioRuntimePage(page, {
      setIdentitySha256: page.set_identity_sha256,
      candidateMapSha256: page.candidate_map_sha256,
      alternativeIndex: '1',
      memberPageNumber: '1'
    });
    if (
      !expectedPair ||
      page.set_contract !== 'portfolio-alternative-set.v1' ||
      pageValidationError
    ) {
      return 'invalid coverage portfolio payload';
    }
    return null;
  }
  if (payload.content.payload_kind === 'pc-score-field-summary') {
    return validatePcScoreFieldSummary(
      payload.contract,
      payload.result_kind,
      payload.content.payload
    );
  }
  if (payload.content.payload_kind === 'score-pattern-winner-family') {
    const family = payload.content.payload;
    const expectedPair =
      payload.contract === 'pc.score-finder' &&
      payload.result_kind === 'pc-fixed-score-witness.v2';
    if (
      !expectedPair ||
      family.ordering !== 'pattern-id-ascending-then-candidate-id-ascending' ||
      family.equality !== 'score-only-attack-informational' ||
      family.page_size !== PRODUCT_MEMBER_PAGE_SIZE.toString() ||
      !isCanonicalDecimal(family.winner_count) ||
      family.winner_count !== family.winners.length.toString() ||
      family.winners.some(
        (winner) =>
          !isCanonicalDecimal(winner.pattern_id) ||
          !isCanonicalDecimal(winner.candidate_id) ||
          !isCanonicalDecimal(winner.score) ||
          !isCanonicalDecimal(winner.informational_attack) ||
          !winner.normalized_solution_key
      )
    ) {
      return 'invalid score winner family payload';
    }
    return null;
  }
  if (payload.content.payload_kind === 'pc-path-family') {
    return validatePcPathFamily(
      payload.contract,
      payload.result_kind,
      payload.content.payload
    );
  }
  if (payload.content.payload_kind === 'parity-report-page') {
    const page = payload.content.payload;
    return Number.isInteger(page.page_number) &&
      page.page_number >= 1 &&
      Number.isInteger(page.total_pages) &&
      page.page_number <= page.total_pages &&
      page.feasibility_claim === false &&
      page.pruning_authority === 'none' &&
      page.four_color_counts.length === 4
      ? null
      : 'invalid parity report payload';
  }
  if (payload.content.payload_kind === 'field-document') {
    return validateFieldDocument(payload.content.payload);
  }
  if (payload.content.payload_kind === 'field-document-set') {
    const set = payload.content.payload;
    return set.documents.length > 0 &&
      set.documents.length <= 4096 &&
      set.documents.every((document) => validateFieldDocument(document) === null)
      ? null
      : 'invalid field document set payload';
  }
  const artifact = payload.content.payload;
  return artifact.render_exact === true &&
    /^[0-9a-f]{64}$/.test(artifact.sha256) &&
    Number.isSafeInteger(artifact.byte_length) &&
    artifact.byte_length >= 0 &&
    artifact.byte_length <= artifact.product_max_bytes &&
    artifact.byte_length <= artifact.transport_max_bytes &&
    ((artifact.artifact_format === 'png' && artifact.media_type === 'image/png') ||
      (artifact.artifact_format === 'gif' && artifact.media_type === 'image/gif'))
    ? null
    : 'invalid render artifact payload';
}

function validatePcScoreFieldSummary(
  outerContract: string,
  outerResultKind: string,
  summary: Extract<
    ClearraProductResultPayload,
    { content: { payload_kind: 'pc-score-field-summary' } }
  >['content']['payload']
): string | null {
  const materializedCount = isCanonicalDecimal(summary.materialized_pattern_count)
    ? BigInt(summary.materialized_pattern_count)
    : null;
  const solutionFieldCount = isCanonicalDecimal(summary.solution_field_count)
    ? BigInt(summary.solution_field_count)
    : null;
  const scoredCount = isCanonicalDecimal(summary.scored_pattern_count)
    ? BigInt(summary.scored_pattern_count)
    : null;
  const failedCount = isCanonicalDecimal(summary.failed_pc_pattern_count)
    ? BigInt(summary.failed_pc_pattern_count)
    : null;
  const observedKeys = new Set<string>();
  const fieldsValid = summary.fields.every((field) => {
    if (
      !validPcScoreFieldKey(field.normalized_field_key) ||
      observedKeys.has(field.normalized_field_key) ||
      !finiteInRange(field.average_score, 0, Number.MAX_VALUE) ||
      !isCanonicalDecimal(field.covered_pattern_count) ||
      !isCanonicalDecimal(field.pattern_count) ||
      field.score_complete !== true
    ) {
      return false;
    }
    observedKeys.add(field.normalized_field_key);
    const coveredPatternCount = BigInt(field.covered_pattern_count);
    const patternCount = BigInt(field.pattern_count);
    return (
      materializedCount !== null &&
      patternCount === materializedCount &&
      coveredPatternCount <= patternCount
    );
  });
  const conditionalAverage = summary.score_covered_pattern_conditional_average_score;
  const valid =
    outerContract === 'pc.score' &&
    outerResultKind === 'pc-score-summary.v2' &&
    summary.field_contract === 'pc-score-solution-field-average.v1' &&
    summary.ordering === 'normalized-solution-field-order' &&
    summary.solution_field_average_basis ===
      'whole-materialized-pattern-universe-failed-pc-zero' &&
    summary.score_evaluation_basis === 'all-traces' &&
    summary.score_evaluation_scope === 'full' &&
    summary.overall_score_basis === 'all-materialized-patterns-failed-pc-zero' &&
    summary.piece_source_id.length > 0 &&
    summary.pattern_universe_id.length > 0 &&
    summary.pattern_weight_model_id.length > 0 &&
    materializedCount !== null &&
    solutionFieldCount !== null &&
    scoredCount !== null &&
    failedCount !== null &&
    solutionFieldCount === BigInt(summary.fields.length) &&
    scoredCount + failedCount === materializedCount &&
    isCanonicalProbability(summary.covered_probability) &&
    finiteInRange(summary.overall_score, 0, Number.MAX_VALUE) &&
    (conditionalAverage === null ||
      finiteInRange(conditionalAverage, 0, Number.MAX_VALUE)) &&
    summary.complete === true &&
    fieldsValid;
  return valid ? null : 'invalid PC score field summary payload';
}

function validPcScoreFieldKey(value: string): boolean {
  return /^ctk1\|initial=[0-9a-f]{16}\|placements=[IOTSZJL]:[0-9a-f]{16}(?:,[IOTSZJL]:[0-9a-f]{16})*$/u.test(
    value
  );
}

function validatePcPathFamily(
  outerContract: string,
  outerResultKind: string,
  family: Extract<
    ClearraProductResultPayload,
    { content: { payload_kind: 'pc-path-family' } }
  >['content']['payload']
): string | null {
  let previousOrderKey: readonly [string, string, string] | null = null;
  const witnessesValid = family.witnesses.every((witness) => {
    const orderKey = [
      witness.candidate_id,
      witness.pattern_id,
      witness.normalized_trace_key
    ] as const;
    const ordered = previousOrderKey === null || comparePcPathOrder(previousOrderKey, orderKey) < 0;
    previousOrderKey = orderKey;
    return (
      ordered &&
      [
        witness.candidate_id,
        witness.producer_candidate_id,
        witness.pattern_id,
        witness.consumed_piece_count
      ].every(isCanonicalDecimal) &&
      witness.trace_identity.length > 0 &&
      witness.normalized_trace_key.length > 0 &&
      (witness.terminal_hold_piece === null ||
        /^[IOTSZJL]$/u.test(witness.terminal_hold_piece)) &&
      witness.steps.length > 0 &&
      witness.steps.every(
        (step, stepIndex) =>
          step.step_index === stepIndex.toString() &&
          step.operation_id.length > 0 &&
          /^[IOTSZJL]$/u.test(step.active_piece) &&
          [step.input_cursor, step.output_cursor, step.cleared_lines].every(
            isCanonicalDecimal
          ) &&
          (step.input_hold_piece === null || /^[IOTSZJL]$/u.test(step.input_hold_piece)) &&
          (step.output_hold_piece === null || /^[IOTSZJL]$/u.test(step.output_hold_piece)) &&
          step.hold_decision.length > 0 &&
          step.rotation.length > 0 &&
          isCanonicalSignedDecimal(step.x) &&
          isCanonicalSignedDecimal(step.y) &&
          [
            step.placement_mask,
            step.board_before_mask,
            step.board_after_placement_mask,
            step.board_after_line_clear_mask,
            step.cleared_row_mask
          ].every((mask) => /^0x[0-9a-f]{16}$/u.test(mask)) &&
          step.line_clear_identity.length > 0
      )
    );
  });
  return outerContract === 'pc.path' &&
    outerResultKind === 'pc-path-family.v2' &&
    family.witness_contract === 'pc-path-witness.v2' &&
    family.ordering ===
      'candidate-id-ascending-then-pattern-id-ascending-then-trace-key-ascending' &&
    family.problem_id.length > 0 &&
    isCanonicalDecimal(family.materialized_pattern_count) &&
    isCanonicalDecimal(family.witness_count) &&
    family.witness_count === family.witnesses.length.toString() &&
    family.complete === true &&
    witnessesValid
    ? null
    : 'invalid pc.path replay family payload';
}

function isCanonicalSignedDecimal(value: string): boolean {
  return /^(?:0|[1-9][0-9]*|-[1-9][0-9]*)$/u.test(value);
}

function comparePcPathOrder(
  left: readonly [string, string, string],
  right: readonly [string, string, string]
): number {
  const candidateOrder = compareCanonicalDecimals(left[0], right[0]);
  if (candidateOrder !== 0) return candidateOrder;
  const patternOrder = compareCanonicalDecimals(left[1], right[1]);
  if (patternOrder !== 0) return patternOrder;
  return left[2] < right[2] ? -1 : left[2] > right[2] ? 1 : 0;
}

function validateSetupRankedFamily(
  outerContract: string,
  outerResultKind: string,
  family: Extract<
    ClearraProductResultPayload,
    { content: { payload_kind: 'setup-ranked-family' } }
  >['content']['payload']
): string | null {
  const expected = {
    'setup.joint': [
      'setup-joint-ranking.v2',
      'joint-probability-descending',
      'longer'
    ],
    'setup.build': [
      'setup-build-ranking.v2',
      'build-probability-descending',
      'longer'
    ],
    'setup.pc': [
      'setup-pc-ranking.v2',
      'conditional-pc-probability-descending',
      'shorter'
    ]
  } as const;
  const pair = expected[outerContract as keyof typeof expected];
  const candidateIds = new Set<string>();
  const valid =
    pair !== undefined &&
    outerResultKind === pair[0] &&
    family.schema_id === pair[0] &&
    family.ordering === pair[1] &&
    family.resolved_length_preference === pair[2] &&
    [
      family.query_identity_sha256,
      family.supply_identity_sha256,
      family.universe_identity_sha256
    ].every((identity) => /^[0-9a-f]{64}$/u.test(identity)) &&
    family.rule_profile.length > 0 &&
    family.product_build.length > 0 &&
    isCanonicalDecimal(family.candidate_count) &&
    family.candidate_count === family.candidates.length.toString() &&
    family.candidates.every((candidate) => {
      if (
        !candidate.candidate_id.startsWith('setup-candidate.v1:') ||
        candidate.condition_id.length === 0 ||
        candidate.setup_id.length === 0 ||
        candidateIds.has(candidate.candidate_id)
      ) {
        return false;
      }
      candidateIds.add(candidate.candidate_id);
      return true;
    });
  return valid ? null : 'invalid setup ranked family payload';
}

function validateSetupScoreRanking(
  outerContract: string,
  outerResultKind: string,
  ranking: Extract<
    ClearraProductResultPayload,
    { content: { payload_kind: 'setup-score-ranking' } }
  >['content']['payload']
): string | null {
  const candidateCount = isCanonicalDecimal(ranking.candidate_count)
    ? BigInt(ranking.candidate_count)
    : null;
  const sourcePageCount = isCanonicalDecimal(ranking.source_page_count)
    ? BigInt(ranking.source_page_count)
    : null;
  const setupPatternCount = isCanonicalDecimal(ranking.setup_pattern_count)
    ? BigInt(ranking.setup_pattern_count)
    : null;
  const averageScore = Number(ranking.average_priority_score);
  const candidateIds = new Set<string>();
  let previousScore: number | null = null;
  let previousCandidateId: string | null = null;
  const candidatesValid = ranking.candidates.every((candidate, index) => {
    const score = Number(candidate.unconditional_expected_score);
    const coveredCount = isCanonicalDecimal(candidate.setup_covered_pattern_count)
      ? BigInt(candidate.setup_covered_pattern_count)
      : null;
    const candidateValid =
      candidate.rank === (index + 1).toString() &&
      candidate.candidate_id.length > 0 &&
      candidate.candidate_id.trim() === candidate.candidate_id &&
      !candidateIds.has(candidate.candidate_id) &&
      /^0x[0-9a-f]+$/iu.test(candidate.completed_board_mask) &&
      coveredCount !== null &&
      setupPatternCount !== null &&
      coveredCount <= setupPatternCount &&
      finiteInRange(candidate.setup_covered_probability, 0, 1) &&
      finiteInRange(candidate.continuation_probability, 0, 1) &&
      Number.isFinite(score) &&
      score >= 0 &&
      (previousScore === null ||
        previousScore > score ||
        (previousScore === score &&
          previousCandidateId !== null &&
          previousCandidateId < candidate.candidate_id));
    if (!candidateValid) return false;
    candidateIds.add(candidate.candidate_id);
    previousScore = score;
    previousCandidateId = candidate.candidate_id;
    return true;
  });
  const valid =
    outerContract === 'setup.score' &&
    outerResultKind === 'setup-score-ranking.v1' &&
    ranking.schema_id === 'setup-score-ranking.v1' &&
    /^[0-9a-f]{64}$/u.test(ranking.input_identity_sha256) &&
    /^[0-9a-f]{64}$/u.test(ranking.evaluation_identity_sha256) &&
    ['ctk3', 'fumen'].includes(ranking.document_format) &&
    ranking.rule_profile.length > 0 &&
    ['tetrio', 'guideline', 'jstris-ultra'].includes(ranking.score_profile) &&
    isCanonicalDecimal(ranking.initial_b2b) &&
    ranking.ordering ===
      'unconditional-expected-score-descending-then-canonical-candidate-id' &&
    ranking.complete === true &&
    candidateCount !== null &&
    candidateCount > 0n &&
    candidateCount === BigInt(ranking.candidates.length) &&
    sourcePageCount !== null &&
    sourcePageCount >= candidateCount &&
    setupPatternCount !== null &&
    setupPatternCount > 0n &&
    Number.isFinite(averageScore) &&
    averageScore >= 0 &&
    candidatesValid;
  return valid ? null : 'invalid setup score ranking payload';
}

function validateSpinStructureFamily(
  outerContract: string,
  outerResultKind: string,
  family: Extract<
    ClearraProductResultPayload,
    { content: { payload_kind: 'spin-structure-family' } }
  >['content']['payload']
): string | null {
  const regularCount = isCanonicalDecimal(family.regular_count)
    ? BigInt(family.regular_count)
    : null;
  const miniCount = isCanonicalDecimal(family.mini_count) ? BigInt(family.mini_count) : null;
  const candidateCount = isCanonicalDecimal(family.candidate_count)
    ? BigInt(family.candidate_count)
    : null;
  const declaredMinimum =
    family.minimum_placements === null
      ? null
      : isCanonicalDecimal(family.minimum_placements)
        ? BigInt(family.minimum_placements)
        : undefined;
  const candidateIds = new Set<string>();
  let observedMini = false;
  let actualMinimum: bigint | null = null;
  let observedRegular = 0n;
  let observedMiniCount = 0n;
  const candidatesValid = family.candidates.every((candidate) => {
    if (
      !candidate.candidate_id.startsWith('spin-structure-candidate.v1:') ||
      candidateIds.has(candidate.candidate_id) ||
      !isCanonicalDecimal(candidate.placement_count) ||
      !['regular', 'mini'].includes(candidate.partition) ||
      (candidate.partition === 'regular' && observedMini)
    ) {
      return false;
    }
    candidateIds.add(candidate.candidate_id);
    if (candidate.partition === 'regular') {
      observedRegular += 1n;
    } else {
      observedMini = true;
      observedMiniCount += 1n;
    }
    const placementCount = BigInt(candidate.placement_count);
    actualMinimum =
      actualMinimum === null || placementCount < actualMinimum ? placementCount : actualMinimum;
    return true;
  });
  const searchPair =
    outerContract === 'spin-structure.search' &&
    outerResultKind === 'spin-structure-family.v2' &&
    family.schema_id === 'spin-structure-family.v2' &&
    family.guaranteed_final_piece === null &&
    family.guarantee_basis === null &&
    family.dependency_report_included === null &&
    family.dependency_relation === null &&
    family.dependency_edge_count === null;
  const guaranteedPair =
    outerContract === 'spin-structure.guaranteed' &&
    outerResultKind === 'spin-structure-guaranteed.v1' &&
    family.schema_id === 'spin-structure-guaranteed.v1' &&
    ['I', 'O', 'T', 'S', 'Z', 'J', 'L'].includes(family.guaranteed_final_piece ?? '') &&
    family.guarantee_basis ===
      'every-unique-non-target-piece-order-exact-replay-final-piece-last' &&
    (family.dependency_report_included === false
      ? family.dependency_relation === null && family.dependency_edge_count === null
      : family.dependency_report_included === true &&
        family.dependency_relation === 'non-target-universal-precedence' &&
        family.dependency_edge_count === '0');
  const valid =
    (searchPair || guaranteedPair) &&
    [
      family.query_identity_sha256,
      family.supply_identity_sha256,
      family.universe_identity_sha256
    ].every((identity) => /^[0-9a-f]{64}$/u.test(identity)) &&
    family.rule_profile.length > 0 &&
    family.spin_profile.length > 0 &&
    family.product_build.length > 0 &&
    family.ordering === 'regular-then-mini-canonical-operation-key' &&
    family.complete === true &&
    regularCount !== null &&
    miniCount !== null &&
    candidateCount !== null &&
    regularCount + miniCount === candidateCount &&
    candidateCount === BigInt(family.candidates.length) &&
    observedRegular === regularCount &&
    observedMiniCount === miniCount &&
    declaredMinimum !== undefined &&
    declaredMinimum === actualMinimum &&
    candidatesValid;
  return valid ? null : 'invalid spin structure family payload';
}

function finiteInRange(value: string, minimum: number, maximum: number): boolean {
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed >= minimum && parsed <= maximum;
}

function validateBuildV2(
  outerContract: string,
  outerResultKind: string,
  build: Extract<
    ClearraProductResultPayload,
    { content: { payload_kind: 'build-v2' } }
  >['content']['payload']
): string | null {
  const pairs: Record<string, readonly [string, string, readonly string[]]> = {
    'build.congruent': [
      'candidate-family',
      'build-congruence-family.v1',
      ['all', 'unique']
    ],
    'build.evaluate.cover': ['candidate-family', 'build-supplied-coverage.v1', ['all']],
    'build.evaluate.b2b-cover': [
      'candidate-family',
      'build-supplied-b2b-coverage.v1',
      ['all']
    ],
    'build.setup-cover-percent': [
      'probability',
      'build-setup-cover-probability.v1',
      ['all', 'unique']
    ],
    'build.evaluate.cover-percent': [
      'probability',
      'build-supplied-probability.v1',
      ['unique']
    ],
    'build.congruent-cover': [
      'portfolio',
      'build-congruence-coverage.v1',
      ['min-cover', 'max-probability-minimum']
    ],
    'build.setup-cover': [
      'portfolio',
      'build-setup-cover.v1',
      ['min-cover', 'max-probability-minimum']
    ],
    'build.evaluate.minimals': [
      'portfolio',
      'build-supplied-minimum-cover.v1',
      ['min-cover']
    ],
    'build.setup-cover-score': [
      'score-portfolio',
      'build-setup-cover-score.v1',
      ['max-score-cover']
    ],
    'build.evaluate.score': [
      'score-portfolio',
      'build-supplied-score.v1',
      ['max-score-cover']
    ]
  };
  const pair = pairs[build.capability_id];
  if (
    !pair ||
    pair[0] !== build.kind ||
    pair[1] !== build.result_contract ||
    !pair[2].includes(build.objective) ||
    outerContract !== build.capability_id ||
    outerResultKind !== build.result_contract ||
    !/^[0-9a-f]{64}$/u.test(build.input_identity_sha256) ||
    (build.evaluation_identity_sha256 !== null &&
      !/^[0-9a-f]{64}$/u.test(build.evaluation_identity_sha256)) ||
    ![
      build.source_candidate_count,
      build.reachable_candidate_count,
      build.pattern_count
    ].every(isCanonicalDecimal) ||
    [
      build.selected_candidate_count,
      build.covered_pattern_count,
      build.required_pattern_count,
      build.initial_b2b
    ].some((value) => value !== null && !isCanonicalDecimal(value)) ||
    compareCanonicalDecimals(build.reachable_candidate_count, build.source_candidate_count) > 0 ||
    (build.replay_basis !== null && build.replay_basis.length === 0) ||
    !build.completeness.input_identity_bound ||
    !build.completeness.producer_filter_bound ||
    !build.completeness.buildability_replay_complete ||
    !build.completeness.coverage_rows_complete ||
    !build.completeness.probability_weights_complete
  ) {
    return 'invalid Build v2 payload';
  }
  if (build.kind === 'candidate-family') {
    const b2b = build.capability_id === 'build.evaluate.b2b-cover';
    const expectedB2b = build.capability_id === 'build.congruent' ? null : b2b;
    if (
      build.evaluation_identity_sha256 === null ||
      build.replay_basis !== null ||
      build.covered_pattern_count === null ||
      compareCanonicalDecimals(build.covered_pattern_count, build.pattern_count) > 0 ||
      build.union_probability === null ||
      !isCanonicalProbability(build.union_probability) ||
      build.source_candidate_count !== build.candidates.length.toString() ||
      !canonicalBuildCandidateRows(build.candidates) ||
      build.candidates.some(
        (candidate) =>
          compareCanonicalDecimals(candidate.covered_pattern_count, build.pattern_count) > 0
      ) ||
      build.reachable_candidate_count !==
        build.candidates.filter((candidate) => candidate.covered_pattern_count !== '0').length.toString() ||
      build.selected_candidate_count !== null ||
      build.required_pattern_count !== null ||
      build.canonical_candidate_keys.length !== 0 ||
      build.winners.length !== 0 ||
      build.page_source_available ||
      build.page_source_identity_sha256 !== null ||
      build.completeness.exact_minimum_proven ||
      build.completeness.score_evidence_complete ||
      build.score_profile !== null ||
      build.initial_b2b !== null ||
      build.score_accuracy !== null ||
      build.profile_specific_exact !== null ||
      build.score_equality_basis !== null ||
      build.informational_attack_basis !== null ||
      build.b2b_preservation_required !== expectedB2b
    ) {
      return 'invalid Build v2 candidate family';
    }
    return null;
  }
  if (build.kind === 'probability') {
    const supplied = build.capability_id === 'build.evaluate.cover-percent';
    return build.evaluation_identity_sha256 !== null &&
      (supplied
        ? build.replay_basis !== null && build.replay_basis.length > 0
        : build.replay_basis === null) &&
      build.covered_pattern_count !== null &&
      compareCanonicalDecimals(build.covered_pattern_count, build.pattern_count) <= 0 &&
      build.union_probability !== null &&
      isCanonicalProbability(build.union_probability) &&
      build.selected_candidate_count === null &&
      build.required_pattern_count === null &&
      build.candidates.length === 0 &&
      build.canonical_candidate_keys.length === 0 &&
      build.winners.length === 0 &&
      !build.page_source_available &&
      build.page_source_identity_sha256 === null &&
      !build.completeness.exact_minimum_proven &&
      !build.completeness.score_evidence_complete &&
      build.score_profile === null &&
      build.initial_b2b === null &&
      build.score_accuracy === null &&
      build.profile_specific_exact === null &&
      build.score_equality_basis === null &&
      build.informational_attack_basis === null &&
      build.b2b_preservation_required === null
      ? null
      : 'invalid Build v2 probability';
  }
  const canonicalKeys = build.canonical_candidate_keys;
  const suppliedMinimum = build.capability_id === 'build.evaluate.minimals';
  if (
    build.evaluation_identity_sha256 !== null ||
    (suppliedMinimum
      ? build.replay_basis === null || build.replay_basis.length === 0
      : build.replay_basis !== null) ||
    build.selected_candidate_count === null ||
    build.required_pattern_count === null ||
    build.selected_candidate_count === '0' ||
    compareCanonicalDecimals(build.selected_candidate_count, build.reachable_candidate_count) > 0 ||
    compareCanonicalDecimals(build.required_pattern_count, build.pattern_count) > 0 ||
    build.selected_candidate_count !== canonicalKeys.length.toString() ||
    canonicalKeys.length === 0 ||
    !strictlySortedNonempty(canonicalKeys) ||
    build.candidates.length !== 0 ||
    !build.page_source_available ||
    build.page_source_identity_sha256 === null ||
    !/^[0-9a-f]{64}$/u.test(build.page_source_identity_sha256) ||
    !build.completeness.exact_minimum_proven ||
    build.covered_pattern_count !== null ||
    build.b2b_preservation_required !== null
  ) {
    return 'invalid Build v2 portfolio';
  }
  if (build.kind === 'portfolio') {
    return build.union_probability !== null &&
      isCanonicalProbability(build.union_probability) &&
      build.winners.length === 0 &&
      build.score_profile === null &&
      build.initial_b2b === null &&
      build.score_accuracy === null &&
      build.profile_specific_exact === null &&
      build.score_equality_basis === null &&
      build.informational_attack_basis === null &&
      !build.completeness.score_evidence_complete
      ? null
      : 'invalid Build v2 minimum portfolio';
  }
  return build.objective === 'max-score-cover' &&
    build.replay_basis === null &&
    build.union_probability === null &&
    build.score_profile !== null &&
    ['tetrio', 'guideline', 'jstris-ultra'].includes(build.score_profile) &&
    build.initial_b2b !== null &&
    compareCanonicalDecimals(build.initial_b2b, '65535') <= 0 &&
    build.score_accuracy === 'basic-approximation' &&
    build.profile_specific_exact === false &&
    build.score_equality_basis === 'score-only' &&
    build.informational_attack_basis === 'canonical-equal-score-trace' &&
    build.completeness.score_evidence_complete &&
    build.required_pattern_count === build.winners.length.toString() &&
    build.winners.every(
      (winner, index) =>
        isCanonicalDecimal(winner.pattern_id) &&
        isCanonicalDecimal(winner.score) &&
        isCanonicalDecimal(winner.informational_attack) &&
        canonicalKeys.includes(winner.candidate_key) &&
        compareCanonicalDecimals(winner.pattern_id, build.pattern_count) < 0 &&
        (index === 0 ||
          compareCanonicalDecimals(build.winners[index - 1]!.pattern_id, winner.pattern_id) < 0)
    )
    ? null
    : 'invalid Build v2 score portfolio';
}

function canonicalBuildCandidateRows(
  rows: readonly { candidate_key: string; covered_pattern_count: string }[]
): boolean {
  return (
    rows.every((row) => Boolean(row.candidate_key) && isCanonicalDecimal(row.covered_pattern_count)) &&
    strictlySortedNonempty(rows.map((row) => row.candidate_key))
  );
}

function strictlySortedNonempty(values: readonly string[]): boolean {
  return values.every(
    (value, index) => Boolean(value) && (index === 0 || values[index - 1]! < value)
  );
}

export function compareCanonicalDecimals(left: string, right: string): number {
  return left.length === right.length
    ? left.localeCompare(right)
    : left.length - right.length;
}

function validateFieldDocument(document: {
  format: 'ctk3' | 'fumen';
  document: string;
  page_count: number;
  canonical_sha256: string;
  filename: string;
}): string | null {
  if (
    !/^[0-9a-f]{64}$/.test(document.canonical_sha256) ||
    !Number.isInteger(document.page_count) ||
    document.page_count < 1 ||
    document.page_count > 4096 ||
    !document.filename ||
    (document.format === 'ctk3'
      ? !/^ctk3(?:b_|_|@)/.test(document.document)
      : !/^(?:v115|[Ddm]115)@/.test(document.document))
  ) {
    return 'invalid field document payload';
  }
  return null;
}
