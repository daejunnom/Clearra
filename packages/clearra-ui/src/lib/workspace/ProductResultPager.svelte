<script lang="ts">
  import { ChevronLeft, ChevronRight, LoaderCircle } from '@lucide/svelte';
  import { onDestroy } from 'svelte';

  import type {
    ClearraCoveragePortfolioRuntimePage,
    ClearraProductResultPayload
  } from '../wasm/wasmCommandClient';
  import {
    CoveragePortfolioPagerController,
    PRODUCT_MEMBER_PAGE_SIZE,
    compareCanonicalDecimals,
    coveragePortfolioPageReference,
    decrementCanonicalDecimal,
    incrementCanonicalDecimal,
    productResultIdentity,
    requireCoveragePortfolioPageResponse,
    validateProductResultPayload,
    type CoveragePortfolioPagerSnapshot,
    type ProductMemberPageLoader,
    type ProductNextPageLoader,
    type ProductPageRelease
  } from './productResultPager';
  import type { WorkspaceLanguage } from './workspaceI18n';

  const MAX_RETAINED_MEMBER_PAGES = 3;

  export let payload: ClearraProductResultPayload | null | undefined = null;
  export let language: WorkspaceLanguage = 'en';
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
  let savePageIndex = 0;
  let buildCandidatePageIndex = 0;
  let buildScorePageIndex = 0;
  let setupRankedPageIndex = 0;
  let setupScorePageIndex = 0;
  let spinStructurePageIndex = 0;
  let outerPager: CoveragePortfolioPagerController | null = null;
  const memberCache = new Map<string, ClearraCoveragePortfolioRuntimePage['members']>();

  $: nextIdentity = productResultIdentity(payload);
  $: if (nextIdentity !== activeIdentity) resetForPayload(payload ?? null, nextIdentity);
  $: coveragePage = coveragePages[outerPageIndex] ?? null;
  $: currentAlternativeIndex = coveragePage?.alternative_index ?? null;
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
  $: pathFamily = payload?.content.payload_kind === 'pc-path-family'
    ? payload.content.payload
    : null;
  $: pathPageCount = pathFamily
    ? Math.max(1, Math.ceil(pathFamily.witnesses.length / PRODUCT_MEMBER_PAGE_SIZE))
    : 0;
  $: pathWitnesses = pathFamily
    ? pathFamily.witnesses.slice(
        pathPageIndex * PRODUCT_MEMBER_PAGE_SIZE,
        (pathPageIndex + 1) * PRODUCT_MEMBER_PAGE_SIZE
      )
    : [];
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
  $: saveFamily = payload?.content.payload_kind === 'pc-save-groups'
    ? payload.content.payload
    : null;
  $: bestSaveFamily = payload?.content.payload_kind === 'pc-best-save'
    ? payload.content.payload
    : null;
  $: saveItemCount = saveFamily?.groups.length ?? bestSaveFamily?.winners.length ?? 0;
  $: savePageCount = saveFamily || bestSaveFamily
    ? Math.max(1, Math.ceil(saveItemCount / PRODUCT_MEMBER_PAGE_SIZE))
    : 0;
  $: saveGroups = saveFamily
    ? saveFamily.groups.slice(
        savePageIndex * PRODUCT_MEMBER_PAGE_SIZE,
        (savePageIndex + 1) * PRODUCT_MEMBER_PAGE_SIZE
      )
    : [];
  $: bestSaveWinners = bestSaveFamily
    ? bestSaveFamily.winners.slice(
        savePageIndex * PRODUCT_MEMBER_PAGE_SIZE,
        (savePageIndex + 1) * PRODUCT_MEMBER_PAGE_SIZE
      )
    : [];
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
  $: buildScorePageCount = buildV2?.kind === 'score-portfolio'
    ? Math.max(1, Math.ceil(buildV2.winners.length / PRODUCT_MEMBER_PAGE_SIZE))
    : 0;
  $: buildScoreWinners = buildV2?.kind === 'score-portfolio'
    ? buildV2.winners.slice(
        buildScorePageIndex * PRODUCT_MEMBER_PAGE_SIZE,
        (buildScorePageIndex + 1) * PRODUCT_MEMBER_PAGE_SIZE
      )
    : [];
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
  $: scoreMinimalCoverage = payload?.contract === 'pc.score-minimals';
  $: scoreOnlyPortfolio = scoreMinimalCoverage || buildV2?.kind === 'score-portfolio';

  onDestroy(() => releaseHandle());

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
    savePageIndex = 0;
    buildCandidatePageIndex = 0;
    buildScorePageIndex = 0;
    setupRankedPageIndex = 0;
    setupScorePageIndex = 0;
    spinStructurePageIndex = 0;
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
      initializeOuterPager(identity, canonical, page_handle_available && Boolean(loadNextPage));
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
    coveragePages = [...snapshot.pages];
    outerPageIndex = snapshot.outerPageIndex;
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
      const response = await loadMemberPage?.('1', '1', signal);
      if (!response) {
        throw new Error('Build portfolio page does not match the active result');
      }
      const initialPage = requireCoveragePortfolioPageResponse(response, {
        setIdentitySha256: pageSourceIdentity,
        alternativeIndex: '1',
        memberPageNumber: '1'
      });
      if (signal.aborted || activeIdentity !== payloadIdentity) return;
      currentMembers = initialPage.members;
      memberCache.set(`${initialPage.alternative_index}:1`, initialPage.members);
      initializeOuterPager(payloadIdentity, initialPage, !initialPage.enumeration_complete);
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
      const response = await loadMemberPage(
        alternativeIndex,
        nextMemberPage,
        requestSignal
      );
      if (requestSignal?.aborted || activeIdentity !== payloadIdentity) return;
      const loadedPage = requireCoveragePortfolioPageResponse(response, {
        setIdentitySha256: referencePage.set_identity_sha256,
        candidateMapSha256: referencePage.candidate_map_sha256,
        alternativeIndex,
        memberPageNumber: nextMemberPage,
        referencePage,
        requireSameAlternativeMetadata: true
      });
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
</script>

{#if payload && !error}
  {#if (payload.content.payload_kind === 'coverage-portfolio' || buildPortfolioActive) && coveragePage}
    <section class="product-pager" aria-label={korean ? '최적 해법 페이지' : 'Optimal solution pages'}>
      <header>
        <div>
          <strong>{buildPortfolioActive
            ? (korean ? 'Build 최적 포트폴리오 전체' : 'All optimal Build portfolios')
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
      <ol>
        {#each currentMembers as member (member.candidate_id)}
          <li><code>{member.normalized_solution_key}</code><span>ID {member.candidate_id}</span></li>
        {/each}
      </ol>
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
          <ol start={buildScorePageIndex * PRODUCT_MEMBER_PAGE_SIZE + 1}>
            {#each buildScoreWinners as winner (winner.pattern_id)}
              <li class="score-row"><code>{winner.candidate_key}</code><span>{korean ? '패턴' : 'Pattern'} {winner.pattern_id} · {korean ? '점수' : 'Score'} {winner.score} · {korean ? '참고 공격력' : 'Informational attack'} {winner.informational_attack}</span></li>
            {/each}
          </ol>
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
          <strong>{buildV2.capability_id}</strong>
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
        <span>{korean ? '목표' : 'Objective'}: {buildV2.objective}</span>
        <span>{korean ? '도달 후보' : 'Reachable candidates'}: {buildV2.reachable_candidate_count} / {buildV2.source_candidate_count}</span>
        <span>{korean ? '커버 패턴' : 'Covered patterns'}: {buildV2.covered_pattern_count} / {buildV2.pattern_count}</span>
        <span>{korean ? '합집합 확률' : 'Union probability'}: {buildV2.union_probability}</span>
      </div>
      {#if buildV2.kind === 'candidate-family'}
        <ol start={buildCandidatePageIndex * PRODUCT_MEMBER_PAGE_SIZE + 1}>
          {#each buildCandidatePage as candidate (candidate.candidate_key)}
            <li><code>{candidate.candidate_key}</code><span>{korean ? '커버 패턴' : 'Covered patterns'} {candidate.covered_pattern_count}</span></li>
          {/each}
        </ol>
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
        <span>{korean ? '목표' : 'Objective'}: {buildSetupFamily.objective}</span>
        <span>{korean ? '도달 후보' : 'Reachable candidates'}: {buildSetupFamily.reachable_candidate_count} / {buildSetupFamily.source_candidate_count}</span>
        <span>{korean ? '합집합 확률' : 'Union probability'}: {buildSetupFamily.union_probability}</span>
      </div>
      <ol start={buildCandidatePageIndex * PRODUCT_MEMBER_PAGE_SIZE + 1}>
        {#each buildCandidatePage as candidate (candidate.candidate_key)}
          <li><code>{candidate.candidate_key}</code><span>{korean ? '커버 패턴' : 'Covered patterns'} {candidate.covered_pattern_count}</span></li>
        {/each}
      </ol>
    </section>
  {:else if setupRankedFamily}
    <section class="product-pager ordinary-family" aria-label={korean ? 'Setup 순위 결과' : 'Setup ranking result'}>
      <header>
        <div>
          <strong>{payload.contract} · {setupRankedFamily.ordering}</strong>
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
        <span>{korean ? '길이 선호' : 'Length preference'}: {setupRankedFamily.resolved_length_preference}</span>
        <span>{korean ? '규칙' : 'Rule'}: {setupRankedFamily.rule_profile}</span>
      </div>
      <ol start={setupRankedPageIndex * PRODUCT_MEMBER_PAGE_SIZE + 1}>
        {#each setupRankedCandidates as candidate (candidate.candidate_id)}
          <li><code>{candidate.setup_id}</code><span>{candidate.condition_id} · ID {candidate.candidate_id}</span></li>
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
            <code>{candidate.candidate_id}</code>
            <span>{korean ? '순위' : 'Rank'} {candidate.rank} · {korean ? '기대 점수' : 'Expected score'} {candidate.unconditional_expected_score} · {korean ? 'Setup 확률' : 'Setup probability'} {candidate.setup_covered_probability} · {korean ? '연속 성공 확률' : 'Continuation probability'} {candidate.continuation_probability}</span>
          </li>
        {/each}
      </ol>
    </section>
  {:else if spinStructureFamily}
    <section class="product-pager ordinary-family" aria-label={korean ? 'Spin 구조 family' : 'Spin structure family'}>
      <header>
        <div>
          <strong>{spinStructureFamily.schema_id}</strong>
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
        {#each spinStructureCandidates as candidate (candidate.candidate_id)}
          <li><code>{candidate.candidate_id}</code><span>{candidate.partition} · {candidate.placement_count} {korean ? '배치' : 'placements'}</span></li>
        {/each}
      </ol>
    </section>
  {:else if payload.content.payload_kind === 'pc-path-family' && pathFamily}
    <section class="product-pager path-family" aria-label={korean ? '전체 PC 리플레이 경로' : 'Complete PC replay paths'}>
      <header>
        <div>
          <strong>{korean ? '전체 PC 리플레이 경로' : 'Complete PC replay family'}</strong>
          <span>{korean ? '포트폴리오 동점이 아닌 완전한 일반 해법 family입니다.' : 'This is a complete ordinary solution family, not a portfolio tie.'}</span>
        </div>
        <nav aria-label={korean ? '리플레이 경로 페이지 이동' : 'Replay path page navigation'}>
          <button type="button" disabled={pathPageIndex === 0} on:click={() => (pathPageIndex -= 1)} aria-label={korean ? '이전 경로 100개' : 'Previous 100 paths'}><ChevronLeft size={16} /></button>
          <span>{pathPageIndex + 1} / {pathPageCount}</span>
          <button type="button" disabled={pathPageIndex + 1 >= pathPageCount} on:click={() => (pathPageIndex += 1)} aria-label={korean ? '다음 경로 100개' : 'Next 100 paths'}><ChevronRight size={16} /></button>
        </nav>
      </header>
      <div class="member-meta">
        <span>{korean ? '경로' : 'Paths'}: {pathFamily.witness_count}</span>
        <span>{korean ? '구체화 패턴' : 'Materialized patterns'}: {pathFamily.materialized_pattern_count}</span>
        <span>{korean ? '문제' : 'Problem'}: {pathFamily.problem_id}</span>
      </div>
      <ol start={pathPageIndex * PRODUCT_MEMBER_PAGE_SIZE + 1}>
        {#each pathWitnesses as witness (witness.candidate_id + ':' + witness.pattern_id + ':' + witness.normalized_trace_key)}
          <li class="path-row">
            <code>{witness.normalized_trace_key}</code>
            <span>ID {witness.candidate_id} · {korean ? '패턴' : 'Pattern'} {witness.pattern_id} · {korean ? '소비 조각' : 'Consumed pieces'} {witness.consumed_piece_count} · {korean ? '최종 홀드' : 'Terminal hold'} {witness.terminal_hold_piece ?? 'empty'}</span>
            <details>
              <summary>{korean ? '전체 리플레이 단계 확인' : 'Inspect every replay step'} ({witness.steps.length})</summary>
              <ul>
                {#each witness.steps as step (step.step_index)}
                  <li><span>#{step.step_index} · {step.active_piece} {step.rotation} ({step.x}, {step.y}) · {step.hold_decision} · {korean ? '클리어' : 'Cleared'} {step.cleared_lines} · {step.line_clear_identity}</span></li>
                {/each}
              </ul>
            </details>
          </li>
        {/each}
      </ol>
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
      <ol start={scorePageIndex * PRODUCT_MEMBER_PAGE_SIZE + 1}>
        {#each scoreWinners as winner (winner.pattern_id + ':' + winner.candidate_id)}
          <li class="score-row"><code>{winner.normalized_solution_key}</code><span>{korean ? '패턴' : 'Pattern'} {winner.pattern_id} · ID {winner.candidate_id} · {korean ? '점수' : 'Score'} {winner.score} · {korean ? '참고 공격력' : 'Informational attack'} {winner.informational_attack}</span></li>
        {/each}
      </ol>
    </section>
  {:else if payload.content.payload_kind === 'pc-save-groups' && saveFamily}
    <section class="product-pager save-family" aria-label={korean ? '세이브 그룹 전체 결과' : 'Complete save groups'}>
      <header>
        <div>
          <strong>{korean ? '세이브 그룹 전체' : 'All save groups'}</strong>
          <span>{korean ? '전체 우주 확률과 PC 성공 조건부 확률은 서로 다른 값입니다.' : 'Whole-universe and conditional-on-PC probabilities are distinct.'}</span>
        </div>
        <nav aria-label={korean ? '세이브 그룹 페이지 이동' : 'Save group page navigation'}>
          <button type="button" disabled={savePageIndex === 0} on:click={() => (savePageIndex -= 1)} aria-label={korean ? '이전 세이브 그룹 100개' : 'Previous 100 save groups'}><ChevronLeft size={16} /></button>
          <span>{savePageIndex + 1} / {savePageCount}</span>
          <button type="button" disabled={savePageIndex + 1 >= savePageCount} on:click={() => (savePageIndex += 1)} aria-label={korean ? '다음 세이브 그룹 100개' : 'Next 100 save groups'}><ChevronRight size={16} /></button>
        </nav>
      </header>
      <div class="member-meta">
        <span>{korean ? '그룹' : 'Groups'}: {saveFamily.group_count}</span>
        <span>{korean ? '전체 PC 확률' : 'Overall PC probability'}: {saveFamily.metadata.pc_probability}</span>
      </div>
      <ol start={savePageIndex * PRODUCT_MEMBER_PAGE_SIZE + 1}>
        {#each saveGroups as group (group.identity.canonical_id)}
          <li class="save-row">
            <code>{group.identity.canonical_id}</code>
            <span>{korean ? '전체 우주 확률' : 'Whole-universe probability'} {group.unconditional_probability} · {korean ? 'PC 조건부 확률' : 'Conditional on PC'} {group.conditional_probability_given_pc} · ID {group.canonical_candidate_id} · {korean ? '성공 패턴' : 'Successful patterns'} {group.successful_pattern_count}</span>
            <details>
              <summary>{korean ? '전체 증거 확인' : 'Inspect every witness'} ({group.witnesses.length})</summary>
              <ul>
                {#each group.witnesses as witness (witness.pattern_index)}
                  <li><span>{korean ? '패턴' : 'Pattern'} {witness.pattern_index} · ID {witness.candidate_id} · {korean ? '추적' : 'Trace'} {witness.trace_identity} · {korean ? '홀드' : 'Hold'} {witness.terminal_hold ?? 'empty'} · {witness.active_bag_remainder.canonical_id}</span></li>
                {/each}
              </ul>
            </details>
          </li>
        {/each}
      </ol>
    </section>
  {:else if payload.content.payload_kind === 'pc-best-save' && bestSaveFamily}
    <section class="product-pager save-family" aria-label={korean ? '최고 세이브 동점 전체 결과' : 'Complete best-save ties'}>
      <header>
        <div>
          <strong>{korean ? '최고 세이브 동점 전체' : 'All exact best-save ties'}</strong>
          <span>{korean ? 'GUI는 모든 동점 승자를 보존해 페이지로 표시합니다. Discord만 가장 작은 canonical candidate ID 하나를 선택합니다.' : 'The GUI preserves and pages every tied winner. Only Discord selects the smallest canonical candidate ID.'}</span>
        </div>
        <nav aria-label={korean ? '최고 세이브 동점 페이지 이동' : 'Best-save tie page navigation'}>
          <button type="button" disabled={savePageIndex === 0} on:click={() => (savePageIndex -= 1)} aria-label={korean ? '이전 동점 100개' : 'Previous 100 ties'}><ChevronLeft size={16} /></button>
          <span>{savePageIndex + 1} / {savePageCount}</span>
          <button type="button" disabled={savePageIndex + 1 >= savePageCount} on:click={() => (savePageIndex += 1)} aria-label={korean ? '다음 동점 100개' : 'Next 100 ties'}><ChevronRight size={16} /></button>
        </nav>
      </header>
      <div class="member-meta">
        <span>{korean ? '동점 승자' : 'Tied winners'}: {bestSaveFamily.winner_count}</span>
        <span>{korean ? '확률 기준' : 'Probability basis'}: {bestSaveFamily.probability_basis}</span>
      </div>
      <ol start={savePageIndex * PRODUCT_MEMBER_PAGE_SIZE + 1}>
        {#each bestSaveWinners as winner (winner.group.identity.canonical_id)}
          <li class="save-row">
            <code>{winner.group.identity.canonical_id}</code>
            <span>{korean ? '가중 합계' : 'Weighted total'} {winner.weighted_total} · {korean ? '균형 J/L' : 'Balanced J/L'} {winner.balanced_jl_count} · {korean ? '전체 우주 확률' : 'Whole-universe probability'} {winner.exact_group_probability} · ID {winner.group.canonical_candidate_id}</span>
            <details>
              <summary>{korean ? '동점 승자 증거 확인' : 'Inspect tied-winner evidence'} ({winner.group.witnesses.length})</summary>
              <ul>
                {#each winner.group.witnesses as witness (witness.pattern_index)}
                  <li><span>{korean ? '패턴' : 'Pattern'} {witness.pattern_index} · ID {witness.candidate_id} · {korean ? '추적' : 'Trace'} {witness.trace_identity}</span></li>
                {/each}
              </ul>
            </details>
          </li>
        {/each}
      </ol>
    </section>
  {/if}
{:else if error}
  <p class="pager-error" role="alert">{error}</p>
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
  code { color: #23322d; font-size: 11px; overflow-wrap: anywhere; }
  footer { border-top: 1px solid #e3e8e5; justify-content: space-between; padding: 9px 15px; }
  .score-row { align-items: flex-start; flex-direction: column; gap: 3px; }
  .build-score-evidence { border-top: 1px solid #dce3df; }
  .path-row, .save-row { align-items: stretch; flex-direction: column; gap: 4px; }
  details { color: #52615c; font-size: 11px; width: 100%; }
  summary { cursor: pointer; font-weight: 700; }
  details ul { list-style: none; margin-top: 5px; max-height: 180px; padding: 0 0 0 12px; }
  details li { justify-content: flex-start; min-height: 26px; }
  .pager-loading { align-items: center; background: #f5f8f6; border: 1px solid #dce3df; border-radius: 6px; color: #52615c; display: flex; font-size: 12px; gap: 8px; margin: 16px 0; padding: 12px; }
  .pager-error { background: #fff1f0; border: 1px solid #efc3be; border-radius: 6px; color: #8b2820; font-size: 12px; margin: 16px 0; padding: 10px 12px; }
  :global(.spin) { animation: spin 800ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
