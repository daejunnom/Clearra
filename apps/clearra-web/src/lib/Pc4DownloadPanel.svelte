<script lang="ts">
  import { onDestroy } from 'svelte';
  import type { WorkspaceLanguage } from '@clearra/ui/workspace';
  import { pc4LocalStorageSupported } from '../workers/pc4LocalStore';
  export let language: WorkspaceLanguage;
  const texts = {
    ko: { title: 'TB 전체 다운로드', help: '선택한 킥테이블의 그래프와 두 인덱스만 저장합니다. 완성·호환성 확인이 된 프로필만 다운로드할 수 있습니다. 저장 후에는 필요한 구간을 로컬로 읽으며 다른 브라우저·주소와 공유되지 않습니다.',
      check: '최신 데이터·용량 확인', download: '표시한 데이터 전체 다운로드', remove: '저장한 TB 삭제', cancel: '취소', none: '저장된 TB 없음', ready: '로컬 TB 사용 가능',
      available: '다운로드 용량', revision: '데이터 버전', idle: '완료', waiting: '처리 중…', refresh: '상태 확인',
      changed: '완료되었습니다. 실행 중인 탐색은 변경되지 않습니다. TB 사용을 껐다 켜면 다음 탐색부터 반영됩니다.',
      cleanup: '새 TB 저장은 완료했지만 이전 파일 정리가 남았습니다. 저장한 TB 삭제 또는 다음 업데이트에서 다시 정리합니다.',
      error: '완료하지 못했습니다. 연결과 저장 공간을 확인해 주세요. 기존 TB는 유지됩니다.', busy: '다른 탭의 탐색 또는 다운로드가 사용 중입니다. 완료 후 다시 시도해 주세요.',
      unsupported: '이 브라우저에서는 로컬 TB 저장을 지원하지 않습니다. 부분 조회는 계속 사용할 수 있습니다.', unavailable: '현재 완성·호환성 확인이 된 프로필을 다운로드할 수 없습니다.',
      space: '업데이트 중에는 기존 데이터와 새 데이터를 함께 보존할 공간이 필요합니다. 공간을 확보하거나 저장한 TB를 먼저 삭제해 주세요.', cancelled: '다운로드를 취소했습니다. 기존 TB는 유지됩니다.' },
    en: { title: 'Download full TB', help: 'Store the selected kick table’s graph and two indexes. Only completed, compatible profiles can be downloaded. Searches read local slices. Storage is specific to this browser and site address.',
      check: 'Check latest data and size', download: 'Download the displayed dataset', remove: 'Delete saved TB', cancel: 'Cancel', none: 'No saved TB', ready: 'Local TB available',
      available: 'Download size', revision: 'Data revision', idle: 'Done', waiting: 'Working…', refresh: 'Refresh status',
      changed: 'Done. Active searches are unchanged. Turn TB off and on to use the change in the next search.',
      cleanup: 'The new TB is saved, but old-file cleanup is pending. Delete saved TB or retry cleanup on the next update.',
      error: 'Could not finish. Check the connection and storage space. The previous TB is preserved.', busy: 'Another tab is searching or downloading. Try again after it finishes.',
      unsupported: 'Local TB storage is unavailable in this browser. Partial online reads remain available.', unavailable: 'No completed, compatible profile is currently available to download.',
      space: 'An update needs space for both the old and new data. Free some space or delete the saved TB first.', cancelled: 'Download cancelled. The previous TB is preserved.' },
    ja: { title: 'TBを全体ダウンロード', help: '選択したキックテーブルのグラフと2つのインデックスのみ保存します。完成と互換性を確認できたプロファイルのみダウンロードできます。必要な部分をローカルで読み取り、別のブラウザやサイトとは共有されません。',
      check: '最新データと容量を確認', download: '表示されたデータをダウンロード', remove: '保存済みTBを削除', cancel: 'キャンセル', none: '保存済みTBなし', ready: 'ローカルTBを使用可能',
      available: 'ダウンロード容量', revision: 'データのバージョン', idle: '完了', waiting: '処理中…', refresh: '状態を確認',
      changed: '完了しました。実行中の検索は変わりません。TBをオフにして再度オンにすると、次の検索から反映されます。',
      cleanup: '新しいTBの保存は完了しましたが、旧ファイルの削除が残っています。保存済みTBの削除または次回の更新で再試行します。',
      error: '完了できませんでした。接続と空き容量を確認してください。以前のTBは保持されます。', busy: '別のタブで検索またはダウンロード中です。終了後に再試行してください。',
      unsupported: 'このブラウザではローカルTBを保存できません。オンラインの部分読み取りは引き続き利用できます。', unavailable: '完成と互換性を確認できたプロファイルを現在ダウンロードできません。',
      space: '更新には旧データと新データの両方を保存できる空き容量が必要です。空き容量を確保するか、保存済みTBを先に削除してください。', cancelled: 'ダウンロードをキャンセルしました。以前のTBは保持されます。' }
  };
  $: t = texts[language] ?? texts.en;
  let worker: Worker | null = null;
  let profile = 'jstris-180';
  const profiles = [ ['srs', 'SRS'], ['srs-plus', 'SRS+'], ['srs-x', 'SRS-X'], ['jstris-180', 'Jstris 180'], ['no-kick', 'No kick'] ];
  let busy = false, destroyed = false, supported = true;
  let plan: { totalBytes: number; revision: string } | null = null;
  let local: { storedBytes: number; generation: { revision: string } } | null = null;
  let progress = 0, total = 0;
  let message: keyof typeof texts.en | '' = '';
  const size = (bytes: number) => `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
  function initialize() {
    supported = pc4LocalStorageSupported();
    if (!supported || worker) return;
    worker = new Worker(new URL('../workers/pc4DownloadWorker.ts', import.meta.url), { type: 'module' });
    worker.onmessage = ({ data }) => {
      if (data.type === 'idle') {
        busy = false;
        if (destroyed) { worker?.terminate(); worker = null; }
      } else if (data.type === 'progress') { progress = data.transferredBytes; total = data.totalBytes; }
      else if (data.type === 'prepared') { plan = data.plan; message = ''; }
      else if (data.type === 'status' || data.type === 'installed') {
        local = data.local;
        if (data.type === 'installed') { message = data.cleanupPending ? 'cleanup' : 'changed'; plan = null; }
      } else if (data.type === 'removed') { local = null; message = 'changed'; }
      else if (data.type === 'error') {
        message = data.code === 'pc4_download_cancelled' ? 'cancelled' : data.code === 'pc4_download_storage_busy' ? 'busy'
          : data.code === 'pc4_download_insufficient_space' || data.code === 'QuotaExceededError' ? 'space'
          : data.code === 'pc4_download_profile_unavailable' ? 'unavailable' : 'error';
      }
    };
    worker.onerror = () => { busy = false; message = 'error'; worker?.terminate(); worker = null; };
    act('status');
  }
  function act(action: string) {
    if (!worker || (busy && action !== 'cancel')) return;
    if (action !== 'cancel') { busy = true; message = ''; progress = 0; total = 0; }
    worker.postMessage({ action, profile });
  }
  onDestroy(() => {
    destroyed = true;
    if (busy) worker?.postMessage({ action: 'cancel' });
    else { worker?.terminate(); worker = null; }
  });
</script>

<details class="pc4-download" on:toggle={(event) => { if ((event.currentTarget as HTMLDetailsElement).open) initialize(); }}>
  <summary>{t.title}</summary>
  <p>{t.help}</p>
  {#if !supported}<p role="status">{t.unsupported}</p>
  {:else}
    <label>
      {language === 'ko' ? '다운로드할 킥테이블' : language === 'ja' ? 'ダウンロードするキックテーブル' : 'Kick table to download'}
      <select value={profile} disabled={busy} on:change={(event) => { profile = (event.currentTarget as HTMLSelectElement).value; plan = null; local = null; act('status'); }}>
        {#each profiles as [value, name]}<option {value}>{name}</option>{/each}
      </select>
    </label>
    {#if profile !== 'jstris-180'}<p>{t.unavailable}</p>{/if}
    <p>{local ? `${t.ready} · ${size(local.storedBytes)}` : t.none}</p>
    <div class="actions">
      <button type="button" disabled={busy} on:click={() => { plan = null; act('prepare'); }}>{t.check}</button>
      <button type="button" disabled={busy} on:click={() => act('status')}>{t.refresh}</button>
      {#if local}<button type="button" disabled={busy} on:click={() => act('remove')}>{t.remove}</button>{/if}
    </div>
    {#if plan}
      <p>{t.available}: {size(plan.totalBytes)} · {t.revision}: {plan.revision.slice(0, 12)}</p>
      <button type="button" disabled={busy} on:click={() => act('download')}>{t.download}</button>
    {/if}
    {#if busy}
      <p role="status">{total ? `${size(progress)} / ${size(total)}` : t.waiting}</p>
      {#if total}<progress value={progress} max={total}></progress>{/if}
      <button type="button" on:click={() => act('cancel')}>{t.cancel}</button>
    {/if}
    {#if message}<p role="status">{t[message]}</p>{/if}
  {/if}
</details>

<style>
  .pc4-download { margin-top: .5rem; font-size: .85rem; }
  summary { cursor: pointer; }
  p { overflow-wrap: anywhere; margin: .5rem 0; }
  .actions { display: flex; flex-wrap: wrap; gap: .5rem; }
  button { font: inherit; padding: .3rem .5rem; cursor: pointer; }
  button:disabled { cursor: default; }
  progress { max-width: 100%; }
</style>
