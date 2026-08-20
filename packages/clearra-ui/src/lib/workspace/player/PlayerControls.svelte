<!-- SRP rationale: this component has one behavior-level change reason: editing and validating the complete Player practice configuration contract. -->
<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  import {
    PLAYER_INITIAL_QUEUE_MAX_PIECES,
    PLAYER_UI_NUMBER_CONSTRAINTS,
    createDefaultPlayerUiSettings,
    formatPlayerInitialQueue,
    parsePlayerInitialQueue,
    togglePlayerGravity,
    validatePlayerUiSettings,
    type PlayerBindingAction,
    type PlayerKeyBindings,
    type PlayerUiNumberField,
    type PlayerUiSettings
  } from './playerUiModel';
  import {
    PLAYER_KICK_PROFILES,
    type PlayerKickProfile,
    type PlayerPiece
  } from './playerRules';
  import {
    PLAYER_SCORE_PROFILES,
    PLAYER_SPIN_PROFILES,
    playerScoreModelForProfile,
    validatePlayerScoreModel,
    type PlayerScoreModel,
    type PlayerScoreProfile,
    type PlayerSpinProfile
  } from './playerSettings';
  import PlayerKeyBindingEditor from './PlayerKeyBindingEditor.svelte';
  import {
    workspaceMessage,
    type WorkspaceLanguage,
    type WorkspaceMessageKey
  } from '../workspaceI18n';

  export let language: WorkspaceLanguage;
  export let settings: PlayerUiSettings = createDefaultPlayerUiSettings();
  export let capturingAction: PlayerBindingAction | null = null;
  export let initialFieldText = '';
  export let fieldInvalid = false;
  export let fieldFailureKey: WorkspaceMessageKey = 'playerImportInvalid';
  export let disabled = false;
  let garbageLinesValid = true;
  let initialQueueText = formatPlayerInitialQueue(settings.initialQueue);
  let initialQueueError: 'invalid-piece' | 'too-long' | null = null;
  let lastInitialQueueCanonical = initialQueueText;

  type CaptureBindingDetail = {
    action: PlayerBindingAction;
    currentCode: string;
    bindings: PlayerKeyBindings;
  };

  type PlayerScoreTableField =
    | 'lineClearScores'
    | 'spinScores'
    | 'miniSpinScores'
    | 'perfectClearBonuses';
  type PlayerScoreNumberField =
    | 'backToBackTetrisPerfectClearBonus'
    | 'comboBonusPerStep'
    | 'backToBackMultiplier'
    | 'softDropScorePerCell'
    | 'hardDropScorePerCell';

  const dispatch = createEventDispatcher<{
    settingschange: PlayerUiSettings;
    capturebinding: CaptureBindingDetail;
    cancelbinding: void;
    restoredefaults: void;
    fieldinput: { source: string };
    loadfield: { source: string };
    clearfield: void;
    applygarbage: { lines: number; holeSpreadPercent: number };
    applyqueue: { queue: PlayerPiece[] };
  }>();

  const numberFields: readonly {
    field: PlayerUiNumberField;
    labelKey: WorkspaceMessageKey;
    helpKey?: WorkspaceMessageKey;
  }[] = [
    { field: 'gravityG', labelKey: 'playerGravity', helpKey: 'playerGravityHelp' },
    { field: 'lockDelayMs', labelKey: 'playerLockDelay', helpKey: 'playerLockDelayHelp' },
    {
      field: 'lockResetLimit',
      labelKey: 'playerLockResetLimit',
      helpKey: 'playerLockResetLimitHelp'
    },
    { field: 'dasMs', labelKey: 'playerDas' },
    { field: 'arrMs', labelKey: 'playerArr' },
    { field: 'softDropFactor', labelKey: 'playerSdf', helpKey: 'playerSdfHelp' }
  ];

  const scoreTables: readonly {
    field: PlayerScoreTableField;
    labelKey: WorkspaceMessageKey;
  }[] = [
    { field: 'lineClearScores', labelKey: 'playerScoreLineClear' },
    { field: 'spinScores', labelKey: 'playerScoreSpin' },
    { field: 'miniSpinScores', labelKey: 'playerScoreMini' },
    { field: 'perfectClearBonuses', labelKey: 'playerScorePerfectClear' }
  ];

  const scoreNumbers: readonly {
    field: PlayerScoreNumberField;
    labelKey: WorkspaceMessageKey;
    step: number;
  }[] = [
    { field: 'comboBonusPerStep', labelKey: 'playerScoreComboBonus', step: 1 },
    { field: 'backToBackMultiplier', labelKey: 'playerScoreB2bMultiplier', step: 0.05 },
    {
      field: 'backToBackTetrisPerfectClearBonus',
      labelKey: 'playerScoreB2bPerfectClear',
      step: 1
    },
    { field: 'softDropScorePerCell', labelKey: 'playerScoreSoftDrop', step: 1 },
    { field: 'hardDropScorePerCell', labelKey: 'playerScoreHardDrop', step: 1 }
  ];

  $: label = (key: WorkspaceMessageKey) => workspaceMessage(language, key);
  $: {
    const canonical = formatPlayerInitialQueue(settings.initialQueue);
    if (canonical !== lastInitialQueueCanonical) {
      lastInitialQueueCanonical = canonical;
      initialQueueText = canonical;
      initialQueueError = null;
    }
  }

  function updateNumber(field: PlayerUiNumberField, event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    if (!input.validity.valid || !Number.isFinite(input.valueAsNumber)) return;
    const next = {
      ...settings,
      [field]: input.valueAsNumber,
      ...(field === 'gravityG' && input.valueAsNumber > 0
        ? { lastGravityG: input.valueAsNumber }
        : {})
    } as PlayerUiSettings;
    commitSettings(next);
  }

  function toggleGravity() {
    commitSettings(togglePlayerGravity(settings));
  }

  function restoreInvalidNumber(field: PlayerUiNumberField, event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    if (!input.validity.valid || !Number.isFinite(input.valueAsNumber)) {
      input.value = String(settings[field]);
    }
  }

  function updateGarbageLines(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    garbageLinesValid =
      input.validity.valid && Number.isFinite(input.valueAsNumber);
    if (garbageLinesValid) updateNumber('garbageLines', event);
  }

  function restoreInvalidGarbageLines(event: Event) {
    if (!garbageLinesValid) restoreInvalidNumber('garbageLines', event);
    garbageLinesValid = true;
  }

  function updateToggle(
    field: 'irs' | 'ihs' | 'clutchClear' | 'unlimitedHold',
    event: Event
  ) {
    const input = event.currentTarget as HTMLInputElement;
    commitSettings({ ...settings, [field]: input.checked });
  }

  function updateRuleProfile(
    field: 'kickProfile' | 'spinProfile',
    event: Event
  ) {
    const value = (event.currentTarget as HTMLSelectElement).value;
    if (
      field === 'kickProfile' &&
      (PLAYER_KICK_PROFILES as readonly string[]).includes(value)
    ) {
      commitSettings({ ...settings, kickProfile: value as PlayerKickProfile });
    } else if (
      field === 'spinProfile' &&
      (PLAYER_SPIN_PROFILES as readonly string[]).includes(value)
    ) {
      commitSettings({ ...settings, spinProfile: value as PlayerSpinProfile });
    }
  }

  function updateScoreProfile(event: Event) {
    const value = (event.currentTarget as HTMLSelectElement).value;
    if (!(PLAYER_SCORE_PROFILES as readonly string[]).includes(value)) return;
    const profile = value as PlayerScoreProfile;
    commitSettings({
      ...settings,
      scoreProfile: profile,
      scoreModel:
        profile === 'custom'
          ? settings.scoreModel
          : cloneScoreModel(playerScoreModelForProfile(profile))
    });
  }

  function updateScoreTable(
    field: PlayerScoreTableField,
    index: number,
    event: Event
  ) {
    const input = event.currentTarget as HTMLInputElement;
    if (!input.validity.valid || !Number.isFinite(input.valueAsNumber)) return;
    const table = Array.from(settings.scoreModel[field]);
    table[index] = input.valueAsNumber;
    commitScoreModel({ ...settings.scoreModel, [field]: table });
  }

  function updateScoreNumber(field: PlayerScoreNumberField, event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    if (!input.validity.valid || !Number.isFinite(input.valueAsNumber)) return;
    commitScoreModel({ ...settings.scoreModel, [field]: input.valueAsNumber });
  }

  function restoreInvalidScoreTable(
    field: PlayerScoreTableField,
    index: number,
    event: Event
  ) {
    const input = event.currentTarget as HTMLInputElement;
    if (!input.validity.valid || !Number.isFinite(input.valueAsNumber)) {
      input.value = String(settings.scoreModel[field][index]);
    }
  }

  function restoreInvalidScoreNumber(field: PlayerScoreNumberField, event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    if (!input.validity.valid || !Number.isFinite(input.valueAsNumber)) {
      input.value = String(settings.scoreModel[field]);
    }
  }

  function commitScoreModel(
    candidate: Parameters<typeof validatePlayerScoreModel>[0]
  ) {
    try {
      commitSettings({
        ...settings,
        scoreProfile: 'custom',
        scoreModel: validatePlayerScoreModel(candidate)
      });
    } catch {
      // Native input constraints keep incomplete edits local until they become valid.
    }
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

  function profileLabel(
    profile: string,
    currentLanguage: WorkspaceLanguage
  ): string {
    if (profile === 'srs-plus') return 'SRS+';
    if (profile === 'srs') return 'SRS';
    if (profile === 'srs-x') return 'SRS-X';
    if (profile === 'jstris-180') return 'Jstris 180';
    if (profile === 't-spins') return 'T-Spin';
    if (profile === 't-spins-plus') return 'T-Spin+';
    if (profile === 'all-spin') return 'All-Spin';
    if (profile === 'all-spin-plus') return 'All-Spin+';
    if (profile === 'all-mini') return 'All-Mini';
    if (profile === 'all-mini-plus') return 'All-Mini+';
    if (profile === 'guideline') {
      return workspaceMessage(currentLanguage, 'playerScoreGuideline');
    }
    if (profile === 'jstris-ultra') return 'Jstris Ultra';
    if (profile === 'tetrio') return 'TETR.IO';
    if (profile === 'custom') {
      return workspaceMessage(currentLanguage, 'playerScoreCustom');
    }
    return profile;
  }

  function commitSettings(next: PlayerUiSettings) {
    if (validatePlayerUiSettings(next).length > 0) return;
    settings = next;
    dispatch('settingschange', next);
  }

  function restoreDefaults() {
    settings = createDefaultPlayerUiSettings();
    garbageLinesValid = true;
    initialQueueText = '';
    initialQueueError = null;
    lastInitialQueueCanonical = '';
    dispatch('settingschange', settings);
    dispatch('applyqueue', { queue: [] });
    dispatch('restoredefaults');
  }

  function updateInitialQueue(event: Event) {
    initialQueueText = (event.currentTarget as HTMLInputElement).value;
    const parsed = parsePlayerInitialQueue(initialQueueText);
    initialQueueError = parsed.ok ? null : parsed.reason;
  }

  function applyInitialQueue() {
    const parsed = parsePlayerInitialQueue(initialQueueText);
    if (!parsed.ok) {
      initialQueueError = parsed.reason;
      return;
    }
    const queue = Array.from(parsed.queue);
    initialQueueText = parsed.canonical;
    initialQueueError = null;
    lastInitialQueueCanonical = parsed.canonical;
    commitSettings({ ...settings, initialQueue: queue });
    dispatch('applyqueue', { queue: Array.from(queue) });
  }

  function useRandomBag() {
    initialQueueText = '';
    initialQueueError = null;
    lastInitialQueueCanonical = '';
    commitSettings({ ...settings, initialQueue: [] });
    dispatch('applyqueue', { queue: [] });
  }

  function initialQueueCopy(
    key: 'label' | 'help' | 'invalid' | 'too-long' | 'apply' | 'random',
    currentLanguage: WorkspaceLanguage
  ): string {
    const messageKey = ({
      label: 'playerInitialQueueLabel',
      help: 'playerInitialQueueHelp',
      invalid: 'playerInitialQueueInvalid',
      'too-long': 'playerInitialQueueTooLong',
      apply: 'playerApplyInitialQueue',
      random: 'playerUseRandomBag'
    } as const)[key];
    return workspaceMessage(currentLanguage, messageKey, {
      max: PLAYER_INITIAL_QUEUE_MAX_PIECES
    });
  }

  function updateFieldInput(event: Event) {
    initialFieldText = (event.currentTarget as HTMLTextAreaElement).value;
    dispatch('fieldinput', { source: initialFieldText });
  }

  function loadField() {
    const source = initialFieldText.trim();
    if (!source) return;
    dispatch('loadfield', { source });
  }

  function clearField() {
    initialFieldText = '';
    dispatch('fieldinput', { source: '' });
    dispatch('clearfield');
  }

  function applyGarbage() {
    if (!garbageLinesValid) return;
    dispatch('applygarbage', {
      lines: settings.garbageLines,
      holeSpreadPercent: settings.garbageHoleSpread
    });
  }
</script>

<div class="player-controls" aria-label={label('playerHandling')}>
  <fieldset {disabled}>
    <section aria-labelledby="player-handling-heading">
      <div class="section-heading">
        <div>
          <h2 id="player-handling-heading">{label('playerHandling')}</h2>
          <p>{label('playerHandlingHelp')}</p>
        </div>
        <button class="quiet" type="button" on:click={restoreDefaults}>
          {label('playerRestoreDefaults')}
        </button>
      </div>

      <div class="number-grid">
        {#each numberFields as definition}
          {@const constraint = PLAYER_UI_NUMBER_CONSTRAINTS[definition.field]}
          <div class="number-control">
            <label>
              <span>{label(definition.labelKey)}</span>
              <input
                type="number"
                min={constraint.min}
                max={constraint.max}
                step={constraint.step}
                value={settings[definition.field]}
                required
                on:input={(event) => updateNumber(definition.field, event)}
                on:blur={(event) => restoreInvalidNumber(definition.field, event)}
              />
              {#if definition.helpKey}<small>{label(definition.helpKey)}</small>{/if}
            </label>
            {#if definition.field === 'gravityG'}
              <button
                class="gravity-toggle"
                type="button"
                aria-pressed={settings.gravityG > 0}
                on:click={toggleGravity}
              >
                {settings.gravityG > 0
                  ? label('playerGravityOn')
                  : label('playerGravityOff')}
              </button>
            {/if}
          </div>
        {/each}
      </div>

      <div class="appearance-grid">
        <label>
          <span>
            {label('playerGhostOpacity')}
            <output>{Math.round(settings.ghostOpacity * 100)}%</output>
          </span>
          <input
            type="range"
            min={PLAYER_UI_NUMBER_CONSTRAINTS.ghostOpacity.min}
            max={PLAYER_UI_NUMBER_CONSTRAINTS.ghostOpacity.max}
            step={PLAYER_UI_NUMBER_CONSTRAINTS.ghostOpacity.step}
            value={settings.ghostOpacity}
            on:input={(event) => updateNumber('ghostOpacity', event)}
          />
        </label>
        <label>
          <span>
            {label('playerGridOpacity')}
            <output>{Math.round(settings.gridOpacity * 100)}%</output>
          </span>
          <input
            type="range"
            min={PLAYER_UI_NUMBER_CONSTRAINTS.gridOpacity.min}
            max={PLAYER_UI_NUMBER_CONSTRAINTS.gridOpacity.max}
            step={PLAYER_UI_NUMBER_CONSTRAINTS.gridOpacity.step}
            value={settings.gridOpacity}
            on:input={(event) => updateNumber('gridOpacity', event)}
          />
        </label>
      </div>
    </section>

    <section aria-labelledby="player-rules-heading">
      <div class="section-heading compact">
        <div>
          <h2 id="player-rules-heading">{label('playerRules')}</h2>
          <p>{label('playerRulesHelp')}</p>
        </div>
      </div>
      <div class="select-grid">
        <label>
          <span>{label('playerKickTable')}</span>
          <select
            value={settings.kickProfile}
            on:change={(event) => updateRuleProfile('kickProfile', event)}
          >
            {#each PLAYER_KICK_PROFILES as profile}
              <option value={profile}>{profileLabel(profile, language)}</option>
            {/each}
          </select>
        </label>
        <label>
          <span>{label('playerSpinTable')}</span>
          <select
            value={settings.spinProfile}
            on:change={(event) => updateRuleProfile('spinProfile', event)}
          >
            {#each PLAYER_SPIN_PROFILES as profile}
              <option value={profile}>{profileLabel(profile, language)}</option>
            {/each}
          </select>
        </label>
      </div>
      <div class="rule-toggle-grid">
        <label class="toggle">
          <input
            type="checkbox"
            checked={settings.clutchClear}
            on:change={(event) => updateToggle('clutchClear', event)}
          />
          <span>
            {label('playerClutchClear')}
            <small>{label('playerClutchClearHelp')}</small>
          </span>
        </label>
        <label class="toggle">
          <input
            type="checkbox"
            checked={settings.unlimitedHold}
            on:change={(event) => updateToggle('unlimitedHold', event)}
          />
          <span>
            {label('playerUnlimitedHold')}
            <small>{label('playerUnlimitedHoldHelp')}</small>
          </span>
        </label>
      </div>
    </section>

    <section aria-labelledby="player-score-heading">
      <div class="section-heading compact">
        <div>
          <h2 id="player-score-heading">{label('playerScoreSettings')}</h2>
          <p>{label('playerScoreHelp')}</p>
        </div>
      </div>
      <label class="score-profile">
        <span>{label('playerScorePreset')}</span>
        <select value={settings.scoreProfile} on:change={updateScoreProfile}>
          {#each PLAYER_SCORE_PROFILES as profile}
            <option value={profile}>{profileLabel(profile, language)}</option>
          {/each}
        </select>
      </label>
      <details class="score-details">
        <summary>{label('playerScoreAdvanced')}</summary>
        <div class="score-tables">
          <div class="score-table-heading" aria-hidden="true">
            <span></span>
            {#each [0, 1, 2, 3, 4] as lines}<span>{lines}</span>{/each}
          </div>
          {#each scoreTables as definition}
            <div class="score-table-row">
              <strong>{label(definition.labelKey)}</strong>
              {#each settings.scoreModel[definition.field] as value, index}
                <input
                  type="number"
                  min="0"
                  max="1000000000"
                  step="1"
                  required
                  {value}
                  aria-label={`${label(definition.labelKey)} · ${index}`}
                  on:input={(event) => updateScoreTable(definition.field, index, event)}
                  on:blur={(event) =>
                    restoreInvalidScoreTable(definition.field, index, event)}
                />
              {/each}
            </div>
          {/each}
        </div>
        <div class="score-number-grid">
          {#each scoreNumbers as definition}
            <label>
              <span>{label(definition.labelKey)}</span>
              <input
                type="number"
                min="0"
                max={definition.field === 'backToBackMultiplier' ? 100 : 1_000_000_000}
                step={definition.step}
                value={settings.scoreModel[definition.field]}
                required
                on:input={(event) => updateScoreNumber(definition.field, event)}
                on:blur={(event) => restoreInvalidScoreNumber(definition.field, event)}
              />
            </label>
          {/each}
        </div>
      </details>
    </section>

    <section aria-labelledby="player-buffer-heading">
      <div class="section-heading compact">
        <h2 id="player-buffer-heading">{label('playerInputBuffering')}</h2>
      </div>
      <div class="toggle-grid">
        <label class="toggle">
          <input
            type="checkbox"
            checked={settings.irs}
            on:change={(event) => updateToggle('irs', event)}
          />
          <span>{label('playerIrs')}</span>
        </label>
        <label class="toggle">
          <input
            type="checkbox"
            checked={settings.ihs}
            on:change={(event) => updateToggle('ihs', event)}
          />
          <span>{label('playerIhs')}</span>
        </label>
      </div>
    </section>

    <section aria-labelledby="player-keys-heading">
      <div class="section-heading compact">
        <h2 id="player-keys-heading">{label('playerKeys')}</h2>
      </div>
      <PlayerKeyBindingEditor
        {language}
        bindings={settings.bindings}
        {capturingAction}
        {disabled}
        on:capturebinding={(event) => dispatch('capturebinding', event.detail)}
        on:cancelbinding={() => dispatch('cancelbinding')}
      />
    </section>

    <section aria-labelledby="player-next-settings-heading">
      <div class="section-heading compact">
        <div>
          <h2 id="player-next-settings-heading">{label('playerNext')}</h2>
          <p id="player-next-help">{initialQueueCopy('help', language)}</p>
        </div>
      </div>
      <div class="queue-entry">
        <label for="player-initial-queue">{initialQueueCopy('label', language)}</label>
        <input
          id="player-initial-queue"
          type="text"
          value={initialQueueText}
          placeholder="IOTSZJL"
          maxlength={PLAYER_INITIAL_QUEUE_MAX_PIECES * 4}
          autocomplete="off"
          autocapitalize="characters"
          spellcheck="false"
          aria-invalid={initialQueueError !== null}
          aria-describedby={initialQueueError ? 'player-next-help player-next-error' : 'player-next-help'}
          on:input={updateInitialQueue}
          on:keydown={(event) => {
            if (event.key === 'Enter' && initialQueueError === null) {
              event.preventDefault();
              applyInitialQueue();
            }
          }}
        />
        {#if initialQueueError}
          <p id="player-next-error" class="field-error" role="alert">
            {initialQueueCopy(
              initialQueueError === 'too-long' ? 'too-long' : 'invalid',
              language
            )}
          </p>
        {/if}
      </div>
      <div class="field-actions">
        <button
          class="primary"
          type="button"
          disabled={initialQueueError !== null}
          on:click={applyInitialQueue}
        >
          {initialQueueCopy('apply', language)}
        </button>
        <button class="quiet" type="button" on:click={useRandomBag}>
          {initialQueueCopy('random', language)}
        </button>
      </div>
    </section>

    <section aria-labelledby="player-garbage-heading">
      <div class="section-heading compact">
        <div>
          <h2 id="player-garbage-heading">{label('playerGarbage')}</h2>
          <p>{label('playerGarbageHelp')}</p>
        </div>
      </div>
      <div class="garbage-grid" aria-labelledby="player-garbage-heading">
        <label>
          <span>{label('playerGarbageLines')}</span>
          <input
            type="number"
            min={PLAYER_UI_NUMBER_CONSTRAINTS.garbageLines.min}
            max={PLAYER_UI_NUMBER_CONSTRAINTS.garbageLines.max}
            step={PLAYER_UI_NUMBER_CONSTRAINTS.garbageLines.step}
            value={settings.garbageLines}
            required
            on:input={updateGarbageLines}
            on:blur={restoreInvalidGarbageLines}
          />
        </label>
        <label class="spread-control">
          <span>
            {label('playerGarbageSpread')}
            <output>{Math.round(settings.garbageHoleSpread)}%</output>
          </span>
          <input
            type="range"
            min={PLAYER_UI_NUMBER_CONSTRAINTS.garbageHoleSpread.min}
            max={PLAYER_UI_NUMBER_CONSTRAINTS.garbageHoleSpread.max}
            step={PLAYER_UI_NUMBER_CONSTRAINTS.garbageHoleSpread.step}
            value={settings.garbageHoleSpread}
            on:input={(event) => updateNumber('garbageHoleSpread', event)}
          />
          <small>{label('playerGarbageSpreadHelp')}</small>
        </label>
      </div>
      <div class="field-actions">
        <button
          class="primary"
          type="button"
          disabled={!garbageLinesValid}
          on:click={applyGarbage}
        >
          {label('playerApplyGarbage')}
        </button>
      </div>
    </section>

    <section aria-labelledby="player-field-heading">
      <div class="section-heading compact">
        <div>
          <h2 id="player-field-heading">{label('playerImport')}</h2>
          <p id="player-field-help">{label('playerImportHelp')}</p>
        </div>
      </div>
      <textarea
        rows="4"
        value={initialFieldText}
        placeholder={label('playerImportPlaceholder')}
        aria-labelledby="player-field-heading"
        aria-invalid={fieldInvalid}
        aria-describedby={fieldInvalid ? 'player-field-help player-field-error' : 'player-field-help'}
        on:input={updateFieldInput}
      ></textarea>
      {#if fieldInvalid}
        <p id="player-field-error" class="field-error" role="alert">
          {label(fieldFailureKey)}
        </p>
      {/if}
      <div class="field-actions">
        <button
          class="primary"
          type="button"
          disabled={!initialFieldText.trim()}
          on:click={loadField}
        >
          {label('playerLoadField')}
        </button>
        <button class="quiet" type="button" on:click={clearField}>
          {label('playerClearField')}
        </button>
      </div>
    </section>
  </fieldset>
</div>

<style>
  .player-controls {
    color: #26332e;
    min-width: 0;
  }

  fieldset {
    border: 0;
    display: grid;
    gap: 12px;
    margin: 0;
    min-width: 0;
    padding: 0;
  }

  section {
    background: #fff;
    border: 1px solid #d9e0dc;
    border-radius: 7px;
    padding: 14px;
  }

  .section-heading {
    align-items: flex-start;
    display: flex;
    gap: 12px;
    justify-content: space-between;
    margin-bottom: 13px;
  }

  .section-heading.compact {
    margin-bottom: 9px;
  }

  h2 {
    color: #1c2a25;
    font-size: 12px;
    margin: 0;
  }

  p {
    color: #697570;
    font-size: 10px;
    line-height: 1.45;
    margin: 5px 0 0;
  }

  .number-grid {
    display: grid;
    gap: 9px;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .select-grid,
  .score-number-grid {
    display: grid;
    gap: 9px;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .number-grid label,
  .appearance-grid label,
  .garbage-grid label,
  .select-grid label,
  .score-number-grid label,
  .score-profile {
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }

  .number-control {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .gravity-toggle {
    align-self: flex-start;
    background: #edf5f1;
    border: 1px solid #b9cbc2;
    border-radius: 999px;
    color: #2d5d49;
    cursor: pointer;
    font-size: 10px;
    font-weight: 800;
    padding: 5px 10px;
  }

  .gravity-toggle[aria-pressed='false'] {
    background: #f4f5f4;
    border-color: #d3d7d5;
    color: #68716d;
  }

  label > span {
    color: #46534d;
    font-size: 10px;
    font-weight: 700;
  }

  input[type='number'],
  input[type='text'],
  select,
  textarea {
    background: #fbfcfb;
    border: 1px solid #cbd4cf;
    border-radius: 5px;
    color: #17211e;
    font: 12px ui-monospace, SFMono-Regular, Consolas, monospace;
    min-width: 0;
  }

  input[type='number'] {
    height: 35px;
    padding: 0 8px;
    width: 100%;
  }

  input[type='text'] {
    box-sizing: border-box;
    height: 38px;
    letter-spacing: .08em;
    padding: 0 10px;
    text-transform: uppercase;
    width: 100%;
  }

  select {
    height: 35px;
    padding: 0 8px;
    width: 100%;
  }

  input:focus-visible,
  select:focus-visible,
  textarea:focus-visible,
  button:focus-visible {
    outline: 2px solid #16877d;
    outline-offset: 2px;
  }

  input:invalid,
  textarea[aria-invalid='true'] {
    border-color: #bd654d;
  }

  small {
    color: #71807a;
    font-size: 9px;
    line-height: 1.35;
  }

  .appearance-grid {
    border-top: 1px solid #e4e9e6;
    display: grid;
    gap: 13px;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    margin-top: 13px;
    padding-top: 12px;
  }

  .garbage-grid {
    display: grid;
    gap: 12px;
    grid-template-columns: 96px minmax(0, 1fr);
  }

  .queue-entry {
    display: grid;
    gap: 6px;
  }

  .queue-entry > label {
    color: #46534d;
    font-size: 10px;
    font-weight: 700;
  }

  .spread-control > span {
    display: flex;
    justify-content: space-between;
  }

  .score-details {
    border-top: 1px solid #e3e9e6;
    margin-top: 12px;
    padding-top: 10px;
  }

  .score-details summary {
    color: #34443d;
    cursor: pointer;
    font-size: 10px;
    font-weight: 800;
  }

  .score-tables {
    display: grid;
    gap: 5px;
    margin-top: 11px;
    overflow-x: auto;
    padding-bottom: 3px;
  }

  .score-table-heading,
  .score-table-row {
    align-items: center;
    display: grid;
    gap: 4px;
    grid-template-columns: minmax(78px, 1.3fr) repeat(5, minmax(42px, 1fr));
    min-width: 330px;
  }

  .score-table-heading span {
    color: #7a8580;
    font: 750 9px ui-monospace, SFMono-Regular, Consolas, monospace;
    text-align: center;
  }

  .score-table-row strong {
    color: #52605a;
    font-size: 9px;
    font-weight: 750;
  }

  .score-table-row input {
    height: 31px;
    padding: 0 4px;
    text-align: right;
  }

  .score-number-grid {
    border-top: 1px solid #e3e9e6;
    margin-top: 11px;
    padding-top: 11px;
  }

  .appearance-grid label > span {
    display: flex;
    justify-content: space-between;
  }

  output {
    color: #0b6b63;
    font: 700 10px ui-monospace, SFMono-Regular, Consolas, monospace;
  }

  input[type='range'] {
    accent-color: #16877d;
    cursor: pointer;
    margin: 2px 0;
    width: 100%;
  }

  .toggle-grid {
    display: grid;
    gap: 8px;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .rule-toggle-grid {
    display: grid;
    gap: 8px;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    margin-top: 10px;
  }

  .toggle {
    align-items: center;
    background: #f6f8f7;
    border: 1px solid #dce3df;
    border-radius: 5px;
    display: flex;
    gap: 8px;
    min-height: 38px;
    padding: 6px 9px;
  }

  .toggle input {
    accent-color: #16877d;
    height: 15px;
    margin: 0;
    width: 15px;
  }

  .toggle > span {
    display: grid;
    gap: 2px;
  }

  textarea {
    box-sizing: border-box;
    line-height: 1.45;
    min-height: 84px;
    padding: 9px;
    resize: vertical;
    width: 100%;
  }

  .field-error {
    color: #9c3f2a;
    margin-top: 7px;
  }

  .field-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
    margin-top: 10px;
  }

  button {
    border-radius: 5px;
    cursor: pointer;
    font-size: 10px;
    font-weight: 750;
    min-height: 32px;
    padding: 5px 10px;
  }

  button.primary {
    background: #0d7168;
    border: 1px solid #0d7168;
    color: #fff;
  }

  button.quiet {
    background: #f5f7f6;
    border: 1px solid #cbd4cf;
    color: #34423c;
  }

  button:hover:not(:disabled) {
    filter: brightness(.97);
  }

  button:disabled,
  fieldset:disabled {
    cursor: default;
    opacity: .62;
  }

  @media (max-width: 520px) {
    .number-grid,
    .appearance-grid,
    .garbage-grid,
    .select-grid,
    .score-number-grid,
    .toggle-grid {
      grid-template-columns: 1fr;
    }

    .section-heading {
      align-items: stretch;
      flex-direction: column;
    }
  }
</style>
