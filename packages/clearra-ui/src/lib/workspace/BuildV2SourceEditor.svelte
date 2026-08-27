<script lang="ts">
  import { createEventDispatcher } from 'svelte';

  import {
    buildV2SourceKind,
    type BuildV2Request
  } from './buildV2Model';
  import type { WorkspaceLanguage } from './workspaceI18n';

  export let request: BuildV2Request;
  export let language: WorkspaceLanguage = 'en';

  const dispatch = createEventDispatcher<{ change: Partial<BuildV2Request> }>();
  $: korean = language === 'ko';
  $: sourceKind = buildV2SourceKind(request.capability);

  function updateMask(field: 'baseMask' | 'targetMask', value: string) {
    try {
      const parsed = BigInt(value.trim());
      if (parsed >= 0n) dispatch('change', { [field]: parsed });
    } catch {}
  }
</script>

<section class="source-editor" aria-label={korean ? 'Build 입력 소유자' : 'Build input owner'}>
  <header>
    <h2>{korean ? 'Build 입력' : 'Build input'}</h2>
    <p>{sourceKind === 'target-document'
      ? (korean ? '색상 target 문서를 producer 입력으로 보존합니다.' : 'The colored target document remains a producer input.')
      : sourceKind === 'solution-document'
        ? (korean ? '제공된 해법 문서는 검증·replay 대상이며 target으로 재해석하지 않습니다.' : 'The supplied solution document is replayed and never reinterpreted as a target.')
        : (korean ? '기존 필드와 목표 셀 마스크를 서로 겹치지 않게 입력합니다.' : 'Enter disjoint existing-field and target-cell masks.')}</p>
  </header>

  {#if sourceKind === 'mask'}
    <label>
      <span>{korean ? '기존 필드 마스크' : 'Existing-field mask'}</span>
      <input
        value={`0x${request.baseMask.toString(16)}`}
        spellcheck="false"
        on:change={(event) => updateMask('baseMask', (event.currentTarget as HTMLInputElement).value)}
      />
    </label>
    <label>
      <span>{korean ? '목표 셀 마스크' : 'Target-cell mask'}</span>
      <input
        value={`0x${request.targetMask.toString(16)}`}
        spellcheck="false"
        on:change={(event) => updateMask('targetMask', (event.currentTarget as HTMLInputElement).value)}
      />
    </label>
    <label>
      <span>{korean ? '공급 조각 수 (자동이면 비움)' : 'Source piece count (blank for automatic)'}</span>
      <input
        type="number"
        min="1"
        max="4294967295"
        value={request.sourcePieceCount ?? ''}
        on:input={(event) => {
          const value = (event.currentTarget as HTMLInputElement).value;
          dispatch('change', { sourcePieceCount: value === '' ? null : Number(value) });
        }}
      />
    </label>
  {:else if sourceKind === 'target-document'}
    <label>
      <span>{korean ? 'Target 문서 형식' : 'Target document format'}</span>
      <select
        value={request.targetFormat}
        on:change={(event) => dispatch('change', { targetFormat: (event.currentTarget as HTMLSelectElement).value as 'ctk3' | 'fumen' })}
      >
        <option value="ctk3">CTK3</option>
        <option value="fumen">Fumen</option>
      </select>
    </label>
    <label>
      <span>{korean ? '색상 Target 문서' : 'Colored target document'}</span>
      <textarea
        rows="10"
        spellcheck="false"
        value={request.targetDocument}
        placeholder={request.targetFormat === 'ctk3' ? 'ctk3_…' : 'v115@…'}
        on:input={(event) => dispatch('change', { targetDocument: (event.currentTarget as HTMLTextAreaElement).value })}
      ></textarea>
    </label>
  {:else}
    <label>
      <span>{korean ? '제공 해법 형식' : 'Supplied solution format'}</span>
      <select
        value={request.solutionFormat}
        on:change={(event) => dispatch('change', { solutionFormat: (event.currentTarget as HTMLSelectElement).value as 'ctk3' | 'fumen' })}
      >
        <option value="ctk3">CTK3</option>
        <option value="fumen">Fumen</option>
      </select>
    </label>
    <label>
      <span>{korean ? '제공 해법 문서' : 'Supplied solution document'}</span>
      <textarea
        rows="10"
        spellcheck="false"
        value={request.solutionDocument}
        placeholder={request.solutionFormat === 'ctk3' ? 'ctk3_…' : 'v115@…'}
        on:input={(event) => dispatch('change', { solutionDocument: (event.currentTarget as HTMLTextAreaElement).value })}
      ></textarea>
    </label>
  {/if}
</section>

<style>
  .source-editor { display: grid; gap: 16px; }
  header { display: grid; gap: 5px; }
  h2 { color: #17211e; font-size: 15px; margin: 0; }
  p { color: #65716c; font-size: 11px; line-height: 1.55; margin: 0; }
  label { display: grid; gap: 6px; }
  label > span { color: #53605b; font-size: 11px; font-weight: 720; }
  input, select, textarea { background: #fff; border: 1px solid #cbd3ce; border-radius: 5px; color: #26322e; font-size: 12px; min-width: 0; padding: 9px 10px; width: 100%; }
  input, select { height: 39px; }
  textarea { line-height: 1.45; overflow-wrap: anywhere; resize: vertical; }
  input:focus, select:focus, textarea:focus { border-color: #16877d; box-shadow: 0 0 0 3px #16877d1f; outline: 0; }
</style>
