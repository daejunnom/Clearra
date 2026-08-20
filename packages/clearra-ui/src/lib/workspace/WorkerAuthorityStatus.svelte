<script lang="ts">
  import type { WorkerAuthorityReport } from '../wasm';
  import type { WorkspaceLanguage } from './workspaceI18n';

  export let authority: WorkerAuthorityReport;
  export let language: WorkspaceLanguage;

  $: reason = reasonLabel(authority.reason, language);

  function reasonLabel(
    value: WorkerAuthorityReport['reason'],
    currentLanguage: WorkspaceLanguage
  ): string {
    const labels = currentLanguage === 'ko'
      ? {
          'reserved-main-thread': '메인 스레드 1개 예약',
          'all-logical-processors': '모든 논리 프로세서 사용',
          'explicit-request': '명시적 요청',
          'host-cap': '호스트 상한 적용',
          'invalid-request': '잘못된 요청을 안전값으로 조정'
        }
      : {
          'reserved-main-thread': 'one main-thread processor reserved',
          'all-logical-processors': 'all logical processors requested',
          'explicit-request': 'explicit request',
          'host-cap': 'limited by the host snapshot',
          'invalid-request': 'invalid request reduced to the safe floor'
        };
    return labels[value];
  }
</script>

<p
  class="worker-authority"
  data-snapshot-id={authority.snapshotId}
  data-workers-requested={authority.workersRequested}
  data-workers-effective={authority.workersEffective}
  data-worker-reason={authority.reason}
  aria-live="polite"
>
  {#if language === 'ko'}
    워커 요청 {authority.workersRequested}개 · 적용 {authority.workersEffective}개 · {reason}
  {:else}
    Workers: {authority.workersRequested} requested · {authority.workersEffective} effective · {reason}
  {/if}
</p>

<style>
  .worker-authority {
    color: #596a64;
    font-size: 10px;
    line-height: 1.45;
    margin: 5px 0 0;
    overflow-wrap: anywhere;
  }
</style>
