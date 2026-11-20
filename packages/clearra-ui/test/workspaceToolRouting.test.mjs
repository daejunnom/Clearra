import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';

import { isWorkspaceMode, workspaceToolRoute } from '../src/lib/workspace/workspaceMode.ts';

function source(path) {
  return readFileSync(new URL(path, import.meta.url), 'utf8');
}

test('one registry admits all existing workspace links and determines their renderer', () => {
  const ordinary = [
    'pc', 'setup', 'setup-score', 'spin-structure', 'build', 'build-probability',
    'sequence', 'sequence-dependencies', 'recovery', 'ctk', 'player',
  ];
  for (const tool of ordinary) {
    assert.equal(isWorkspaceMode(tool), true);
    assert.deepEqual(workspaceToolRoute(tool), { kind: tool });
  }

  for (const tool of ['parity', 'fumen', 'render', 'to-gray', 'mirror']) {
    assert.equal(isWorkspaceMode(tool), true);
    assert.deepEqual(workspaceToolRoute(tool), { kind: 'document-utility', tool });
  }
  for (const tool of ['damage', 'spin-finder', 'ren']) {
    assert.equal(isWorkspaceMode(tool), true);
    assert.deepEqual(workspaceToolRoute(tool), { kind: 'forward-search', tool });
  }

  for (const value of [null, '', 'PC', 'unknown', '__proto__', 'constructor', 'toString']) {
    assert.equal(isWorkspaceMode(value), false);
  }
  assert.deepEqual(workspaceToolRoute(null), { kind: 'pc' });
});

test('host roots keep their own navigation and use one workspace dispatcher', () => {
  const web = source('../../../apps/clearra-web/src/routes/+page.svelte');
  const desktop = source('../../../apps/clearra-desktop/src/routes/+page.svelte');
  const router = source('../src/lib/workspace/WorkspaceToolRouter.svelte');

  assert.match(web, /<WorkspaceToolRouter\s+runtime="web"[\s\S]*?ctkDocument=\{ctkViewer\.document \?\? undefined\}[\s\S]*?\{workerFactory\}/u);
  assert.match(desktop, /<WorkspaceToolRouter runtime="desktop" selectedTool=\{workspaceMode\} \/>/u);
  assert.match(web, /searchParams\.get\('tool'\) \?\? \(ctkViewer\.document \? 'ctk' : null\)/u);
  assert.match(desktop, /searchParams\.get\('tool'\) \?\? 'pc'/u);

  // A standalone/deep link may open a public workspace without putting it in
  // the restricted Pages menu; link admission remains the registry's job.
  assert.match(web, /setContext\(WORKSPACE_MODE_VISIBILITY_CONTEXT, pagesModes\)/u);
  assert.doesNotMatch(desktop, /WORKSPACE_MODE_VISIBILITY_CONTEXT|new Worker\(/u);
  for (const host of [web, desktop]) {
    assert.match(host, /workspaceMode = isWorkspaceMode\(selectedTool\) \? selectedTool : null/u);
    assert.match(host, /if \(workspaceMode === null\)[\s\S]*?\/\?tool=pc/u);
  }

  // Changing the specific tool within either shared component must dispose the
  // previous instance and its jobs, not preserve stale form/output state.
  for (const name of ['DocumentUtilityWorkspace', 'ForwardSearchWorkspace']) {
    const keyedComponent = new RegExp(
      '\\{#key route\\.tool\\}\\s+<' + name + ' tool=\\{route\\.tool\\} \\{runtime\\} \\{workerFactory\\}',
      'u'
    );
    assert.match(router, keyedComponent);
  }
});
