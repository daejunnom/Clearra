import assert from "node:assert/strict";
import test from "node:test";

import {
  validateRemoteAnnotatedTagOutput,
  verifyRemoteAnnotatedTag,
} from "./verify-remote-annotated-tag.mjs";

const tag = "v0.7.5";
const tagObject = "1111111111111111111111111111111111111111";
const expectedCommit = "2222222222222222222222222222222222222222";
const movedCommit = "3333333333333333333333333333333333333333";
const tagRef = `refs/tags/${tag}`;
const exactAnnotatedOutput = [
  `${tagObject}\t${tagRef}`,
  `${expectedCommit}\t${tagRef}^{}`,
  "",
].join("\n");

test("exact annotated remote tag succeeds and queries only its object and peeled ref", () => {
  const result = verifyRemoteAnnotatedTag({
    tag,
    expectedCommit,
    runGit(command, args, options) {
      assert.equal(command, "git");
      assert.deepEqual(args, ["ls-remote", "origin", tagRef, `${tagRef}^{}`]);
      assert.equal(options.encoding, "utf8");
      return {
        error: undefined,
        signal: null,
        status: 0,
        stdout: exactAnnotatedOutput,
      };
    },
  });

  assert.deepEqual(result, { tag, tagObject, peeledCommit: expectedCommit });
});

test("moved annotated remote tag fails closed", () => {
  const output = [
    `${tagObject}\t${tagRef}`,
    `${movedCommit}\t${tagRef}^{}`,
    "",
  ].join("\n");
  assert.throws(
    () => validateRemoteAnnotatedTagOutput(output, { tag, expectedCommit }),
    /moved or resolves to a different commit/,
  );
});

test("lightweight remote tag fails closed", () => {
  assert.throws(
    () =>
      validateRemoteAnnotatedTagOutput(`${expectedCommit}\t${tagRef}\n`, {
        tag,
        expectedCommit,
      }),
    /lightweight, not annotated/,
  );
});

test("missing remote tag fails closed", () => {
  assert.throws(
    () => validateRemoteAnnotatedTagOutput("", { tag, expectedCommit }),
    /is missing/,
  );
});

test("malformed remote tag response fails closed", () => {
  assert.throws(
    () =>
      validateRemoteAnnotatedTagOutput(`not-a-sha\t${tagRef}\n`, {
        tag,
        expectedCommit,
      }),
    /response is malformed/,
  );
});

test("ambiguous remote tag response fails closed", () => {
  const duplicateOutput = [
    `${tagObject}\t${tagRef}`,
    `${tagObject}\t${tagRef}`,
    `${expectedCommit}\t${tagRef}^{}`,
    "",
  ].join("\n");
  assert.throws(
    () =>
      validateRemoteAnnotatedTagOutput(duplicateOutput, {
        tag,
        expectedCommit,
      }),
    /ambiguous: duplicate ref/,
  );
});
