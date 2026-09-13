import { downloadPc4Profile, pc4DownloadPlan } from '../../../../scripts/release/pc4/pc4-download.mjs';
import { qualifyPc4UpstreamGeneration, type Pc4HostGeneration } from '../../../../scripts/release/pc4/qualify-upstream-generation.mjs';
import { localPc4Status, pc4LocalDownloadStore, removeLocalPc4 } from './pc4LocalStore';

let prepared: Pc4HostGeneration | null = null;
let active: AbortController | null = null;
self.onmessage = async ({ data }: MessageEvent<{ action: string; profile?: string }>) => {
  if (data.action === 'cancel') { active?.abort(); return; }
  if (active) { postMessage({ type: 'error', code: 'pc4_download_storage_busy' }); return; }
  const controller = new AbortController(); active = controller;
  const profile = data.profile;
  try {
    if (!profile || !['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick'].includes(profile)) {
      throw new Error('pc4_download_profile_required');
    }
    switch (data.action) {
      case 'status': postMessage({ type: 'status', local: await localPc4Status(profile) }); break;
      case 'prepare': {
        prepared = null;
        if (profile !== 'jstris-180') throw new Error('pc4_download_profile_unavailable');
        prepared = await qualifyPc4UpstreamGeneration({ signal: controller.signal });
        postMessage({ type: 'prepared', plan: pc4DownloadPlan(prepared, profile) }); break;
      }
      case 'download': {
        if (!prepared) throw new Error('pc4_download_prepare_required');
        const installed = await downloadPc4Profile(prepared, pc4LocalDownloadStore, { intent: 'explicit-download', profile, signal: controller.signal,
          onProgress: progress => postMessage({ type: 'progress', ...progress }) });
        postMessage({ type: 'installed', local: await localPc4Status(profile), cleanupPending: installed.cleanupPending }); break;
      }
      case 'remove': await removeLocalPc4(profile); postMessage({ type: 'removed' }); break;
      default: throw new Error('pc4_download_action_invalid');
    }
  } catch (error) {
    postMessage({ type: 'error', code: controller.signal.aborted ? 'pc4_download_cancelled'
      : error instanceof DOMException && error.name === 'QuotaExceededError' ? error.name
      : error instanceof Error ? error.message : 'pc4_download_failed' });
  } finally { active = null; postMessage({ type: 'idle' }); }
};
