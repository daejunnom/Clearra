import assert from 'node:assert/strict';
import test from 'node:test';

import {
  DEFAULT_PLAYER_SETTINGS,
  DEFAULT_PLAYER_SCORE_MODEL,
  GUIDELINE_PLAYER_SCORE_MODEL,
  JSTRIS_ULTRA_PLAYER_SCORE_MODEL,
  PLAYER_BUILT_IN_SCORE_MODELS,
  PLAYER_BUILT_IN_SCORE_PROFILES,
  PLAYER_INSTANT_SDF,
  PLAYER_SCORE_PROFILES,
  PLAYER_SPIN_PROFILES,
  TETRIO_PLAYER_SCORE_MODEL,
  createPlayerSettingsStorageAdapter,
  deserializePlayerSettings,
  serializePlayerSettings,
  validatePlayerSettings,
} from '../src/lib/workspace/player/playerSettings.ts';

test('player defaults match the practice profile while handling stays within hard limits', () => {
  assert.deepEqual(
    {
      gravityG: DEFAULT_PLAYER_SETTINGS.gravityG,
      lockDelayMs: DEFAULT_PLAYER_SETTINGS.lockDelayMs,
      lockResetLimit: DEFAULT_PLAYER_SETTINGS.lockResetLimit,
      dasMs: DEFAULT_PLAYER_SETTINGS.dasMs,
      arrMs: DEFAULT_PLAYER_SETTINGS.arrMs,
      sdf: DEFAULT_PLAYER_SETTINGS.sdf,
    },
    { gravityG: 0.02, lockDelayMs: 500, lockResetLimit: 15, dasMs: 83, arrMs: 0, sdf: 41 },
  );
  assert.equal(PLAYER_INSTANT_SDF, 41);
  const wide = validatePlayerSettings({
    gravityG: 1000,
    lockDelayMs: 60_000,
    lockResetCap: 15,
    dasMs: 5000,
    arrMs: 5000,
  });
  assert.equal(wide.lockResetLimit, 15);
  assert.equal(DEFAULT_PLAYER_SETTINGS.kickProfile, 'srs-plus');
  assert.equal(DEFAULT_PLAYER_SETTINGS.spinProfile, 't-spins');
  assert.equal(DEFAULT_PLAYER_SETTINGS.scoreProfile, 'guideline');
  assert.equal(DEFAULT_PLAYER_SCORE_MODEL, GUIDELINE_PLAYER_SCORE_MODEL);
  assert.deepEqual(DEFAULT_PLAYER_SCORE_MODEL.lineClearScores, [0, 100, 300, 500, 800]);
  assert.deepEqual(DEFAULT_PLAYER_SCORE_MODEL.spinScores, [400, 800, 1200, 1600, 1600]);
  assert.equal(DEFAULT_PLAYER_SCORE_MODEL.comboBonusPerStep, 50);
  assert.equal(DEFAULT_PLAYER_SCORE_MODEL.backToBackMultiplier, 1.5);
  assert.deepEqual(PLAYER_SPIN_PROFILES, [
    't-spins',
    't-spins-plus',
    'all-spin',
    'all-spin-plus',
    'all-mini',
    'all-mini-plus',
  ]);
  assert.deepEqual(PLAYER_BUILT_IN_SCORE_PROFILES, [
    'tetrio',
    'guideline',
    'jstris-ultra',
  ]);
  assert.deepEqual(PLAYER_SCORE_PROFILES, [
    'tetrio',
    'guideline',
    'jstris-ultra',
    'custom',
  ]);
});

test('PC solver score profiles select immutable profile-exact Player models', () => {
  assert.equal(PLAYER_BUILT_IN_SCORE_MODELS.guideline, GUIDELINE_PLAYER_SCORE_MODEL);
  assert.equal(validatePlayerSettings({ scoreProfile: 'guideline' }).scoreModel, GUIDELINE_PLAYER_SCORE_MODEL);
  assert.deepEqual(GUIDELINE_PLAYER_SCORE_MODEL.perfectClearBonuses, [0, 800, 1200, 1800, 2000]);
  assert.equal(GUIDELINE_PLAYER_SCORE_MODEL.backToBackTetrisPerfectClearBonus, 3200);
  assert.equal(GUIDELINE_PLAYER_SCORE_MODEL.perfectClearMode, 'additive');

  const jstris = validatePlayerSettings({
    scoreProfile: 'jstris-ultra',
    scoreModel: { comboBonusPerStep: 999 },
  });
  assert.equal(jstris.scoreModel, JSTRIS_ULTRA_PLAYER_SCORE_MODEL);
  assert.deepEqual(jstris.scoreModel.miniSpinScores, [100, 200, 1200, 1600, 1600]);
  assert.deepEqual(jstris.scoreModel.perfectClearBonuses, [3000, 3000, 3000, 3000, 3000]);
  assert.equal(jstris.scoreModel.backToBackTetrisPerfectClearBonus, 3000);
  assert.equal(jstris.scoreModel.perfectClearMode, 'additive');
  assert.equal(jstris.scoreModel.softDropScorePerCell, 0);
  assert.equal(jstris.scoreModel.hardDropScorePerCell, 0);

  const tetrio = validatePlayerSettings({ scoreProfile: 'tetrio' });
  assert.equal(tetrio.scoreModel, TETRIO_PLAYER_SCORE_MODEL);
  assert.deepEqual(tetrio.scoreModel.spinScores, [400, 800, 1200, 1600, 2600]);
  assert.deepEqual(tetrio.scoreModel.miniSpinScores, [100, 200, 400, 800, 1600]);
  assert.deepEqual(tetrio.scoreModel.perfectClearBonuses, [3500, 3500, 3500, 3500, 3500]);
  assert.equal(tetrio.scoreModel.backToBackTetrisPerfectClearBonus, 3500);
  assert.equal(tetrio.scoreModel.perfectClearMode, 'replace-action');
  assert.equal(Object.isFrozen(tetrio.scoreModel), true);
});

test('player settings reject invalid numeric state before it reaches the engine', () => {
  assert.throws(() => validatePlayerSettings({ gravityG: Number.NaN }), /finite number/i);
  assert.throws(() => validatePlayerSettings({ lockResetLimit: 1.5 }), /integer/i);
  assert.throws(() => validatePlayerSettings({ lockResetLimit: 16 }), /between 0 and 15/i);
  assert.throws(() => validatePlayerSettings({ sdf: 42 }), /between 1 and 41/i);
  assert.throws(() => validatePlayerSettings({ arrMs: 5001 }), /between 0 and 5000/i);
  assert.throws(() => validatePlayerSettings({ kickProfile: 'unknown' }), /kickProfile/i);
  assert.throws(() => validatePlayerSettings({ spinProfile: 'unknown' }), /spinProfile/i);
  assert.throws(() => validatePlayerSettings({ clutchClear: 1 }), /clutchClear.*boolean/i);
  assert.throws(() => validatePlayerSettings({ unlimitedHold: 'yes' }), /unlimitedHold.*boolean/i);
  assert.throws(
    () => validatePlayerSettings({ scoreProfile: 'custom', scoreModel: { lineClearScores: [0, 1] } }),
    /exactly five/i,
  );
  assert.throws(
    () => validatePlayerSettings({ scoreProfile: 'custom', scoreModel: { comboBonusPerStep: 0.5 } }),
    /integer/i,
  );
});

test('guideline score coefficients support immutable partial user overrides', () => {
  const settings = validatePlayerSettings({
    kickProfile: 'srs',
    spinProfile: 'all-spin-plus',
    scoreProfile: 'custom',
    scoreModel: {
      lineClearScores: [0, 10, 20, 30, 40],
      comboBonusPerStep: 7,
      backToBackMultiplier: 2,
    },
  });
  assert.equal(settings.kickProfile, 'srs');
  assert.equal(settings.spinProfile, 'all-spin-plus');
  assert.equal(settings.scoreProfile, 'custom');
  assert.deepEqual(settings.scoreModel.lineClearScores, [0, 10, 20, 30, 40]);
  assert.deepEqual(settings.scoreModel.spinScores, DEFAULT_PLAYER_SCORE_MODEL.spinScores);
  assert.equal(settings.scoreModel.comboBonusPerStep, 7);
  assert.equal(settings.scoreModel.backToBackMultiplier, 2);
  assert.equal(Object.isFrozen(settings.scoreModel), true);
  assert.equal(Object.isFrozen(settings.scoreModel.lineClearScores), true);
});

test('guideline preset resets coefficients while custom preserves explicit values', () => {
  const custom = validatePlayerSettings({
    scoreProfile: 'custom',
    scoreModel: { comboBonusPerStep: 9 },
  });
  assert.equal(custom.scoreModel.comboBonusPerStep, 9);
  const guideline = validatePlayerSettings(
    { scoreProfile: 'guideline', scoreModel: { comboBonusPerStep: 999 } },
    custom,
  );
  assert.equal(guideline.scoreModel, DEFAULT_PLAYER_SCORE_MODEL);
  assert.equal(guideline.scoreModel.comboBonusPerStep, 50);
});

test('custom scoring preserves the selected perfect-clear operation', () => {
  const replacement = validatePlayerSettings({
    scoreProfile: 'custom',
    scoreModel: {
      perfectClearMode: 'replace-action',
      perfectClearBonuses: [7, 11, 13, 17, 19],
    },
  });
  assert.equal(replacement.scoreModel.perfectClearMode, 'replace-action');
  assert.deepEqual(replacement.scoreModel.perfectClearBonuses, [7, 11, 13, 17, 19]);
  assert.throws(
    () => validatePlayerSettings({
      scoreProfile: 'custom',
      scoreModel: { perfectClearMode: 'unknown' },
    }),
    /perfectClearMode/i,
  );
});

test('settings serialization is versioned and round trips without a DOM', () => {
  const customized = validatePlayerSettings({ gravityG: 2.5, arrMs: 12, sdf: 20 });
  assert.deepEqual(deserializePlayerSettings(serializePlayerSettings(customized)), customized);
  const customScore = validatePlayerSettings({
    scoreProfile: 'custom',
    scoreModel: { spinScores: [1, 2, 3, 4, 5], backToBackMultiplier: 2.25 },
  });
  assert.deepEqual(deserializePlayerSettings(serializePlayerSettings(customScore)), customScore);
  assert.throws(
    () => deserializePlayerSettings('{"version":999,"settings":{}}'),
    /unsupported player settings version/i,
  );
  const legacy = deserializePlayerSettings('{"version":1,"settings":{"gravityG":1}}');
  assert.equal(legacy.gravityG, 1);
  assert.equal(legacy.kickProfile, 'srs-plus');
  assert.equal(legacy.spinProfile, 't-spins');
  assert.equal(legacy.scoreProfile, 'guideline');
});

test('the storage adapter is dependency-injected and never reads global localStorage', () => {
  const values = new Map();
  const storage = {
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => values.set(key, value),
    removeItem: (key) => values.delete(key),
  };
  const adapter = createPlayerSettingsStorageAdapter(storage, 'test.player');
  assert.equal(adapter.load(), null);
  adapter.save({ gravityG: 0.5 });
  assert.equal(adapter.load().gravityG, 0.5);
  adapter.clear();
  assert.equal(adapter.load(), null);
});
