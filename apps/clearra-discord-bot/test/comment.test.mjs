import assert from "node:assert/strict";
import test from "node:test";

import { encodeCtk3 } from "ctk3";
import { encoder as fumenEncoder, Field } from "tetris-fumen";

import { Clearrabot } from "../src/bot.mjs";
import {
  normalizeViewerComment,
  paintViewerCommentPanel,
  prepareViewerCommentPanels,
} from "../src/viewer/comment.mjs";
import { decodeViewerDocument } from "../src/viewer/document.mjs";
import { renderDocumentGif } from "../src/viewer/gif.mjs";

test("viewer comments normalize untrusted text without interpreting markup", () => {
  const normalized = normalizeViewerComment(
    "\0  <b>@everyone&</b>\r\n\u202e\u1112\u1161\u11ab\u1100\u1173\u11af\t\uc8fc\uc11d  ",
  );
  assert.equal(normalized, "<b>@everyone&</b>\n\ud55c\uae00 \uc8fc\uc11d");
  assert.equal(normalizeViewerComment(" \0\u202e\t\r\n "), "");

  const bounded = normalizeViewerComment("\ud83d\ude00".repeat(200));
  assert.equal([...bounded].length, 160);
  assert.equal(bounded.endsWith("…"), true);
  assert.equal(
    [...bounded.slice(0, -1)].every((character) => character === "\ud83d\ude00"),
    true,
  );
});

test("comment layout wraps and rasterizes distinct Hangul syllables", () => {
  const panel = prepareViewerCommentPanels([
    { comment: "\ud55c\uae00 \uc8fc\uc11d \ud14c\uc2a4\ud2b8" },
    { comment: "\uac00\ub098\ub2e4" },
  ], 80);
  assert.ok(panel);
  assert.deepEqual(panel.linesByPage[0], ["\ud55c\uae00 \uc8fc\uc11d", "\ud14c\uc2a4\ud2b8"]);
  assert.equal(panel.linesByPage.length, 2);
  assert.equal(panel.height <= 51, true);

  const first = new Uint8Array(panel.width * panel.height);
  const second = new Uint8Array(panel.width * panel.height);
  paintViewerCommentPanel(first, panel.width, 0, panel, 0);
  paintViewerCommentPanel(second, panel.width, 0, panel, 1);
  assert.equal(first.some((pixel) => pixel === 12), true);
  assert.equal(second.some((pixel) => pixel === 12), true);
  assert.notDeepEqual(first, second);
});

test("empty comments keep the original board-only GIF dimensions", () => {
  const pages = [{ height: 0, cells: [], comment: " \n\t\u202e" }];
  assert.equal(prepareViewerCommentPanels(pages, 80), null);
  const gif = renderDocumentGif({ width: 10, pages }, {
    tileSize: 8,
    maxBytes: 1024 * 1024,
  });
  assert.deepEqual(logicalScreenSize(gif), { width: 80, height: 32 });
});

test("CTK3 and Fumen page comments render below the board as inert GIF pixels", () => {
  const comment = "<b>@everyone</b> \ud55c\uae00 \uc8fc\uc11d";
  const ctk3 = encodeCtk3({
    width: 10,
    pages: [{ height: 1, cells: ["T", ...Array(9).fill(null)], comment }],
  });
  const fumen = fumenEncoder.encode([{
    field: Field.create("T_________"),
    comment,
  }]);

  for (const source of [ctk3, fumen]) {
    const decoded = decodeViewerDocument(source);
    assert.equal(decoded.pages[0].comment, comment);
    const gif = renderDocumentGif(decoded, {
      tileSize: 8,
      maxBytes: 1024 * 1024,
    });
    assert.equal(logicalScreenSize(gif).height > 32, true);
    assert.equal(new TextDecoder().decode(gif).includes("@everyone"), false);
    assert.equal(gif.at(-1), 0x3b);
  }
});

test("Discord preview attachment keeps comments inside the GIF only", async () => {
  const comment = "@everyone <script> \ud55c\uae00 \uc8fc\uc11d";
  const source = encodeCtk3({
    width: 10,
    pages: [{ height: 0, cells: [], comment }],
  });
  const decoded = decodeViewerDocument(source);
  const bot = new Clearrabot({}, {
    oracleMaxGifBytes: 1024 * 1024,
    oracleMaxPages: 128,
    maxConcurrentSearches: 1,
  }, {
    executor: { async execute() { throw new Error("must not search"); } },
    gifRenderer: {
      async render(document, options) {
        return renderDocumentGif(document, { ...options, tileSize: 8 });
      },
      stop() {},
    },
  });

  try {
    const outgoing = await bot.buildOraclePreviewMessage(
      { format: "ctk3", source, document: decoded },
      "Preview ready.",
      "en",
    );
    assert.equal(outgoing.files.length, 1);
    assert.equal(outgoing.files[0].name, "clearra-input-preview.gif");
    assert.equal(logicalScreenSize(outgoing.files[0].bytes).height > 32, true);
    assert.equal(outgoing.payload.content, "Preview ready.");
    assert.deepEqual(outgoing.payload.allowed_mentions, { parse: [] });
    assert.equal(outgoing.payload.content.includes("@everyone"), false);
  } finally {
    bot.stop();
  }
});

test("long multi-line comments are bounded to three rendered lines with an ellipsis", () => {
  const panel = prepareViewerCommentPanels([{
    comment: Array.from({ length: 20 }, (_, index) => `line-${index}`).join("\n"),
  }], 80);
  assert.ok(panel);
  assert.equal(panel.linesByPage[0].length, 3);
  assert.equal(panel.linesByPage[0].at(-1).endsWith("…"), true);
  assert.equal(panel.height, 51);
});

function logicalScreenSize(bytes) {
  const word = (offset) => bytes[offset] | (bytes[offset + 1] << 8);
  return { width: word(6), height: word(8) };
}
