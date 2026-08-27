<script lang="ts">
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
  $: korean = language === 'ko';
  $: objectives = buildV2AllowedObjectives(request.capability);
  $: scoreCapable = buildV2ScoreCapable(request.capability);

  function setCapability(capability: BuildV2Capability) {
    dispatch('change', {
      capability,
      objective: buildV2DefaultObjective(capability)
    });
  }

  function errorLabel(code: BuildV2ValidationCode): string {
    const labels: Record<BuildV2ValidationCode, readonly [string, string]> = {
      queue_invalid: ['Queue 또는 pattern 식을 확인하세요.', 'Enter a valid queue or pattern expression.'],
      target_lines_invalid: ['높이는 1..24여야 합니다.', 'Height must be between 1 and 24.'],
      build_target_empty: ['목표 셀을 하나 이상 입력하세요.', 'Enter at least one target cell.'],
      build_target_not_tileable: ['목표 셀 수는 4의 배수여야 합니다.', 'Target cell count must be divisible by four.'],
      build_target_overlap: ['기존 필드와 목표 셀이 겹칩니다.', 'Existing and target masks overlap.'],
      source_pieces_invalid: ['공급 조각 수 범위를 확인하세요.', 'Source piece count is out of range.'],
      target_document_invalid: ['형식에 맞는 색상 target 문서를 입력하세요.', 'Enter a colored target document in the selected format.'],
      solution_document_invalid: ['형식에 맞는 제공 해법 문서를 입력하세요.', 'Enter a supplied solution document in the selected format.'],
      objective_invalid: ['이 기능에서 허용되지 않는 objective입니다.', 'The objective is not allowed for this capability.'],
      initial_b2b_invalid: ['초기 B2B는 0..65535여야 합니다.', 'Initial B2B must be between 0 and 65535.'],
      worker_count_invalid: ['Worker 수는 1 이상이어야 합니다.', 'Worker count must be positive.']
    };
    return labels[code][korean ? 0 : 1];
  }
</script>

<section class="controls" aria-label={korean ? 'Build v2 제어' : 'Build v2 controls'}>
  <label>
    <span>{korean ? '기능' : 'Capability'}</span>
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
    <span>{korean ? 'Objective' : 'Objective'}</span>
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
    <span>{korean ? 'Queue / pattern' : 'Queue / pattern'}</span>
    <input
      value={request.queue}
      spellcheck="false"
      placeholder="IOTSZJL or *p7"
      on:input={(event) => dispatch('change', { queue: (event.currentTarget as HTMLInputElement).value })}
    />
  </label>

  <div class="two-columns">
    <label>
      <span>{korean ? 'Queue 지식' : 'Queue knowledge'}</span>
      <select
        value={request.queueKnowledge}
        on:change={(event) => dispatch('change', { queueKnowledge: (event.currentTarget as HTMLSelectElement).value as 'oracle' | 'visible-7' })}
      >
        <option value="oracle">oracle</option>
        <option value="visible-7">visible-7</option>
      </select>
    </label>
    <label>
      <span>{korean ? '규칙' : 'Rule'}</span>
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
      <span>{korean ? 'Hold 사용' : 'Enable hold'}</span>
    </label>
    <label>
      <span>{korean ? '초기 Hold' : 'Initial hold'}</span>
      <select
        value={request.holdPiece}
        disabled={!request.holdEnabled}
        on:change={(event) => dispatch('change', { holdPiece: (event.currentTarget as HTMLSelectElement).value as BuildV2Request['holdPiece'] })}
      >
        <option value="empty">empty</option>
        {#each ['I', 'O', 'T', 'S', 'Z', 'J', 'L'] as piece}
          <option value={piece}>{piece}</option>
        {/each}
      </select>
    </label>
  </div>

  {#if scoreCapable}
    <div class="score-options">
      <strong>{korean ? '점수 옵션' : 'Score options'}</strong>
      <div class="two-columns">
        <label>
          <span>{korean ? '점수 프로필' : 'Score profile'}</span>
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
          <span>{korean ? '초기 B2B' : 'Initial B2B'}</span>
          <input
            type="number"
            min="0"
            max="65535"
            value={request.initialB2B}
            on:input={(event) => dispatch('change', { initialB2B: Number((event.currentTarget as HTMLInputElement).value) })}
          />
        </label>
      </div>
      <p>{korean ? '동점·선정·정렬은 score만 사용합니다. Attack은 canonical equal-score trace의 참고 값입니다.' : 'Equality, selection, and ordering use score only. Attack is informational data from the canonical equal-score trace.'}</p>
    </div>
  {/if}

  <div class="two-columns">
    <label>
      <span>{korean ? 'Worker 수' : 'Workers'}</span>
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
      <span>{korean ? '모든 논리 프로세서' : 'All logical processors'}</span>
    </label>
  </div>

  <p class="authority">{korean ? 'Build v2는 CPU 전용입니다. 유한 메모리 authority가 연결되기 전에는 memory 옵션을 노출하지 않습니다.' : 'Build v2 is CPU-only. Memory options remain unavailable until finite request/response authority is connected.'}</p>

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
