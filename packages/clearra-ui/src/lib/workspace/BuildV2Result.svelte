<script lang="ts">
  import { Download, TriangleAlert } from '@lucide/svelte';

  import type { ClearraSolutionSetArtifactFormatPayload } from '../wasm/wasmCommandClient';
  import ProductResultPager from './ProductResultPager.svelte';
  import {
    validateSolutionSetArtifactPayload,
    type ProductMemberPageLoader,
    type ProductNextPageLoader,
    type ProductPageRelease
  } from './productResultPager';
  import type { WorkspaceLanguage } from './workspaceI18n';
  import type { WorkspaceRuntimeView } from './workspaceRuntime';

  export let view: WorkspaceRuntimeView;
  export let language: WorkspaceLanguage = 'en';
  export let elapsedMs = 0;
  export let loadNextProductPage: ProductNextPageLoader | null = null;
  export let loadProductMemberPage: ProductMemberPageLoader | null = null;
  export let releaseProductPages: ProductPageRelease | null = null;

  $: korean = language === 'ko';
  $: productResult = view.response?.product_result_payload ?? null;
  $: artifact = view.response?.solution_set_artifact ?? null;
  $: artifactError = artifact ? validateSolutionSetArtifactPayload(artifact) : null;
  $: elapsed = `${(elapsedMs / 1000).toFixed(1)}s`;

  function downloadArtifact(format: ClearraSolutionSetArtifactFormatPayload) {
    if (
      format.state !== 'available' ||
      !format.document ||
      !format.media_type ||
      !format.filename
    ) return;
    const url = URL.createObjectURL(new Blob([format.document], { type: format.media_type }));
    const anchor = document.createElement('a');
    anchor.href = url;
    anchor.download = format.filename;
    anchor.click();
    URL.revokeObjectURL(url);
  }
</script>

<section class="result" aria-label={korean ? 'Build v2 결과' : 'Build v2 result'}>
  <header>
    <div>
      <h2>{korean ? '결과' : 'Result'}</h2>
      <span>{view.status} · {elapsed}</span>
    </div>
    {#if view.backendReport}
      <span class="backend">CPU · {view.backendReport.backend_selected ?? view.backendReport.backend_requested ?? 'cpu'}</span>
    {/if}
  </header>

  {#if view.error}
    <p class="error" role="alert"><TriangleAlert size={16} />{view.error}</p>
  {/if}
  {#if view.diagnostics.length}
    <ul class="diagnostics">
      {#each view.diagnostics as diagnostic}
        <li><code>{diagnostic.code}</code><span>{diagnostic.message}</span></li>
      {/each}
    </ul>
  {/if}

  {#if productResult}
    <ProductResultPager
      payload={productResult}
      {language}
      loadNextPage={loadNextProductPage}
      loadMemberPage={loadProductMemberPage}
      releasePages={releaseProductPages}
    />
  {:else if view.status === 'completed'}
    <p class="error" role="alert">{korean ? '완료 응답에 Build product payload가 없습니다.' : 'The completed response has no Build product payload.'}</p>
  {:else if view.status === 'idle'}
    <p class="empty">{korean ? '입력을 확인한 뒤 실행하세요.' : 'Review the input and run the capability.'}</p>
  {:else if view.status === 'running' || view.status === 'validating'}
    <p class="empty">{korean ? '검증된 Build 결과를 계산하고 있습니다.' : 'Computing the validated Build result.'}</p>
  {/if}

  {#if artifact && !artifactError}
    <section class="artifact" aria-label={korean ? '해법 문서 artifact' : 'Solution document artifact'}>
      <div>
        <strong>{korean ? '완전한 해법 문서' : 'Complete solution documents'}</strong>
        <span>{artifact.selection_kind} · {artifact.solution_count} {korean ? '개' : 'solutions'}</span>
      </div>
      <div class="artifact-actions">
        {#each artifact.formats as format (format.format)}
          <button
            type="button"
            disabled={format.state !== 'available'}
            title={format.unavailable_reason ?? format.sha256 ?? ''}
            on:click={() => downloadArtifact(format)}
          >
            <Download size={14} />{format.format.toUpperCase()}
          </button>
        {/each}
      </div>
    </section>
  {:else if artifactError}
    <p class="error" role="alert">{artifactError}</p>
  {/if}
</section>

<style>
  .result { background: #fff; border-top: 1px solid #d5dcd7; margin-top: 2px; padding: 22px max(24px, calc((100vw - 1460px) / 2)) 36px; }
  header, header > div, .artifact, .artifact > div, .artifact-actions, .diagnostics li, .error { align-items: center; display: flex; }
  header { justify-content: space-between; }
  header > div, .artifact > div { align-items: flex-start; flex-direction: column; gap: 3px; }
  h2 { font-size: 15px; margin: 0; }
  header span, .artifact span, .empty { color: #68736f; font-size: 11px; }
  .backend { background: #edf5f2; border-radius: 999px; color: #17675f; font-weight: 720; padding: 5px 9px; }
  .diagnostics { display: grid; gap: 5px; list-style: none; margin: 14px 0; padding: 0; }
  .diagnostics li { background: #f7f8f7; border: 1px solid #e0e5e2; border-radius: 5px; gap: 10px; padding: 8px 10px; }
  .diagnostics code { color: #6a4138; font-size: 10px; }
  .diagnostics span { color: #596560; font-size: 11px; }
  .error { background: #fff1f0; border: 1px solid #efc3be; border-radius: 6px; color: #8b2820; font-size: 11px; gap: 8px; margin: 14px 0; padding: 10px 12px; }
  .empty { margin: 18px 0; }
  .artifact { border: 1px solid #dce3df; border-radius: 7px; justify-content: space-between; margin-top: 16px; padding: 12px 14px; }
  .artifact strong { color: #26322e; font-size: 12px; }
  .artifact-actions { gap: 7px; }
  .artifact button { align-items: center; background: #fff; border: 1px solid #cbd3ce; border-radius: 5px; color: #35443f; display: inline-flex; font-size: 11px; font-weight: 720; gap: 5px; min-height: 32px; padding: 5px 9px; }
  .artifact button:disabled { opacity: .45; }
  @media (max-width: 560px) { .artifact { align-items: stretch; flex-direction: column; gap: 10px; } }
</style>
