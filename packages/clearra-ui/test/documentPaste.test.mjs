import assert from 'node:assert/strict';
import test from 'node:test';

import { selectDocumentPastePayload } from '../src/lib/workspace/documentPaste.ts';

test('CTK3 file items take priority over clipboard text', () => {
  const file = { name: 'opening.ctk3', type: 'application/vnd.clearra.ctk3' };
  let textReads = 0;
  const payload = selectDocumentPastePayload(
    {
      files: [],
      items: [
        { kind: 'string' },
        { kind: 'file', getAsFile: () => file }
      ],
      getData: () => {
        textReads += 1;
        return 'ctk3_browser_text';
      }
    },
    (candidate) => candidate.name.endsWith('.ctk3'),
    (source) => source.startsWith('ctk3_')
  );

  assert.deepEqual(payload, { kind: 'file', file });
  assert.equal(textReads, 0);
});

test('clipboard text remains available when no CTK3 file exists', () => {
  const payload = selectDocumentPastePayload(
    {
      files: [{ name: 'notes.txt' }],
      items: [],
      getData: () => '  ctk3_text_document  '
    },
    (candidate) => candidate.name.endsWith('.ctk3'),
    (source) => source.startsWith('ctk3_')
  );

  assert.deepEqual(payload, { kind: 'text', source: 'ctk3_text_document' });
});
