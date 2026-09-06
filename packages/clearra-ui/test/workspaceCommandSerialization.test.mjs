import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import {
  buildWorkspaceCommand,
  buildWorkspaceCommandArguments,
  createDefaultWorkspaceRequest,
  workspaceRequestForDesktop
} from '../src/lib/workspace/solverWorkspaceModel.ts';
import {
  cliCommandRequestForDesktop,
  serializeCliCommandArguments
} from '../src/lib/workspace/cliCommandModel.ts';

const canonicalGuiPcFullSolutionArguments = readFileSync(
  new URL('../../../tests/fixtures/contracts/gui_pc_full_solution_argv.tsv', import.meta.url),
  'utf8'
).trimEnd().split('\t');

test('browser command text and Desktop argv preserve the same quoted queue field', () => {
  const queue = 'I O"\\T';
  const request = {
    ...createDefaultWorkspaceRequest(),
    queue
  };
  const expectedArguments = buildWorkspaceCommandArguments(request);
  const browserCommand = buildWorkspaceCommand(request);
  const desktopRequest = workspaceRequestForDesktop(request, 'en');

  assert.equal(expectedArguments[expectedArguments.indexOf('--queue') + 1], queue);
  assert.deepEqual(tokenizeBrowserCommandForContract(browserCommand), expectedArguments);
  assert.deepEqual(desktopRequest.arguments, expectedArguments);
  assert.match(browserCommand, /--queue "I O\\"\\\\T"/u);
});

test('ordinary PC full solutions use one canonical argv envelope in browser and Desktop', () => {
  const request = {
    ...createDefaultWorkspaceRequest(),
    lines: 1,
    boardMask: 0x3fn,
    queue: '[I]',
    holdEnabled: false,
    scoreMode: 'off',
    backend: 'cpu',
    workers: 1
  };
  const arguments_ = buildWorkspaceCommandArguments(request);
  const browserCommand = buildWorkspaceCommand(request);
  const desktopRequest = workspaceRequestForDesktop(request, 'en');

  assert.deepEqual(arguments_, canonicalGuiPcFullSolutionArguments);
  assert.equal(arguments_.includes('--cpu-warmup'), false,
    'asynchronously ready browser workers must not enter the native all-worker barrier');
  assert.deepEqual(tokenizeBrowserCommandForContract(browserCommand), arguments_);
  assert.deepEqual(desktopRequest.arguments, arguments_);
});

test('minimum-cover GUI emits one canonical pc minimals command without a DTO count authority', () => {
  const request = {
    ...createDefaultWorkspaceRequest(),
    lines: 2,
    boardMask: 0n,
    queue: 'IIOOO',
    holdEnabled: false,
    scoreMode: 'minimum-cover',
    backend: 'cpu',
    workers: 1
  };
  const arguments_ = buildWorkspaceCommandArguments(request);
  const browserCommand = buildWorkspaceCommand(request);
  const desktopRequest = workspaceRequestForDesktop(request, 'ko');

  assert.deepEqual(arguments_.slice(0, 3), ['clearra', 'pc', 'minimals']);
  assert.equal(arguments_.includes('--count'), false);
  assert.deepEqual(tokenizeBrowserCommandForContract(browserCommand), arguments_);
  assert.deepEqual(desktopRequest.arguments, arguments_);
  assert.deepEqual(Object.keys(desktopRequest).sort(), [
    'app_request_model',
    'arguments',
    'command',
    'language'
  ]);
});

test('browser command text and Desktop argv preserve literal process markers and C0 whitespace', () => {
  const comment = "literal | && ` $(x) > < ; &\tline\nquote\" slash\\ apostrophe'\u0007";
  const expectedArguments = [
    'clearra',
    'utility',
    'fumen',
    'text-to-fumen',
    '--format',
    'fumen',
    '--comment',
    comment
  ];
  const browserCommand = serializeCliCommandArguments(expectedArguments);
  const desktopRequest = cliCommandRequestForDesktop(expectedArguments, 'ko');

  assert.deepEqual(tokenizeBrowserCommandForContract(browserCommand), expectedArguments);
  assert.deepEqual(desktopRequest.arguments, expectedArguments);
  assert.match(browserCommand, /--comment "/u);
  assert.throws(
    () => serializeCliCommandArguments([...expectedArguments, 'NUL\0value']),
    /must not contain NUL/u
  );
  assert.throws(
    () => cliCommandRequestForDesktop([...expectedArguments, 'NUL\0value'], 'en'),
    /must not contain NUL/u
  );
});

// Independent contract model for the browser WebCommandParser's closed quoted
// token grammar: only a quote or backslash may follow a quoted backslash.
function tokenizeBrowserCommandForContract(commandText) {
  const tokens = [];
  let token = '';
  let tokenStarted = false;
  let quoted = false;
  let escaped = false;
  for (const character of commandText) {
    if (quoted) {
      if (escaped) {
        assert.match(character, /["\\]/u);
        token += character;
        escaped = false;
      } else if (character === '\\') {
        escaped = true;
      } else if (character === '"') {
        quoted = false;
      } else {
        token += character;
      }
    } else if (character === '"') {
      quoted = true;
      tokenStarted = true;
    } else if (/\s/u.test(character)) {
      if (tokenStarted) {
        tokens.push(token);
        token = '';
        tokenStarted = false;
      }
    } else {
      token += character;
      tokenStarted = true;
    }
  }
  assert.equal(quoted || escaped, false);
  if (tokenStarted) tokens.push(token);
  return tokens;
}
