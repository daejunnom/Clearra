<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onDestroy } from 'svelte';
  import type { WorkspaceLanguage } from '@clearra/ui/workspace';

  export let language: WorkspaceLanguage;
  const products = ['exact-legal-board', 'board-conditioned-reachability'] as const;
  const profiles = ['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick'] as const;
  const profileNames = ['SRS', 'SRS+', 'SRS-X', 'Jstris 180', 'No kick'];
  const messages = {
    ko: { title: '정확 가속 자산 관리', help: 'TB와 별개의 Legal-board·조건부 도달성 자산입니다. 선택한 킥테이블의 서명된 자산은 다운로드를 명시적으로 요청할 때만 받습니다.', product: '자산', profile: '킥테이블', legal: 'Exact legal-board', relation: '조건부 도달성', check: '상태·용량 확인', download: '표시한 자산 다운로드', remove: '저장한 자산 삭제', cancel: '취소', unavailable: '이 프로필은 아직 자격 검증이 끝나지 않았습니다.', none: '설치된 자산 없음', ready: '검증된 자산 설치됨', stale: '설치된 자산은 현재 catalog와 다릅니다.', candidate: '미자격 로컬 후보가 저장돼 있습니다.', invalid: '저장된 자산이 유효하지 않습니다.', size: '다운로드 크기', working: '처리 중…', cancelled: '다운로드를 취소했습니다. 기존 자산은 유지됩니다.', changed: '변경되었습니다. 진행 중인 탐색은 바뀌지 않습니다.', error: '자산 작업에 실패했습니다.' },
    en: { title: 'Exact accelerator assets', help: 'Legal-board and conditioned reachability are separate from the TB. A signed asset for the selected kick table is downloaded only on explicit request.', product: 'Asset', profile: 'Kick table', legal: 'Exact legal-board', relation: 'Conditioned reachability', check: 'Check status and size', download: 'Download displayed asset', remove: 'Delete saved asset', cancel: 'Cancel', unavailable: 'This profile is not qualified yet.', none: 'No installed asset', ready: 'Verified asset installed', stale: 'The installed asset differs from the current catalog.', candidate: 'An unqualified local candidate is stored.', invalid: 'The stored asset is invalid.', size: 'Download size', working: 'Working…', cancelled: 'Download cancelled. The previous asset is preserved.', changed: 'Changed. Active searches are unaffected.', error: 'Asset operation failed.' },
    ja: { title: '正確な加速アセット', help: 'Legal-boardと条件付き到達性はTBと別です。選択したキックテーブルの署名済みアセットは、明示的に要求したときだけダウンロードします。', product: 'アセット', profile: 'キックテーブル', legal: 'Exact legal-board', relation: '条件付き到達性', check: '状態と容量を確認', download: '表示中のアセットをダウンロード', remove: '保存したアセットを削除', cancel: '中止', unavailable: 'このプロファイルはまだ適格ではありません。', none: 'インストール済みアセットなし', ready: '検証済みアセットをインストール済み', stale: 'インストール済みアセットは現在のcatalogと異なります。', candidate: '未適格のローカル候補が保存されています。', invalid: '保存したアセットは無効です。', size: 'ダウンロード容量', working: '処理中…', cancelled: 'ダウンロードを中止しました。以前のアセットは保持されます。', changed: '変更しました。実行中の検索には影響しません。', error: 'アセットの操作に失敗しました。' }
  };
  $: t = messages[language] ?? messages.en;
  let product: typeof products[number] = products[0];
  let profile: typeof profiles[number] = profiles[0];
  let catalog: { qualified?: boolean; compressed_bytes?: number; catalog_status?: string } | null = null;
  let local: { installed?: boolean; qualified?: boolean; validation?: string; installed_payload_bytes?: number; candidate_bundle_bytes?: number; forward_layer_count?: number; legal_layer_count?: number } | null = null;
  $: hasStoredFiles = Boolean(local?.installed || local?.candidate_bundle_bytes || local?.forward_layer_count || local?.legal_layer_count);
  let busy = false, destroyed = false, cancelling = false;
  let operationId: number | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let transferred = 0, total = 0;
  let message: 'changed' | 'cancelled' | 'error' | '' = '';
  const size = (bytes: number) => `${(bytes / 1024 / 1024).toFixed(1)} MiB`;

  function clearTimer() {
    if (timer !== null) clearTimeout(timer);
    timer = null;
  }
  function errorText(error: unknown): 'cancelled' | 'error' {
    const text = String(error);
    return text.includes('download cancelled') ? 'cancelled' : 'error';
  }
  async function inspect() {
    if (busy || destroyed) return;
    busy = true;
    message = '';
    try {
      const [check, status] = await Promise.all([
        invoke<string>('accelerator_asset_action', { product, action: 'check', profile }),
        invoke<string>('accelerator_asset_action', { product, action: 'status', profile })
      ]);
      if (destroyed) return;
      catalog = JSON.parse(check);
      local = JSON.parse(status);
    } catch {
      if (!destroyed) message = 'error';
    } finally {
      busy = false;
    }
  }
  async function action(name: 'remove' | 'download') {
    if (busy || destroyed) return;
    busy = true;
    message = '';
    if (name === 'download') {
      try {
        const startedId = await invoke<number>('accelerator_asset_start_download', { product, profile });
        operationId = startedId;
        if (destroyed) {
          await invoke('accelerator_asset_cancel', { operationId: startedId });
          return;
        }
        transferred = 0;
        total = catalog?.compressed_bytes ?? 0;
        void poll(startedId);
      } catch (error) {
        message = errorText(error);
        busy = false;
      }
      return;
    }
    try {
      await invoke<string>('accelerator_asset_action', { product, action: 'remove', profile });
      if (!destroyed) {
        local = null;
        message = 'changed';
      }
    } catch (error) {
      if (!destroyed) message = errorText(error);
    } finally {
      busy = false;
    }
  }
  async function poll(id: number, failures = 0) {
    if (destroyed) return;
    try {
      const raw = await invoke<string>('accelerator_asset_progress', { operationId: id });
      const progress: { done: boolean; transferred_bytes: number; total_bytes: number; result?: string; error?: string } = JSON.parse(raw);
      if (destroyed) return;
      transferred = progress.transferred_bytes;
      total = progress.total_bytes || total;
      if (progress.done) {
        operationId = null;
        busy = false;
        message = progress.error ? errorText(progress.error) : 'changed';
        cancelling = false;
        if (!progress.error) void inspect();
      } else {
        timer = setTimeout(() => void poll(id), 250);
      }
    } catch (error) {
      if (!destroyed) {
        if (failures < 2) {
          timer = setTimeout(() => void poll(id, failures + 1), 250);
        } else {
          void invoke('accelerator_asset_cancel', { operationId: id }).catch(() => {});
          operationId = null;
          busy = false;
          message = errorText(error);
        }
      }
    }
  }
  async function cancel() {
    if (operationId === null) return;
    cancelling = true;
    try { await invoke('accelerator_asset_cancel', { operationId }); }
    catch { if (!destroyed) message = 'error'; }
  }
  function selectionChanged() {
    catalog = null;
    local = null;
    void inspect();
  }
  onDestroy(() => {
    destroyed = true;
    clearTimer();
    if (operationId !== null) void invoke('accelerator_asset_cancel', { operationId }).catch(() => {});
  });
</script>

<details class="accelerator-download" on:toggle={(event) => { if ((event.currentTarget as HTMLDetailsElement).open) void inspect(); }}>
  <summary>{t.title}</summary>
  <p>{t.help}</p>
  <label>{t.product}
    <select bind:value={product} disabled={busy} on:change={selectionChanged}>
      <option value={products[0]}>{t.legal}</option>
      <option value={products[1]}>{t.relation}</option>
    </select>
  </label>
  <label>{t.profile}
    <select bind:value={profile} disabled={busy} on:change={selectionChanged}>
      {#each profiles as item, index}<option value={item}>{profileNames[index]}</option>{/each}
    </select>
  </label>
  <p role="status">{local?.qualified ? `${t.ready} · ${size(local.installed_payload_bytes ?? 0)}` : local?.validation === 'snapshot_mismatch' ? t.stale : local?.installed ? t.invalid : hasStoredFiles ? t.candidate : t.none}</p>
  {#if catalog?.catalog_status === 'not_qualified'}<p role="status">{t.unavailable}</p>{/if}
  {#if catalog?.qualified && catalog.compressed_bytes}<p>{t.size}: {size(catalog.compressed_bytes)}</p>{/if}
  <div class="actions">
    <button type="button" disabled={busy} on:click={() => void inspect()}>{t.check}</button>
    {#if catalog?.qualified && !local?.qualified}
      <button type="button" disabled={busy} on:click={() => void action('download')}>{t.download}</button>
    {/if}
    {#if hasStoredFiles}<button type="button" disabled={busy} on:click={() => void action('remove')}>{t.remove}</button>{/if}
  </div>
  {#if busy && operationId !== null}
    <p role="status">{total ? `${size(transferred)} / ${size(total)}` : t.working}</p>
    {#if total}<progress value={transferred} max={total}></progress>{/if}
    <button type="button" disabled={cancelling} on:click={() => void cancel()}>{t.cancel}</button>
  {:else if busy}<p role="status">{t.working}</p>{/if}
  {#if message}<p role="status">{t[message]}</p>{/if}
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
