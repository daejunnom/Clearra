import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

import { build } from 'esbuild';

const bundle = await build({
  bundle: true,
  entryPoints: [
    fileURLToPath(new URL('../src/lib/workspace/player/playerUiModel.ts', import.meta.url))
  ],
  format: 'esm',
  logLevel: 'silent',
  platform: 'node',
  write: false
});
const model = await import(
  `data:text/javascript;base64,${Buffer.from(bundle.outputFiles[0].text).toString('base64')}`
);

test('Player UI defaults preserve the selected practice handling profile', () => {
  const settings = model.createDefaultPlayerUiSettings();
  assert.equal(settings.gravityG, 0.02);
  assert.equal(settings.lastGravityG, 0.02);
  assert.equal(settings.lockDelayMs, 500);
  assert.equal(settings.dasMs, 83);
  assert.equal(settings.arrMs, 0);
  assert.equal(settings.softDropFactor, 41);
  assert.equal(settings.ghostOpacity, 0.55);
  assert.equal(settings.garbageLines, 0);
  assert.equal(settings.garbageHoleSpread, 50);
  assert.equal(settings.lockResetLimit, 15);
  assert.equal(settings.clutchClear, false);
  assert.equal(settings.unlimitedHold, false);
  assert.deepEqual(settings.initialQueue, []);
  assert.equal(settings.kickProfile, 'srs-plus');
  assert.equal(settings.spinProfile, 't-spins');
  assert.equal(settings.scoreProfile, 'guideline');
  assert.deepEqual(settings.scoreModel.lineClearScores, [0, 100, 300, 500, 800]);
  assert.equal(settings.bindings['hard-drop'], 'Space');
  assert.equal(settings.bindings['rotate-ccw'], 'KeyZ');
  assert.equal(settings.bindings['rotate-180'], 'KeyA');
  assert.equal(settings.bindings.hold, 'ShiftLeft');
  assert.equal(settings.bindings.undo, 'Control+KeyZ');
  assert.equal(settings.bindings.redo, 'Control+KeyY');
});

test('Player UI settings persistence is versioned and validated', () => {
  const settings = model.createDefaultPlayerUiSettings();
  settings.gravityG = 12.5;
  settings.initialQueue = ['T', 'I', 'O'];
  settings.bindings.pause = 'Escape';
  const restored = model.deserializePlayerUiSettings(
    model.serializePlayerUiSettings(settings)
  );
  assert.deepEqual(restored, settings);
  assert.notEqual(restored.bindings, settings.bindings);
  assert.notEqual(restored.initialQueue, settings.initialQueue);
  assert.notEqual(restored.scoreModel, settings.scoreModel);
  assert.notEqual(restored.scoreModel.lineClearScores, settings.scoreModel.lineClearScores);
  assert.throws(
    () => model.deserializePlayerUiSettings('{"version":1,"settings":{"gravityG":-1}}'),
    /incomplete|invalid/i
  );
});

test('version 1 and 2 Player UI settings preserve the original forced-placement delay', () => {
  const previous = model.createDefaultPlayerUiSettings();
  delete previous.garbageLines;
  delete previous.garbageHoleSpread;
  delete previous.kickProfile;
  delete previous.spinProfile;
  delete previous.scoreProfile;
  delete previous.scoreModel;
  delete previous.initialQueue;
  delete previous.clutchClear;
  delete previous.unlimitedHold;
  delete previous.bindings.undo;
  delete previous.bindings.redo;
  previous.lockDelayMs = 500;
  previous.bindings['rotate-ccw'] = 'ControlLeft';
  for (const version of [1, 2]) {
    const restored = model.deserializePlayerUiSettings(
      JSON.stringify({ version, settings: previous })
    );
    assert.equal(restored.garbageLines, 0);
    assert.equal(restored.garbageHoleSpread, 50);
    assert.equal(restored.lastGravityG, previous.gravityG);
    assert.equal(restored.lockDelayMs, 500);
    assert.deepEqual(restored.initialQueue, []);
    assert.equal(restored.kickProfile, 'srs-plus');
    assert.equal(restored.spinProfile, 't-spins');
    assert.equal(restored.scoreProfile, 'guideline');
    assert.equal(restored.clutchClear, false);
    assert.equal(restored.unlimitedHold, false);
    assert.equal(restored.bindings.undo, 'Control+KeyZ');
    assert.equal(restored.bindings.redo, 'Control+KeyY');
    assert.equal(restored.bindings['rotate-ccw'], 'KeyZ');
  }
});

test('version 3 through 5 restore the former 500 ms default and remember disabled gravity', () => {
  for (const version of [3, 4, 5]) {
    const previous = model.createDefaultPlayerUiSettings();
    delete previous.lastGravityG;
    previous.gravityG = 0;
    previous.lockDelayMs = 800;
    const restored = model.deserializePlayerUiSettings(
      JSON.stringify({ version, settings: previous })
    );
    assert.equal(restored.gravityG, 0);
    assert.equal(restored.lastGravityG, 0.02);
    assert.equal(restored.lockDelayMs, 500);
  }
});

test('gravity restoration value is persisted and must remain positive', () => {
  const settings = model.createDefaultPlayerUiSettings();
  settings.gravityG = 0;
  settings.lastGravityG = 2.5;
  const restored = model.deserializePlayerUiSettings(
    model.serializePlayerUiSettings(settings)
  );
  assert.equal(restored.gravityG, 0);
  assert.equal(restored.lastGravityG, 2.5);

  settings.lastGravityG = 0;
  assert.throws(() => model.serializePlayerUiSettings(settings), /gravityG/i);
});

test('gravity toggle writes zero and restores the latest nonzero value', () => {
  const settings = model.createDefaultPlayerUiSettings();
  settings.gravityG = 1.25;
  settings.lastGravityG = 1.25;

  const disabled = model.togglePlayerGravity(settings);
  assert.equal(disabled.gravityG, 0);
  assert.equal(disabled.lastGravityG, 1.25);
  assert.equal(settings.gravityG, 1.25);

  const restored = model.togglePlayerGravity(disabled);
  assert.equal(restored.gravityG, 1.25);
  assert.equal(restored.lastGravityG, 1.25);
});

test('version 4 clamps the former lock-reset range and adds practice toggles safely', () => {
  const previous = model.createDefaultPlayerUiSettings();
  previous.lockResetLimit = 999;
  delete previous.clutchClear;
  delete previous.unlimitedHold;
  delete previous.bindings.undo;
  delete previous.bindings.redo;

  const restored = model.deserializePlayerUiSettings(
    JSON.stringify({ version: 4, settings: previous })
  );
  assert.equal(restored.lockResetLimit, 15);
  assert.equal(restored.clutchClear, false);
  assert.equal(restored.unlimitedHold, false);
  assert.equal(restored.bindings.undo, 'Control+KeyZ');
  assert.equal(restored.bindings.redo, 'Control+KeyY');
});

test('version 3 migrates the former Ctrl default without stealing an assigned Z key', () => {
  const legacyDefault = model.createDefaultPlayerUiSettings();
  legacyDefault.bindings['rotate-ccw'] = 'ControlLeft';
  const migrated = model.deserializePlayerUiSettings(
    JSON.stringify({ version: 3, settings: legacyDefault })
  );
  assert.equal(migrated.bindings['rotate-ccw'], 'KeyZ');

  const custom = model.createDefaultPlayerUiSettings();
  custom.bindings['rotate-ccw'] = 'ControlLeft';
  custom.bindings['hard-drop'] = 'KeyZ';
  const preserved = model.deserializePlayerUiSettings(
    JSON.stringify({ version: 3, settings: custom })
  );
  assert.equal(preserved.bindings['rotate-ccw'], 'ControlLeft');
  assert.equal(preserved.bindings['hard-drop'], 'KeyZ');
});

test('starting NEXT accepts case-insensitive compact, spaced, and comma-separated queues', () => {
  for (const source of ['iotszjl', 'I O T S Z J L', 'i, o,t, s,z,j,l', ' I,O T\nS,Z,J,L ']) {
    const parsed = model.parsePlayerInitialQueue(source);
    assert.equal(parsed.ok, true);
    assert.deepEqual(parsed.queue, ['I', 'O', 'T', 'S', 'Z', 'J', 'L']);
    assert.equal(parsed.canonical, 'IOTSZJL');
  }
});

test('an empty starting NEXT restores random-bag mode and malformed queues stay invalid', () => {
  assert.deepEqual(model.parsePlayerInitialQueue('  , \n'), {
    ok: true,
    queue: [],
    canonical: ''
  });
  assert.deepEqual(model.parsePlayerInitialQueue('IOX'), {
    ok: false,
    reason: 'invalid-piece',
    invalidToken: 'X'
  });
  assert.equal(
    model.parsePlayerInitialQueue('I'.repeat(model.PLAYER_INITIAL_QUEUE_MAX_PIECES + 1)).reason,
    'too-long'
  );
});

test('starting NEXT persistence normalizes and clones its exact engine queue', () => {
  const parsed = model.parsePlayerInitialQueue('t, i o');
  assert.equal(parsed.ok, true);
  const settings = model.createDefaultPlayerUiSettings();
  settings.initialQueue = parsed.queue;
  const restored = model.deserializePlayerUiSettings(
    model.serializePlayerUiSettings(settings)
  );

  assert.deepEqual(restored.initialQueue, ['T', 'I', 'O']);
  assert.equal(model.formatPlayerInitialQueue(restored.initialQueue), 'TIO');
  assert.notEqual(restored.initialQueue, settings.initialQueue);

  settings.initialQueue = ['T', 'X'];
  assert.throws(() => model.serializePlayerUiSettings(settings), /initialQueue/i);
});

test('custom score tables round-trip without sharing mutable arrays', () => {
  const settings = model.createDefaultPlayerUiSettings();
  settings.scoreProfile = 'custom';
  settings.scoreModel = {
    ...settings.scoreModel,
    lineClearScores: [0, 125, 350, 600, 900]
  };
  const restored = model.deserializePlayerUiSettings(
    model.serializePlayerUiSettings(settings)
  );
  assert.equal(restored.scoreProfile, 'custom');
  assert.deepEqual(restored.scoreModel.lineClearScores, [0, 125, 350, 600, 900]);
  assert.notEqual(restored.scoreModel.lineClearScores, settings.scoreModel.lineClearScores);
});

test('guideline persistence canonicalizes the built-in score table', () => {
  const settings = model.createDefaultPlayerUiSettings();
  const payload = JSON.parse(model.serializePlayerUiSettings(settings));
  payload.settings.scoreModel.lineClearScores[1] = 999;
  const restored = model.deserializePlayerUiSettings(JSON.stringify(payload));
  assert.equal(restored.scoreProfile, 'guideline');
  assert.equal(restored.scoreModel.lineClearScores[1], 100);

  settings.scoreModel = {
    ...settings.scoreModel,
    lineClearScores: [0, 999, 300, 500, 800]
  };
  assert.throws(() => model.serializePlayerUiSettings(settings), /scoreModel/i);
});

test('key reassignment rejects ambiguous event.code mappings', () => {
  const settings = model.createDefaultPlayerUiSettings();
  assert.deepEqual(
    model.assignPlayerKeyBinding(settings, 'hold', 'Space'),
    { ok: false, reason: 'duplicate-code', conflict: 'hard-drop' }
  );
  const assignment = model.assignPlayerKeyBinding(settings, 'hold', 'KeyC');
  assert.equal(assignment.ok, true);
  assert.equal(assignment.settings.bindings.hold, 'KeyC');
  assert.equal(settings.bindings.hold, 'ShiftLeft');
});

test('history shortcuts are configurable modifier chords without stealing bare piece controls', () => {
  const settings = model.createDefaultPlayerUiSettings();
  assert.equal(
    model.playerKeyboardShortcutFromEvent({ code: 'KeyZ', ctrlKey: true }),
    'Control+KeyZ'
  );
  assert.equal(
    model.playerKeyboardShortcutMatches(
      { code: 'KeyZ', ctrlKey: true },
      settings.bindings.undo
    ),
    true
  );
  assert.equal(
    model.playerKeyboardShortcutMatches({ code: 'KeyZ' }, settings.bindings.undo),
    false
  );
  assert.equal(model.playerKeyboardCodeLabel('Control+Shift+KeyZ'), 'Ctrl + Shift + Z');

  const conflict = model.assignPlayerKeyBinding(settings, 'undo', 'Space');
  assert.deepEqual(conflict, {
    ok: false,
    reason: 'duplicate-code',
    conflict: 'hard-drop'
  });
  const assignment = model.assignPlayerKeyBinding(settings, 'undo', 'Alt+KeyU');
  assert.equal(assignment.ok, true);
  assert.equal(assignment.settings.bindings.undo, 'Alt+KeyU');
  assert.deepEqual(
    model.assignPlayerKeyBinding(settings, 'hold', 'Alt+KeyU'),
    { ok: false, reason: 'invalid-code' }
  );
});

test('built-in score profiles restore their canonical PC-search score model', () => {
  const settings = model.createDefaultPlayerUiSettings();
  settings.scoreProfile = 'tetrio';
  settings.scoreModel = {
    ...settings.scoreModel,
    lineClearScores: [0, 999, 999, 999, 999]
  };
  assert.throws(() => model.serializePlayerUiSettings(settings), /scoreModel/i);

  const payload = JSON.parse(model.serializePlayerUiSettings(model.createDefaultPlayerUiSettings()));
  payload.settings.scoreProfile = 'tetrio';
  payload.settings.scoreModel = settings.scoreModel;
  const restored = model.deserializePlayerUiSettings(JSON.stringify(payload));
  assert.equal(restored.scoreProfile, 'tetrio');
  assert.deepEqual(restored.scoreModel.lineClearScores, [0, 100, 300, 500, 800]);
  assert.equal(restored.scoreModel.perfectClearMode, 'replace-action');
});
