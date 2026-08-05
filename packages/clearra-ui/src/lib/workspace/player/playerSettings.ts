import {
  isPlayerKickProfile,
  type PlayerKickProfile,
} from "./playerRules.ts";

export const PLAYER_SETTINGS_VERSION = 1;
export const PLAYER_SETTINGS_STORAGE_KEY = "clearra.player.settings.v1";
export const PLAYER_INSTANT_SDF = 41;

// Base profiles use exact T three-corner recognition. `plus` adds an immobile
// T-mini fallback; `all-spin` awards immobile non-T rotations as regular spins,
// while `all-mini` maps those non-T rotations to the mini score table.
export const PLAYER_SPIN_PROFILES = [
  "t-spins",
  "t-spins-plus",
  "all-spin",
  "all-spin-plus",
  "all-mini",
  "all-mini-plus",
] as const;
export type PlayerSpinProfile = (typeof PLAYER_SPIN_PROFILES)[number];

// Keep the built-in ids and ordering aligned with the PC solver's ScoreProfile
// contract. Custom remains a Player-only escape hatch for the advanced editor.
export const PLAYER_BUILT_IN_SCORE_PROFILES = [
  "tetrio",
  "guideline",
  "jstris-ultra",
] as const;
export type PlayerBuiltInScoreProfile =
  (typeof PLAYER_BUILT_IN_SCORE_PROFILES)[number];
export const PLAYER_SCORE_PROFILES = [
  ...PLAYER_BUILT_IN_SCORE_PROFILES,
  "custom",
] as const;
export type PlayerScoreProfile = (typeof PLAYER_SCORE_PROFILES)[number];

export const PLAYER_PERFECT_CLEAR_MODES = ["additive", "replace-action"] as const;
export type PlayerPerfectClearMode = (typeof PLAYER_PERFECT_CLEAR_MODES)[number];

export type PlayerScoreTable = readonly [number, number, number, number, number];

export type PlayerScoreModel = Readonly<{
  lineClearScores: PlayerScoreTable;
  spinScores: PlayerScoreTable;
  miniSpinScores: PlayerScoreTable;
  perfectClearBonuses: PlayerScoreTable;
  backToBackTetrisPerfectClearBonus: number;
  comboBonusPerStep: number;
  backToBackMultiplier: number;
  softDropScorePerCell: number;
  hardDropScorePerCell: number;
  // In replace-action mode perfectClearBonuses stores the final PC action
  // score by cleared line count rather than an amount added to actionScore.
  perfectClearMode: PlayerPerfectClearMode;
}>;

export type PlayerScoreModelInput = Partial<{
  lineClearScores: readonly number[];
  spinScores: readonly number[];
  miniSpinScores: readonly number[];
  perfectClearBonuses: readonly number[];
  backToBackTetrisPerfectClearBonus: number;
  comboBonusPerStep: number;
  backToBackMultiplier: number;
  softDropScorePerCell: number;
  hardDropScorePerCell: number;
  perfectClearMode: PlayerPerfectClearMode;
}>;

export const GUIDELINE_PLAYER_SCORE_MODEL: PlayerScoreModel = Object.freeze({
  lineClearScores: scoreTable([0, 100, 300, 500, 800]),
  spinScores: scoreTable([400, 800, 1200, 1600, 1600]),
  miniSpinScores: scoreTable([100, 200, 400, 800, 800]),
  perfectClearBonuses: scoreTable([0, 800, 1200, 1800, 2000]),
  backToBackTetrisPerfectClearBonus: 3200,
  comboBonusPerStep: 50,
  backToBackMultiplier: 1.5,
  softDropScorePerCell: 1,
  hardDropScorePerCell: 2,
  perfectClearMode: "additive",
});

export const JSTRIS_ULTRA_PLAYER_SCORE_MODEL: PlayerScoreModel = Object.freeze({
  lineClearScores: scoreTable([0, 100, 300, 500, 800]),
  spinScores: scoreTable([400, 800, 1200, 1600, 1600]),
  miniSpinScores: scoreTable([100, 200, 1200, 1600, 1600]),
  perfectClearBonuses: scoreTable([3000, 3000, 3000, 3000, 3000]),
  backToBackTetrisPerfectClearBonus: 3000,
  comboBonusPerStep: 50,
  backToBackMultiplier: 1.5,
  softDropScorePerCell: 0,
  hardDropScorePerCell: 0,
  perfectClearMode: "additive",
});

export const TETRIO_PLAYER_SCORE_MODEL: PlayerScoreModel = Object.freeze({
  lineClearScores: scoreTable([0, 100, 300, 500, 800]),
  spinScores: scoreTable([400, 800, 1200, 1600, 2600]),
  miniSpinScores: scoreTable([100, 200, 400, 800, 1600]),
  perfectClearBonuses: scoreTable([3500, 3500, 3500, 3500, 3500]),
  backToBackTetrisPerfectClearBonus: 3500,
  comboBonusPerStep: 50,
  backToBackMultiplier: 1.5,
  softDropScorePerCell: 1,
  hardDropScorePerCell: 2,
  perfectClearMode: "replace-action",
});

export const PLAYER_BUILT_IN_SCORE_MODELS: Readonly<
  Record<PlayerBuiltInScoreProfile, PlayerScoreModel>
> = Object.freeze({
  guideline: GUIDELINE_PLAYER_SCORE_MODEL,
  "jstris-ultra": JSTRIS_ULTRA_PLAYER_SCORE_MODEL,
  tetrio: TETRIO_PLAYER_SCORE_MODEL,
});

export const DEFAULT_PLAYER_SCORE_MODEL = GUIDELINE_PLAYER_SCORE_MODEL;

export type PlayerSettings = Readonly<{
  gravityG: number;
  lockDelayMs: number;
  lockResetLimit: number;
  dasMs: number;
  arrMs: number;
  sdf: number;
  fixedStepMs: number;
  maxCatchUpSteps: number;
  previewCount: number;
  kickProfile: PlayerKickProfile;
  spinProfile: PlayerSpinProfile;
  scoreProfile: PlayerScoreProfile;
  scoreModel: PlayerScoreModel;
  clutchClear: boolean;
  unlimitedHold: boolean;
}>;

export type PlayerSettingsInput = Omit<Partial<PlayerSettings>, "scoreModel"> & {
  lockResetCap?: number;
  scoreModel?: PlayerScoreModelInput;
};

export const DEFAULT_PLAYER_SETTINGS: PlayerSettings = Object.freeze({
  gravityG: 0.02,
  lockDelayMs: 500,
  lockResetLimit: 15,
  dasMs: 83,
  arrMs: 0,
  sdf: PLAYER_INSTANT_SDF,
  fixedStepMs: 1000 / 60,
  maxCatchUpSteps: 8,
  previewCount: 5,
  kickProfile: "srs-plus",
  spinProfile: "t-spins",
  scoreProfile: "guideline",
  scoreModel: DEFAULT_PLAYER_SCORE_MODEL,
  clutchClear: false,
  unlimitedHold: false,
});

export interface PlayerSettingsStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem?(key: string): void;
}

export interface PlayerSettingsStorageAdapter {
  load(): PlayerSettings | null;
  save(settings: PlayerSettingsInput): PlayerSettings;
  clear(): void;
}

export function validatePlayerSettings(
  input: PlayerSettingsInput = {},
  base: PlayerSettings = DEFAULT_PLAYER_SETTINGS,
): PlayerSettings {
  if (input === null || typeof input !== "object" || Array.isArray(input)) {
    throw new TypeError("Player settings must be an object.");
  }
  const lockResetLimit = input.lockResetLimit ?? input.lockResetCap ?? base.lockResetLimit;
  const scoreProfile = enumValue(
    "scoreProfile",
    input.scoreProfile ?? base.scoreProfile,
    PLAYER_SCORE_PROFILES,
  );
  const scoreModel = scoreProfile === "custom"
    ? validatePlayerScoreModel(
        input.scoreModel ?? {},
        base.scoreProfile === "custom" ? base.scoreModel : DEFAULT_PLAYER_SCORE_MODEL,
      )
    : playerScoreModelForProfile(scoreProfile);
  return Object.freeze({
    gravityG: finiteNumber("gravityG", input.gravityG ?? base.gravityG, 0, 1000),
    lockDelayMs: finiteNumber(
      "lockDelayMs",
      input.lockDelayMs ?? base.lockDelayMs,
      0,
      60_000,
    ),
    lockResetLimit: integer(
      "lockResetLimit",
      lockResetLimit,
      0,
      15,
    ),
    dasMs: finiteNumber("dasMs", input.dasMs ?? base.dasMs, 0, 5000),
    arrMs: finiteNumber("arrMs", input.arrMs ?? base.arrMs, 0, 5000),
    sdf: finiteNumber("sdf", input.sdf ?? base.sdf, 1, PLAYER_INSTANT_SDF),
    fixedStepMs: finiteNumber(
      "fixedStepMs",
      input.fixedStepMs ?? base.fixedStepMs,
      1,
      50,
    ),
    maxCatchUpSteps: integer(
      "maxCatchUpSteps",
      input.maxCatchUpSteps ?? base.maxCatchUpSteps,
      1,
      32,
    ),
    previewCount: integer(
      "previewCount",
      input.previewCount ?? base.previewCount,
      1,
      14,
    ),
    kickProfile: kickProfile(input.kickProfile ?? base.kickProfile),
    spinProfile: enumValue(
      "spinProfile",
      input.spinProfile ?? base.spinProfile,
      PLAYER_SPIN_PROFILES,
    ),
    scoreProfile,
    scoreModel,
    clutchClear: booleanValue(
      "clutchClear",
      input.clutchClear ?? base.clutchClear,
    ),
    unlimitedHold: booleanValue(
      "unlimitedHold",
      input.unlimitedHold ?? base.unlimitedHold,
    ),
  });
}

export function validatePlayerScoreModel(
  input: PlayerScoreModelInput = {},
  base: PlayerScoreModel = DEFAULT_PLAYER_SCORE_MODEL,
): PlayerScoreModel {
  if (input === null || typeof input !== "object" || Array.isArray(input)) {
    throw new TypeError("Player score model must be an object.");
  }
  return Object.freeze({
    lineClearScores: scoreTable(input.lineClearScores ?? base.lineClearScores, "lineClearScores"),
    spinScores: scoreTable(input.spinScores ?? base.spinScores, "spinScores"),
    miniSpinScores: scoreTable(input.miniSpinScores ?? base.miniSpinScores, "miniSpinScores"),
    perfectClearBonuses: scoreTable(
      input.perfectClearBonuses ?? base.perfectClearBonuses,
      "perfectClearBonuses",
    ),
    backToBackTetrisPerfectClearBonus: scoreCoefficient(
      "backToBackTetrisPerfectClearBonus",
      input.backToBackTetrisPerfectClearBonus ?? base.backToBackTetrisPerfectClearBonus,
    ),
    comboBonusPerStep: scoreCoefficient(
      "comboBonusPerStep",
      input.comboBonusPerStep ?? base.comboBonusPerStep,
    ),
    backToBackMultiplier: finiteNumber(
      "backToBackMultiplier",
      input.backToBackMultiplier ?? base.backToBackMultiplier,
      0,
      100,
    ),
    softDropScorePerCell: scoreCoefficient(
      "softDropScorePerCell",
      input.softDropScorePerCell ?? base.softDropScorePerCell,
    ),
    hardDropScorePerCell: scoreCoefficient(
      "hardDropScorePerCell",
      input.hardDropScorePerCell ?? base.hardDropScorePerCell,
    ),
    perfectClearMode: enumValue(
      "perfectClearMode",
      input.perfectClearMode ?? base.perfectClearMode,
      PLAYER_PERFECT_CLEAR_MODES,
    ),
  });
}

export function playerScoreModelForProfile(
  profile: PlayerBuiltInScoreProfile,
): PlayerScoreModel {
  const model = PLAYER_BUILT_IN_SCORE_MODELS[profile];
  if (model === undefined) {
    throw new RangeError(
      `scoreProfile must be one of ${PLAYER_BUILT_IN_SCORE_PROFILES.join(", ")}.`,
    );
  }
  return model;
}

export function serializePlayerSettings(settings: PlayerSettingsInput): string {
  return JSON.stringify({
    version: PLAYER_SETTINGS_VERSION,
    settings: validatePlayerSettings(settings),
  });
}

export function deserializePlayerSettings(serialized: string): PlayerSettings {
  if (typeof serialized !== "string" || serialized.length === 0) {
    throw new TypeError("Serialized player settings must be a non-empty string.");
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(serialized);
  } catch (error) {
    throw new SyntaxError(
      `Player settings are not valid JSON: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
  if (!isRecord(parsed) || parsed.version !== PLAYER_SETTINGS_VERSION) {
    throw new RangeError(`Unsupported player settings version '${String(isRecord(parsed) ? parsed.version : undefined)}'.`);
  }
  if (!isRecord(parsed.settings)) {
    throw new TypeError("Serialized player settings do not contain a settings object.");
  }
  return validatePlayerSettings(parsed.settings as PlayerSettingsInput);
}

export function createPlayerSettingsStorageAdapter(
  storage: PlayerSettingsStorage,
  key = PLAYER_SETTINGS_STORAGE_KEY,
): PlayerSettingsStorageAdapter {
  if (!storage || typeof storage.getItem !== "function" || typeof storage.setItem !== "function") {
    throw new TypeError("Player settings storage must implement getItem and setItem.");
  }
  if (typeof key !== "string" || key.length === 0) {
    throw new TypeError("Player settings storage key must be a non-empty string.");
  }
  return Object.freeze({
    load() {
      const serialized = storage.getItem(key);
      return serialized === null ? null : deserializePlayerSettings(serialized);
    },
    save(input: PlayerSettingsInput) {
      const settings = validatePlayerSettings(input);
      storage.setItem(key, serializePlayerSettings(settings));
      return settings;
    },
    clear() {
      storage.removeItem?.(key);
    },
  });
}

function finiteNumber(name: string, value: unknown, minimum: number, maximum: number): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new TypeError(`${name} must be a finite number.`);
  }
  if (value < minimum || value > maximum) {
    throw new RangeError(`${name} must be between ${minimum} and ${maximum}.`);
  }
  return Object.is(value, -0) ? 0 : value;
}

function integer(name: string, value: unknown, minimum: number, maximum: number): number {
  const number = finiteNumber(name, value, minimum, maximum);
  if (!Number.isSafeInteger(number)) throw new TypeError(`${name} must be an integer.`);
  return number;
}

function booleanValue(name: string, value: unknown): boolean {
  if (typeof value !== "boolean") {
    throw new TypeError(`${name} must be a boolean.`);
  }
  return value;
}

function kickProfile(value: unknown): PlayerKickProfile {
  if (!isPlayerKickProfile(value)) {
    throw new RangeError(`Unsupported kickProfile '${String(value)}'.`);
  }
  return value;
}

function enumValue<const T extends readonly string[]>(
  name: string,
  value: unknown,
  supported: T,
): T[number] {
  if (typeof value !== "string" || !(supported as readonly string[]).includes(value)) {
    throw new RangeError(`${name} must be one of ${supported.join(", ")}.`);
  }
  return value as T[number];
}

function scoreCoefficient(name: string, value: unknown): number {
  return integer(name, value, 0, 1_000_000_000);
}

function scoreTable(
  value: readonly number[],
  name = "scoreTable",
): PlayerScoreTable {
  if (!Array.isArray(value) || value.length !== 5) {
    throw new RangeError(`${name} must contain exactly five scores for 0 through 4 lines.`);
  }
  return Object.freeze(
    value.map((entry, index) => scoreCoefficient(`${name}[${index}]`, entry)),
  ) as unknown as PlayerScoreTable;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}
