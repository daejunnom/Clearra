// SRP: explicit, streamed, generation-bound installation. Storage and I/O are
// injected. Normal Range reads can never enter this full-response path.
import { PC4_READER_CONTRACT } from './qualify-upstream-generation.mjs';
import { Pc4StreamSha256 } from './pc4-stream-sha256.mjs';

export function pc4DownloadPlan(generation, profile) {
  if (!['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick'].includes(profile)) fail('pc4_download_profile_required');
  if (generation?.schema !== 'clearra.pc4.host-generation.v1' ||
      generation.repository !== 'muse918/tetris-4lpc-mdp-vstar-policy' ||
      !/^[0-9a-f]{40}$/.test(generation.revision) || generation.profiles?.length !== 5 ||
      ['srs','srs-plus','srs-x','jstris-180','no-kick'].some(name => generation.profiles.filter(p => p.profile === name).length !== 1)) {
    fail('pc4_download_generation_invalid');
  }
  const slots = generation.profiles.filter(p => p.profile === profile);
  const slot = slots[0];
  if (slots.length !== 1 || profile !== 'jstris-180' || slot.status !== 'ready' ||
      slot.upstream_complete !== true || slot.reader_contract !== PC4_READER_CONTRACT) fail('pc4_download_profile_unavailable');
  const files = ['fields', 'offsets', 'graph'].map(key => ({ ...slot.artifacts?.[key] }));
  const paths = ['field_hash_to_id.v1.bin', 'graph_offsets.u32.bin', 'graph.bin'];
  for (const [i, file] of files.entries()) {
    if (file.path !== paths[i] || !Number.isSafeInteger(file.byte_length) || file.byte_length <= 0 ||
        file.byte_length > 2 ** 31 || !/^sha256:[0-9a-f]{64}$/.test(file.content_identity)) fail('pc4_download_artifact_invalid');
  }
  return { profile, revision: generation.revision, repository: generation.repository,
    files, totalBytes: files.reduce((n, file) => n + file.byte_length, 0) };
}

export async function downloadPc4Profile(generation, store, { intent, profile, signal,
  fetcher = fetch, onProgress = () => {}, hasherFactory = () => new Pc4StreamSha256() } = {}) {
  if (intent !== 'explicit-download') fail('pc4_download_explicit_action_required');
  // Immutable copy: a caller cannot change size/digest/revision during I/O.
  const pinned = JSON.parse(JSON.stringify(generation));
  const plan = pc4DownloadPlan(pinned, profile);
  if (signal?.aborted) fail('pc4_download_cancelled');
  const transaction = await store.begin(plan);
  let transferred = 0;
  try {
    for (const file of plan.files) {
      if (signal?.aborted) fail('pc4_download_cancelled');
      const controller = new AbortController();
      const abort = () => controller.abort();
      signal?.addEventListener('abort', abort, { once: true });
      let timer = setTimeout(abort, 60_000), writer, stream;
      try {
        if (signal?.aborted) fail('pc4_download_cancelled');
        const response = await fetcher(`https://huggingface.co/datasets/${plan.repository}/resolve/${plan.revision}/${file.path}`,
          { credentials: 'omit', signal: controller.signal });
        const declared = response.headers.get('content-length');
        if (response.status !== 200 || (declared !== null && Number(declared) !== file.byte_length)) {
          await response.body?.cancel();
          fail(response.status === 429 ? 'pc4_download_rate_limited' : 'pc4_download_response_invalid');
        }
        stream = response.body?.getReader();
        if (!stream) fail('pc4_download_empty_body');
        writer = await transaction.open(file);
        const digest = hasherFactory();
        let length = 0;
        while (true) {
          const { value, done } = await stream.read();
          if (done) break;
          clearTimeout(timer); timer = setTimeout(abort, 60_000);
          if (signal?.aborted) fail('pc4_download_cancelled');
          if (length + value.length > file.byte_length) fail('pc4_download_size_mismatch');
          // Stream backpressure bounds memory to one network chunk + writer.
          digest.update(value); await writer.write(value);
          length += value.length; transferred += value.length;
          onProgress({ transferredBytes: transferred, totalBytes: plan.totalBytes, file: file.path });
        }
        if (length !== file.byte_length || `sha256:${digest.hex()}` !== file.content_identity) fail('pc4_download_integrity_mismatch');
        await writer.close(); writer = null;
      } catch (error) {
        await stream?.cancel().catch(() => {});
        await writer?.abort().catch(() => {});
        if (signal?.aborted) fail('pc4_download_cancelled');
        if (controller.signal.aborted) fail('pc4_download_timeout');
        throw error;
      } finally { clearTimeout(timer); signal?.removeEventListener('abort', abort); }
    }
    if (signal?.aborted) fail('pc4_download_cancelled');
    const publication = await transaction.commit(pinned);
    return { profile, revision: plan.revision, storedBytes: plan.totalBytes, cleanupPending: publication?.cleanupPending === true };
  } catch (error) { await transaction.abort().catch(() => {}); throw error; }
}
function fail(code) { const error = new Error(code); error.code = code; throw error; }
