import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { componentMessage } from '../src/lib/i18n/componentCatalog.ts';
import { RELEASED_WORKSPACE_LANGUAGES } from '../src/lib/i18n/languageManifest.ts';
import {
  buildOperationDocumentCommand,
  buildOperationDocumentCommandArguments,
  operationDocumentRequestForDesktop
} from '../src/lib/workspace/operationDocumentCommandModel.ts';
import {
  operationDocumentWorkspaceModes,
  publicOperationDocumentReportFields
} from '../src/lib/workspace/operationDocumentWorkspaceModel.ts';
import { workspaceMessage } from '../src/lib/workspace/workspaceI18n.ts';

const workspaceSource = (file) => readFileSync(
  new URL(`../src/lib/workspace/${file}`, import.meta.url),
  'utf8'
);

test('public operation workspaces forward their own mode and both runtime props', () => {
  for (const [file, mode] of [
    ['OperationSequenceWorkspace.svelte', 'sequence'],
    ['SequenceDependenciesWorkspace.svelte', 'sequence-dependencies']
  ]) {
    assert.ok(workspaceSource(file).includes(
      `<OperationDocumentWorkspace mode="${mode}" {runtime} {workerFactory} />`
    ));
    const input = {
      capability: mode,
      document: 'v115@vhAAgH',
      ruleProfile: 'srs',
      kickProfile: 'srs-plus',
      timeoutSeconds: 42
    };
    const argv = [
      'clearra', 'utility', mode,
      '--document', input.document,
      '--rule-profile', 'srs',
      '--kick-profile', 'srs-plus',
      '--timeout-seconds', '42'
    ];
    assert.deepEqual(buildOperationDocumentCommandArguments(input), argv);
    assert.equal(buildOperationDocumentCommand(input), argv.join(' '));
    assert.deepEqual(operationDocumentRequestForDesktop(input, 'ko'), {
      app_request_model: 'clearra-cli/CommandRequest',
      command: 'cli',
      language: 'ko',
      arguments: argv
    });
  }

  const implementation = workspaceSource('OperationDocumentWorkspace.svelte');
  assert.match(implementation, /activeMode=\{mode\}/u);
  assert.match(implementation, /capability: mode/u);
  assert.match(implementation, /publicOperationDocumentReportFields\(/u);
});

test('operation reports keep their own localized human fields and hide machine identities', () => {
  const fields = [
    ['trace_key', 'trace'],
    ['exact_order_count', '20'],
    ['operation_count', '3'],
    ['__proto__', 'inherited'],
    ['cleared_line_count', '2'],
    ['constructor', 'inherited'],
    ['universal_dependency_count', '4'],
    ['rule_profile', 'srs'],
    ['transitive_reduction_count', '5'],
    ['normalized_trace', 'trace'],
    ['kick_profile', 'srs-plus'],
    ['independent_pair_count', '6'],
    ['representative_order', 'order'],
    ['toString', 'inherited'],
    ['candidate_id', '7'],
    ['operation_count', '8']
  ];
  for (const lang of RELEASED_WORKSPACE_LANGUAGES) {
    const message = (key) => componentMessage(lang, key);
    assert.deepEqual(publicOperationDocumentReportFields('sequence', fields, lang), [
      [message('operations'), '3'],
      [message('clearedLines'), '2'],
      [message('ruleProfile'), 'srs'],
      [message('kickProfile'), 'srs-plus'],
      [message('operations'), '8']
    ]);
    assert.deepEqual(publicOperationDocumentReportFields('sequence-dependencies', fields, lang), [
      [message('validOrders'), '20'],
      [message('operations'), '3'],
      [message('requiredPrecedenceRelations'), '4'],
      [message('essentialDependencyEdges'), '5'],
      [message('independentPairs'), '6'],
      [message('operations'), '8']
    ]);

    const sequence = operationDocumentWorkspaceModes.sequence;
    const dependencies = operationDocumentWorkspaceModes['sequence-dependencies'];
    assert.notEqual(workspaceMessage(lang, sequence.title), workspaceMessage(lang, dependencies.title));
    for (const key of ['description', 'documentLabel', 'documentHelp', 'resultTitle', 'emptyResult']) {
      assert.notEqual(message(sequence[key]), message(dependencies[key]), `${lang}/${key}`);
    }
  }
});
