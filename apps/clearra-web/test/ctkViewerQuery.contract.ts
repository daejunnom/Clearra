import assert from 'node:assert/strict';

import { resolveCtkViewerQuery } from '../src/lib/ctkViewerQuery.ts';

const ctk = 'ctk3_AQID';
const fumen = 'v115@vhAAgH';

assert.deepEqual(
  resolveCtkViewerQuery(
    new URL(`https://example.test/Clearra/?tool=ctk&ctk=${encodeURIComponent(ctk)}&viewer=1`)
  ),
  { document: ctk, viewer: true }
);
assert.deepEqual(
  resolveCtkViewerQuery(
    new URL(`https://example.test/Clearra/?fumen=${encodeURIComponent(fumen)}`)
  ),
  { document: fumen, viewer: true }
);
assert.deepEqual(
  resolveCtkViewerQuery(
    new URL(`https://example.test/Clearra/?document=${encodeURIComponent(ctk)}`)
  ),
  { document: ctk, viewer: true }
);
assert.deepEqual(
  resolveCtkViewerQuery(new URL(`https://example.test/Clearra/?${fumen}`)),
  { document: fumen, viewer: true }
);
assert.deepEqual(
  resolveCtkViewerQuery(new URL('https://example.test/Clearra/?tool=pc')),
  { document: null, viewer: false }
);
