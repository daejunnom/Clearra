<script lang="ts">
  import ProductResultPager from './ProductResultPager.svelte';
  import WorkspaceFailureNotice from './WorkspaceFailureNotice.svelte';
  import type {
    ProductMemberPageLoader,
    ProductNextPageLoader,
    ProductPageRelease
  } from './productResultPager';
  import type { WorkspaceLanguage } from './workspaceI18n';
  import type { WorkspaceRuntimeView } from './workspaceRuntime';
  import { workspacePublicFailure } from './workspacePublicFailure';

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
  $: missingPayloadFailures = view.status === 'completed' && !productResult
    ? [workspacePublicFailure('result-invalid')]
    : [];
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

  <WorkspaceFailureNotice failures={view.publicFailures} {language} compact />

  {#if productResult}
    <ProductResultPager
      payload={productResult}
      {language}
      loadNextPage={loadNextProductPage}
      loadMemberPage={loadProductMemberPage}
      releasePages={releaseProductPages}
    />
  {:else if view.status === 'completed'}
    <WorkspaceFailureNotice failures={missingPayloadFailures} {language} compact />
  {:else if view.status === 'idle'}
    <p class="empty">{korean ? '입력을 확인한 뒤 실행하세요.' : 'Review the input and run the capability.'}</p>
  {:else if view.status === 'running' || view.status === 'validating'}
    <p class="empty">{korean ? '검증된 결과를 계산하고 있습니다.' : 'Computing the validated result.'}</p>
  {/if}
</section>

<style>
  .result { background: #fff; border-top: 1px solid #d5dcd7; margin-top: 2px; padding: 22px max(24px, calc((100vw - 1460px) / 2)) 36px; }
  header, header > div { align-items: center; display: flex; }
  header { justify-content: space-between; }
  header > div { align-items: flex-start; flex-direction: column; gap: 3px; }
  h2 { font-size: 15px; margin: 0; }
  header span, .empty { color: #68736f; font-size: 11px; }
  .backend { background: #edf5f2; border-radius: 999px; color: #17675f; font-weight: 720; padding: 5px 9px; }
  .empty { margin: 18px 0; }
</style>
