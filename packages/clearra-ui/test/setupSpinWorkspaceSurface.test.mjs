import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

function source(path) {
  return readFileSync(new URL(path, import.meta.url), 'utf8');
}

test('Setup and Spin workspaces are real Web and Desktop entry surfaces', () => {
  const webRoute = source('../../../apps/clearra-web/src/routes/+page.svelte');
  const desktopRoute = source('../../../apps/clearra-desktop/src/routes/+page.svelte');
  const setupScore = source('../src/lib/workspace/SetupScoreWorkspace.svelte');
  const spin = source('../src/lib/workspace/SpinStructureWorkspace.svelte');
  const catalog = source('../src/lib/i18n/componentCatalog.ts');

  for (const route of [webRoute, desktopRoute]) {
    assert.match(route, /selectedTool === 'setup-score'/u);
    assert.match(route, /selectedTool === 'spin-structure'/u);
    assert.match(route, /SetupScoreWorkspace/u);
    assert.match(route, /SpinStructureWorkspace/u);
  }
  assert.match(setupScore, /buildSetupScoreCommand\(request\)/u);
  assert.match(setupScore, /setupScoreRequestForDesktop\(request, language\)/u);
  assert.match(setupScore, /componentMessage\(language, 'setupScoreIsCpuOnlyAndExposes'\)/u);
  assert.match(catalog, /Equal scores use a stable display order without mixing attack/u);
  assert.match(catalog, /동일 score는 안정적인 순서로 표시하며 attack을 혼합하지 않습니다/u);
  assert.match(spin, /buildSpinStructureCommand\(request\)/u);
  assert.match(spin, /spinStructureRequestForDesktop\(request, language\)/u);
  assert.match(spin, /workerController\.loadNextProductPage/u);
  assert.match(spin, /loadNextDesktopProductPage/u);
  assert.match(spin, /componentMessage\(language, 'theObjectiveIsFixedToMinCover'\)/u);
  assert.match(catalog, /common result pager exposes every exact equal-cardinality optimum/u);
  assert.match(catalog, /모든 동일 최소 크기 exact portfolio/u);
});

test('ordinary Setup and Spin families render separately from exact cover paging', () => {
  const pager = source('../src/lib/workspace/ProductResultPager.svelte');
  const setupFinder = source('../src/lib/workspace/SetupFinderResult.svelte');
  const catalog = source('../src/lib/i18n/componentCatalog.ts');

  assert.match(pager, /setupRankedFamily/u);
  assert.match(pager, /setupScoreFamily/u);
  assert.match(pager, /spinStructureFamily/u);
  for (const key of ['thisIsAnOrdinaryRankedFamilyAnd', 'equalScoresRemainMembersOfTheOrdinary', 'searchAndGuaranteedAreOrdinaryCompleteFamilies']) {
    assert.ok(pager.includes(`componentMessage(language, '${key}')`), key);
  }
  assert.match(catalog, /ordinary ranked family and is not reclassified as a tie portfolio/u);
  assert.match(catalog, /Equal scores remain members of the ordinary ranking family/u);
  assert.match(catalog, /Search and guaranteed are ordinary complete families/u);
  assert.match(pager, /async function nextOuterPage\(\)/u);
  assert.match(setupFinder, /view\.response\?\.product_result_payload/u);
  assert.match(setupFinder, /<ProductResultPager/u);
});
