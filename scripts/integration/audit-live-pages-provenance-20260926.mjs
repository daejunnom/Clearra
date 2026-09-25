// Read-only production audit. No refs, tags, deployments, or acceptance records are changed.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import path from 'node:path';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.BROWSER_ROOT + '/node_modules/playwright');
const origin = 'https://daejunnom.github.io/Clearra/';
const expectedSource = '952280950aeb24528279cbff685fe8acec892223';
const output = process.env.AUDIT_ROOT;
const acceptedRoot = process.env.ACCEPTED_ROOT;
await mkdir(output, { recursive: true });
const report = { schema: 'clearra.live-pages-provenance-audit.v1', inspectedAt: new Date().toISOString(), expectedSource, acceptanceRun: '36152785910', checks: [], files: [], browser: null };
async function boundedFetch(relative, maximum, capture = false) {
  assert.ok(typeof relative === 'string' && !relative.startsWith('/') && !relative.includes('\\') && !relative.split('/').includes('..'));
  const url = new URL(relative, origin);
  assert.equal(url.origin, new URL(origin).origin);
  assert.ok(url.pathname.startsWith('/Clearra/'));
  const response = await fetch(url, { cache: 'no-store', headers: { 'cache-control': 'no-cache' }, signal: AbortSignal.timeout(45000) });
  if (!response.ok) throw new Error(`HTTP ${response.status}: ${relative}`);
  const chunks = []; const digest = createHash('sha256'); let size = 0;
  for await (const chunk of response.body) {
    size += chunk.length;
    if (size > maximum) throw new Error(`Public byte budget exceeded: ${relative}`);
    digest.update(chunk); if (capture) chunks.push(chunk);
  }
  return { size, sha256: digest.digest('hex'), headers: Object.fromEntries(response.headers), bytes: capture ? Buffer.concat(chunks) : undefined };
}
try {
  const identityResponse = await boundedFetch('clearra-build-identity.json', 4 * 1024 * 1024, true);
  const liveIdentity = JSON.parse(identityResponse.bytes.toString('utf8'));
  const acceptedIdentity = JSON.parse(await readFile(path.join(acceptedRoot, 'clearra-build-identity.json'), 'utf8'));
  const { files: entries, ...identityFields } = liveIdentity;
  report.liveIdentity = identityFields;
  report.identityHeaders = identityResponse.headers;
  assert.equal(liveIdentity.sourceCommit, expectedSource);
  assert.equal(liveIdentity.engineBuildId, expectedSource);
  assert.equal(String(liveIdentity.acceptedRunId), '36152785910');
  assert.deepEqual(liveIdentity, acceptedIdentity, 'Public identity differs from the accepted artifact');
  report.checks.push('public identity equals the accepted Pages artifact identity');
  assert.ok(Array.isArray(entries) && entries.length > 0 && entries.length <= 8192);
  assert.ok(entries.reduce((sum, row) => sum + row.size, 0) <= 512 * 1024 * 1024);
  let cursor = 0;
  await Promise.all(Array.from({ length: 4 }, async () => {
    while (cursor < entries.length) {
      const file = entries[cursor++];
      assert.ok(Number.isSafeInteger(file.size) && file.size >= 0 && file.size <= 128 * 1024 * 1024);
      try {
        const remote = await boundedFetch(file.path, file.size);
        const accepted = await readFile(path.join(acceptedRoot, file.path));
        const acceptedHash = createHash('sha256').update(accepted).digest('hex');
        const exact = remote.size === file.size && remote.sha256 === file.sha256 && acceptedHash === file.sha256;
        report.files.push({ path: file.path, size: remote.size, sha256: remote.sha256, exact });
      } catch (error) { report.files.push({ path: file.path, exact: false, error: String(error) }); }
    }
  }));
  report.files.sort((a, b) => a.path.localeCompare(b.path));
  report.fileSummary = { expected: entries.length, checked: report.files.length, mismatched: report.files.filter(file => !file.exact) };
  const wasmManifest = await boundedFetch('wasm/clearra_wasm.manifest.json', 2 * 1024 * 1024, true);
  report.wasmManifest = JSON.parse(wasmManifest.bytes.toString('utf8'));
  console.log('PROVENANCE_IDENTITY ' + JSON.stringify(report.liveIdentity));
  console.log('PROVENANCE_FILES ' + JSON.stringify(report.fileSummary));
  console.log('PROVENANCE_WASM ' + JSON.stringify(report.wasmManifest));
} catch (error) {
  report.provenanceError = String(error);
  console.error('PROVENANCE_ERROR ' + String(error));
}
const browser = await chromium.launch({ headless: true });
try {
  const context = await browser.newContext({ locale: 'ko-KR', viewport: { width: 1440, height: 1000 }, serviceWorkers: 'block' });
  await context.addInitScript(() => {
    window.__clearraAudit = { requests: [], outputs: [], finals: [], workerUrls: [], errors: [] };
    const NativeWorker = window.Worker;
    window.Worker = class extends NativeWorker {
      constructor(...args) {
        super(...args);
        window.__clearraAudit.workerUrls.push(String(args[0]));
        this.addEventListener('error', event => window.__clearraAudit.errors.push(event.message));
        this.addEventListener('message', event => {
          try {
            const serialized = JSON.stringify(event.data, (_key, value) => typeof value === 'bigint' ? value.toString() : value instanceof ArrayBuffer ? { arrayBufferBytes: value.byteLength } : value);
            if (!serialized || serialized.length > 4 * 1024 * 1024) return;
            const value = JSON.parse(serialized);
            window.__clearraAudit.outputs.push(value);
            if (window.__clearraAudit.outputs.length > 40) window.__clearraAudit.outputs.shift();
            if (/runtime_identity|normalized_solution_keys|product_result_payload/.test(serialized)) {
              window.__clearraAudit.finals.push(value);
              if (window.__clearraAudit.finals.length > 8) window.__clearraAudit.finals.shift();
            }
          } catch {}
        });
      }
      postMessage(...args) {
        try {
          const serialized = JSON.stringify(args[0]);
          if (serialized && serialized.length < 50000) {
            window.__clearraAudit.requests.push(JSON.parse(serialized));
            if (window.__clearraAudit.requests.length > 40) window.__clearraAudit.requests.shift();
          }
        } catch {}
        return super.postMessage(...args);
      }
    };
  });
  const page = await context.newPage();
  const pageErrors = [];
  page.on('pageerror', error => pageErrors.push(String(error)));
  await page.goto(origin + '?tool=pc', { waitUntil: 'domcontentloaded', timeout: 60000 });
  await page.locator('.workspace-queue-input').waitFor({ timeout: 45000 });
  const before = await page.locator('body').innerText();
  const navigation = await page.locator('.product-tabs a').evaluateAll(nodes => nodes.map(node => ({ text: node.textContent.trim(), href: node.getAttribute('href') })));
  const test = { name: 'STOILJZ-18-terminal-hold-public-UI', expectedSolutionCount: 18, before, navigation, pageErrors };
  try {
    await page.locator('.fumen-import input:not([type="file"])').first().fill('ctk3_w0kCQBhwwAEHHABh4Q');
    await page.locator('.fumen-import button').filter({ hasText: '필드 불러오기' }).last().click();
    await page.locator('.workspace-queue-input').fill('STOILJZ');
    await page.getByRole('button', { name: '탐색 실행', exact: true }).first().click();
    await page.waitForFunction(() => {
      const values = window.__clearraAudit?.finals ?? [];
      return values.some(value => /runtime_identity|normalized_solution_keys/.test(JSON.stringify(value)));
    }, null, { timeout: 150000 });
  } catch (error) { test.error = String(error); }
  test.after = await page.locator('body').innerText();
  test.worker = await page.evaluate(() => window.__clearraAudit);
  test.resources = await page.evaluate(() => performance.getEntriesByType('resource').map(entry => ({ name: entry.name, initiatorType: entry.initiatorType, transferSize: entry.transferSize })));
  test.mandatoryCheckboxCount = await page.locator('.mandatory-choice').count();
  await page.screenshot({ path: path.join(output, 'live-stoiljz-result.png'), fullPage: true });
  report.browser = test;
  console.log('PUBLIC_UI_RESULT ' + JSON.stringify(test));
  await context.close();
} catch (error) { report.browserError = String(error); console.error('BROWSER_ERROR ' + String(error)); }
finally { await browser.close(); }
await writeFile(path.join(output, 'provenance.json'), JSON.stringify(report, null, 2) + '\n');
console.log('AUDIT_COMPLETE ' + JSON.stringify({ expectedSource, version: report.liveIdentity?.version, source: report.liveIdentity?.sourceCommit, fileSummary: report.fileSummary, browserError: report.browser?.error ?? report.browserError ?? null }));
if (report.provenanceError || report.fileSummary?.mismatched.length || report.browserError) process.exitCode = 1;
