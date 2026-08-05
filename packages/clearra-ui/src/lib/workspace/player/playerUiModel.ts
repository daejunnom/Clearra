import type { WorkspaceMessageKey } from '../workspaceI18n';
import {
  isPlayerKickProfile,
  isPlayerPiece,
  type PlayerKickProfile,
  type PlayerPiece
} from './playerRules';
import {
  DEFAULT_PLAYER_SCORE_MODEL,
  PLAYER_SCORE_PROFILES,
  PLAYER_SPIN_PROFILES,
  playerScoreModelForProfile,
  validatePlayerScoreModel,
  type PlayerBuiltInScoreProfile,
  type PlayerScoreModel,
  type PlayerScoreProfile,
  type PlayerSpinProfile
} from './playerSettings';

export type PlayerBindingAction =
  | 'move-left'
  | 'move-right'
  | 'soft-drop'
  | 'hard-drop'
  | 'rotate-ccw'
  | 'rotate-cw'
  | 'rotate-180'
  | 'hold'
  | 'restart'
  | 'undo'
  | 'redo'
  | 'pause';

export type PlayerKeyBindings = Record<PlayerBindingAction, string>;

export type PlayerUiSettings = {
  gravityG: number;
  lastGravityG: number;
  lockDelayMs: number;
  lockResetLimit: number;
  dasMs: number;
  arrMs: number;
  softDropFactor: number;
  ghostOpacity: number;
  gridOpacity: number;
  garbageLines: number;
  garbageHoleSpread: number;
  initialQueue: PlayerPiece[];
  kickProfile: PlayerKickProfile;
  spinProfile: PlayerSpinProfile;
  scoreProfile: PlayerScoreProfile;
  scoreModel: PlayerScoreModel;
  irs: boolean;
  ihs: boolean;
  clutchClear: boolean;
  unlimitedHold: boolean;
  bindings: PlayerKeyBindings;
};

export type PlayerUiNumberField =
  | 'gravityG'
  | 'lockDelayMs'
  | 'lockResetLimit'
  | 'dasMs'
  | 'arrMs'
  | 'softDropFactor'
  | 'ghostOpacity'
  | 'gridOpacity'
  | 'garbageLines'
  | 'garbageHoleSpread';

export type PlayerUiNumberConstraint = {
  min: number;
  max: number;
  step: number;
  integer?: boolean;
};

export type PlayerBindingDefinition = {
  action: PlayerBindingAction;
  labelKey: WorkspaceMessageKey;
};

export type PlayerUiValidationIssue = {
  field:
    | PlayerUiNumberField
    | 'irs'
    | 'ihs'
    | 'clutchClear'
    | 'unlimitedHold'
    | 'kickProfile'
    | 'spinProfile'
    | 'scoreProfile'
    | 'scoreModel'
    | 'initialQueue'
    | `bindings.${PlayerBindingAction}`;
  reason:
    | 'not-finite'
    | 'out-of-range'
    | 'not-integer'
    | 'invalid-code'
    | 'duplicate-code'
    | 'invalid-profile'
    | 'invalid-score-model'
    | 'invalid-queue';
  conflict?: PlayerBindingAction;
};

export type PlayerBindingAssignment =
  | { ok: true; settings: PlayerUiSettings }
  | { ok: false; reason: 'invalid-code' | 'duplicate-code'; conflict?: PlayerBindingAction };

export type PlayerInitialQueueParseResult =
  | Readonly<{
      ok: true;
      queue: PlayerPiece[];
      canonical: string;
    }>
  | Readonly<{
      ok: false;
      reason: 'invalid-piece' | 'too-long';
      invalidToken?: string;
    }>;

export const PLAYER_INITIAL_QUEUE_MAX_PIECES = 256;
export const PLAYER_UI_SETTINGS_VERSION = 6;
export const PLAYER_UI_SETTINGS_STORAGE_KEY = 'clearra.player.ui.v1';

export const PLAYER_BINDING_DEFINITIONS = [
  { action: 'move-left', labelKey: 'playerKeyLeft' },
  { action: 'move-right', labelKey: 'playerKeyRight' },
  { action: 'soft-drop', labelKey: 'playerKeySoftDrop' },
  { action: 'hard-drop', labelKey: 'playerKeyHardDrop' },
  { action: 'rotate-ccw', labelKey: 'playerKeyRotateCcw' },
  { action: 'rotate-cw', labelKey: 'playerKeyRotateCw' },
  { action: 'rotate-180', labelKey: 'playerKeyRotate180' },
  { action: 'hold', labelKey: 'playerKeyHold' },
  { action: 'restart', labelKey: 'playerKeyRestart' },
  { action: 'undo', labelKey: 'playerKeyUndo' },
  { action: 'redo', labelKey: 'playerKeyRedo' },
  { action: 'pause', labelKey: 'playerKeyPause' }
] as const satisfies readonly PlayerBindingDefinition[];

export const PLAYER_UI_NUMBER_CONSTRAINTS: Readonly<
  Record<PlayerUiNumberField, PlayerUiNumberConstraint>
> = {
  gravityG: { min: 0, max: 1_000, step: 0.01 },
  lockDelayMs: { min: 0, max: 60_000, step: 1, integer: true },
  lockResetLimit: { min: 0, max: 15, step: 1, integer: true },
  dasMs: { min: 0, max: 5_000, step: 1, integer: true },
  arrMs: { min: 0, max: 5_000, step: 1, integer: true },
  softDropFactor: { min: 1, max: 41, step: 1, integer: true },
  ghostOpacity: { min: 0, max: 1, step: 0.01 },
  gridOpacity: { min: 0, max: 1, step: 0.01 },
  garbageLines: { min: 0, max: 40, step: 1, integer: true },
  garbageHoleSpread: { min: 0, max: 100, step: 1 }
};

const DEFAULT_BINDINGS: PlayerKeyBindings = {
  'move-left': 'ArrowLeft',
  'move-right': 'ArrowRight',
  'soft-drop': 'ArrowDown',
  'hard-drop': 'Space',
  'rotate-ccw': 'KeyZ',
  'rotate-cw': 'ArrowUp',
  'rotate-180': 'KeyA',
  hold: 'ShiftLeft',
  restart: 'KeyR',
  undo: 'Control+KeyZ',
  redo: 'Control+KeyY',
  pause: 'KeyP'
};

export const DEFAULT_PLAYER_UI_SETTINGS: Readonly<PlayerUiSettings> = Object.freeze({
  gravityG: 0.02,
  lastGravityG: 0.02,
  lockDelayMs: 500,
  lockResetLimit: 15,
  dasMs: 83,
  arrMs: 0,
  softDropFactor: 41,
  ghostOpacity: 0.55,
  gridOpacity: 0.75,
  garbageLines: 0,
  garbageHoleSpread: 50,
  initialQueue: Object.freeze([]) as unknown as PlayerPiece[],
  kickProfile: 'srs-plus',
  spinProfile: 't-spins',
  scoreProfile: 'guideline',
  scoreModel: DEFAULT_PLAYER_SCORE_MODEL,
  irs: true,
  ihs: true,
  clutchClear: false,
  unlimitedHold: false,
  bindings: Object.freeze({ ...DEFAULT_BINDINGS }) as PlayerKeyBindings
});

export function createDefaultPlayerUiSettings(): PlayerUiSettings {
  return {
    ...DEFAULT_PLAYER_UI_SETTINGS,
    initialQueue: Array.from(DEFAULT_PLAYER_UI_SETTINGS.initialQueue),
    scoreModel: cloneScoreModel(DEFAULT_PLAYER_UI_SETTINGS.scoreModel),
    bindings: { ...DEFAULT_PLAYER_UI_SETTINGS.bindings }
  };
}

export function togglePlayerGravity(settings: PlayerUiSettings): PlayerUiSettings {
  const next = settings.gravityG > 0
    ? {
        ...settings,
        gravityG: 0,
        lastGravityG: settings.gravityG
      }
    : {
        ...settings,
        gravityG: settings.lastGravityG
      };
  const issues = validatePlayerUiSettings(next);
  if (issues.length > 0) {
    throw new RangeError(`Player UI settings are invalid: ${issues[0].field}`);
  }
  return next;
}

export function serializePlayerUiSettings(settings: PlayerUiSettings): string {
  const issues = validatePlayerUiSettings(settings);
  if (issues.length > 0) {
    throw new RangeError(`Player UI settings are invalid: ${issues[0].field}`);
  }
  return JSON.stringify({
    version: PLAYER_UI_SETTINGS_VERSION,
    settings
  });
}

export function deserializePlayerUiSettings(serialized: string): PlayerUiSettings {
  const parsed: unknown = JSON.parse(serialized);
  if (
    !isRecord(parsed) ||
    (parsed.version !== 1 &&
      parsed.version !== 2 &&
      parsed.version !== 3 &&
      parsed.version !== 4 &&
      parsed.version !== 5 &&
      parsed.version !== PLAYER_UI_SETTINGS_VERSION)
  ) {
    throw new RangeError('Unsupported Player UI settings version.');
  }
  if (!isRecord(parsed.settings) || !isRecord(parsed.settings.bindings)) {
    throw new TypeError('Player UI settings payload is incomplete.');
  }
  const source = parsed.settings;
  const bindingSource = source.bindings as Record<string, unknown>;
  const legacy = parsed.version !== PLAYER_UI_SETTINGS_VERSION;
  const legacyDefaultRotateCcw =
    (parsed.version === 1 || parsed.version === 2 || parsed.version === 3) &&
    bindingSource['rotate-ccw'] === 'ControlLeft' &&
    !Object.entries(bindingSource).some(
      ([action, code]) => action !== 'rotate-ccw' && code === 'KeyZ'
    )
      ? 'KeyZ'
      : bindingSource['rotate-ccw'];
  const scoreProfile = source.scoreProfile ?? DEFAULT_PLAYER_UI_SETTINGS.scoreProfile;
  const lastGravityG = legacy
    ? typeof source.gravityG === 'number' && source.gravityG > 0
      ? source.gravityG
      : DEFAULT_PLAYER_UI_SETTINGS.lastGravityG
    : source.lastGravityG;
  const lockDelayMs =
    (parsed.version === 3 || parsed.version === 4 || parsed.version === 5) &&
    source.lockDelayMs === 800
      ? DEFAULT_PLAYER_UI_SETTINGS.lockDelayMs
      : source.lockDelayMs;
  const lockResetLimit =
    legacy && typeof source.lockResetLimit === 'number'
      ? Math.min(source.lockResetLimit, PLAYER_UI_NUMBER_CONSTRAINTS.lockResetLimit.max)
      : source.lockResetLimit;
  const candidate = {
    gravityG: source.gravityG,
    lastGravityG,
    lockDelayMs,
    lockResetLimit,
    dasMs: source.dasMs,
    arrMs: source.arrMs,
    softDropFactor: source.softDropFactor,
    ghostOpacity: source.ghostOpacity,
    gridOpacity: source.gridOpacity,
    garbageLines: source.garbageLines ?? DEFAULT_PLAYER_UI_SETTINGS.garbageLines,
    garbageHoleSpread:
      source.garbageHoleSpread ?? DEFAULT_PLAYER_UI_SETTINGS.garbageHoleSpread,
    initialQueue: source.initialQueue ?? DEFAULT_PLAYER_UI_SETTINGS.initialQueue,
    kickProfile: source.kickProfile ?? DEFAULT_PLAYER_UI_SETTINGS.kickProfile,
    spinProfile: source.spinProfile ?? DEFAULT_PLAYER_UI_SETTINGS.spinProfile,
    scoreProfile,
    scoreModel: deserializeScoreModel(scoreProfile, source.scoreModel),
    irs: source.irs,
    ihs: source.ihs,
    clutchClear: legacy
      ? source.clutchClear ?? DEFAULT_PLAYER_UI_SETTINGS.clutchClear
      : source.clutchClear,
    unlimitedHold: legacy
      ? source.unlimitedHold ?? DEFAULT_PLAYER_UI_SETTINGS.unlimitedHold
      : source.unlimitedHold,
    bindings: Object.fromEntries(
      PLAYER_BINDING_DEFINITIONS.map(({ action }) => [
        action,
        action === 'rotate-ccw'
          ? legacyDefaultRotateCcw
          : legacy
            ? bindingSource[action] ?? DEFAULT_PLAYER_UI_SETTINGS.bindings[action]
            : bindingSource[action]
      ])
    )
  } as PlayerUiSettings;
  const issues = validatePlayerUiSettings(candidate);
  if (issues.length > 0) {
    throw new RangeError(`Player UI settings are invalid: ${issues[0].field}`);
  }
  return {
    ...candidate,
    initialQueue: Array.from(candidate.initialQueue),
    scoreModel: cloneScoreModel(candidate.scoreModel),
    bindings: { ...candidate.bindings }
  };
}

export function validatePlayerUiSettings(
  settings: PlayerUiSettings
): PlayerUiValidationIssue[] {
  const issues: PlayerUiValidationIssue[] = [];
  for (const field of Object.keys(PLAYER_UI_NUMBER_CONSTRAINTS) as PlayerUiNumberField[]) {
    const constraint = PLAYER_UI_NUMBER_CONSTRAINTS[field];
    const value = settings[field];
    if (!Number.isFinite(value)) {
      issues.push({ field, reason: 'not-finite' });
    } else if (value < constraint.min || value > constraint.max) {
      issues.push({ field, reason: 'out-of-range' });
    } else if (constraint.integer && !Number.isInteger(value)) {
      issues.push({ field, reason: 'not-integer' });
    }
  }
  if (
    !Number.isFinite(settings.lastGravityG) ||
    settings.lastGravityG <= 0 ||
    settings.lastGravityG > PLAYER_UI_NUMBER_CONSTRAINTS.gravityG.max
  ) {
    issues.push({ field: 'gravityG', reason: 'out-of-range' });
  }
  if (typeof settings.irs !== 'boolean') issues.push({ field: 'irs', reason: 'out-of-range' });
  if (typeof settings.ihs !== 'boolean') issues.push({ field: 'ihs', reason: 'out-of-range' });
  if (typeof settings.clutchClear !== 'boolean') {
    issues.push({ field: 'clutchClear', reason: 'out-of-range' });
  }
  if (typeof settings.unlimitedHold !== 'boolean') {
    issues.push({ field: 'unlimitedHold', reason: 'out-of-range' });
  }
  if (!isValidPlayerInitialQueue(settings.initialQueue)) {
    issues.push({ field: 'initialQueue', reason: 'invalid-queue' });
  }
  if (!isPlayerKickProfile(settings.kickProfile)) {
    issues.push({ field: 'kickProfile', reason: 'invalid-profile' });
  }
  if (!(PLAYER_SPIN_PROFILES as readonly unknown[]).includes(settings.spinProfile)) {
    issues.push({ field: 'spinProfile', reason: 'invalid-profile' });
  }
  if (!(PLAYER_SCORE_PROFILES as readonly unknown[]).includes(settings.scoreProfile)) {
    issues.push({ field: 'scoreProfile', reason: 'invalid-profile' });
  }
  try {
    validatePlayerScoreModel(settings.scoreModel);
    if (
      settings.scoreProfile !== 'custom' &&
      !playerScoreModelsEqual(
        settings.scoreModel,
        playerScoreModelForProfile(settings.scoreProfile)
      )
    ) {
      issues.push({ field: 'scoreModel', reason: 'invalid-score-model' });
    }
  } catch {
    issues.push({ field: 'scoreModel', reason: 'invalid-score-model' });
  }

  const actionsByCode = new Map<string, PlayerBindingAction>();
  for (const { action } of PLAYER_BINDING_DEFINITIONS) {
    const code = settings.bindings[action];
    if (!(isPlayerHistoryBindingAction(action)
      ? isPlayerKeyboardShortcut(code)
      : isPlayerKeyboardCode(code))) {
      issues.push({ field: `bindings.${action}`, reason: 'invalid-code' });
      continue;
    }
    const conflict = actionsByCode.get(code);
    if (conflict) {
      issues.push({
        field: `bindings.${action}`,
        reason: 'duplicate-code',
        conflict
      });
    } else {
      actionsByCode.set(code, action);
    }
  }
  return issues;
}

/**
 * Parses an exact starting queue. Piece letters may be adjacent or separated
 * by whitespace and commas; an empty value selects the engine's random bag.
 */
export function parsePlayerInitialQueue(source: string): PlayerInitialQueueParseResult {
  if (typeof source !== 'string') {
    return { ok: false, reason: 'invalid-piece' };
  }
  const compact = source.replace(/[\s,]+/gu, '');
  if (compact.length > PLAYER_INITIAL_QUEUE_MAX_PIECES) {
    return { ok: false, reason: 'too-long' };
  }
  const queue: PlayerPiece[] = [];
  for (const token of compact.toUpperCase()) {
    if (!isPlayerPiece(token)) {
      return { ok: false, reason: 'invalid-piece', invalidToken: token };
    }
    queue.push(token);
  }
  return { ok: true, queue, canonical: queue.join('') };
}

export function formatPlayerInitialQueue(queue: readonly PlayerPiece[]): string {
  if (!isValidPlayerInitialQueue(queue)) {
    throw new RangeError('Player initial queue must contain only I, O, T, S, Z, J, and L.');
  }
  return queue.join('');
}

function isValidPlayerInitialQueue(value: unknown): value is PlayerPiece[] {
  return (
    Array.isArray(value) &&
    value.length <= PLAYER_INITIAL_QUEUE_MAX_PIECES &&
    value.every(isPlayerPiece)
  );
}

function playerScoreModelsEqual(left: PlayerScoreModel, right: PlayerScoreModel): boolean {
  return (
    scoreTablesEqual(left.lineClearScores, right.lineClearScores) &&
    scoreTablesEqual(left.spinScores, right.spinScores) &&
    scoreTablesEqual(left.miniSpinScores, right.miniSpinScores) &&
    scoreTablesEqual(left.perfectClearBonuses, right.perfectClearBonuses) &&
    left.backToBackTetrisPerfectClearBonus ===
      right.backToBackTetrisPerfectClearBonus &&
    left.comboBonusPerStep === right.comboBonusPerStep &&
    left.backToBackMultiplier === right.backToBackMultiplier &&
    left.softDropScorePerCell === right.softDropScorePerCell &&
    left.hardDropScorePerCell === right.hardDropScorePerCell &&
    left.perfectClearMode === right.perfectClearMode
  );
}

function scoreTablesEqual(left: readonly number[], right: readonly number[]): boolean {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}

export function assignPlayerKeyBinding(
  settings: PlayerUiSettings,
  action: PlayerBindingAction,
  keyboardCode: string
): PlayerBindingAssignment {
  const code = keyboardCode.trim();
  if (!(isPlayerHistoryBindingAction(action)
    ? isPlayerKeyboardShortcut(code)
    : isPlayerKeyboardCode(code))) {
    return { ok: false, reason: 'invalid-code' };
  }
  const conflict = PLAYER_BINDING_DEFINITIONS.find(
    (definition) =>
      definition.action !== action && settings.bindings[definition.action] === code
  )?.action;
  if (conflict) return { ok: false, reason: 'duplicate-code', conflict };
  return {
    ok: true,
    settings: {
      ...settings,
      bindings: { ...settings.bindings, [action]: code }
    }
  };
}

export function isPlayerKeyboardCode(value: unknown): value is string {
  return (
    typeof value === 'string' &&
    value.length > 0 &&
    value.length <= 64 &&
    /^[A-Za-z][A-Za-z0-9]*$/.test(value)
  );
}

export function isPlayerKeyboardShortcut(value: unknown): value is string {
  if (isPlayerKeyboardCode(value)) return true;
  if (typeof value !== 'string') return false;
  const parts = value.split('+');
  if (parts.length < 2 || !isPlayerKeyboardCode(parts.at(-1))) return false;
  const modifiers = parts.slice(0, -1);
  const modifierOrder = ['Control', 'Meta', 'Alt', 'Shift'] as const;
  return (
    new Set(modifiers).size === modifiers.length &&
    modifiers.every((modifier) =>
      modifierOrder.includes(modifier as (typeof modifierOrder)[number])) &&
    modifiers.every(
      (modifier, index) =>
        modifierOrder.indexOf(modifier as (typeof modifierOrder)[number]) >
        modifierOrder.indexOf(modifiers[index - 1] as (typeof modifierOrder)[number])
    )
  );
}

export function playerKeyboardShortcutFromEvent(event: Readonly<{
  code: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  altKey?: boolean;
  shiftKey?: boolean;
}>): string {
  const code = event.code;
  if (!isPlayerKeyboardCode(code)) return '';
  const modifiers: string[] = [];
  if (event.ctrlKey && code !== 'ControlLeft' && code !== 'ControlRight') modifiers.push('Control');
  if (event.metaKey && code !== 'MetaLeft' && code !== 'MetaRight') modifiers.push('Meta');
  if (event.altKey && code !== 'AltLeft' && code !== 'AltRight') modifiers.push('Alt');
  if (event.shiftKey && code !== 'ShiftLeft' && code !== 'ShiftRight') modifiers.push('Shift');
  return [...modifiers, code].join('+');
}

export function playerKeyboardShortcutMatches(
  event: Parameters<typeof playerKeyboardShortcutFromEvent>[0],
  shortcut: string
): boolean {
  return playerKeyboardShortcutFromEvent(event) === shortcut;
}

export function isPlayerModifierCode(code: string): boolean {
  return /^(?:Control|Meta|Alt|Shift)(?:Left|Right)$/.test(code);
}

export function isPlayerHistoryBindingAction(
  action: PlayerBindingAction
): action is 'undo' | 'redo' {
  return action === 'undo' || action === 'redo';
}

export function playerKeyboardCodeLabel(code: string): string {
  if (code.includes('+')) {
    return code
      .split('+')
      .map((part) => SHORTCUT_PART_LABELS[part] ?? playerKeyboardCodeLabel(part))
      .join(' + ');
  }
  const label = KEYBOARD_CODE_LABELS[code];
  if (label) return label;
  if (code.startsWith('Key') && code.length === 4) return code.slice(3);
  if (code.startsWith('Digit') && code.length === 6) return code.slice(5);
  if (code.startsWith('Numpad')) return `Num ${code.slice(6)}`;
  return code.replace(/(Left|Right)$/, ' $1');
}

const SHORTCUT_PART_LABELS: Readonly<Record<string, string>> = {
  Control: 'Ctrl',
  Meta: 'Cmd',
  Alt: 'Alt',
  Shift: 'Shift'
};

const KEYBOARD_CODE_LABELS: Readonly<Record<string, string>> = {
  ArrowLeft: '←',
  ArrowRight: '→',
  ArrowUp: '↑',
  ArrowDown: '↓',
  Space: 'Space',
  ControlLeft: 'Left Ctrl',
  ControlRight: 'Right Ctrl',
  ShiftLeft: 'Left Shift',
  ShiftRight: 'Right Shift',
  AltLeft: 'Left Alt',
  AltRight: 'Right Alt',
  Enter: 'Enter',
  Backspace: 'Backspace',
  Tab: 'Tab'
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function cloneScoreModel(model: PlayerScoreModel): PlayerScoreModel {
  return validatePlayerScoreModel({
    ...model,
    lineClearScores: Array.from(model.lineClearScores),
    spinScores: Array.from(model.spinScores),
    miniSpinScores: Array.from(model.miniSpinScores),
    perfectClearBonuses: Array.from(model.perfectClearBonuses)
  });
}

function deserializeScoreModel(profile: unknown, source: unknown): PlayerScoreModel {
  if (profile === 'custom') {
    return isRecord(source)
      ? validatePlayerScoreModel(source as Parameters<typeof validatePlayerScoreModel>[0])
      : cloneScoreModel(DEFAULT_PLAYER_UI_SETTINGS.scoreModel);
  }
  if (
    typeof profile === 'string' &&
    (PLAYER_SCORE_PROFILES as readonly string[]).includes(profile)
  ) {
    return cloneScoreModel(playerScoreModelForProfile(profile as PlayerBuiltInScoreProfile));
  }
  return cloneScoreModel(DEFAULT_PLAYER_UI_SETTINGS.scoreModel);
}
