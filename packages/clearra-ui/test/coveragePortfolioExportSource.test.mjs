import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { build } from 'esbuild';

const bundle = await build({
  bundle: true,
  format: 'esm',
  logLevel: 'silent',
  platform: 'node',
  stdin: {
    contents: `
      export {
        createCoveragePortfolioExportKeySource,
        tryCreateCoveragePortfolioExportKeySource
      }
        from './src/lib/workspace/coveragePortfolioExportSource.ts';
    `,
    loader: 'ts',
    resolveDir: fileURLToPath(new URL('..', import.meta.url))
  },
  write: false
});
const {
  createCoveragePortfolioExportKeySource,
  tryCreateCoveragePortfolioExportKeySource
} = await import(
  `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`
);

const SET_IDENTITY = 'a'.repeat(64);
const CANDIDATE_MAP_IDENTITY = 'b'.repeat(64);

test('portfolio export materializes the complete selected outer alternative in canonical order', async () => {
  const initialPage = page(1, 100, 1);
  const requests = [];
  const source = createCoveragePortfolioExportKeySource({
    initialPage,
    isCurrent: () => true,
    async loadMemberPage(alternativeIndex, memberPageNumber, signal) {
      requests.push([alternativeIndex, memberPageNumber, signal]);
      return response(
        memberPageNumber === '2' ? page(2, 100, 101) : page(3, 5, 201)
      );
    }
  });

  assert.ok(source);
  assert.equal(source.keyCount, 205);
  assert.deepEqual(
    await source.readKeys(95, 110),
    Array.from({ length: 110 }, (_, index) => `solution-${index + 96}`)
  );
  assert.deepEqual(
    requests.map(([alternative, memberPage]) => [alternative, memberPage]),
    [['1', '2'], ['1', '3']]
  );

  assert.deepEqual(await source.readKeys(100, 2), ['solution-101', 'solution-102']);
  assert.equal(requests.length, 2, 'validated pages are cached for the same outer alternative');
});

test('whole-set copy resumes an evicted selected page after 1 -> 2 -> 3 -> 4 -> 2 -> 1 cache churn', async () => {
  const selectedPage = selectedAlternativePage(1, 100, 1);
  const requests = [];
  let pageTwoAttempts = 0;
  const source = createCoveragePortfolioExportKeySource({
    initialPage: selectedPage,
    isCurrent: () => true,
    async loadMemberPage(alternativeIndex, memberPageNumber, signal, maximumWorkSteps) {
      requests.push([alternativeIndex, memberPageNumber, maximumWorkSteps, signal]);
      if (memberPageNumber === '2' && pageTwoAttempts++ === 0) {
        return {
          schema_version: 1,
          runtime: 'clearra-wasm',
          product_page_kind: 'coverage-portfolio',
          state: 'work-budget-exhausted',
          known_alternative_count: '4',
          enumeration_complete: true,
          work_steps: 10_000,
          replay_cursor_alternative_index: '1'
        };
      }
      return response(
        memberPageNumber === '2'
          ? selectedAlternativePage(2, 100, 101)
          : selectedAlternativePage(3, 5, 201)
      );
    }
  });

  assert.ok(source);
  assert.deepEqual(
    await source.readKeys(0, 205),
    Array.from({ length: 205 }, (_, index) => `solution-${index + 1}`),
    'copy exports the entire selected outer set after bounded replay resumes'
  );
  assert.deepEqual(
    requests.map(([alternative, memberPage, workSteps]) => [
      alternative,
      memberPage,
      workSteps
    ]),
    [
      ['2', '2', 10_000],
      ['2', '2', 10_000],
      ['2', '3', 10_000]
    ],
    'a work-budget stop retries the same exact coordinate before later member pages'
  );
});

test('portfolio export rejects identity mismatches, missing members, and cross-page duplicates', async (t) => {
  await t.test('candidate-map mismatch', async () => {
    const source = sourceWithPage2({
      ...page(2, 100, 101),
      candidate_map_sha256: 'c'.repeat(64)
    });
    await assert.rejects(source.readKeys(0, 101), /identity|active portfolio/u);
  });

  await t.test('different immutable alternative snapshot', async () => {
    const source = sourceWithPage2({
      ...page(2, 100, 101),
      known_alternative_count: '2',
      total_alternative_count: '2'
    });
    await assert.rejects(source.readKeys(0, 101), /active portfolio/u);
  });

  await t.test('missing member', async () => {
    const source = sourceWithPage2(page(2, 99, 101));
    await assert.rejects(source.readKeys(0, 101), /counts|member count/u);
  });

  await t.test('duplicate candidate ID across pages', async () => {
    const source = sourceWithPage2(page(2, 100, 100));
    await assert.rejects(source.readKeys(0, 101), /duplicated or out of canonical order/u);
  });

  await t.test('duplicate normalized key across pages', async () => {
    const duplicate = page(2, 100, 101);
    duplicate.members[0].normalized_solution_key = 'solution-100';
    const source = sourceWithPage2(duplicate);
    await assert.rejects(source.readKeys(0, 101), /duplicated or out of canonical order/u);
  });
});

test('portfolio export propagates cancellation and rejects an outer-generation replacement', async () => {
  const alreadyAborted = new AbortController();
  const abortReason = new Error('cancelled by test');
  abortReason.name = 'AbortError';
  alreadyAborted.abort(abortReason);
  let calls = 0;
  const cancelledSource = createCoveragePortfolioExportKeySource({
    initialPage: page(1, 100, 1),
    isCurrent: () => true,
    async loadMemberPage() {
      calls += 1;
      return response(page(2, 100, 101));
    }
  });
  await assert.rejects(cancelledSource.readKeys(0, 101, alreadyAborted.signal), abortReason);
  assert.equal(calls, 0);

  const inFlightAbort = new AbortController();
  const inFlightSource = createCoveragePortfolioExportKeySource({
    initialPage: page(1, 100, 1),
    isCurrent: () => true,
    loadMemberPage: (_alternative, _memberPage, signal) => new Promise((_, reject) => {
      signal.addEventListener('abort', () => reject(signal.reason), { once: true });
    })
  });
  const inFlightRead = inFlightSource.readKeys(0, 101, inFlightAbort.signal);
  inFlightAbort.abort(abortReason);
  await assert.rejects(inFlightRead, abortReason);

  let current = true;
  let resolvePage;
  const pendingPage = new Promise((resolve) => {
    resolvePage = resolve;
  });
  const replacedSource = createCoveragePortfolioExportKeySource({
    initialPage: page(1, 100, 1),
    isCurrent: () => current,
    loadMemberPage: () => pendingPage
  });
  const pendingRead = replacedSource.readKeys(0, 101);
  current = false;
  resolvePage(response(page(2, 100, 101)));
  await assert.rejects(pendingRead, /replaced by another result or alternative/u);
});

test('oversized full-set export becomes an explicit pager error instead of throwing', () => {
  let loaderCalls = 0;
  const activation = tryCreateCoveragePortfolioExportKeySource({
    initialPage: {
      ...page(1, 100, 1),
      optimal_cardinality: '1000001',
      total_member_pages: '10001'
    },
    isCurrent: () => true,
    loadMemberPage() {
      loaderCalls += 1;
      throw new Error('oversized export must fail before paging');
    }
  });

  assert.equal(activation.keySource, null);
  assert.match(activation.error, /exceeds the supported page bound/u);
  assert.equal(loaderCalls, 0);
});

function sourceWithPage2(secondPage) {
  const initialPage = {
    ...page(1, 100, 1),
    optimal_cardinality: '200',
    total_member_pages: '2'
  };
  return createCoveragePortfolioExportKeySource({
    initialPage,
    isCurrent: () => true,
    loadMemberPage: () => Promise.resolve(response({
      ...secondPage,
      optimal_cardinality: '200',
      total_member_pages: '2'
    }))
  });
}

function response(value) {
  return {
    schema_version: 1,
    runtime: 'clearra-wasm',
    product_page_kind: 'coverage-portfolio',
    state: 'page',
    page: value
  };
}

function page(memberPageNumber, memberCount, firstCandidateId) {
  return {
    page_contract: 'portfolio-alternative-page.v1',
    member_page_contract: 'portfolio-member-page.v1',
    set_identity_sha256: SET_IDENTITY,
    candidate_map_sha256: CANDIDATE_MAP_IDENTITY,
    alternative_index: '1',
    optimal_cardinality: '205',
    known_alternative_count: '1',
    total_alternative_count: '1',
    enumeration_complete: true,
    member_page_number: memberPageNumber.toString(),
    total_member_pages: '3',
    members: Array.from({ length: memberCount }, (_, index) => {
      const candidateId = firstCandidateId + index;
      return {
        candidate_id: candidateId.toString(),
        normalized_solution_key: `solution-${candidateId}`
      };
    })
  };
}

function selectedAlternativePage(memberPageNumber, memberCount, firstCandidateId) {
  return {
    ...page(memberPageNumber, memberCount, firstCandidateId),
    alternative_index: '2',
    known_alternative_count: '4',
    total_alternative_count: '4'
  };
}
