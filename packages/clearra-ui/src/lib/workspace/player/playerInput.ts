export type PlayerHorizontalPriority = "left" | "right" | null;

export type PlayerHeldInput = Readonly<{
  left: boolean;
  right: boolean;
  softDrop: boolean;
  horizontalPriority: PlayerHorizontalPriority;
}>;

export type PlayerImmediateInputAction =
  | "hard-drop"
  | "rotate-cw"
  | "rotate-ccw"
  | "rotate-180"
  | "hold"
  | "toggle-pause"
  | "reset";

export type PlayerControl =
  | "moveLeft"
  | "moveRight"
  | "softDrop"
  | "hardDrop"
  | "rotateCw"
  | "rotateCcw"
  | "rotate180"
  | "hold"
  | "togglePause"
  | "reset";

export type PlayerKeyBindings = Readonly<Record<PlayerControl, readonly string[]>>;
export type PlayerKeyBindingsInput = Partial<Record<PlayerControl, string | readonly string[]>>;

export interface PlayerKeyboardEventLike {
  readonly code: string;
  readonly key?: string;
  readonly repeat?: boolean;
  readonly target?: unknown;
  preventDefault?(): void;
}

export interface PlayerInputEventTarget {
  addEventListener(type: string, listener: (event: any) => void): void;
  removeEventListener(type: string, listener: (event: any) => void): void;
}

export interface PlayerVisibilityTarget extends PlayerInputEventTarget {
  readonly hidden?: boolean;
}

export interface PlayerInputController {
  readonly held: PlayerHeldInput;
  readonly enabled: boolean;
  isPressed(control: PlayerControl): boolean;
  handleKeyDown(event: PlayerKeyboardEventLike): boolean;
  handleKeyUp(event: PlayerKeyboardEventLike): boolean;
  releaseAll(): void;
  setEnabled(enabled: boolean): void;
  setBindings(bindings: PlayerKeyBindingsInput): void;
  attach(keyboardTarget: PlayerInputEventTarget, visibilityTarget?: PlayerVisibilityTarget): () => void;
  dispose(): void;
}

export const EMPTY_PLAYER_HELD_INPUT: PlayerHeldInput = Object.freeze({
  left: false,
  right: false,
  softDrop: false,
  horizontalPriority: null,
});

const CONTROLS: readonly PlayerControl[] = Object.freeze([
  "moveLeft",
  "moveRight",
  "softDrop",
  "hardDrop",
  "rotateCw",
  "rotateCcw",
  "rotate180",
  "hold",
  "togglePause",
  "reset",
]);

export const DEFAULT_PLAYER_KEY_BINDINGS: PlayerKeyBindings = freezeBindings({
  moveLeft: ["ArrowLeft"],
  moveRight: ["ArrowRight"],
  softDrop: ["ArrowDown"],
  hardDrop: ["Space"],
  rotateCw: ["KeyX", "ArrowUp"],
  rotateCcw: ["KeyZ"],
  rotate180: ["KeyA", "ShiftLeft"],
  hold: ["KeyC", "ShiftRight"],
  togglePause: ["Escape"],
  reset: ["KeyR"],
});

export function normalizePlayerKeyBindings(
  input: PlayerKeyBindingsInput = {},
  base: PlayerKeyBindings = DEFAULT_PLAYER_KEY_BINDINGS,
): PlayerKeyBindings {
  if (input === null || typeof input !== "object" || Array.isArray(input)) {
    throw new TypeError("Player key bindings must be an object.");
  }
  const result = {} as Record<PlayerControl, readonly string[]>;
  const claimed = new Map<string, PlayerControl>();
  for (const control of CONTROLS) {
    const raw = input[control] ?? base[control];
    const values = typeof raw === "string" ? [raw] : Array.from(raw ?? []);
    if (values.length === 0) throw new RangeError(`${control} must have at least one key binding.`);
    const unique = Array.from(new Set(values.map(validateCode)));
    for (const code of unique) {
      const owner = claimed.get(code);
      if (owner) throw new RangeError(`Keyboard code '${code}' is assigned to both ${owner} and ${control}.`);
      claimed.set(code, control);
    }
    result[control] = Object.freeze(unique);
  }
  return Object.freeze(result);
}

export function createPlayerInputController(options: {
  bindings?: PlayerKeyBindingsInput;
  onAction?: (action: Readonly<{ type: PlayerImmediateInputAction }>) => void;
  enabled?: boolean;
} = {}): PlayerInputController {
  let bindings = normalizePlayerKeyBindings(options.bindings);
  let codeToControl = buildCodeMap(bindings);
  let enabled = options.enabled ?? true;
  let disposed = false;
  const pressed = new Set<string>();
  const releasedUntilKeyUp = new Set<string>();
  const heldState: {
    left: boolean;
    right: boolean;
    softDrop: boolean;
    horizontalPriority: PlayerHorizontalPriority;
  } = { left: false, right: false, softDrop: false, horizontalPriority: null };
  const detachCallbacks = new Set<() => void>();

  const controller: PlayerInputController = {
    get held() {
      return heldState;
    },
    get enabled() {
      return enabled && !disposed;
    },
    isPressed(control) {
      if (!(CONTROLS as readonly string[]).includes(control)) {
        throw new RangeError(`Unsupported player control '${String(control)}'.`);
      }
      return controlIsPressed(control);
    },
    handleKeyDown(event) {
      if (!enabled || disposed || shouldIgnorePlayerKeyboardTarget(event.target)) return false;
      const control = codeToControl.get(event.code);
      if (!control) return false;
      event.preventDefault?.();
      // A key can begin while controls are disabled and only deliver repeat
      // events after capture is enabled. Treat the first event observed for a
      // physical code as its press regardless of KeyboardEvent.repeat.
      if (pressed.has(event.code)) return true;
      // releaseAll() may run synchronously from an immediate action (pause,
      // reset, etc.). Do not let the still-held key's repeat events recreate
      // that action before the matching physical keyup arrives.
      if (event.repeat && releasedUntilKeyUp.has(event.code)) return true;
      releasedUntilKeyUp.delete(event.code);
      pressed.add(event.code);
      if (control === "moveLeft") {
        heldState.left = true;
        heldState.horizontalPriority = "left";
      } else if (control === "moveRight") {
        heldState.right = true;
        heldState.horizontalPriority = "right";
      } else if (control === "softDrop") {
        heldState.softDrop = true;
      } else {
        const action = IMMEDIATE_ACTIONS[control];
        if (action) options.onAction?.(Object.freeze({ type: action }));
      }
      return true;
    },
    handleKeyUp(event) {
      const wasReleased = releasedUntilKeyUp.delete(event.code);
      const control = codeToControl.get(event.code);
      if (!control) {
        pressed.delete(event.code);
        return false;
      }
      const wasPressed = pressed.delete(event.code);
      if (!wasPressed && !wasReleased) return false;
      event.preventDefault?.();
      refreshHeld(control);
      return true;
    },
    releaseAll() {
      for (const code of pressed) releasedUntilKeyUp.add(code);
      pressed.clear();
      heldState.left = false;
      heldState.right = false;
      heldState.softDrop = false;
      heldState.horizontalPriority = null;
    },
    setEnabled(value) {
      enabled = Boolean(value);
      if (!enabled) controller.releaseAll();
    },
    setBindings(input) {
      bindings = normalizePlayerKeyBindings(input);
      codeToControl = buildCodeMap(bindings);
      controller.releaseAll();
    },
    attach(keyboardTarget, visibilityTarget) {
      if (disposed) throw new Error("Player input controller has been disposed.");
      if (!isEventTarget(keyboardTarget)) throw new TypeError("Keyboard target must support event listeners.");
      const keydown = (event: PlayerKeyboardEventLike) => controller.handleKeyDown(event);
      const keyup = (event: PlayerKeyboardEventLike) => controller.handleKeyUp(event);
      const blur = () => controller.releaseAll();
      const visibility = () => {
        if (visibilityTarget?.hidden) controller.releaseAll();
      };
      keyboardTarget.addEventListener("keydown", keydown);
      keyboardTarget.addEventListener("keyup", keyup);
      keyboardTarget.addEventListener("blur", blur);
      visibilityTarget?.addEventListener("visibilitychange", visibility);
      let attached = true;
      const detach = () => {
        if (!attached) return;
        attached = false;
        keyboardTarget.removeEventListener("keydown", keydown);
        keyboardTarget.removeEventListener("keyup", keyup);
        keyboardTarget.removeEventListener("blur", blur);
        visibilityTarget?.removeEventListener("visibilitychange", visibility);
        detachCallbacks.delete(detach);
        controller.releaseAll();
      };
      detachCallbacks.add(detach);
      return detach;
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      for (const detach of Array.from(detachCallbacks)) detach();
      controller.releaseAll();
      releasedUntilKeyUp.clear();
    },
  };

  function refreshHeld(releasedControl: PlayerControl) {
    if (releasedControl === "moveLeft") heldState.left = controlIsPressed("moveLeft");
    if (releasedControl === "moveRight") heldState.right = controlIsPressed("moveRight");
    if (releasedControl === "softDrop") heldState.softDrop = controlIsPressed("softDrop");
    if (!heldState.left && !heldState.right) heldState.horizontalPriority = null;
    else if (!heldState.left) heldState.horizontalPriority = "right";
    else if (!heldState.right) heldState.horizontalPriority = "left";
  }

  function controlIsPressed(control: PlayerControl): boolean {
    return bindings[control].some((code) => pressed.has(code));
  }

  return controller;
}

export function shouldIgnorePlayerKeyboardTarget(target: unknown): boolean {
  if (!target || typeof target !== "object") return false;
  const candidate = target as { tagName?: unknown; isContentEditable?: unknown };
  if (candidate.isContentEditable === true) return true;
  const tagName = typeof candidate.tagName === "string" ? candidate.tagName.toUpperCase() : "";
  return tagName === "INPUT" || tagName === "TEXTAREA" || tagName === "SELECT";
}

export function shouldActivatePlayerBoardFromKey(
  event: Pick<PlayerKeyboardEventLike, "code" | "key">,
  playing: boolean,
): boolean {
  const enter =
    event.key === "Enter" ||
    event.code === "Enter" ||
    event.code === "NumpadEnter";
  if (enter) return true;
  return !playing && (event.key === " " || event.code === "Space");
}

function buildCodeMap(bindings: PlayerKeyBindings): Map<string, PlayerControl> {
  const result = new Map<string, PlayerControl>();
  for (const control of CONTROLS) {
    for (const code of bindings[control]) result.set(code, control);
  }
  return result;
}

function freezeBindings(bindings: Record<PlayerControl, readonly string[]>): PlayerKeyBindings {
  const result = {} as Record<PlayerControl, readonly string[]>;
  for (const control of CONTROLS) result[control] = Object.freeze(Array.from(bindings[control]));
  return Object.freeze(result);
}

function validateCode(value: unknown): string {
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new TypeError("Keyboard bindings must be non-empty event.code strings.");
  }
  return value.trim();
}

function isEventTarget(value: unknown): value is PlayerInputEventTarget {
  return Boolean(
    value &&
      typeof value === "object" &&
      typeof (value as PlayerInputEventTarget).addEventListener === "function" &&
      typeof (value as PlayerInputEventTarget).removeEventListener === "function",
  );
}

const IMMEDIATE_ACTIONS: Partial<Record<PlayerControl, PlayerImmediateInputAction>> =
  Object.freeze({
    hardDrop: "hard-drop",
    rotateCw: "rotate-cw",
    rotateCcw: "rotate-ccw",
    rotate180: "rotate-180",
    hold: "hold",
    togglePause: "toggle-pause",
    reset: "reset",
  });
