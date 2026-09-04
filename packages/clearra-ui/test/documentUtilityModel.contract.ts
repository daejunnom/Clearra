import assert from 'node:assert/strict';

import type { ClearraRenderArtifactPayload } from '../src/lib/wasm/wasmCommandClient';
import {
  buildDocumentUtilityCommand,
  buildDocumentUtilityCommandArguments,
  decodeValidatedRenderArtifact,
  detectFieldDocumentFormat,
  documentUtilityRequestForDesktop,
  fumenDocumentInputs,
  isBoundedCanonicalFieldDocument,
  validateFieldDocumentPayload
} from '../src/lib/workspace/documentUtilityModel';
import {
  buildOperationDocumentCommand,
  buildOperationDocumentCommandArguments,
  operationDocumentRequestForDesktop
} from '../src/lib/workspace/operationDocumentCommandModel';

assert.equal(detectFieldDocumentFormat('ctk3_w0kCERPPgGduYXRpdmWycg'), 'ctk3');
assert.equal(detectFieldDocumentFormat('v115@vhAAgH'), 'fumen');
assert.equal(detectFieldDocumentFormat('..........'), null);
assert.equal(isBoundedCanonicalFieldDocument('plain grid'), false);
assert.deepEqual(
  fumenDocumentInputs('v115@vhAAgH\nv115@vhAAQH', true),
  ['v115@vhAAgH', 'v115@vhAAQH']
);
const utilityInput = {
  tool: 'fumen' as const,
  format: 'fumen' as const,
  document: '',
  transform: 'text-to-fumen' as const,
  documents: [],
  pageNumber: 1,
  pageShift: 0,
  comments: ['comment with spaces', 'literal | && ` $(x) > < ; & quote"slash\\'],
  artifactFormat: 'png' as const
};
const utilityArguments = buildDocumentUtilityCommandArguments(utilityInput);
assert.equal(optionValue(utilityArguments, '--comment'), 'comment with spaces');
assert.match(buildDocumentUtilityCommand(utilityInput), /--comment "comment with spaces"/u);
assert.deepEqual(
  documentUtilityRequestForDesktop(utilityInput, 'ko'),
  {
    app_request_model: 'clearra-cli/CommandRequest',
    command: 'cli',
    language: 'ko',
    arguments: utilityArguments
  }
);

for (const capability of ['sequence', 'sequence-dependencies'] as const) {
  const operationInput = {
    capability,
    document: 'ctk3_document with-space',
    ruleProfile: 'srs-plus',
    kickProfile: 'srs-x',
    timeoutSeconds: 12
  };
  const arguments_ = buildOperationDocumentCommandArguments(operationInput);
  assert.equal(optionValue(arguments_, '--document'), 'ctk3_document with-space');
  assert.match(buildOperationDocumentCommand(operationInput), /--document "ctk3_document with-space"/u);
  assert.deepEqual(
    operationDocumentRequestForDesktop(operationInput, 'en').arguments,
    arguments_
  );
}
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
  const tool: 'to-gray' | 'mirror' = command === 'utility-to-gray' ? 'to-gray' : 'mirror';
  const input = {
    ...utilityInput,
    tool,
    format: 'fumen' as const,
    document: 'v115@vhAAgH'
  };
  assert.deepEqual(documentUtilityRequestForDesktop(input, 'ko'), {
    app_request_model: 'clearra-cli/CommandRequest',
    command: 'cli',
    language: 'ko',
    arguments: buildDocumentUtilityCommandArguments(input)
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

function optionValue(arguments_: readonly string[], option: string): string | undefined {
  const index = arguments_.indexOf(option);
  return index < 0 ? undefined : arguments_[index + 1];
}

console.log(JSON.stringify({
  document_authority: 'canonical-prefix-only',
  render_validation: 'length-signature-sha256',
  quoted_comments: 'lossless'
}));
