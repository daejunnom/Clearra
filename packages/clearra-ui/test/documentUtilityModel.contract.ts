import assert from 'node:assert/strict';

import type { ClearraRenderArtifactPayload } from '../src/lib/wasm/wasmCommandClient';
import { buildDesktopAppRequest } from '../src/lib/host/clearraDesktopHost';
import {
  decodeValidatedRenderArtifact,
  detectFieldDocumentFormat,
  fumenDocumentInputs,
  isBoundedCanonicalFieldDocument,
  quoteWebCommandToken,
  validateFieldDocumentPayload
} from '../src/lib/workspace/documentUtilityModel';

assert.equal(detectFieldDocumentFormat('ctk3_w0kCERPPgGduYXRpdmWycg'), 'ctk3');
assert.equal(detectFieldDocumentFormat('v115@vhAAgH'), 'fumen');
assert.equal(detectFieldDocumentFormat('..........'), null);
assert.equal(isBoundedCanonicalFieldDocument('plain grid'), false);
assert.deepEqual(
  fumenDocumentInputs('v115@vhAAgH\nv115@vhAAQH', true),
  ['v115@vhAAgH', 'v115@vhAAQH']
);
assert.equal(quoteWebCommandToken('first comment'), '"first comment"');
assert.equal(quoteWebCommandToken('quote"slash\\'), '"quote\\"slash\\\\"');
assert.equal(
  validateFieldDocumentPayload({
    format: 'ctk3',
    document: 'ctk3_w0kCERPPgGduYXRpdmWycg',
    page_count: 1,
    canonical_sha256: 'a'.repeat(64),
    filename: 'clearra-field.ctk3.txt'
  }),
  null
);

for (const command of ['utility-to-gray', 'utility-mirror'] as const) {
  assert.deepEqual(buildDesktopAppRequest({
    command,
    language: 'ko',
    format: 'fumen',
    document: 'v115@vhAAgH'
  }), {
    app_request_model: 'clearra-app/AppRequest',
    command,
    language: 'ko',
    format: 'fumen',
    document: 'v115@vhAAgH'
  });
}

const pngSignature = Uint8Array.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
const sha256 = Array.from(
  new Uint8Array(await crypto.subtle.digest('SHA-256', pngSignature)),
  (byte) => byte.toString(16).padStart(2, '0')
).join('');
const artifact: ClearraRenderArtifactPayload = {
  document_format: 'ctk3',
  artifact_format: 'png',
  selected_page_number: 1,
  document_page_count: 1,
  media_type: 'image/png',
  filename: 'clearra-render-page-0001.png',
  byte_length: pngSignature.length,
  sha256,
  bytes_base64: btoa(String.fromCharCode(...pngSignature)),
  render_exact: true,
  skin_id: 'clearra-exact-default.v1',
  product_max_bytes: 1024,
  transport_max_bytes: 1024
};
assert.deepEqual(await decodeValidatedRenderArtifact(artifact), pngSignature);
await assert.rejects(
  decodeValidatedRenderArtifact({ ...artifact, sha256: '0'.repeat(64) }),
  /digest/u
);
await assert.rejects(
  decodeValidatedRenderArtifact({ ...artifact, bytes_base64: btoa('not-png!') }),
  /signature|byte length/u
);

console.log(JSON.stringify({
  document_authority: 'canonical-prefix-only',
  render_validation: 'length-signature-sha256',
  quoted_comments: 'lossless'
}));
