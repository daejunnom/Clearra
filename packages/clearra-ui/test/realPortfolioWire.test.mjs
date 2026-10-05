// SRP rationale: consume real Rust product/page/copy output through the same
// validation and export functions used by Web and Desktop. No mock candidates,
// benchmarks, network, browser session or dataset authority are created here.
import assert from 'node:assert/strict';
import { readFileSync, realpathSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { build } from 'esbuild';

const configuredRoot = process.env.CLEARRA_REAL_PORTFOLIO_SMOKE_DIR;
const expectedSource = process.env.CLEARRA_REAL_PORTFOLIO_SOURCE_COMMIT;

test('real Rust portfolios validate, backtrack and copy the whole selected set in UI', {
  skip: !configuredRoot && !expectedSource
    ? 'requires the explicit source-bound Rust-produced managed fixture' : false
}, async () => {
  assert.ok(configuredRoot, 'explicit managed fixture root is required');
  assert.match(expectedSource ?? '', /^[a-f0-9]{40}$/u, 'exact source commit is required');
  const repository = realpathSync(fileURLToPath(new URL('../../../', import.meta.url)));
  const root = realpathSync(configuredRoot);
  assert.equal(root, join(repository, '_local', 'artifacts', 'v081-product-page-smoke'));
  const fixture = JSON.parse(readFileSync(join(root, 'portfolio-wire-smoke.json'), 'utf8'));
  assert.equal(fixture.schema_id, 'clearra.v081.real-portfolio-wire-smoke.v2');
  assert.equal(fixture.cases.length, 8);
  assert.equal(fixture.multi_member_cases.length, 4);
  const compiledIdentity = fixture.runtime_identity;
  assert.equal(compiledIdentity?.source_commit, expectedSource, 'stale Rust fixture cannot be reused');
  assert.equal(compiledIdentity?.engine_build_id, expectedSource);

  const bundle = await build({
    bundle: true, format: 'esm', platform: 'node', logLevel: 'silent', write: false,
    stdin: {
      contents: `
        export { validateProductResultPayload, validateCoveragePortfolioRuntimePage,
          validateSolutionSetArtifactPayload } from './src/lib/workspace/productResultPager.ts';
        export { createCoveragePortfolioExportKeySource }
          from './src/lib/workspace/coveragePortfolioExportSource.ts';
        export { encodeSolutionKeySourceForClipboard }
          from './src/lib/workspace/solutionExportAsync.ts';
        export { decodeCtk3 } from './src/lib/workspace/ctk3Codec.ts';
      `,
      loader: 'ts', resolveDir: fileURLToPath(new URL('..', import.meta.url))
    }
  });
  const api = await import(`data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`);
  const seen = new Set();
  for (const value of fixture.cases) {
    const caseIdentity = `${value.build}:${value.legal}:${value.conditioned}`;
    assert.equal(seen.has(caseIdentity), false);
    seen.add(caseIdentity);
    const response = value.final_response;
    assert.deepEqual(response.runtime_identity, compiledIdentity, 'one compiled generation across all requests');
    assert.equal(response.status, 'success');
    assert.equal(api.validateProductResultPayload(response.product_result_payload), null, caseIdentity);
    const pages = value.pages;
    assert.equal(pages.length, value.build ? 1 : 4);
    const first = pages[0].wire.page;
    assert.equal(first.known_alternative_count, '1');
    assert.equal(first.optimal_cardinality, value.build ? '2' : '1');
    if (!value.build) {
      assert.equal(first.total_alternative_count, null);
      assert.equal(first.enumeration_complete, false);
    }
    const alternatives = new Set();
    for (const [index, snapshot] of pages.entries()) {
      const wire = snapshot.wire;
      assert.equal(wire.state, 'page');
      assert.equal(wire.product_page_kind, 'coverage-portfolio');
      const page = wire.page;
      assert.equal(api.validateCoveragePortfolioRuntimePage(page, {
        setIdentitySha256: first.set_identity_sha256,
        candidateMapSha256: first.candidate_map_sha256,
        alternativeIndex: (index + 1).toString(), memberPageNumber: '1'
      }), null, `${caseIdentity} page ${index + 1}`);
      const keys = page.members.map(member => member.normalized_solution_key);
      assert.equal(alternatives.has(JSON.stringify(keys)), false, 'ties cannot duplicate one exact set');
      alternatives.add(JSON.stringify(keys));
      if (!value.build) assert.deepEqual(keys, [value.candidate_keys[index]], 'canonical tie order');
      else assert.deepEqual(keys, value.candidate_keys, 'all mandatory Build members are retained');
      assert.equal(api.validateSolutionSetArtifactPayload(snapshot.artifact), null);
      assert.equal(snapshot.artifact.selection_id, page.alternative_index);
      assert.equal(snapshot.artifact.page_source_identity_sha256, page.set_identity_sha256);
      assert.equal(snapshot.artifact.solution_count, keys.length);

      let current = true;
      let loaderCalls = 0;
      const source = api.createCoveragePortfolioExportKeySource({
        initialPage: page, isCurrent: () => current,
        async loadMemberPage() {
          loaderCalls += 1;
          throw new Error('these real small portfolios have exactly one member page');
        }
      });
      assert.ok(source);
      assert.equal(source.keyCount, keys.length);
      assert.deepEqual(await source.readKeys(0, source.keyCount), keys);
      const document = await api.encodeSolutionKeySourceForClipboard(source, 'ctk');
      const native = snapshot.artifact.formats.find(format => format.format === 'ctk3');
      const actual = api.decodeCtk3(document);
      const expected = api.decodeCtk3(native.document);
      assert.equal(actual.pages.length, keys.length);
      assert.deepEqual(fieldMeaning(actual), fieldMeaning(expected), 'native and UI copy keep the same fields/order');
      assert.equal(loaderCalls, 0);

      current = false;
      await assert.rejects(source.readKeys(0, source.keyCount), /replaced by another result/u);
      const cancelled = new AbortController();
      const reason = new Error('user cancelled copy');
      reason.name = 'AbortError';
      cancelled.abort(reason);
      await assert.rejects(source.readKeys(0, source.keyCount, cancelled.signal), reason);
    }
    assert.deepEqual(value.restored, pages[0], 'native eviction/backtrack preserves page and copy bytes');
  }
  await verifyRealMultiMemberPortfolios(api, fixture.multi_member_cases, compiledIdentity);
});

async function verifyRealMultiMemberPortfolios(api, cases, identity) {
  const policies = new Set();
  for (const value of cases) {
    const policy = `${value.legal}:${value.conditioned}`;
    assert.equal(policies.has(policy), false);
    policies.add(policy);
    assert.equal(value.final_response.status, 'success');
    assert.deepEqual(value.final_response.runtime_identity, identity);
    assert.equal(api.validateProductResultPayload(value.final_response.product_result_payload), null);
    assert.equal(api.validateSolutionSetArtifactPayload(value.artifact), null);
    const pages = value.member_pages;
    assert.equal(pages.length, 3);
    const first = pages[0].page;
    assert.equal(first.optimal_cardinality, '246');
    assert.equal(first.total_member_pages, '3');
    const keys = [];
    for (const [index, wire] of pages.entries()) {
      assert.equal(wire.state, 'page');
      assert.equal(wire.product_page_kind, 'coverage-portfolio');
      assert.equal(api.validateCoveragePortfolioRuntimePage(wire.page, {
        setIdentitySha256: first.set_identity_sha256,
        candidateMapSha256: first.candidate_map_sha256,
        alternativeIndex: '1', memberPageNumber: (index + 1).toString()
      }), null);
      assert.equal(wire.page.members.length, index === 2 ? 46 : 100);
      keys.push(...wire.page.members.map(member => member.normalized_solution_key));
    }
    assert.deepEqual(keys, value.candidate_keys);
    assert.equal(new Set(keys).size, 246);
    assert.equal(value.artifact.solution_count, 246);
    assert.equal(value.artifact.selection_id, '1');
    assert.equal(value.artifact.page_source_identity_sha256, first.set_identity_sha256);

    // Render member page 2 (100 members), but keep the selected outer set's
    // export source on page 1. The real native page payloads supply pages 2/3.
    const renderedKeys = pages[1].page.members.map(member => member.normalized_solution_key);
    assert.equal(renderedKeys.length, 100);
    const requests = [];
    let current = true;
    const source = api.createCoveragePortfolioExportKeySource({
      initialPage: first, isCurrent: () => current,
      async loadMemberPage(alternativeIndex, memberPageNumber, signal) {
        assert.equal(signal?.aborted ?? false, false);
        requests.push([alternativeIndex, memberPageNumber]);
        assert.equal(alternativeIndex, '1');
        return pages[Number(memberPageNumber) - 1];
      }
    });
    assert.ok(source);
    assert.equal(source.keyCount, 246);
    const document = await api.encodeSolutionKeySourceForClipboard(source, 'ctk');
    assert.deepEqual(requests, [['1', '2'], ['1', '3']]);
    const actual = api.decodeCtk3(document);
    const native = value.artifact.formats.find(format => format.format === 'ctk3');
    assert.equal(actual.pages.length, 246, 'copy cannot truncate to the rendered 100 members');
    assert.deepEqual(fieldMeaning(actual), fieldMeaning(api.decodeCtk3(native.document)));
    assert.deepEqual(await source.readKeys(95, 110), keys.slice(95, 205));
    assert.equal(requests.length, 2, 'validated complete selection is reused');

    current = false;
    await assert.rejects(source.readKeys(0, source.keyCount), /replaced by another result/u);
    const controller = new AbortController();
    const cancelledSource = api.createCoveragePortfolioExportKeySource({
      initialPage: first, isCurrent: () => true,
      async loadMemberPage(alternativeIndex, memberPageNumber) {
        assert.equal(alternativeIndex, '1');
        assert.equal(memberPageNumber, '2');
        controller.abort();
        return pages[1];
      }
    });
    await assert.rejects(api.encodeSolutionKeySourceForClipboard(cancelledSource, 'ctk', {
      signal: controller.signal
    }), error => error.name === 'AbortError');
    console.log(`real_portfolio_copy=passed policy=${policy} rendered=100 exported=246 pages=3`);
  }
  assert.equal(policies.size, 4);
}

function fieldMeaning(document) {
  return {
    width: document.width,
    pages: document.pages.map(page => ({ height: page.height, cells: page.cells }))
  };
}
