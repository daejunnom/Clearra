<script lang="ts">
  import { TriangleAlert } from '@lucide/svelte';

  import ProductResultPager from './ProductResultPager.svelte';
  import type {
    ProductMemberPageLoader,
    ProductNextPageLoader,
    ProductPageRelease
  } from './productResultPager';
  import type { WorkspaceLanguage } from './workspaceI18n';
  import type { WorkspaceRuntimeView } from './workspaceRuntime';

  export let view: WorkspaceRuntimeView;
  export let language: WorkspaceLanguage = 'en';
  export let elapsedMs = 0;
  export let capabilityLabel: string;
  export let loadNextProductPage: ProductNextPageLoader | null = null;
  export let loadProductMemberPage: ProductMemberPageLoader | null = null;
  export let releaseProductPages: ProductPageRelease | null = null;

  $: korean = language === 'ko';
  $: productResult = view.response?.product_result_payload ?? null;
  $: elapsed = `${(elapsedMs / 1000).toFixed(1)}s`;
</script>

<section class="result" aria-label={`${capabilityLabel} ${korean ? '결과' : 'result'}`}>
  <header>
    <div>
      <h2>{capabilityLabel} · {korean ? '결과' : 'Result'}</h2>
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
    <p class="error" role="alert">{korean ? '완료 응답에 typed product 결과가 없습니다.' : 'The completed response has no typed product result.'}</p>
  {:else if view.status === 'idle'}
    <p class="empty">{korean ? '입력을 확인한 뒤 실행하세요.' : 'Review the input and run the capability.'}</p>
  {:else if view.status === 'running' || view.status === 'validating'}
    <p class="empty">{korean ? '검증된 결과를 계산하고 있습니다.' : 'Computing the validated result.'}</p>
  {/if}
</section>

<style>
  .result { background: #fff; border-top: 1px solid #d5dcd7; margin-top: 2px; padding: 22px max(24px, calc((100vw - 1460px) / 2)) 36px; }
  header, header > div, .diagnostics li, .error { align-items: center; display: flex; }
  header { justify-content: space-between; }
  header > div { align-items: flex-start; flex-direction: column; gap: 3px; }
  h2 { font-size: 15px; margin: 0; }
  header span, .empty { color: #68736f; font-size: 11px; }
  .backend { background: #edf5f2; border-radius: 999px; color: #17675f; font-weight: 720; padding: 5px 9px; }
  .diagnostics { display: grid; gap: 5px; list-style: none; margin: 14px 0; padding: 0; }
  .diagnostics li { background: #f7f8f7; border: 1px solid #e0e5e2; border-radius: 5px; gap: 10px; padding: 8px 10px; }
  .diagnostics code { color: #6a4138; font-size: 10px; }
  .diagnostics span { color: #596560; font-size: 11px; }
  .error { background: #fff1f0; border: 1px solid #efc3be; border-radius: 6px; color: #8b2820; font-size: 11px; gap: 8px; margin: 14px 0; padding: 10px 12px; }
  .empty { margin: 18px 0; }
</style>
