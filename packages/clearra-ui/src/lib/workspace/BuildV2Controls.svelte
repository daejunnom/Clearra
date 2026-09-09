<script lang="ts">
  import { componentMessage, type ComponentMessageKey } from '../i18n/componentCatalog';
  import { createEventDispatcher } from 'svelte';

  import {
    BUILD_V2_CAPABILITIES,
    buildV2AllowedObjectives,
    buildV2DefaultObjective,
    buildV2ScoreCapable,
    type BuildV2Capability,
    type BuildV2Objective,
    type BuildV2Request,
    type BuildV2ValidationCode
  } from './buildV2Model';
  import type { WorkspaceLanguage } from './workspaceI18n';

  export let request: BuildV2Request;
  export let language: WorkspaceLanguage = 'en';
  export let validationCodes: BuildV2ValidationCode[] = [];

  const dispatch = createEventDispatcher<{ change: Partial<BuildV2Request> }>();
  $: objectives = buildV2AllowedObjectives(request.capability);
  $: scoreCapable = buildV2ScoreCapable(request.capability);

  function setCapability(capability: BuildV2Capability) {
    dispatch('change', {
      capability,
      objective: buildV2DefaultObjective(capability)
    });
  }

  function errorLabel(code: BuildV2ValidationCode): string {
    const labels: Record<BuildV2ValidationCode, ComponentMessageKey> = {
      queue_invalid: 'enterAValidQueueOrPatternExpression',
      target_lines_invalid: 'heightMustBeBetween1And24',
      build_target_empty: 'enterAtLeastOneTargetCell',
      build_target_not_tileable: 'targetCellCountMustBeDivisibleBy',
      build_target_overlap: 'existingAndTargetMasksOverlap',
      source_pieces_invalid: 'sourcePieceCountIsOutOfRange',
      target_document_invalid: 'enterAColoredTargetDocumentInThe',
      solution_document_invalid: 'enterASuppliedSolutionDocumentInThe',
      objective_invalid: 'theObjectiveIsNotAllowedForThis',
      initial_b2b_invalid: 'initialB2bMustBeBetween0And',
      worker_count_invalid: 'workerCountMustBePositive'
    };
    return componentMessage(language, labels[code]);
  }
</script>

<section class="controls" aria-label={componentMessage(language, 'buildV2Controls')}>
  <label>
    <span>{componentMessage(language, 'capability')}</span>
    <select
      value={request.capability}
      on:change={(event) => setCapability((event.currentTarget as HTMLSelectElement).value as BuildV2Capability)}
    >
      {#each BUILD_V2_CAPABILITIES as capability}
        <option value={capability}>{capability}</option>
      {/each}
    </select>
  </label>

  <label>
    <span>{componentMessage(language, 'objective')}</span>
    <select
      value={request.objective}
      on:change={(event) => dispatch('change', { objective: (event.currentTarget as HTMLSelectElement).value as BuildV2Objective })}
    >
      {#each objectives as objective}
        <option value={objective}>{objective}</option>
      {/each}
    </select>
  </label>

  <label>
    <span>{componentMessage(language, 'queuePattern')}</span>
    <input
      value={request.queue}
      spellcheck="false"
      placeholder={componentMessage(language, 'surfaceIotszjlOrP7')}
      on:input={(event) => dispatch('change', { queue: (event.currentTarget as HTMLInputElement).value })}
    />
  </label>

  <div class="two-columns">
    <label>
      <span>{componentMessage(language, 'queueKnowledge')}</span>
      <select
        value={request.queueKnowledge}
        on:change={(event) => dispatch('change', { queueKnowledge: (event.currentTarget as HTMLSelectElement).value as 'oracle' | 'visible-7' })}
      >
        <option value="oracle">{componentMessage(language, 'surfaceOracle')}</option>
        <option value="visible-7">{componentMessage(language, 'surfaceVisible7')}</option>
      </select>
    </label>
    <label>
      <span>{componentMessage(language, 'rule')}</span>
      <select
        value={request.rule}
        on:change={(event) => dispatch('change', { rule: (event.currentTarget as HTMLSelectElement).value as BuildV2Request['rule'] })}
      >
        <option value="srs-plus">srs-plus</option>
        <option value="srs">srs</option>
        <option value="srs-x">srs-x</option>
        <option value="jstris-180">jstris-180</option>
      </select>
    </label>
  </div>

  <div class="two-columns">
    <label class="check-row">
      <input
        type="checkbox"
        checked={request.holdEnabled}
        on:change={(event) => dispatch('change', { holdEnabled: (event.currentTarget as HTMLInputElement).checked })}
      />
      <span>{componentMessage(language, 'enableHold')}</span>
    </label>
    <label>
      <span>{componentMessage(language, 'initialHold')}</span>
      <select
        value={request.holdPiece}
        disabled={!request.holdEnabled}
        on:change={(event) => dispatch('change', { holdPiece: (event.currentTarget as HTMLSelectElement).value as BuildV2Request['holdPiece'] })}
      >
        <option value="empty">{componentMessage(language, 'surfaceEmpty')}</option>
        {#each ['I', 'O', 'T', 'S', 'Z', 'J', 'L'] as piece}
          <option value={piece}>{piece}</option>
        {/each}
      </select>
    </label>
  </div>

  {#if scoreCapable}
    <div class="score-options">
      <strong>{componentMessage(language, 'scoreOptions')}</strong>
      <div class="two-columns">
        <label>
          <span>{componentMessage(language, 'scoreProfile')}</span>
          <select
            value={request.scoreProfile}
            on:change={(event) => dispatch('change', { scoreProfile: (event.currentTarget as HTMLSelectElement).value as BuildV2Request['scoreProfile'] })}
          >
            <option value="tetrio">tetrio</option>
            <option value="guideline">guideline</option>
            <option value="jstris-ultra">jstris-ultra</option>
          </select>
        </label>
        <label>
          <span>{componentMessage(language, 'initialB2b')}</span>
          <input
            type="number"
            min="0"
            max="65535"
            value={request.initialB2B}
            on:input={(event) => dispatch('change', { initialB2B: Number((event.currentTarget as HTMLInputElement).value) })}
          />
        </label>
      </div>
      <p>{componentMessage(language, 'equalitySelectionAndOrderingUseScoreOnly')}</p>
    </div>
  {/if}

  <div class="two-columns">
    <label>
      <span>{componentMessage(language, 'workers')}</span>
      <input
        type="number"
        min="1"
        value={request.workers}
        disabled={request.useAllLogicalProcessors}
        on:input={(event) => dispatch('change', { workers: Number((event.currentTarget as HTMLInputElement).value) })}
      />
    </label>
    <label class="check-row">
      <input
        type="checkbox"
        checked={request.useAllLogicalProcessors}
        on:change={(event) => dispatch('change', { useAllLogicalProcessors: (event.currentTarget as HTMLInputElement).checked })}
      />
      <span>{componentMessage(language, 'allLogicalProcessors')}</span>
    </label>
  </div>

  <p class="authority">{componentMessage(language, 'buildV2IsCpuOnlyMemoryOptions')}</p>

  {#if validationCodes.length}
    <ul class="errors" aria-live="polite">
      {#each validationCodes as code}
        <li>{errorLabel(code)}</li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .controls { display: grid; gap: 14px; }
  label { display: grid; gap: 6px; min-width: 0; }
  label > span, .score-options > strong { color: #53605b; font-size: 11px; font-weight: 720; }
  input, select { background: #fff; border: 1px solid #cbd3ce; border-radius: 5px; color: #26322e; font-size: 12px; height: 39px; min-width: 0; padding: 0 10px; width: 100%; }
  input:focus, select:focus { border-color: #16877d; box-shadow: 0 0 0 3px #16877d1f; outline: 0; }
  .two-columns { display: grid; gap: 10px; grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .check-row { align-items: center; display: flex; gap: 8px; min-height: 39px; }
  .check-row input { height: 16px; margin: 0; width: 16px; }
  .score-options { background: #f5f8f6; border: 1px solid #dce3df; border-radius: 6px; display: grid; gap: 10px; padding: 12px; }
  .score-options p, .authority { color: #68736f; font-size: 10px; line-height: 1.5; margin: 0; }
  .authority { background: #f7f3ea; border: 1px solid #e3d8bd; border-radius: 5px; color: #725d29; padding: 9px 10px; }
  .errors { background: #fff1f0; border: 1px solid #efc3be; border-radius: 5px; color: #8b2820; display: grid; font-size: 11px; gap: 4px; margin: 0; padding: 9px 12px 9px 28px; }
  @media (max-width: 560px) { .two-columns { grid-template-columns: 1fr; } }
</style>
