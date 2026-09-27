<script lang="ts">
  import { getContext, onDestroy } from 'svelte';
  import { base } from '$app/paths';
  import type { WorkspaceLanguage } from '@clearra/ui/workspace';
  import {
    HOST_CAPABILITY_SNAPSHOT_CONTEXT,
    sharedBrowserHostCapabilitySnapshot,
    type HostCapabilitySnapshot
  } from '@clearra/ui/wasm';
  import type { AcceleratorCatalogPlan } from '../workers/clearraWasmRuntime';
  import { acceleratorLocalStorageSupported } from '../workers/acceleratorLocalStore';

  export let language: WorkspaceLanguage;
  const profiles = ['SRS', 'SRS+', 'SRS-X', 'Jstris 180', 'No kick'];
  const text = {
    ko: { title: '정확 가속 자산 관리', help: 'Legal-board와 조건부 도달성은 TB와 별개입니다. 선택한 킥테이블의 서명된 자산만 명시 요청 후 다운로드합니다. 설치 상태는 이 브라우저에만 적용됩니다.', product: '자산', profile: '킥테이블', legal: 'Exact legal-board', relation: '조건부 도달성', check: '상태·용량 확인', download: '표시한 자산 다운로드', remove: '저장한 자산 삭제', cancel: '취소', none: '저장된 자산 없음', current: '검증된 자산 저장됨', stale: '저장된 자산은 현재 버전과 다름', invalid: '저장된 자산이 손상됐거나 읽을 수 없습니다. 다시 다운로드하거나 삭제할 수 있습니다.', unavailable: '이 프로필은 아직 자격 검증이 끝나지 않았습니다.', size: '다운로드 크기', working: '처리 중…', changed: '변경되었습니다. 진행 중인 탐색은 바뀌지 않습니다.', cancelled: '취소했습니다. 이전 자산은 유지됩니다.', busy: '다른 탭이 자산을 사용 중입니다.', space: '업데이트에는 이전 자산과 새 자산을 함께 보존할 공간이 필요합니다.', error: '검증·저장에 실패했습니다. 이전 자산은 유지됩니다.', unsupported: '이 브라우저는 로컬 자산 저장을 지원하지 않습니다.', cleanup: '새 자산은 저장됐지만 이전 파일 정리가 남았습니다.' },
    en: { title: 'Exact accelerator assets', help: 'Legal-board and conditioned reachability are independent of the TB. A signed asset for one kick table downloads only after an explicit request. Storage belongs to this browser.', product: 'Asset', profile: 'Kick table', legal: 'Exact legal-board', relation: 'Conditioned reachability', check: 'Check status and size', download: 'Download displayed asset', remove: 'Delete saved asset', cancel: 'Cancel', none: 'No saved asset', current: 'Verified asset saved', stale: 'Saved asset differs from this version', invalid: 'The saved asset is damaged or unreadable. You can download it again or delete it.', unavailable: 'This profile is not qualified yet.', size: 'Download size', working: 'Working…', changed: 'Changed. Active searches are unaffected.', cancelled: 'Cancelled. The previous asset is preserved.', busy: 'Another tab is using this asset.', space: 'An update requires space for both the old and new assets.', error: 'Verification or storage failed. The previous asset is preserved.', unsupported: 'This browser does not support local asset storage.', cleanup: 'The new asset is saved, but old-file cleanup is pending.' },
    ja: { title: '正確な加速アセット', help: 'Legal-boardと条件付き到達性はTBと別です。選択したキックテーブルの署名済みアセットのみ、明示的な操作でダウンロードします。このブラウザだけに保存されます。', product: 'アセット', profile: 'キックテーブル', legal: 'Exact legal-board', relation: '条件付き到達性', check: '状態と容量を確認', download: '表示中のアセットをダウンロード', remove: '保存済みアセットを削除', cancel: '中止', none: '保存済みアセットなし', current: '検証済みアセットを保存済み', stale: '保存済みアセットは現在のバージョンと異なります', invalid: '保存済みアセットが破損しているか読み取れません。再ダウンロードまたは削除できます。', unavailable: 'このプロファイルはまだ適格ではありません。', size: 'ダウンロード容量', working: '処理中…', changed: '変更しました。実行中の検索には影響しません。', cancelled: '中止しました。以前のアセットは保持されます。', busy: '別のタブがこのアセットを使用中です。', space: '更新には旧・新アセットを保存できる容量が必要です。', error: '検証または保存に失敗しました。以前のアセットは保持されます。', unsupported: 'このブラウザはローカル保存に対応していません。', cleanup: '新アセットは保存されましたが、旧ファイルの削除が残っています。' }
  };
  const runtimeLimitText = {
    ko: '이 기기의 탐색 워커로 옮길 수 있는 자산 크기를 초과합니다. 저장은 가능하지만 탐색은 기존 정확 경로를 사용합니다.',
    en: 'This asset exceeds this device’s search-worker transfer limit. You may save it, but searches will use the existing exact path.',
    ja: 'この端末の検索ワーカーに転送できる容量を超えています。保存はできますが、検索には従来の正確な経路を使用します。'
  };
  $: t = text[language] ?? text.en;
  const runtimeTransferByteCap =
    (getContext<HostCapabilitySnapshot>(HOST_CAPABILITY_SNAPSHOT_CONTEXT) ??
      sharedBrowserHostCapabilitySnapshot()).wasmTransferByteCap;
  let kind = 0, profile = 0, worker: Worker | null = null;
  let busy = false, destroyed = false, supported = true;
  let plan: AcceleratorCatalogPlan | null = null;
  let local: { payload_bytes: number; current: boolean } | null = null;
  let localInvalid = false;
  let progress = 0, total = 0;
  let message: keyof typeof text.en | '' = '';
  const size = (bytes: number) => `${(bytes / 1024 / 1024).toFixed(1)} MiB`;

  function initialize() {
    supported = acceleratorLocalStorageSupported();
    if (!supported || worker) return;
    worker = new Worker(new URL('../workers/acceleratorDownloadWorker.ts', import.meta.url), { type: 'module' });
    worker.onmessage = ({ data }) => {
      if (data.type === 'idle') {
        busy = false;
        if (destroyed) { worker?.terminate(); worker = null; }
      } else if (data.type === 'progress') { progress = data.transferredBytes; total = data.totalBytes; }
      else if (data.type === 'status') { plan = data.plan; local = data.local; localInvalid = data.localState === 'invalid_asset'; message = ''; }
      else if (data.type === 'installed') { local = data.local; localInvalid = false; message = data.cleanupPending ? 'cleanup' : 'changed'; }
      else if (data.type === 'removed') { local = null; localInvalid = false; message = 'changed'; }
      else if (data.type === 'error') {
        message = data.code === 'accelerator_download_cancelled' ? 'cancelled'
          : data.code === 'accelerator_store_busy' ? 'busy'
          : data.code === 'accelerator_store_insufficient_space' || data.code === 'QuotaExceededError' ? 'space' : 'error';
      }
    };
    worker.onerror = () => { busy = false; message = 'error'; worker?.terminate(); worker = null; };
    act('status');
  }
  function act(action: 'status' | 'download' | 'remove' | 'cancel') {
    if (!worker || (busy && action !== 'cancel')) return;
    if (action !== 'cancel') { busy = true; message = ''; progress = 0; total = 0; }
    worker.postMessage({ action, kind, profile, base });
  }
  function changedSelection() { plan = null; local = null; localInvalid = false; act('status'); }
  onDestroy(() => {
    destroyed = true;
    if (busy) worker?.postMessage({ action: 'cancel', kind, profile, base });
    else { worker?.terminate(); worker = null; }
  });
</script>

<details class="accelerator-download" on:toggle={(event) => { if ((event.currentTarget as HTMLDetailsElement).open) initialize(); }}>
  <summary>{t.title}</summary>
  <p>{t.help}</p>
  {#if !supported}<p role="status">{t.unsupported}</p>
  {:else}
    <label>{t.product}
      <select bind:value={kind} disabled={busy} on:change={changedSelection}>
        <option value={0}>{t.legal}</option><option value={1}>{t.relation}</option>
      </select>
    </label>
    <label>{t.profile}
      <select bind:value={profile} disabled={busy} on:change={changedSelection}>
        {#each profiles as name, index}<option value={index}>{name}</option>{/each}
      </select>
    </label>
    <p role="status">{localInvalid ? t.invalid : local ? `${local.current ? t.current : t.stale} · ${size(local.payload_bytes)}` : t.none}</p>
    {#if plan?.state === 'not_qualified'}<p role="status">{t.unavailable}</p>{/if}
    {#if plan?.state === 'qualified' && plan.payload_bytes}<p>{t.size}: {size(plan.payload_bytes)}</p>{/if}
    {#if plan?.state === 'qualified' && plan.payload_bytes && plan.payload_bytes > runtimeTransferByteCap}
      <p role="status">{runtimeLimitText[language] ?? runtimeLimitText.en} ({size(runtimeTransferByteCap)})</p>
    {/if}
    <div class="actions">
      <button type="button" disabled={busy} on:click={() => act('status')}>{t.check}</button>
      {#if plan?.state === 'qualified' && !local?.current}
        <button type="button" disabled={busy} on:click={() => act('download')}>{t.download}</button>
      {/if}
      {#if local || localInvalid}<button type="button" disabled={busy} on:click={() => act('remove')}>{t.remove}</button>{/if}
    </div>
    {#if busy}
      <p role="status">{total ? `${size(progress)} / ${size(total)}` : t.working}</p>
      {#if total}<progress value={progress} max={total}></progress>{/if}
      <button type="button" on:click={() => act('cancel')}>{t.cancel}</button>
    {/if}
    {#if message}<p role="status">{t[message]}</p>{/if}
  {/if}
</details>

<style>
  .accelerator-download { margin-top: .5rem; font-size: .85rem; }
  summary { cursor: pointer; }
  label { display: inline-flex; flex-direction: column; margin: .25rem .5rem .25rem 0; }
  p { overflow-wrap: anywhere; margin: .5rem 0; }
  .actions { display: flex; flex-wrap: wrap; gap: .5rem; }
  button { font: inherit; padding: .3rem .5rem; cursor: pointer; }
  progress { max-width: 100%; }
</style>
