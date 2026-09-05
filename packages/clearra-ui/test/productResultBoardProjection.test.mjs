import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { build } from 'esbuild';

const packageRoot = fileURLToPath(new URL('..', import.meta.url));
const source = (path) => readFileSync(new URL(path, import.meta.url), 'utf8');

const bundle = await build({
  bundle: true,
  format: 'esm',
  logLevel: 'silent',
  platform: 'node',
  stdin: {
    contents: `
      export {
        solutionBoardPreviewFromKey,
        solutionBoardPreviewFromReplay
      } from './src/lib/workspace/solutionBoardPreview.ts';
      export {
        productResultIdentity,
        productResultOwnsSolutionPage
      } from './src/lib/workspace/productResultPager.ts';
    `,
    loader: 'ts',
    resolveDir: packageRoot
  },
  write: false
});
const production = await import(
  `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`
);

const solutionKey =
  'ctk1|initial=00000000000003f0|placements=I:000000000000000f';

test('portfolio and score normalized solution keys project to the shared board view', () => {
  for (const family of ['portfolio', 'score']) {
    const preview = production.solutionBoardPreviewFromKey(solutionKey, 4);
    assert.equal(preview.source, 'solution-key', family);
    assert.equal(preview.board?.height, 4, family);
    assert.equal(preview.board?.cells.length, 40, family);
    assert.equal(preview.board?.cells.filter((cell) => cell === 'I').length, 4, family);
    assert.equal(preview.board?.cells.filter((cell) => cell === 'G').length, 6, family);
  }
});

test('path witnesses project their last replay placement to board cells', () => {
  const preview = production.solutionBoardPreviewFromReplay([
    {
      active_piece: 'I',
      placement_mask: '0x000000000000000f',
      board_before_mask: '0x00000000000003f0',
      board_after_placement_mask: '0x00000000000003ff',
      board_after_line_clear_mask: '0x0000000000000000'
    }
  ], 4);
  assert.equal(preview.source, 'replay-last-placement');
  assert.equal(preview.board?.height, 4);
  assert.equal(preview.board?.cells.length, 40);
  assert.equal(preview.board?.cells.filter((cell) => cell === 'I').length, 4);
  assert.equal(preview.board?.cells.filter((cell) => cell === 'G').length, 6);
});

test('product result UI renders boards and governed exports without exposing internal IDs', () => {
  const pager = source('../src/lib/workspace/ProductResultPager.svelte');
  const preview = source('../src/lib/workspace/SolutionBoardPreview.svelte');
  const replayGif = source('../src/lib/workspace/PcPathReplayGif.svelte');
  const gallery = source('../src/lib/workspace/SolutionGallery.svelte');
  const resultWorkspace = source('../src/lib/workspace/ResultWorkspace.svelte');
  const pcResult = source('../src/lib/workspace/PcSolverResult.svelte');

  assert.match(pager, /import PcPathReplayGif from '\.\/PcPathReplayGif\.svelte'/u);
  assert.match(pager, /import SolutionCopyFormatControl from '\.\/SolutionCopyFormatControl\.svelte'/u);
  assert.match(pager, /groupPcPathWitnesses\(pathFamily\?\.witnesses \?\? \[\]\)/u);
  assert.match(pager, /pcPathCandidateGroupExportPages\([\s\S]*?pathCandidateGroup,[\s\S]*?targetLines,[\s\S]*?target_terminal_board_mask/u);
  assert.match(pager, /<PcPathReplayGif/u);
  assert.match(pager, /loadPages=\{pathCandidateGroup \? loadVisiblePathPages : null\}/u);
  assert.doesNotMatch(pager, /\$: pathCandidatePages/u);
  assert.doesNotMatch(pager, /<code>\{member\.normalized_solution_key\}<\/code>/u);
  assert.doesNotMatch(pager, /<code>\{winner\.normalized_solution_key\}<\/code>/u);
  assert.doesNotMatch(pager, /<code>\{winner\.candidate_key\}<\/code>/u);
  assert.doesNotMatch(pager, /<code>\{candidate\.candidate_key\}<\/code>/u);
  assert.doesNotMatch(pager, /<code>\{witness\.normalized_trace_key\}<\/code>/u);

  assert.doesNotMatch(preview, /rawKey|raw-key-details|raw-solution-key|copyRawKey/u);
  assert.match(preview, /\{#each board\.cells as cell\}/u);
  assert.match(preview, /role="img"/u);

  assert.match(replayGif, /<img/u);
  assert.match(replayGif, /buildPcPathReplayFrames\(witness, targetLines\)/u);
  assert.match(replayGif, /encodePcPathReplayGif\(frames\)/u);
  assert.doesNotMatch(replayGif, /raw-key-details|raw-solution-key|copyRawKey/u);
  assert.doesNotMatch(pager, /\{pathFamily\.problem_id\}/u);
  assert.doesNotMatch(pager, /ID \{pathCandidateGroup\.candidateId\}/u);
  assert.doesNotMatch(pager, /\{witness\.pattern_id\}/u);
  assert.doesNotMatch(pager, /\{step\.line_clear_identity\}/u);
  assert.doesNotMatch(pager, /\{candidate\.setup_id\}|\{candidate\.condition_id\}/u);
  assert.doesNotMatch(pager, /<code>\{candidate\.candidate_id\}<\/code>/u);
  assert.doesNotMatch(pager, /\{spinStructureFamily\.schema_id\}/u);
  assert.doesNotMatch(pager, /\{candidate\.partition\}/u);

  assert.match(gallery, /<SolutionBoardPreview/u);
  assert.match(gallery, /solutionBoardPreviewFromKey\(key, lines\)/u);
  assert.doesNotMatch(gallery, /rawKey=/u);
  assert.match(gallery, /solutionAverageScores\[solution\.key\]/u);
  assert.match(resultWorkspace, /<ProductResultPager[\s\S]*?\{targetLines\}/u);
  assert.match(pcResult, /<ProductResultPager[\s\S]*?\{targetLines\}/u);
});

test('normal operation report surfaces whitelist human metrics and omit machine identities', () => {
  const sequence = source('../src/lib/workspace/OperationSequenceWorkspace.svelte');
  const dependencies = source('../src/lib/workspace/SequenceDependenciesWorkspace.svelte');

  assert.match(sequence, /publicOperationReportFields/u);
  assert.match(sequence, /operation_count: \['Operations', '배치 수'\]/u);
  assert.doesNotMatch(sequence, /candidate_id|trace_key|normalized_trace/u);
  assert.match(dependencies, /publicDependencyReportFields/u);
  assert.match(dependencies, /exact_order_count: \['Valid orders', '유효한 순서 수'\]/u);
  assert.doesNotMatch(dependencies, /candidate_id|representative_order|trace_key/u);
});

test('field average score keeps the v0.7.4 per-field and whole-score presentation', () => {
  const pager = source('../src/lib/workspace/ProductResultPager.svelte');
  const resultWorkspace = source('../src/lib/workspace/ResultWorkspace.svelte');
  const pcResult = source('../src/lib/workspace/PcSolverResult.svelte');

  assert.doesNotMatch(pager, /score-field-summary/u);
  assert.doesNotMatch(pager, /Per-pattern field scores/u);
  assert.doesNotMatch(pager, /field\.candidate_id/u);
  assert.match(
    resultWorkspace,
    /pcScoreFieldSummary\.fields\.map\(\(field\) => \[field\.normalized_field_key, field\]\)/u
  );
  assert.match(
    pcResult,
    /pcScoreFieldSummary\.fields\.map\(\(field\) => \[field\.normalized_field_key, field\]\)/u
  );
  assert.match(resultWorkspace, /pcScoreFieldSummary\?\.overall_score/u);
  assert.match(resultWorkspace, /label\('overallScore'\)/u);
  assert.doesNotMatch(resultWorkspace, /\{#if !pcScoreFieldSummary\}/u);
  assert.match(resultWorkspace, /<SolutionGallery[\s\S]*?solutionAverageScores=\{solutionAverageScoreByKey\}/u);
  assert.match(pcResult, /pcScoreFieldSummary\?\.overall_score/u);
  assert.match(pcResult, /label\('overallScore'\)/u);
  assert.doesNotMatch(pcResult, /\{#if !pcScoreFieldSummary\}/u);
  assert.match(pcResult, /<SolutionGallery[\s\S]*?solutionAverageScores=\{solutionAverageScoreByKey\}/u);
});

test('solution subsets reuse the ordinary gallery while portfolio export stays bound to the outer set', () => {
  const pager = source('../src/lib/workspace/ProductResultPager.svelte');
  const subsetPage = source('../src/lib/workspace/SolutionSubsetPage.svelte');
  const gallery = source('../src/lib/workspace/SolutionGallery.svelte');
  const resultWorkspace = source('../src/lib/workspace/ResultWorkspace.svelte');
  const pcResult = source('../src/lib/workspace/PcSolverResult.svelte');
  const start = pager.indexOf('{#if coveragePage.optimal_cardinality');
  const end = pager.indexOf('<footer>', start);
  assert.ok(start >= 0 && end > start, 'coverage member branch');
  const members = pager.slice(start, end);

  assert.match(pager, /currentMembers\.map\(\(member\) => member\.normalized_solution_key\)/u);
  assert.ok(
    pager.includes('${coveragePage.alternative_index}:${memberPageNumber}'),
    'the selected member page owns only the rendered gallery identity'
  );
  const activateExport = pager.slice(
    pager.indexOf('function activateCoverageExportSource('),
    pager.indexOf('async function nextOuterPage()', pager.indexOf('function activateCoverageExportSource('))
  );
  assert.match(activateExport, /page\.alternative_index/u);
  assert.doesNotMatch(activateExport, /memberPageNumber/u);
  const memberNavigation = pager.slice(
    pager.indexOf('async function showMemberPage('),
    pager.indexOf('function pruneMemberCache(', pager.indexOf('async function showMemberPage('))
  );
  assert.doesNotMatch(memberNavigation, /activateCoverageExportSource/u);
  assert.match(
    pager,
    /selectedPage\.alternative_index !== previousAlternativeIndex[\s\S]*?memberPageNumber = '1';[\s\S]*?currentMembers = selectedPage\.members;/u
  );
  assert.match(members, /<SolutionSubsetPage/u);
  assert.match(members, /solutionKeys=\{coverageSolutionKeys\}/u);
  assert.match(members, /exportKeySource=\{coverageExportKeySource\}/u);
  assert.match(pager, /최고 점수 최소 해법 집합 전체/u);
  assert.doesNotMatch(members, /member\.candidate_id/u);
  assert.doesNotMatch(members, /member\.normalized_solution_key/u);

  assert.match(subsetPage, /import SolutionCopyFormatControl/u);
  assert.match(subsetPage, /import SolutionGallery/u);
  assert.match(
    subsetPage,
    /<SolutionCopyFormatControl[\s\S]*?solutionKeys=\{exportSolutionKeys \?\? solutionKeys\}/u
  );
  assert.match(subsetPage, /<SolutionGallery[\s\S]*?solutionCount=\{solutionKeys\.length\}/u);
  assert.match(subsetPage, /keySource=\{exportKeySource\}/u);
  assert.match(subsetPage, /\{#key exportSetIdentity \|\| solutionSetIdentity\}/u);
  assert.match(subsetPage, /solutionOrdinalBase/u);
  assert.match(gallery, /BigInt\(solutionOrdinalBase\) \+ BigInt\(index \+ 1\)/u);

  assert.match(pager, /allBuildCandidateSolutionKeys/u);
  assert.match(pager, /allBuildScoreSolutionKeys/u);
  assert.match(pager, /allScoreWinnerSolutionKeys/u);
  assert.equal(
    (pager.match(/<SolutionSubsetPage/gu) ?? []).length,
    5,
    'coverage, Build score evidence, Build candidate, Build setup, and PC score winners'
  );
  assert.doesNotMatch(pager, /solutionBoardPreviewFromKey/u);
  assert.doesNotMatch(pager, /rawKey=\{(?:winner|candidate)\.(?:candidate_key|normalized_solution_key)\}/u);

  for (const result of [resultWorkspace, pcResult]) {
    assert.match(result, /productResultOwnsSolutionPage\(productResultPayload\)/u);
    assert.ok(result.includes('{#if !productSolutionPageActive}'));
  }
  assert.match(pcResult, /solutionCount !== null && !productSolutionPageActive/u);
  assert.match(members, /exportSetIdentity=\{coverageExportIdentity\}/u);
  assert.equal(
    (pager.match(/exportSetIdentity=/gu) ?? []).length,
    5,
    'every product solution page separates its full-export identity from its visible page'
  );
});

test('portfolio export activation fails through the pager error boundary without page-only fallback', () => {
  const pager = source('../src/lib/workspace/ProductResultPager.svelte');
  const activation = pager.slice(
    pager.indexOf('function activateCoverageExportSource('),
    pager.indexOf('async function nextOuterPage()', pager.indexOf('function activateCoverageExportSource('))
  );

  assert.match(activation, /tryCreateCoveragePortfolioExportKeySource/u);
  assert.match(activation, /if \(activation\.error !== null\)/u);
  assert.match(activation, /coverageExportKeySource = null;/u);
  assert.match(activation, /coverageExportIdentity = '';/u);
  assert.match(activation, /error = activation\.error;/u);
  assert.match(activation, /return false;/u);
  assert.doesNotMatch(activation, /coverageSolutionKeys/u);
  assert.match(pager, /\{#if payload && !error\}/u);
  assert.match(pager, /projectWorkspacePublicFailure\(\{/u);
  assert.match(pager, /fallbackCode: 'result-invalid'/u);
  assert.match(
    pager,
    /<WorkspaceFailureNotice failures=\{pagerFailure\?\.publicFailures \?\? \[\]\} \{language\} compact \/>/u
  );
  assert.doesNotMatch(pager, />\{error\}<\//u);
});

test('typed product ownership suppresses only normalized field families, not replay or setup records', () => {
  const payload = (payloadKind, kind) => ({
    contract: 'test',
    result_kind: 'test',
    content: {
      payload_kind: payloadKind,
      payload: kind ? { kind } : {}
    }
  });

  for (const product of [
    payload('coverage-portfolio'),
    payload('build-coverage-portfolio-v2'),
    payload('build-setup-family-v1'),
    payload('score-pattern-winner-family'),
    payload('build-v2', 'candidate-family'),
    payload('build-v2', 'portfolio'),
    payload('build-v2', 'score-portfolio')
  ]) {
    assert.equal(production.productResultOwnsSolutionPage(product), true);
  }
  for (const product of [
    payload('build-v2', 'probability'),
    payload('pc-score-field-summary'),
    payload('pc-path-family'),
    payload('setup-ranked-family'),
    payload('setup-score-ranking'),
    payload('spin-structure-family'),
    payload('field-document')
  ]) {
    assert.equal(production.productResultOwnsSolutionPage(product), false);
  }
});

test('score winner result identity changes even when a replacement has the same member count', () => {
  const scoreFamily = (key) => ({
    contract: 'pc.score-finder',
    result_kind: 'pc-fixed-score-witness.v2',
    content: {
      payload_kind: 'score-pattern-winner-family',
      payload: {
        winner_contract: 'winner.v1',
        winner_count: '1',
        winners: [{
          pattern_id: '0',
          candidate_id: '1',
          normalized_solution_key: key,
          score: '100',
          informational_attack: '4'
        }]
      }
    }
  });
  assert.notEqual(
    production.productResultIdentity(scoreFamily('solution-a')),
    production.productResultIdentity(scoreFamily('solution-b'))
  );
});
