<script lang="ts">
  import { componentMessage, type ComponentMessageKey } from '../i18n/componentCatalog';
  // SRP rationale: this component's single change reason is browsing one
  // validated product-result family: navigation, selected-family export binding
  // and source release. Payload validation, backend enumeration, replay rendering
  // and copy encoding remain delegated to their existing dedicated modules.
  import { ChevronLeft, ChevronRight, LoaderCircle } from '@lucide/svelte';
  import { onDestroy } from 'svelte';

  import type {
    ClearraCoveragePortfolioRuntimePage,
    ClearraPcReplayRuntimePage,
    ClearraProductResultPayload
  } from '../wasm/wasmCommandClient';
  import PcPathReplayGif from './PcPathReplayGif.svelte';
  import { collectPcReplayGeometryExportPages, loadPcReplayPage } from './pcReplayPager';
  import SolutionCopyFormatControl from './SolutionCopyFormatControl.svelte';
  import SolutionSubsetPage from './SolutionSubsetPage.svelte';
  import type { SolutionCopyFormat } from './solutionExport';
  import type { SolutionExportKeySource } from './solutionExportAsync';
  import { tryCreateCoveragePortfolioExportKeySource } from './coveragePortfolioExportSource';
  import {
    groupPcPathWitnesses,
    pcPathCandidateGroupExportPages
  } from './pcPathReplayPresentation';
  import {
    CoveragePortfolioPagerController,
    PRODUCT_MEMBER_PAGE_SIZE,
    compareCanonicalDecimals,
    coveragePortfolioPageReference,
    decrementCanonicalDecimal,
    incrementCanonicalDecimal,
    loadCoveragePortfolioExactPage,
    productResultIdentity,
    validateProductResultPayload,
    type CoveragePortfolioPagerSnapshot,
    type ProductMemberPageLoader,
    type ProductNextPageLoader,
    type ProductPageRelease
  } from './productResultPager';
  import type { WorkspaceLanguage } from './workspaceI18n';
  import WorkspaceFailureNotice from './WorkspaceFailureNotice.svelte';
  import { projectWorkspacePublicFailure } from './workspacePublicFailure';

  const MAX_RETAINED_MEMBER_PAGES = 3;

  export let payload: ClearraProductResultPayload | null | undefined = null;
  export let language: WorkspaceLanguage = 'en';
  export let targetLines = 4;
  export let loadNextPage: ProductNextPageLoader | null = null;
  export let loadMemberPage: ProductMemberPageLoader | null = null;
  export let releasePages: ProductPageRelease | null = null;

  let activeIdentity = '';
  let activePayload: ClearraProductResultPayload | null = null;
  let lazyReplayPage: ClearraPcReplayRuntimePage | null = null;
  let replayExportController: AbortController | null = null;
  let abortController: AbortController | null = null;
  let coveragePages: ClearraCoveragePortfolioRuntimePage[] = [];
  let outerPageIndex = 0;
  let currentMembers: ClearraCoveragePortfolioRuntimePage['members'] = [];
  let memberPageNumber = '1';
  let prefetchedPage: ClearraCoveragePortfolioRuntimePage | null = null;
  let prefetchInFlight = false;
  let enumerationSealed = false;
  let highestMaterializedAlternativeIndex: string | null = null;
  let navigatingOuter = false;
  let loadingMember = false;
  let error = '';
  let handleOwned = false;
  let pathPageIndex = 0;
  let scorePageIndex = 0;
  let buildCandidatePageIndex = 0;
  let buildScorePageIndex = 0;
  let setupRankedPageIndex = 0;
  let setupScorePageIndex = 0;
  let spinStructurePageIndex = 0;
  let solutionCopyFormat: SolutionCopyFormat = 'ctk';
  let coverageExportKeySource: SolutionExportKeySource | null = null;
  let coverageExportIdentity = '';
  let outerPager: CoveragePortfolioPagerController | null = null;
  const memberCache = new Map<string, ClearraCoveragePortfolioRuntimePage['members']>();

  $: pagerFailure = error
    ? projectWorkspacePublicFailure({
        status: 'failed',
        error,
        fallbackCode: 'result-invalid'
      })
    : null;
  $: nextIdentity = productResultIdentity(payload);
  $: if (nextIdentity !== activeIdentity || (payload ?? null) !== activePayload) resetForPayload(payload ?? null, nextIdentity);
  $: coveragePage = coveragePages[outerPageIndex] ?? null;
  $: currentAlternativeIndex = coveragePage?.alternative_index ?? null;
  $: coverageSolutionKeys = currentMembers.map((member) => member.normalized_solution_key);
  $: coverageSolutionPageIdentity = coveragePage
    ? `${coveragePage.set_identity_sha256}:${coveragePage.candidate_map_sha256}:${coveragePage.alternative_index}:${memberPageNumber}`
    : '';
  $: coverageSolutionOrdinalBase = memberOrdinalBase(memberPageNumber);
  $: nextAlternativeIndex = currentAlternativeIndex === null
    ? null
    : incrementCanonicalDecimal(currentAlternativeIndex);
  $: previousOuterAvailable = outerPageIndex > 0 || (
    currentAlternativeIndex !== null &&
    currentAlternativeIndex !== '1' &&
    Boolean(loadMemberPage)
  );
  $: nextOuterAvailable =
    outerPageIndex + 1 < coveragePages.length ||
    (currentAlternativeIndex !== null &&
      highestMaterializedAlternativeIndex !== null &&
      compareCanonicalDecimals(
        currentAlternativeIndex,
        highestMaterializedAlternativeIndex
      ) < 0 &&
      Boolean(loadMemberPage)) ||
    (nextAlternativeIndex !== null &&
      prefetchedPage?.alternative_index === nextAlternativeIndex) ||
    (!enumerationSealed && Boolean(loadNextPage));
  $: pathFamily = payload?.content.payload_kind === 'pc-path-family' ||
    payload?.content.payload_kind === 'build-path-family'
    ? payload.content.payload
    : null;
  $: buildPathFamily = payload?.content.payload_kind === 'build-path-family'
    ? payload.content.payload
    : null;
  $: pathCandidateGroups = groupPcPathWitnesses(lazyReplayPage?.witnesses ?? pathFamily?.witnesses ?? []);
  $: pathPageCount = lazyReplayPage?.geometry_count ?? String(pathCandidateGroups.length);
  $: pathOrdinal = lazyReplayPage?.geometry_page_number ?? String(pathCandidateGroups.length ? pathPageIndex + 1 : 0);
  $: pathCandidateGroup = pathCandidateGroups[lazyReplayPage ? 0 : pathPageIndex] ?? null;
  $: scoreFamily = payload?.content.payload_kind === 'score-pattern-winner-family'
    ? payload.content.payload
    : null;
  $: scorePageCount = scoreFamily
    ? Math.max(1, Math.ceil(scoreFamily.winners.length / PRODUCT_MEMBER_PAGE_SIZE))
    : 0;
  $: scoreWinners = scoreFamily
    ? scoreFamily.winners.slice(
        scorePageIndex * PRODUCT_MEMBER_PAGE_SIZE,
        (scorePageIndex + 1) * PRODUCT_MEMBER_PAGE_SIZE
      )
    : [];
  $: scoreWinnerSolutionKeys = scoreWinners.map((winner) => winner.normalized_solution_key);
  $: allScoreWinnerSolutionKeys = scoreFamily?.winners.map(
    (winner) => winner.normalized_solution_key
  ) ?? [];
  $: scoreWinnerCaptions = scoreWinners.map((winner, index) =>
    componentMessage(language, 'resultScoreInformationalAttack', { value0: scorePageIndex * PRODUCT_MEMBER_PAGE_SIZE + index + 1, value1: winner.score, value2: winner.informational_attack })
  );
  $: buildV2 = payload?.content.payload_kind === 'build-v2'
    ? payload.content.payload
    : null;
  $: buildSetupFamily = payload?.content.payload_kind === 'build-setup-family-v1'
    ? payload.content.payload
    : null;
  $: buildCoveragePortfolio =
    payload?.content.payload_kind === 'build-coverage-portfolio-v2'
      ? payload.content.payload
      : null;
  $: buildCandidateRows = buildV2?.kind === 'candidate-family'
    ? buildV2.candidates
    : buildSetupFamily?.candidates ?? [];
  $: buildCandidatePageCount = buildCandidateRows.length
    ? Math.ceil(buildCandidateRows.length / PRODUCT_MEMBER_PAGE_SIZE)
    : 0;
  $: buildCandidatePage = buildCandidateRows.slice(
    buildCandidatePageIndex * PRODUCT_MEMBER_PAGE_SIZE,
    (buildCandidatePageIndex + 1) * PRODUCT_MEMBER_PAGE_SIZE
  );
  $: buildCandidateSolutionKeys = buildCandidatePage.map(
    (candidate) => candidate.candidate_key
  );
  $: allBuildCandidateSolutionKeys = buildCandidateRows.map(
    (candidate) => candidate.candidate_key
  );
  $: buildCandidateCaptions = buildCandidatePage.map((candidate) =>
    componentMessage(language, 'coveredPatterns', { value0: candidate.covered_pattern_count })
  );
  $: buildScorePageCount = buildV2?.kind === 'score-portfolio'
    ? Math.max(1, Math.ceil(buildV2.winners.length / PRODUCT_MEMBER_PAGE_SIZE))
    : 0;
  $: buildScoreWinners = buildV2?.kind === 'score-portfolio'
    ? buildV2.winners.slice(
        buildScorePageIndex * PRODUCT_MEMBER_PAGE_SIZE,
        (buildScorePageIndex + 1) * PRODUCT_MEMBER_PAGE_SIZE
      )
    : [];
  $: buildScoreSolutionKeys = buildScoreWinners.map((winner) => winner.candidate_key);
  $: allBuildScoreSolutionKeys = buildV2?.kind === 'score-portfolio'
    ? buildV2.winners.map((winner) => winner.candidate_key)
    : [];
  $: buildScoreCaptions = buildScoreWinners.map((winner, index) =>
    componentMessage(language, 'resultScoreInformationalAttack2', { value0: buildScorePageIndex * PRODUCT_MEMBER_PAGE_SIZE + index + 1, value1: winner.score, value2: winner.informational_attack })
  );
  $: setupRankedFamily = payload?.content.payload_kind === 'setup-ranked-family'
    ? payload.content.payload
    : null;
  $: setupRankedPageCount = setupRankedFamily
    ? Math.max(1, Math.ceil(setupRankedFamily.candidates.length / PRODUCT_MEMBER_PAGE_SIZE))
    : 0;
  $: setupRankedCandidates = setupRankedFamily
    ? setupRankedFamily.candidates.slice(
        setupRankedPageIndex * PRODUCT_MEMBER_PAGE_SIZE,
        (setupRankedPageIndex + 1) * PRODUCT_MEMBER_PAGE_SIZE
      )
    : [];
  $: setupScoreFamily = payload?.content.payload_kind === 'setup-score-ranking'
    ? payload.content.payload
    : null;
  $: setupScorePageCount = setupScoreFamily
    ? Math.max(1, Math.ceil(setupScoreFamily.candidates.length / PRODUCT_MEMBER_PAGE_SIZE))
    : 0;
  $: setupScoreCandidates = setupScoreFamily
    ? setupScoreFamily.candidates.slice(
        setupScorePageIndex * PRODUCT_MEMBER_PAGE_SIZE,
        (setupScorePageIndex + 1) * PRODUCT_MEMBER_PAGE_SIZE
      )
    : [];
  $: spinStructureFamily = payload?.content.payload_kind === 'spin-structure-family'
    ? payload.content.payload
    : null;
  $: spinStructurePageCount = spinStructureFamily
    ? Math.max(1, Math.ceil(spinStructureFamily.candidates.length / PRODUCT_MEMBER_PAGE_SIZE))
    : 0;
  $: spinStructureCandidates = spinStructureFamily
    ? spinStructureFamily.candidates.slice(
        spinStructurePageIndex * PRODUCT_MEMBER_PAGE_SIZE,
        (spinStructurePageIndex + 1) * PRODUCT_MEMBER_PAGE_SIZE
      )
    : [];
  $: buildPortfolioActive =
    buildCoveragePortfolio !== null ||
    buildV2?.kind === 'portfolio' ||
    buildV2?.kind === 'score-portfolio';
  $: invalidPreviewLabel = componentMessage(language, 'boardPreviewIsUnavailable');
  $: scoreMinimalCoverage = payload?.contract === 'pc.score-minimals' ||
    payload?.contract === 'build.highest-score-minimum-set';
  $: scoreOnlyPortfolio = scoreMinimalCoverage || buildV2?.kind === 'score-portfolio';

  onDestroy(() => releaseHandle());

  function loadVisiblePathPages(signal?: AbortSignal) {
    const page = lazyReplayPage;
    if (page) {
      if (!loadMemberPage) throw new Error('The active replay source has no product-page loader.');
      const identity = activeIdentity;
      const ownerSignal = abortController?.signal;
      replayExportController?.abort();
      const controller = new AbortController();
      replayExportController = controller;
      const cancel = () => controller.abort();
      signal?.addEventListener('abort', cancel, { once: true });
      ownerSignal?.addEventListener('abort', cancel, { once: true });
      if (signal?.aborted || ownerSignal?.aborted) cancel();
      return collectPcReplayGeometryExportPages({
        initialPage: page, loadMemberPage, targetLines, signal: controller.signal,
        isCurrent: () => activeIdentity === identity && lazyReplayPage === page &&
          !navigatingOuter && !ownerSignal?.aborted
      }).finally(() => {
        signal?.removeEventListener('abort', cancel);
        ownerSignal?.removeEventListener('abort', cancel);
        if (replayExportController === controller) replayExportController = null;
      });
    }
    if (!pathCandidateGroup) return [];
    return pcPathCandidateGroupExportPages(
      pathCandidateGroup,
      targetLines,
      buildPathFamily
        ? [buildPathFamily.target_terminal_board_mask,
           ...(buildPathFamily.mirrored_terminal_board_mask ? [buildPathFamily.mirrored_terminal_board_mask] : [])]
        : null
    );
  }

  async function showReplayGeometry(direction: -1 | 1) {
    if (navigatingOuter) return;
    const reference = lazyReplayPage;
    if (!reference) {
      pathPageIndex += direction;
      return;
    }
    if (!loadMemberPage) return;
    const next = BigInt(reference.geometry_page_number) + BigInt(direction);
    if (next < 1n || next > BigInt(reference.geometry_count)) return;
    replayExportController?.abort();
    const identity = activeIdentity;
    const signal = abortController?.signal;
    navigatingOuter = true;
    const isCurrent = () => activeIdentity === identity && lazyReplayPage === reference && !signal?.aborted;
    try {
      const page = await loadPcReplayPage({
        loadMemberPage, reference, geometryPageNumber: String(next), memberPageNumber: '1', signal, isCurrent
      });
      if (isCurrent()) lazyReplayPage = page;
    } catch (reason) {
      if (isCurrent()) error = errorMessage(reason);
    } finally {
      if (activeIdentity === identity && !signal?.aborted) navigatingOuter = false;
    }
  }

  function resetForPayload(
    nextPayload: ClearraProductResultPayload | null,
    identity: string
  ) {
    releaseHandle();
    activeIdentity = identity;
    activePayload = nextPayload;
    lazyReplayPage = null;
    coveragePages = [];
    outerPageIndex = 0;
    currentMembers = [];
    memberPageNumber = '1';
    prefetchedPage = null;
    prefetchInFlight = false;
    enumerationSealed = false;
    highestMaterializedAlternativeIndex = null;
    navigatingOuter = false;
    loadingMember = false;
    error = '';
    pathPageIndex = 0;
    scorePageIndex = 0;
    buildCandidatePageIndex = 0;
    buildScorePageIndex = 0;
    setupRankedPageIndex = 0;
    setupScorePageIndex = 0;
    spinStructurePageIndex = 0;
    coverageExportKeySource = null;
    coverageExportIdentity = '';
    memberCache.clear();
    if (!nextPayload || validateProductResultPayload(nextPayload)) {
      if (nextPayload) error = validateProductResultPayload(nextPayload) ?? '';
      return;
    }
    if (nextPayload.content.payload_kind === 'pc-path-family' && nextPayload.content.payload.page_source_available === true) {
      handleOwned = Boolean(releasePages);
      abortController = new AbortController();
      if (!loadMemberPage) {
        error = 'The active PC replay source has no product-page loader.';
        return;
      }
      lazyReplayPage = nextPayload.content.payload as ClearraPcReplayRuntimePage;
      return;
    }
    if (nextPayload.content.payload_kind === 'coverage-portfolio') {
      abortController = new AbortController();
      const { set_contract: _, page_handle_available, ...canonical } =
        nextPayload.content.payload;
      coveragePages = [canonical];
      currentMembers = canonical.members;
      memberCache.set(`${canonical.alternative_index}:1`, canonical.members);
      handleOwned = page_handle_available && Boolean(releasePages);
      if (!activateCoverageExportSource(canonical, identity)) {
        releaseHandle();
        return;
      }
      // The GUI materializes only the page the user is viewing. Do not advance the
      // exact portfolio enumerator in the background merely to fill a prefetch slot.
      initializeOuterPager(identity, canonical, false);
      return;
    }
    const pageSourceIdentity = buildPageSourceIdentity(nextPayload);
    if (pageSourceIdentity) {
      handleOwned = Boolean(releasePages);
      if (!loadMemberPage) {
        error = 'the active Build portfolio has no product-page loader';
        return;
      }
      const controller = new AbortController();
      abortController = controller;
      loadingMember = true;
      void loadInitialBuildPortfolioPage(pageSourceIdentity, identity, controller.signal);
    }
  }

  function buildPageSourceIdentity(
    nextPayload: ClearraProductResultPayload
  ): string | null {
    if (nextPayload.content.payload_kind === 'build-coverage-portfolio-v2') {
      return nextPayload.content.payload.page_source_available
        ? nextPayload.content.payload.page_source_identity_sha256
        : null;
    }
    if (
      nextPayload.content.payload_kind === 'build-v2' &&
      (nextPayload.content.payload.kind === 'portfolio' ||
        nextPayload.content.payload.kind === 'score-portfolio') &&
      nextPayload.content.payload.page_source_available
    ) {
      return nextPayload.content.payload.page_source_identity_sha256;
    }
    return null;
  }

  function initializeOuterPager(
    identity: string,
    initialPage: ClearraCoveragePortfolioRuntimePage,
    autoPrefetch: boolean
  ) {
    const pager = new CoveragePortfolioPagerController({
      loadNextPage,
      loadMemberPage,
      onChange: (snapshot) => syncOuterPager(pager, identity, snapshot)
    });
    outerPager = pager;
    pager.reset(identity, initialPage, { autoPrefetch });
  }

  function syncOuterPager(
    pager: CoveragePortfolioPagerController,
    identity: string,
    snapshot: CoveragePortfolioPagerSnapshot
  ) {
    if (outerPager !== pager || activeIdentity !== identity) return;
    const previousAlternativeIndex =
      coveragePages[outerPageIndex]?.alternative_index ?? null;
    coveragePages = [...snapshot.pages];
    outerPageIndex = snapshot.outerPageIndex;
    const selectedPage = coveragePages[outerPageIndex] ?? null;
    if (
      selectedPage &&
      selectedPage.alternative_index !== previousAlternativeIndex
    ) {
      memberPageNumber = '1';
      currentMembers = selectedPage.members;
      memberCache.set(`${selectedPage.alternative_index}:1`, selectedPage.members);
      pruneMemberCache(selectedPage.alternative_index, '1');
      if (!activateCoverageExportSource(selectedPage, identity)) {
        releaseHandle();
        return;
      }
    }
    prefetchedPage = snapshot.prefetchedPage;
    prefetchInFlight = snapshot.prefetchInFlight;
    enumerationSealed = snapshot.enumerationSealed;
    highestMaterializedAlternativeIndex = snapshot.highestMaterializedAlternativeIndex;
    navigatingOuter = snapshot.navigating;
    if (snapshot.error) error = snapshot.error;
  }

  async function loadInitialBuildPortfolioPage(
    pageSourceIdentity: string,
    payloadIdentity: string,
    signal: AbortSignal
  ) {
    try {
      if (!loadMemberPage) {
        throw new Error('the active Build portfolio has no product-page loader');
      }
      const initialPage = await loadCoveragePortfolioExactPage({
        loadMemberPage,
        alternativeIndex: '1',
        memberPageNumber: '1',
        signal,
        isCurrent: () => activeIdentity === payloadIdentity,
        expectation: {
          setIdentitySha256: pageSourceIdentity,
          alternativeIndex: '1',
          memberPageNumber: '1'
        }
      });
      if (!initialPage || signal.aborted || activeIdentity !== payloadIdentity) return;
      currentMembers = initialPage.members;
      memberCache.set(`${initialPage.alternative_index}:1`, initialPage.members);
      if (!activateCoverageExportSource(initialPage, payloadIdentity)) {
        releaseHandle();
        return;
      }
      initializeOuterPager(payloadIdentity, initialPage, false);
    } catch (reason) {
      if (!signal.aborted && activeIdentity === payloadIdentity) error = errorMessage(reason);
    } finally {
      if (!signal.aborted && activeIdentity === payloadIdentity) loadingMember = false;
    }
  }

  function releaseHandle() {
    replayExportController?.abort();
    replayExportController = null;
    outerPager?.dispose();
    outerPager = null;
    abortController?.abort();
    abortController = null;
    if (handleOwned) {
      try {
        void releasePages?.();
      } catch {}
    }
    handleOwned = false;
    coverageExportKeySource = null;
    coverageExportIdentity = '';
  }

  function activateCoverageExportSource(
    page: ClearraCoveragePortfolioRuntimePage,
    payloadIdentity: string
  ): boolean {
    const exportIdentity = [
      payloadIdentity,
      page.set_identity_sha256,
      page.candidate_map_sha256,
      page.alternative_index
    ].join(':');
    const activation = tryCreateCoveragePortfolioExportKeySource({
      initialPage: page,
      loadMemberPage,
      isCurrent: () =>
        activeIdentity === payloadIdentity &&
        coverageExportIdentity === exportIdentity
    });
    if (activation.error !== null) {
      coverageExportKeySource = null;
      coverageExportIdentity = '';
      error = activation.error;
      return false;
    }
    coverageExportIdentity = exportIdentity;
    coverageExportKeySource = activation.keySource;
    return true;
  }

  async function nextOuterPage() {
    if (loadingMember) return;
    const pager = outerPager;
    if (!pager) return;
    const payloadIdentity = activeIdentity;
    const page = await pager.next();
    if (!page || outerPager !== pager || activeIdentity !== payloadIdentity) return;
    await showMemberPage('1');
  }

  async function previousOuterPage() {
    if (loadingMember) return;
    const pager = outerPager;
    if (!pager) return;
    const payloadIdentity = activeIdentity;
    const page = await pager.previous();
    if (!page || outerPager !== pager || activeIdentity !== payloadIdentity) return;
    await showMemberPage('1');
  }

  async function showMemberPage(nextMemberPage: string) {
    if (loadingMember) return;
    const page = coveragePages[outerPageIndex];
    if (!page) return;
    if (
      !isPositiveCanonicalDecimal(nextMemberPage) ||
      compareCanonicalDecimals(nextMemberPage, page.total_member_pages) > 0
    ) {
      return;
    }
    const cacheKey = `${page.alternative_index}:${nextMemberPage}`;
    if (nextMemberPage === '1' && !memberCache.has(cacheKey)) {
      memberCache.set(cacheKey, page.members);
    }
    const cached = memberCache.get(cacheKey);
    if (cached) {
      memberPageNumber = nextMemberPage;
      currentMembers = cached;
      pruneMemberCache(page.alternative_index, nextMemberPage);
      return;
    }
    if (!loadMemberPage) return;
    const alternativeIndex = page.alternative_index;
    const referencePage = coveragePortfolioPageReference(page);
    const payloadIdentity = activeIdentity;
    const requestSignal = abortController?.signal;
    loadingMember = true;
    error = '';
    try {
      const loadedPage = await loadCoveragePortfolioExactPage({
        loadMemberPage,
        alternativeIndex,
        memberPageNumber: nextMemberPage,
        signal: requestSignal,
        isCurrent: () =>
          activeIdentity === payloadIdentity &&
          coveragePages[outerPageIndex]?.alternative_index === alternativeIndex,
        expectation: {
          setIdentitySha256: referencePage.set_identity_sha256,
          candidateMapSha256: referencePage.candidate_map_sha256,
          alternativeIndex,
          memberPageNumber: nextMemberPage,
          referencePage,
          requireSameAlternativeMetadata: true
        }
      });
      if (!loadedPage || requestSignal?.aborted || activeIdentity !== payloadIdentity) return;
      memberCache.set(cacheKey, loadedPage.members);
      memberPageNumber = nextMemberPage;
      currentMembers = loadedPage.members;
      pruneMemberCache(alternativeIndex, nextMemberPage);
    } catch (reason) {
      if (activeIdentity === payloadIdentity) error = errorMessage(reason);
    } finally {
      if (activeIdentity === payloadIdentity) loadingMember = false;
    }
  }

  function pruneMemberCache(activeAlternativeIndex: string, activeMemberPage: string) {
    const retainedMemberPages = new Set(
      [
        activeMemberPage === '1' ? null : decrementCanonicalDecimal(activeMemberPage),
        activeMemberPage,
        incrementCanonicalDecimal(activeMemberPage)
      ]
        .filter((pageNumber): pageNumber is string => pageNumber !== null)
        .slice(0, MAX_RETAINED_MEMBER_PAGES)
        .map((pageNumber) => `${activeAlternativeIndex}:${pageNumber}`)
    );
    for (const cacheKey of memberCache.keys()) {
      if (!retainedMemberPages.has(cacheKey)) memberCache.delete(cacheKey);
    }
  }

  function isPositiveCanonicalDecimal(value: string): boolean {
    return /^[1-9][0-9]*$/u.test(value);
  }

  function errorMessage(value: unknown): string {
    return value instanceof Error ? value.message : String(value);
  }

  function objectiveLabel(value: string): string {
    const labels: Record<string, ComponentMessageKey> = {
      all: 'allSolutions',
      unique: 'uniqueSolutions',
      'min-cover': 'minimumSolutions',
      'max-probability-minimum': 'mostProbableMinimumSolutions',
      'max-score-cover': 'highestScoreMinimumSet'
    };
    const label = labels[value];
    return label ? componentMessage(language, label) : (componentMessage(language, 'selectedObjective'));
  }

  function lengthPreferenceLabel(value: 'longer' | 'shorter'): string {
    if (value === 'longer') return componentMessage(language, 'longerSetupsFirst');
    return componentMessage(language, 'shorterSetupsFirst');
  }

  function spinPartitionLabel(value: 'regular' | 'mini'): string {
    if (value === 'regular') return componentMessage(language, 'regularSpin');
    return componentMessage(language, 'miniSpin');
  }

  function rotationLabel(value: string): string {
    const labels: Record<string, ComponentMessageKey> = {
      '0': 'spawnRotation',
      '1': 'rightRotation',
      '2': 'reverseRotation',
      '3': 'leftRotation'
    };
    const label = labels[value];
    return label ? componentMessage(language, label) : (componentMessage(language, 'rotation'));
  }

  function holdDecisionLabel(value: string): string {
    const labels: Record<string, ComponentMessageKey> = {
      none: 'noHold',
      store: 'storedInHold',
      swap: 'swappedHold'
    };
    const label = labels[value];
    return label ? componentMessage(language, label) : (componentMessage(language, 'holdUsed'));
  }

  function memberOrdinalBase(pageNumber: string): string {
    try {
      return ((BigInt(pageNumber) - 1n) * BigInt(PRODUCT_MEMBER_PAGE_SIZE)).toString();
    } catch {
      return '0';
    }
  }
</script>

{#if payload && !error}
  {#if (payload.content.payload_kind === 'coverage-portfolio' || buildPortfolioActive) && coveragePage}
    <section class="product-pager" aria-label={componentMessage(language, 'optimalSolutionPages')}>
      <header>
        <div>
          <strong>{buildPortfolioActive
            ? (componentMessage(language, 'allOptimalBuildPortfolios'))
            : scoreMinimalCoverage
              ? (componentMessage(language, 'allMinimumMaximumScoreSolutionSets'))
              : (componentMessage(language, 'allEqualMinimumSizeSolutions'))}</strong>
          <span>{componentMessage(language, 'solution')} {coveragePage.alternative_index}{coveragePage.total_alternative_count ? ` / ${coveragePage.total_alternative_count}` : ''}</span>
          {#if scoreOnlyPortfolio}
            <span>{componentMessage(language, 'equalityMembershipAndOrderingUseScoreOnly')}</span>
          {/if}
        </div>
        <nav aria-label={componentMessage(language, 'solutionPageNavigation')}>
          <button type="button" disabled={loadingMember || navigatingOuter || !previousOuterAvailable} on:click={previousOuterPage} aria-label={componentMessage(language, 'previousSolution')}><ChevronLeft size={16} /></button>
          <button type="button" disabled={loadingMember || navigatingOuter || !nextOuterAvailable} on:click={nextOuterPage} aria-label={componentMessage(language, 'nextSolution')}>{#if prefetchInFlight && outerPageIndex + 1 >= coveragePages.length}<LoaderCircle class="spin" size={16} />{:else}<ChevronRight size={16} />{/if}</button>
        </nav>
      </header>
      <div class="member-meta">
        <span>{componentMessage(language, 'minimumCardinality')}: {coveragePage.optimal_cardinality}</span>
        <span>{componentMessage(language, 'memberPage')}: {memberPageNumber} / {coveragePage.total_member_pages}</span>
      </div>
      {#if coveragePage.optimal_cardinality === '0'}
        <p class="empty-result" role="status">
          {componentMessage(language, 'noSolutionIsRequiredForThisPattern')}
        </p>
      {:else}
        <SolutionSubsetPage
          solutionKeys={coverageSolutionKeys}
          solutionSetIdentity={coverageSolutionPageIdentity}
          solutionOrdinalBase={coverageSolutionOrdinalBase}
          exportKeySource={coverageExportKeySource}
          exportSetIdentity={coverageExportIdentity}
          bind:copyFormat={solutionCopyFormat}
          {targetLines}
          {language}
        />
      {/if}
      <footer>
        <button type="button" disabled={loadingMember || navigatingOuter || memberPageNumber === '1'} on:click={() => showMemberPage(decrementCanonicalDecimal(memberPageNumber))}><ChevronLeft size={15} />{componentMessage(language, 'previous100')}</button>
        <button type="button" disabled={loadingMember || navigatingOuter || compareCanonicalDecimals(memberPageNumber, coveragePage.total_member_pages) >= 0} on:click={() => showMemberPage(incrementCanonicalDecimal(memberPageNumber))}>{componentMessage(language, 'next100')}<ChevronRight size={15} /></button>
      </footer>
      {#if buildV2?.kind === 'score-portfolio'}
        <div class="build-score-evidence">
          <div class="member-meta">
            <span>{componentMessage(language, 'scoreProfile')}: {buildV2.score_profile}</span>
            <span>{componentMessage(language, 'initialB2b')}: {buildV2.initial_b2b}</span>
            <span>{componentMessage(language, 'scoreEvidencePage')}: {buildScorePageIndex + 1} / {buildScorePageCount}</span>
          </div>
          <SolutionSubsetPage
            solutionKeys={buildScoreSolutionKeys}
            exportSolutionKeys={allBuildScoreSolutionKeys}
            solutionCaptions={buildScoreCaptions}
            solutionSetIdentity={`${activeIdentity}:build-score-evidence:${buildScorePageIndex}`}
            exportSetIdentity={`${activeIdentity}:build-score-evidence`}
            solutionOrdinalBase={(buildScorePageIndex * PRODUCT_MEMBER_PAGE_SIZE).toString()}
            bind:copyFormat={solutionCopyFormat}
            {targetLines}
            {language}
          />
          <footer>
            <button type="button" disabled={buildScorePageIndex === 0} on:click={() => (buildScorePageIndex -= 1)}><ChevronLeft size={15} />{componentMessage(language, 'previousScoreEvidence')}</button>
            <button type="button" disabled={buildScorePageIndex + 1 >= buildScorePageCount} on:click={() => (buildScorePageIndex += 1)}>{componentMessage(language, 'nextScoreEvidence')}<ChevronRight size={15} /></button>
          </footer>
        </div>
      {/if}
    </section>
  {:else if buildPortfolioActive && loadingMember}
    <p class="pager-loading" role="status"><LoaderCircle class="spin" size={16} />{componentMessage(language, 'loadingTheFirstBuildPortfolioPage')}</p>
  {:else if buildV2 && (buildV2.kind === 'candidate-family' || buildV2.kind === 'probability')}
    <section class="product-pager build-family" aria-label={componentMessage(language, 'buildResult')}>
      <header>
        <div>
          <strong>{buildV2.kind === 'probability'
            ? (componentMessage(language, 'buildProbability'))
            : (componentMessage(language, 'buildSolutions'))}</strong>
          <span>{componentMessage(language, 'thisIsAnOrdinaryResultFamilyNot')}</span>
        </div>
        {#if buildV2.kind === 'candidate-family' && buildCandidatePageCount > 1}
          <nav aria-label={componentMessage(language, 'buildCandidateNavigation')}>
            <button type="button" disabled={buildCandidatePageIndex === 0} on:click={() => (buildCandidatePageIndex -= 1)}><ChevronLeft size={16} /></button>
            <span>{buildCandidatePageIndex + 1} / {buildCandidatePageCount}</span>
            <button type="button" disabled={buildCandidatePageIndex + 1 >= buildCandidatePageCount} on:click={() => (buildCandidatePageIndex += 1)}><ChevronRight size={16} /></button>
          </nav>
        {/if}
      </header>
      <div class="member-meta">
        <span>{componentMessage(language, 'objective')}: {objectiveLabel(buildV2.objective)}</span>
        <span>{componentMessage(language, 'reachableCandidates')}: {buildV2.reachable_candidate_count} / {buildV2.source_candidate_count}</span>
        <span>{componentMessage(language, 'coveredPatterns2')}: {buildV2.covered_pattern_count} / {buildV2.pattern_count}</span>
        <span>{componentMessage(language, 'unionProbability')}: {buildV2.union_probability}</span>
      </div>
      {#if buildV2.kind === 'candidate-family'}
        <SolutionSubsetPage
          solutionKeys={buildCandidateSolutionKeys}
          exportSolutionKeys={allBuildCandidateSolutionKeys}
          solutionCaptions={buildCandidateCaptions}
          solutionSetIdentity={`${activeIdentity}:build-candidate-family:${buildCandidatePageIndex}`}
          exportSetIdentity={`${activeIdentity}:build-candidate-family`}
          solutionOrdinalBase={(buildCandidatePageIndex * PRODUCT_MEMBER_PAGE_SIZE).toString()}
          bind:copyFormat={solutionCopyFormat}
          {targetLines}
          {language}
        />
      {/if}
    </section>
  {:else if buildSetupFamily}
    <section class="product-pager build-family" aria-label={componentMessage(language, 'buildSetupCandidates')}>
      <header>
        <div>
          <strong>{componentMessage(language, 'completeBuildSetupCandidateFamily')}</strong>
          <span>{componentMessage(language, 'thisIsAnOrdinaryCandidateFamilyWithout')}</span>
        </div>
        {#if buildCandidatePageCount > 1}
          <nav aria-label={componentMessage(language, 'buildSetupCandidateNavigation')}>
            <button type="button" disabled={buildCandidatePageIndex === 0} on:click={() => (buildCandidatePageIndex -= 1)}><ChevronLeft size={16} /></button>
            <span>{buildCandidatePageIndex + 1} / {buildCandidatePageCount}</span>
            <button type="button" disabled={buildCandidatePageIndex + 1 >= buildCandidatePageCount} on:click={() => (buildCandidatePageIndex += 1)}><ChevronRight size={16} /></button>
          </nav>
        {/if}
      </header>
      <div class="member-meta">
        <span>{componentMessage(language, 'objective')}: {objectiveLabel(buildSetupFamily.objective)}</span>
        <span>{componentMessage(language, 'reachableCandidates')}: {buildSetupFamily.reachable_candidate_count} / {buildSetupFamily.source_candidate_count}</span>
        <span>{componentMessage(language, 'unionProbability')}: {buildSetupFamily.union_probability}</span>
      </div>
      <SolutionSubsetPage
        solutionKeys={buildCandidateSolutionKeys}
        exportSolutionKeys={allBuildCandidateSolutionKeys}
        solutionCaptions={buildCandidateCaptions}
        solutionSetIdentity={`${activeIdentity}:build-setup-family:${buildCandidatePageIndex}`}
        exportSetIdentity={`${activeIdentity}:build-setup-family`}
        solutionOrdinalBase={(buildCandidatePageIndex * PRODUCT_MEMBER_PAGE_SIZE).toString()}
        bind:copyFormat={solutionCopyFormat}
        {targetLines}
        {language}
      />
    </section>
  {:else if setupRankedFamily}
    <section class="product-pager ordinary-family" aria-label={componentMessage(language, 'setupRankingResult')}>
      <header>
        <div>
          <strong>{componentMessage(language, 'setupRanking')}</strong>
          <span>{componentMessage(language, 'thisIsAnOrdinaryRankedFamilyAnd')}</span>
        </div>
        {#if setupRankedPageCount > 1}
          <nav aria-label={componentMessage(language, 'setupCandidateNavigation')}>
            <button type="button" disabled={setupRankedPageIndex === 0} on:click={() => (setupRankedPageIndex -= 1)}><ChevronLeft size={16} /></button>
            <span>{setupRankedPageIndex + 1} / {setupRankedPageCount}</span>
            <button type="button" disabled={setupRankedPageIndex + 1 >= setupRankedPageCount} on:click={() => (setupRankedPageIndex += 1)}><ChevronRight size={16} /></button>
          </nav>
        {/if}
      </header>
      <div class="member-meta">
        <span>{componentMessage(language, 'candidates')}: {setupRankedFamily.candidate_count}</span>
        <span>{componentMessage(language, 'lengthPreference')}: {lengthPreferenceLabel(setupRankedFamily.resolved_length_preference)}</span>
        <span>{componentMessage(language, 'rule')}: {setupRankedFamily.rule_profile}</span>
      </div>
      <ol start={setupRankedPageIndex * PRODUCT_MEMBER_PAGE_SIZE + 1}>
        {#each setupRankedCandidates as candidate, index (candidate.candidate_id)}
          <li><strong>{componentMessage(language, 'setup')} {setupRankedPageIndex * PRODUCT_MEMBER_PAGE_SIZE + index + 1}</strong></li>
        {/each}
      </ol>
    </section>
  {:else if setupScoreFamily}
    <section class="product-pager ordinary-family" aria-label={componentMessage(language, 'setupScoreRanking')}>
      <header>
        <div>
          <strong>{componentMessage(language, 'setupScoreRanking')}</strong>
          <span>{componentMessage(language, 'equalScoresRemainMembersOfTheOrdinary')}</span>
        </div>
        {#if setupScorePageCount > 1}
          <nav aria-label={componentMessage(language, 'setupScoreCandidateNavigation')}>
            <button type="button" disabled={setupScorePageIndex === 0} on:click={() => (setupScorePageIndex -= 1)}><ChevronLeft size={16} /></button>
            <span>{setupScorePageIndex + 1} / {setupScorePageCount}</span>
            <button type="button" disabled={setupScorePageIndex + 1 >= setupScorePageCount} on:click={() => (setupScorePageIndex += 1)}><ChevronRight size={16} /></button>
          </nav>
        {/if}
      </header>
      <div class="member-meta">
        <span>{componentMessage(language, 'candidates')}: {setupScoreFamily.candidate_count}</span>
        <span>{componentMessage(language, 'averagePriorityScore')}: {setupScoreFamily.average_priority_score}</span>
        <span>{componentMessage(language, 'scoreProfile')}: {setupScoreFamily.score_profile}</span>
        <span>{componentMessage(language, 'initialB2b')}: {setupScoreFamily.initial_b2b}</span>
      </div>
      <ol start={setupScorePageIndex * PRODUCT_MEMBER_PAGE_SIZE + 1}>
        {#each setupScoreCandidates as candidate (candidate.candidate_id)}
          <li class="score-row">
            <strong>{componentMessage(language, 'setup')} {candidate.rank}</strong>
            <span>{componentMessage(language, 'rank')} {candidate.rank} · {componentMessage(language, 'expectedScore')} {candidate.unconditional_expected_score} · {componentMessage(language, 'setupProbability')} {candidate.setup_covered_probability} · {componentMessage(language, 'continuationProbability')} {candidate.continuation_probability}</span>
          </li>
        {/each}
      </ol>
    </section>
  {:else if spinStructureFamily}
    <section class="product-pager ordinary-family" aria-label={componentMessage(language, 'spinStructureFamily')}>
      <header>
        <div>
          <strong>{componentMessage(language, 'spinStructureResults')}</strong>
          <span>{componentMessage(language, 'searchAndGuaranteedAreOrdinaryCompleteFamilies')}</span>
        </div>
        {#if spinStructurePageCount > 1}
          <nav aria-label={componentMessage(language, 'spinStructureCandidateNavigation')}>
            <button type="button" disabled={spinStructurePageIndex === 0} on:click={() => (spinStructurePageIndex -= 1)}><ChevronLeft size={16} /></button>
            <span>{spinStructurePageIndex + 1} / {spinStructurePageCount}</span>
            <button type="button" disabled={spinStructurePageIndex + 1 >= spinStructurePageCount} on:click={() => (spinStructurePageIndex += 1)}><ChevronRight size={16} /></button>
          </nav>
        {/if}
      </header>
      <div class="member-meta">
        <span>{componentMessage(language, 'candidates')}: {spinStructureFamily.candidate_count}</span>
        <span>{componentMessage(language, 'surfaceRegular')} {spinStructureFamily.regular_count}</span>
        <span>{componentMessage(language, 'surfaceMini')} {spinStructureFamily.mini_count}</span>
        <span>{componentMessage(language, 'minimumPlacements')}: {spinStructureFamily.minimum_placements ?? '—'}</span>
        {#if spinStructureFamily.guaranteed_final_piece}
          <span>{componentMessage(language, 'guaranteedFinalPiece')}: {spinStructureFamily.guaranteed_final_piece}</span>
        {/if}
        {#if spinStructureFamily.dependency_report_included}
          <span>{componentMessage(language, 'dependencyEdges')}: {spinStructureFamily.dependency_edge_count}</span>
        {/if}
      </div>
      <ol start={spinStructurePageIndex * PRODUCT_MEMBER_PAGE_SIZE + 1}>
        {#each spinStructureCandidates as candidate, index (candidate.candidate_id)}
          <li>
            <strong>{componentMessage(language, 'structure')} {spinStructurePageIndex * PRODUCT_MEMBER_PAGE_SIZE + index + 1}</strong>
            <span>{spinPartitionLabel(candidate.partition)} · {candidate.placement_count} {componentMessage(language, 'placements')}</span>
          </li>
        {/each}
      </ol>
    </section>
  {:else if (payload.content.payload_kind === 'pc-path-family' || payload.content.payload_kind === 'build-path-family') && pathFamily}
    <section class="product-pager path-family" aria-busy={navigatingOuter} aria-label={componentMessage(language, 'completeReplayPaths')}>
      <header>
        <div>
          <strong>{buildPathFamily ? (componentMessage(language, 'buildReplayPaths')) : (componentMessage(language, 'pcReplayPaths'))}</strong>
          <span>{componentMessage(language, 'everyPathCanBeCopiedOneRepresentative')}</span>
        </div>
        <nav aria-label={componentMessage(language, 'solutionReplayNavigation')}>
          <button type="button" disabled={navigatingOuter || BigInt(pathOrdinal) <= 1n} on:click={() => showReplayGeometry(-1)} aria-label={componentMessage(language, 'previousSolution')}><ChevronLeft size={16} /></button>
          <span>{pathOrdinal} / {pathPageCount}</span>
          <button type="button" disabled={navigatingOuter || BigInt(pathOrdinal) >= BigInt(pathPageCount)} on:click={() => showReplayGeometry(1)} aria-label={componentMessage(language, 'nextSolution')}><ChevronRight size={16} /></button>
        </nav>
      </header>
      <div class="member-meta">
        <span>{componentMessage(language, 'solutions2')}: {pathPageCount}</span>
        <span>{componentMessage(language, 'allPaths')}: {pathFamily.witness_count}</span>
        <span>{componentMessage(language, 'materializedPatterns')}: {pathFamily.materialized_pattern_count}</span>
      </div>
      {#if pathCandidateGroup}
        {@const witness = pathCandidateGroup.representative}
        <article class="path-representative">
          {#key witness.candidate_id + ':' + witness.normalized_trace_key}
            <PcPathReplayGif
              {language}
              {witness}
              {targetLines}
              expectedTerminalBoardMask={buildPathFamily ? witness.steps.at(-1)?.board_after_line_clear_mask ?? null : null}
              ariaLabel={buildPathFamily
                ? (componentMessage(language, 'buildReplay', { value0: pathPageIndex + 1 }))
                : (componentMessage(language, 'pcReplay', { value0: pathOrdinal }))}
              invalidLabel={invalidPreviewLabel}
            />
          {/key}
          <div class="path-evidence">
            <strong>{componentMessage(language, 'solution')} {pathOrdinal}</strong>
            <span>{componentMessage(language, 'distinctPatterns')}: {lazyReplayPage?.geometry_pattern_count ?? pathCandidateGroup.distinctPatternCount} / {pathFamily.materialized_pattern_count}</span>
            <span>{componentMessage(language, 'retainedPaths')}: {lazyReplayPage?.geometry_witness_count ?? pathCandidateGroup.witnessCount}</span>
            <span>{componentMessage(language, 'consumedPieces')}: {witness.consumed_piece_count} · {componentMessage(language, 'terminalHold')}: {witness.terminal_hold_piece ?? (componentMessage(language, 'none'))}</span>
            <SolutionCopyFormatControl
              bind:value={solutionCopyFormat}
              {language}
              compact
              loadPages={pathCandidateGroup ? loadVisiblePathPages : null}
            />
            <details>
              <summary>{componentMessage(language, 'inspectRepresentativeReplaySteps')} ({witness.steps.length})</summary>
              <ul>
                {#each witness.steps as step, index (step.step_index)}
                  <li><span>#{index + 1} · {step.active_piece} {rotationLabel(step.rotation)} ({step.x}, {step.y}) · {holdDecisionLabel(step.hold_decision)} · {componentMessage(language, 'cleared')} {step.cleared_lines}</span></li>
                {/each}
              </ul>
            </details>
          </div>
        </article>
      {/if}
    </section>
  {:else if payload.content.payload_kind === 'score-pattern-winner-family' && scoreFamily}
    <section class="product-pager score-family" aria-label={componentMessage(language, 'perPatternScoreWinners')}>
      <header>
        <div>
          <strong>{componentMessage(language, 'perPatternMaximumScoreSolutions')}</strong>
          <span>{componentMessage(language, 'attackIsInformationalAndIsNotUsed')}</span>
        </div>
        <nav aria-label={componentMessage(language, 'scoreWinnerPageNavigation')}>
          <button type="button" disabled={scorePageIndex === 0} on:click={() => (scorePageIndex -= 1)}><ChevronLeft size={16} /></button>
          <span>{scorePageIndex + 1} / {scorePageCount}</span>
          <button type="button" disabled={scorePageIndex + 1 >= scorePageCount} on:click={() => (scorePageIndex += 1)}><ChevronRight size={16} /></button>
        </nav>
      </header>
      <SolutionSubsetPage
        solutionKeys={scoreWinnerSolutionKeys}
        exportSolutionKeys={allScoreWinnerSolutionKeys}
        solutionCaptions={scoreWinnerCaptions}
        solutionSetIdentity={`${activeIdentity}:score-winner-family:${scorePageIndex}`}
        exportSetIdentity={`${activeIdentity}:score-winner-family`}
        solutionOrdinalBase={(scorePageIndex * PRODUCT_MEMBER_PAGE_SIZE).toString()}
        bind:copyFormat={solutionCopyFormat}
        {targetLines}
        {language}
      />
    </section>
  {/if}
{:else if error}
  <WorkspaceFailureNotice failures={pagerFailure?.publicFailures ?? []} {language} compact />
{/if}

<style>
  .product-pager { border: 1px solid #dce3df; border-radius: 8px; margin: 18px 0; overflow: hidden; }
  header, header > div, nav, .member-meta, footer, li { align-items: center; display: flex; }
  header { background: #f5f8f6; justify-content: space-between; padding: 13px 15px; }
  header > div { align-items: flex-start; flex-direction: column; gap: 3px; }
  header strong { color: #17211e; font-size: 13px; }
  header span, .member-meta, li span { color: #68736f; font-size: 11px; }
  nav { gap: 6px; }
  button { align-items: center; background: #fff; border: 1px solid #cfd8d3; border-radius: 5px; color: #35443f; display: inline-flex; gap: 5px; justify-content: center; min-height: 30px; padding: 5px 9px; }
  button:disabled { cursor: not-allowed; opacity: .45; }
  .member-meta { border-bottom: 1px solid #e3e8e5; gap: 18px; padding: 9px 15px; }
  ol { list-style-position: inside; margin: 0; max-height: 420px; overflow: auto; padding: 4px 15px; }
  li { border-bottom: 1px solid #edf0ee; gap: 12px; justify-content: space-between; min-height: 36px; padding: 5px 0; }
  li:last-child { border-bottom: 0; }
  footer { border-top: 1px solid #e3e8e5; justify-content: space-between; padding: 9px 15px; }
  .score-row { align-items: flex-start; flex-direction: column; gap: 3px; }
  .build-score-evidence { border-top: 1px solid #dce3df; }
  .path-representative { align-items: flex-start; display: flex; gap: 18px; padding: 15px; }
  .path-evidence { display: grid; flex: 1; gap: 6px; min-width: 0; }
  .path-evidence > strong { color: #23322d; font-size: 13px; }
  .path-evidence > span { color: #68736f; font-size: 11px; }
  details { color: #52615c; font-size: 11px; width: 100%; }
  summary { cursor: pointer; font-weight: 700; }
  details ul { list-style: none; margin-top: 5px; max-height: 180px; padding: 0 0 0 12px; }
  details li { justify-content: flex-start; min-height: 26px; }
  .pager-loading { align-items: center; background: #f5f8f6; border: 1px solid #dce3df; border-radius: 6px; color: #52615c; display: flex; font-size: 12px; gap: 8px; margin: 16px 0; padding: 12px; }
  :global(.spin) { animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 620px) { .path-representative { flex-direction: column; } }
</style>
