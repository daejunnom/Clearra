import assert from 'node:assert/strict';
import test from 'node:test';

import {
  createPlayerInputController,
  normalizePlayerKeyBindings,
  shouldActivatePlayerBoardFromKey,
  shouldIgnorePlayerKeyboardTarget,
} from '../src/lib/workspace/player/playerInput.ts';

test('input controller owns held state and emits immediate actions once', () => {
  const actions = [];
  const input = createPlayerInputController({ onAction: (action) => actions.push(action.type) });
  let prevented = 0;
  const left = { code: 'ArrowLeft', preventDefault: () => prevented += 1 };
  assert.equal(input.handleKeyDown(left), true);
  assert.equal(input.held.left, true);
  assert.equal(input.held.horizontalPriority, 'left');
  input.handleKeyDown({ ...left, repeat: true });
  assert.equal(input.handleKeyUp(left), true);
  assert.equal(input.held.left, false);
  input.handleKeyDown({ code: 'Space' });
  assert.equal(input.isPressed('hardDrop'), true);
  input.handleKeyDown({ code: 'Space', repeat: true });
  assert.deepEqual(actions, ['hard-drop']);
  input.handleKeyUp({ code: 'Space' });
  assert.equal(input.isPressed('hardDrop'), false);
  assert.ok(prevented >= 2);
});

test('blur and hidden document events release every held key', () => {
  const keyboard = fakeTarget();
  const visibility = fakeTarget();
  visibility.hidden = false;
  const input = createPlayerInputController();
  const detach = input.attach(keyboard, visibility);
  keyboard.emit('keydown', { code: 'ArrowRight' });
  assert.equal(input.held.right, true);
  keyboard.emit('blur', {});
  assert.equal(input.held.right, false);
  keyboard.emit('keydown', { code: 'ArrowDown' });
  keyboard.emit('keydown', { code: 'KeyC' });
  assert.equal(input.isPressed('hold'), true);
  visibility.hidden = true;
  visibility.emit('visibilitychange', {});
  assert.equal(input.held.softDrop, false);
  assert.equal(input.isPressed('hold'), false);
  detach();
});

test('editable controls keep their native keyboard behavior', () => {
  assert.equal(shouldIgnorePlayerKeyboardTarget({ tagName: 'input' }), true);
  assert.equal(shouldIgnorePlayerKeyboardTarget({ isContentEditable: true }), true);
  const input = createPlayerInputController();
  assert.equal(input.handleKeyDown({ code: 'ArrowLeft', target: { tagName: 'TEXTAREA' } }), false);
  assert.equal(input.held.left, false);
});

test('binding validation rejects ambiguous event.code assignments', () => {
  assert.throws(
    () => normalizePlayerKeyBindings({ moveLeft: 'KeyQ', moveRight: 'KeyQ' }),
    /assigned to both/i,
  );
});

test('a repeat arriving after capture activation restores held input until keyup', () => {
  const input = createPlayerInputController({ enabled: false });

  assert.equal(input.handleKeyDown({ code: 'ArrowLeft' }), false);
  input.setEnabled(true);
  assert.equal(input.handleKeyDown({ code: 'ArrowLeft', repeat: true }), true);
  assert.equal(input.held.left, true);
  assert.equal(input.held.horizontalPriority, 'left');

  assert.equal(input.handleKeyUp({ code: 'ArrowLeft' }), true);
  assert.equal(input.held.left, false);
  assert.equal(input.held.horizontalPriority, null);
});

test('releaseAll keeps a still-held repeating key released until physical keyup', () => {
  const input = createPlayerInputController();

  input.handleKeyDown({ code: 'ArrowRight' });
  input.releaseAll();
  assert.equal(input.held.right, false);

  input.handleKeyDown({ code: 'ArrowRight', repeat: true });
  assert.equal(input.held.right, false);
  assert.equal(input.isPressed('moveRight'), false);
  input.handleKeyUp({ code: 'ArrowRight' });
  input.handleKeyDown({ code: 'ArrowRight' });
  assert.equal(input.held.right, true);
  input.handleKeyUp({ code: 'ArrowRight' });
  assert.equal(input.held.right, false);
});

test('releaseAll inside an immediate callback prevents held-key repeat toggles', () => {
  const actions = [];
  let input;
  input = createPlayerInputController({
    onAction: (action) => {
      actions.push(action.type);
      if (action.type === 'toggle-pause') input.releaseAll();
    },
  });

  input.handleKeyDown({ code: 'Escape' });
  input.handleKeyDown({ code: 'Escape', repeat: true });
  input.handleKeyDown({ code: 'Escape', repeat: true });
  assert.deepEqual(actions, ['toggle-pause']);
  input.handleKeyUp({ code: 'Escape' });
  input.handleKeyDown({ code: 'Escape' });
  assert.deepEqual(actions, ['toggle-pause', 'toggle-pause']);
});

test('Escape remains a normal assignable control code', () => {
  const actions = [];
  const bindings = normalizePlayerKeyBindings({ togglePause: 'Escape' });
  const input = createPlayerInputController({
    bindings,
    onAction: (action) => actions.push(action.type),
  });

  assert.equal(input.handleKeyDown({ code: 'Escape' }), true);
  assert.deepEqual(actions, ['toggle-pause']);
  assert.equal(input.handleKeyUp({ code: 'Escape' }), true);
});

test('the default counter-clockwise key leaves Ctrl chords available to the workspace', () => {
  const actions = [];
  const input = createPlayerInputController({
    onAction: (action) => actions.push(action.type),
  });

  assert.equal(input.handleKeyDown({ code: 'ControlLeft' }), false);
  assert.deepEqual(actions, []);
  assert.equal(input.handleKeyDown({ code: 'KeyZ' }), true);
  assert.deepEqual(actions, ['rotate-ccw']);
});

test('board activation keys cover Enter variants and reserve Space while playing', () => {
  assert.equal(shouldActivatePlayerBoardFromKey({ key: 'Enter', code: 'Enter' }, false), true);
  assert.equal(shouldActivatePlayerBoardFromKey({ key: 'Enter', code: 'NumpadEnter' }, false), true);
  assert.equal(shouldActivatePlayerBoardFromKey({ key: 'Unidentified', code: 'Enter' }, false), true);
  assert.equal(shouldActivatePlayerBoardFromKey({ key: ' ', code: 'Space' }, false), true);
  assert.equal(shouldActivatePlayerBoardFromKey({ key: ' ', code: 'Space' }, true), false);
  assert.equal(shouldActivatePlayerBoardFromKey({ key: 'x', code: 'KeyX' }, false), false);
});

function fakeTarget() {
  const listeners = new Map();
  return {
    hidden: false,
    addEventListener(type, listener) {
      const values = listeners.get(type) ?? new Set();
      values.add(listener);
      listeners.set(type, values);
    },
    removeEventListener(type, listener) {
      listeners.get(type)?.delete(listener);
    },
    emit(type, event) {
      for (const listener of listeners.get(type) ?? []) listener(event);
    },
  };
}
