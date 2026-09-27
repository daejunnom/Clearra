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

test('real Rust portfolios validate, backtrack and copy the whole selected set in UI', {
  skip: configuredRoot ? false : 'requires the explicit Rust-produced managed fixture'
}, async () => {
  const repository = realpathSync(fileURLToPath(new URL('../../../', import.meta.url)));
  const root = realpathSync(configuredRoot);
  assert.equal(root, join(repository, '_local', 'artifacts', 'v081-product-page-smoke'));
  const fixture = JSON.parse(readFileSync(join(root, 'portfolio-wire-smoke.json'), 'utf8'));
  assert.equal(fixture.schema_id, 'clearra.v081.real-portfolio-wire-smoke.v1');
  assert.equal(fixture.cases.length, 8);

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
    const identity = `${value.build}:${value.legal}:${value.conditioned}`;
    assert.equal(seen.has(identity), false);
    seen.add(identity);
    const response = value.final_response;
    assert.equal(response.status, 'success');
    assert.equal(api.validateProductResultPayload(response.product_result_payload), null, identity);
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
      }), null, `${identity} page ${index + 1}`);
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
});

function fieldMeaning(document) {
  return {
    width: document.width,
    pages: document.pages.map(page => ({ height: page.height, cells: page.cells }))
  };
}
