<script lang="ts">
  import { ChevronLeft, ChevronRight, LoaderCircle } from '@lucide/svelte';
  import { onDestroy } from 'svelte';

  import type {
    ClearraCoveragePortfolioRuntimePage,
    ClearraProductResultPayload
  } from '../wasm/wasmCommandClient';
  import PcPathReplayGif from './PcPathReplayGif.svelte';
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
  $: if (nextIdentity !== activeIdentity) resetForPayload(payload ?? null, nextIdentity);
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
  $: pathCandidateGroups = groupPcPathWitnesses(pathFamily?.witnesses ?? []);
  $: pathPageCount = pathCandidateGroups.length;
  $: pathCandidateGroup = pathCandidateGroups[pathPageIndex] ?? null;
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
    korean
      ? `결과 ${scorePageIndex * PRODUCT_MEMBER_PAGE_SIZE + index + 1} · 점수 ${winner.score} · 참고 공격력 ${winner.informational_attack}`
      : `Result ${scorePageIndex * PRODUCT_MEMBER_PAGE_SIZE + index + 1} · Score ${winner.score} · Informational attack ${winner.informational_attack}`
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
    korean
      ? `커버 패턴 ${candidate.covered_pattern_count}`
      : `Covered patterns ${candidate.covered_pattern_count}`
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
    korean
      ? `결과 ${buildScorePageIndex * PRODUCT_MEMBER_PAGE_SIZE + index + 1} · 점수 ${winner.score} · 참고 공격력 ${winner.informational_attack}`
      : `Result ${buildScorePageIndex * PRODUCT_MEMBER_PAGE_SIZE + index + 1} · Score ${winner.score} · Informational attack ${winner.informational_attack}`
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
  $: korean = language === 'ko';
  $: invalidPreviewLabel = korean
    ? '보드 미리보기를 생성할 수 없습니다.'
    : 'Board preview is unavailable.';
  $: scoreMinimalCoverage = payload?.contract === 'pc.score-minimals';
  $: scoreOnlyPortfolio = scoreMinimalCoverage || buildV2?.kind === 'score-portfolio';

  onDestroy(() => releaseHandle());

  function loadVisiblePathPages() {
    if (!pathCandidateGroup) return [];
    return pcPathCandidateGroupExportPages(
      pathCandidateGroup,
      targetLines,
      buildPathFamily?.target_terminal_board_mask ?? null
    );
  }

  function resetForPayload(
    nextPayload: ClearraProductResultPayload | null,
    identity: string
  ) {
    releaseHandle();
    activeIdentity = identity;
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
    const labels: Record<string, readonly [string, string]> = {
      all: ['All solutions', '전체 해법'],
      unique: ['Unique solutions', '중복 없는 해법'],
      'min-cover': ['Minimum solutions', '최소 해법'],
      'max-probability-minimum': ['Most probable minimum solutions', '확률이 가장 높은 최소 해법'],
      'max-score-cover': ['Highest-score minimum set', '최고 점수 최소 해법 집합']
    };
    const label = labels[value];
    return label ? label[korean ? 1 : 0] : (korean ? '선택한 목표' : 'Selected objective');
  }

  function lengthPreferenceLabel(value: 'longer' | 'shorter'): string {
    if (value === 'longer') return korean ? '긴 Setup 우선' : 'Longer setups first';
    return korean ? '짧은 Setup 우선' : 'Shorter setups first';
  }

  function spinPartitionLabel(value: 'regular' | 'mini'): string {
    if (value === 'regular') return korean ? 'Regular spin' : 'Regular spin';
    return korean ? 'Mini spin' : 'Mini spin';
  }

  function rotationLabel(value: string): string {
    const labels: Record<string, readonly [string, string]> = {
      '0': ['Spawn rotation', '기본 회전'],
      '1': ['Right rotation', '오른쪽 회전'],
      '2': ['Reverse rotation', '반대 회전'],
      '3': ['Left rotation', '왼쪽 회전']
    };
    const label = labels[value];
    return label ? label[korean ? 1 : 0] : (korean ? '회전' : 'Rotation');
  }

  function holdDecisionLabel(value: string): string {
    const labels: Record<string, readonly [string, string]> = {
      none: ['No hold', '홀드 없음'],
      store: ['Stored in hold', '홀드에 저장'],
      swap: ['Swapped hold', '홀드 교체']
    };
    const label = labels[value];
    return label ? label[korean ? 1 : 0] : (korean ? '홀드 사용' : 'Hold used');
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
    <section class="product-pager" aria-label={korean ? '최적 해법 페이지' : 'Optimal solution pages'}>
      <header>
        <div>
          <strong>{buildPortfolioActive
            ? (korean ? 'Build 최적 포트폴리오 전체' : 'All optimal Build portfolios')
            : scoreMinimalCoverage
              ? (korean ? '최고 점수 최소 해법 집합 전체' : 'All minimum maximum-score solution sets')
              : (korean ? '동일 최소 크기의 전체 해법' : 'All equal minimum-size solutions')}</strong>
          <span>{korean ? '해법' : 'Solution'} {coveragePage.alternative_index}{coveragePage.total_alternative_count ? ` / ${coveragePage.total_alternative_count}` : ''}</span>
          {#if scoreOnlyPortfolio}
            <span>{korean ? '점수만 동점·선정·정렬에 사용하며 공격력은 참고 정보입니다.' : 'Equality, membership, and ordering use score only; attack is informational.'}</span>
          {/if}
        </div>
        <nav aria-label={korean ? '해법 페이지 이동' : 'Solution page navigation'}>
          <button type="button" disabled={loadingMember || navigatingOuter || !previousOuterAvailable} on:click={previousOuterPage} aria-label={korean ? '이전 해법' : 'Previous solution'}><ChevronLeft size={16} /></button>
          <button type="button" disabled={loadingMember || navigatingOuter || !nextOuterAvailable} on:click={nextOuterPage} aria-label={korean ? '다음 해법' : 'Next solution'}>{#if prefetchInFlight && outerPageIndex + 1 >= coveragePages.length}<LoaderCircle class="spin" size={16} />{:else}<ChevronRight size={16} />{/if}</button>
        </nav>
      </header>
      <div class="member-meta">
        <span>{korean ? '최소 해법 크기' : 'Minimum cardinality'}: {coveragePage.optimal_cardinality}</span>
        <span>{korean ? '구성원 페이지' : 'Member page'}: {memberPageNumber} / {coveragePage.total_member_pages}</span>
      </div>
      {#if coveragePage.optimal_cardinality === '0'}
        <p class="empty-result" role="status">
          {korean
            ? '요청한 패턴 집합에서 필요한 해법이 없습니다. 탐색은 정상적으로 완료되었습니다.'
            : 'No solution is required for this pattern set. The search completed successfully.'}
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
        <button type="button" disabled={loadingMember || navigatingOuter || memberPageNumber === '1'} on:click={() => showMemberPage(decrementCanonicalDecimal(memberPageNumber))}><ChevronLeft size={15} />{korean ? '이전 100개' : 'Previous 100'}</button>
        <button type="button" disabled={loadingMember || navigatingOuter || compareCanonicalDecimals(memberPageNumber, coveragePage.total_member_pages) >= 0} on:click={() => showMemberPage(incrementCanonicalDecimal(memberPageNumber))}>{korean ? '다음 100개' : 'Next 100'}<ChevronRight size={15} /></button>
      </footer>
      {#if buildV2?.kind === 'score-portfolio'}
        <div class="build-score-evidence">
          <div class="member-meta">
            <span>{korean ? '점수 프로필' : 'Score profile'}: {buildV2.score_profile}</span>
            <span>{korean ? '초기 B2B' : 'Initial B2B'}: {buildV2.initial_b2b}</span>
            <span>{korean ? '점수 증거 페이지' : 'Score evidence page'}: {buildScorePageIndex + 1} / {buildScorePageCount}</span>
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
            <button type="button" disabled={buildScorePageIndex === 0} on:click={() => (buildScorePageIndex -= 1)}><ChevronLeft size={15} />{korean ? '이전 점수 증거' : 'Previous score evidence'}</button>
            <button type="button" disabled={buildScorePageIndex + 1 >= buildScorePageCount} on:click={() => (buildScorePageIndex += 1)}>{korean ? '다음 점수 증거' : 'Next score evidence'}<ChevronRight size={15} /></button>
          </footer>
        </div>
      {/if}
    </section>
  {:else if buildPortfolioActive && loadingMember}
    <p class="pager-loading" role="status"><LoaderCircle class="spin" size={16} />{korean ? '첫 Build 포트폴리오 페이지를 불러오는 중입니다.' : 'Loading the first Build portfolio page.'}</p>
  {:else if buildV2 && (buildV2.kind === 'candidate-family' || buildV2.kind === 'probability')}
    <section class="product-pager build-family" aria-label={korean ? 'Build 결과' : 'Build result'}>
      <header>
        <div>
          <strong>{buildV2.kind === 'probability'
            ? (korean ? 'Build 구축 확률' : 'Build probability')
            : (korean ? 'Build 해법' : 'Build solutions')}</strong>
          <span>{korean ? '일반 결과 family이며 포트폴리오 동점이 아닙니다.' : 'This is an ordinary result family, not a portfolio tie.'}</span>
        </div>
        {#if buildV2.kind === 'candidate-family' && buildCandidatePageCount > 1}
          <nav aria-label={korean ? 'Build 후보 페이지 이동' : 'Build candidate navigation'}>
            <button type="button" disabled={buildCandidatePageIndex === 0} on:click={() => (buildCandidatePageIndex -= 1)}><ChevronLeft size={16} /></button>
            <span>{buildCandidatePageIndex + 1} / {buildCandidatePageCount}</span>
            <button type="button" disabled={buildCandidatePageIndex + 1 >= buildCandidatePageCount} on:click={() => (buildCandidatePageIndex += 1)}><ChevronRight size={16} /></button>
          </nav>
        {/if}
      </header>
      <div class="member-meta">
        <span>{korean ? '목표' : 'Objective'}: {objectiveLabel(buildV2.objective)}</span>
        <span>{korean ? '도달 후보' : 'Reachable candidates'}: {buildV2.reachable_candidate_count} / {buildV2.source_candidate_count}</span>
        <span>{korean ? '커버 패턴' : 'Covered patterns'}: {buildV2.covered_pattern_count} / {buildV2.pattern_count}</span>
        <span>{korean ? '합집합 확률' : 'Union probability'}: {buildV2.union_probability}</span>
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
    <section class="product-pager build-family" aria-label={korean ? 'Build setup 후보 결과' : 'Build setup candidates'}>
      <header>
        <div>
          <strong>{korean ? 'Build setup 후보 전체' : 'Complete Build setup candidate family'}</strong>
          <span>{korean ? '동점 메타데이터가 없는 정상 후보 family입니다.' : 'This is an ordinary candidate family without tie metadata.'}</span>
        </div>
        {#if buildCandidatePageCount > 1}
          <nav aria-label={korean ? 'Build setup 후보 페이지 이동' : 'Build setup candidate navigation'}>
            <button type="button" disabled={buildCandidatePageIndex === 0} on:click={() => (buildCandidatePageIndex -= 1)}><ChevronLeft size={16} /></button>
            <span>{buildCandidatePageIndex + 1} / {buildCandidatePageCount}</span>
            <button type="button" disabled={buildCandidatePageIndex + 1 >= buildCandidatePageCount} on:click={() => (buildCandidatePageIndex += 1)}><ChevronRight size={16} /></button>
          </nav>
        {/if}
      </header>
      <div class="member-meta">
        <span>{korean ? '목표' : 'Objective'}: {objectiveLabel(buildSetupFamily.objective)}</span>
        <span>{korean ? '도달 후보' : 'Reachable candidates'}: {buildSetupFamily.reachable_candidate_count} / {buildSetupFamily.source_candidate_count}</span>
        <span>{korean ? '합집합 확률' : 'Union probability'}: {buildSetupFamily.union_probability}</span>
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
    <section class="product-pager ordinary-family" aria-label={korean ? 'Setup 순위 결과' : 'Setup ranking result'}>
      <header>
        <div>
          <strong>{korean ? 'Setup 순위' : 'Setup ranking'}</strong>
          <span>{korean ? '일반 순위 family이며 동점 포트폴리오로 재분류하지 않습니다.' : 'This is an ordinary ranked family and is not reclassified as a tie portfolio.'}</span>
        </div>
        {#if setupRankedPageCount > 1}
          <nav aria-label={korean ? 'Setup 후보 페이지 이동' : 'Setup candidate navigation'}>
            <button type="button" disabled={setupRankedPageIndex === 0} on:click={() => (setupRankedPageIndex -= 1)}><ChevronLeft size={16} /></button>
            <span>{setupRankedPageIndex + 1} / {setupRankedPageCount}</span>
            <button type="button" disabled={setupRankedPageIndex + 1 >= setupRankedPageCount} on:click={() => (setupRankedPageIndex += 1)}><ChevronRight size={16} /></button>
          </nav>
        {/if}
      </header>
      <div class="member-meta">
        <span>{korean ? '후보' : 'Candidates'}: {setupRankedFamily.candidate_count}</span>
        <span>{korean ? '길이 선호' : 'Length preference'}: {lengthPreferenceLabel(setupRankedFamily.resolved_length_preference)}</span>
        <span>{korean ? '규칙' : 'Rule'}: {setupRankedFamily.rule_profile}</span>
      </div>
      <ol start={setupRankedPageIndex * PRODUCT_MEMBER_PAGE_SIZE + 1}>
        {#each setupRankedCandidates as candidate, index (candidate.candidate_id)}
          <li><strong>{korean ? '셋업' : 'Setup'} {setupRankedPageIndex * PRODUCT_MEMBER_PAGE_SIZE + index + 1}</strong></li>
        {/each}
      </ol>
    </section>
  {:else if setupScoreFamily}
    <section class="product-pager ordinary-family" aria-label={korean ? 'Setup 점수 순위' : 'Setup score ranking'}>
      <header>
        <div>
          <strong>{korean ? 'Setup 점수 순위' : 'Setup score ranking'}</strong>
          <span>{korean ? '동일 score도 일반 순위 family의 구성원입니다. Attack은 동점 판정에 혼합하지 않습니다.' : 'Equal scores remain members of the ordinary ranking family. Attack is not mixed into equality.'}</span>
        </div>
        {#if setupScorePageCount > 1}
          <nav aria-label={korean ? 'Setup 점수 후보 페이지 이동' : 'Setup score candidate navigation'}>
            <button type="button" disabled={setupScorePageIndex === 0} on:click={() => (setupScorePageIndex -= 1)}><ChevronLeft size={16} /></button>
            <span>{setupScorePageIndex + 1} / {setupScorePageCount}</span>
            <button type="button" disabled={setupScorePageIndex + 1 >= setupScorePageCount} on:click={() => (setupScorePageIndex += 1)}><ChevronRight size={16} /></button>
          </nav>
        {/if}
      </header>
      <div class="member-meta">
        <span>{korean ? '후보' : 'Candidates'}: {setupScoreFamily.candidate_count}</span>
        <span>{korean ? '평균 우선순위 점수' : 'Average priority score'}: {setupScoreFamily.average_priority_score}</span>
        <span>{korean ? '점수 프로필' : 'Score profile'}: {setupScoreFamily.score_profile}</span>
        <span>{korean ? '초기 B2B' : 'Initial B2B'}: {setupScoreFamily.initial_b2b}</span>
      </div>
      <ol start={setupScorePageIndex * PRODUCT_MEMBER_PAGE_SIZE + 1}>
        {#each setupScoreCandidates as candidate (candidate.candidate_id)}
          <li class="score-row">
            <strong>{korean ? '셋업' : 'Setup'} {candidate.rank}</strong>
            <span>{korean ? '순위' : 'Rank'} {candidate.rank} · {korean ? '기대 점수' : 'Expected score'} {candidate.unconditional_expected_score} · {korean ? 'Setup 확률' : 'Setup probability'} {candidate.setup_covered_probability} · {korean ? '연속 성공 확률' : 'Continuation probability'} {candidate.continuation_probability}</span>
          </li>
        {/each}
      </ol>
    </section>
  {:else if spinStructureFamily}
    <section class="product-pager ordinary-family" aria-label={korean ? 'Spin 구조 family' : 'Spin structure family'}>
      <header>
        <div>
          <strong>{korean ? 'Spin 구조 결과' : 'Spin structure results'}</strong>
          <span>{korean ? 'Search와 guaranteed는 일반 완전 family이며 cover 동점 포트폴리오가 아닙니다.' : 'Search and guaranteed are ordinary complete families, not cover tie portfolios.'}</span>
        </div>
        {#if spinStructurePageCount > 1}
          <nav aria-label={korean ? 'Spin 구조 후보 페이지 이동' : 'Spin structure candidate navigation'}>
            <button type="button" disabled={spinStructurePageIndex === 0} on:click={() => (spinStructurePageIndex -= 1)}><ChevronLeft size={16} /></button>
            <span>{spinStructurePageIndex + 1} / {spinStructurePageCount}</span>
            <button type="button" disabled={spinStructurePageIndex + 1 >= spinStructurePageCount} on:click={() => (spinStructurePageIndex += 1)}><ChevronRight size={16} /></button>
          </nav>
        {/if}
      </header>
      <div class="member-meta">
        <span>{korean ? '전체' : 'Candidates'}: {spinStructureFamily.candidate_count}</span>
        <span>Regular: {spinStructureFamily.regular_count}</span>
        <span>Mini: {spinStructureFamily.mini_count}</span>
        <span>{korean ? '최소 배치' : 'Minimum placements'}: {spinStructureFamily.minimum_placements ?? '—'}</span>
        {#if spinStructureFamily.guaranteed_final_piece}
          <span>{korean ? '보장 마지막 조각' : 'Guaranteed final piece'}: {spinStructureFamily.guaranteed_final_piece}</span>
        {/if}
        {#if spinStructureFamily.dependency_report_included}
          <span>{korean ? '의존 간선' : 'Dependency edges'}: {spinStructureFamily.dependency_edge_count}</span>
        {/if}
      </div>
      <ol start={spinStructurePageIndex * PRODUCT_MEMBER_PAGE_SIZE + 1}>
        {#each spinStructureCandidates as candidate, index (candidate.candidate_id)}
          <li>
            <strong>{korean ? '구조' : 'Structure'} {spinStructurePageIndex * PRODUCT_MEMBER_PAGE_SIZE + index + 1}</strong>
            <span>{spinPartitionLabel(candidate.partition)} · {candidate.placement_count} {korean ? '배치' : 'placements'}</span>
          </li>
        {/each}
      </ol>
    </section>
  {:else if (payload.content.payload_kind === 'pc-path-family' || payload.content.payload_kind === 'build-path-family') && pathFamily}
    <section class="product-pager path-family" aria-label={korean ? '전체 리플레이 경로' : 'Complete replay paths'}>
      <header>
        <div>
          <strong>{buildPathFamily ? (korean ? '구축 리플레이 경로' : 'Build replay paths') : (korean ? 'PC 리플레이 경로' : 'PC replay paths')}</strong>
          <span>{korean ? '전체 경로는 복사할 수 있으며, 같은 해법의 대표 리플레이 하나씩 표시합니다.' : 'Every path can be copied; one representative replay is shown for each solution.'}</span>
        </div>
        <nav aria-label={korean ? '해법별 리플레이 이동' : 'Solution replay navigation'}>
          <button type="button" disabled={pathPageIndex === 0} on:click={() => (pathPageIndex -= 1)} aria-label={korean ? '이전 해법' : 'Previous solution'}><ChevronLeft size={16} /></button>
          <span>{pathPageCount === 0 ? 0 : pathPageIndex + 1} / {pathPageCount}</span>
          <button type="button" disabled={pathPageIndex + 1 >= pathPageCount} on:click={() => (pathPageIndex += 1)} aria-label={korean ? '다음 해법' : 'Next solution'}><ChevronRight size={16} /></button>
        </nav>
      </header>
      <div class="member-meta">
        <span>{korean ? '해법' : 'Solutions'}: {pathCandidateGroups.length}</span>
        <span>{korean ? '전체 경로' : 'All paths'}: {pathFamily.witness_count}</span>
        <span>{korean ? '구체화 패턴' : 'Materialized patterns'}: {pathFamily.materialized_pattern_count}</span>
      </div>
      {#if pathCandidateGroup}
        {@const witness = pathCandidateGroup.representative}
        <article class="path-representative">
          {#key witness.candidate_id + ':' + witness.normalized_trace_key}
            <PcPathReplayGif
              {witness}
              {targetLines}
              expectedTerminalBoardMask={buildPathFamily?.target_terminal_board_mask ?? null}
              ariaLabel={buildPathFamily
                ? (korean ? `구축 리플레이 ${pathPageIndex + 1}` : `Build replay ${pathPageIndex + 1}`)
                : (korean ? `PC 리플레이 ${pathPageIndex + 1}` : `PC replay ${pathPageIndex + 1}`)}
              invalidLabel={invalidPreviewLabel}
            />
          {/key}
          <div class="path-evidence">
            <strong>{korean ? '해법' : 'Solution'} {pathPageIndex + 1}</strong>
            <span>{korean ? '서로 다른 패턴' : 'Distinct patterns'}: {pathCandidateGroup.distinctPatternCount} / {pathFamily.materialized_pattern_count}</span>
            <span>{korean ? '보존된 경로' : 'Retained paths'}: {pathCandidateGroup.witnessCount}</span>
            <span>{korean ? '소비 조각' : 'Consumed pieces'}: {witness.consumed_piece_count} · {korean ? '최종 홀드' : 'Terminal hold'}: {witness.terminal_hold_piece ?? (korean ? '없음' : 'None')}</span>
            <SolutionCopyFormatControl
              bind:value={solutionCopyFormat}
              {language}
              compact
              loadPages={pathCandidateGroup ? loadVisiblePathPages : null}
            />
            <details>
              <summary>{korean ? '대표 경로 단계 확인' : 'Inspect representative replay steps'} ({witness.steps.length})</summary>
              <ul>
                {#each witness.steps as step, index (step.step_index)}
                  <li><span>#{index + 1} · {step.active_piece} {rotationLabel(step.rotation)} ({step.x}, {step.y}) · {holdDecisionLabel(step.hold_decision)} · {korean ? '클리어' : 'Cleared'} {step.cleared_lines}</span></li>
                {/each}
              </ul>
            </details>
          </div>
        </article>
      {/if}
    </section>
  {:else if payload.content.payload_kind === 'score-pattern-winner-family' && scoreFamily}
    <section class="product-pager score-family" aria-label={korean ? '패턴별 최고 점수 해법' : 'Per-pattern score winners'}>
      <header>
        <div>
          <strong>{korean ? '패턴별 최고 점수 해법' : 'Per-pattern maximum-score solutions'}</strong>
          <span>{korean ? '공격력은 동점 판정과 정렬에 사용하지 않습니다.' : 'Attack is informational and is not used for equality or ordering.'}</span>
        </div>
        <nav aria-label={korean ? '점수 해법 페이지 이동' : 'Score winner page navigation'}>
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
