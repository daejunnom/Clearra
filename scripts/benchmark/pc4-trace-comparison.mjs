// Local-only bounded comparison. Preserve order-sensitive evidence, and also
// hash received bytes in the recorded prefix order. This distinguishes an
// independent-read swap from lost/changed bytes without re-running an old job.
import { createHash } from 'node:crypto';
import { checkedPc4Read } from '../release/pc4/pc4-range-plan.mjs';

export function createPc4TraceComparison(reference, artifacts) {
  if (!Buffer.isBuffer(reference) || reference.length % 12 || reference.length > 2400000) throw new Error('Invalid trace body');
  const total = reference.length / 12;
  const tupleAt = ordinal => [reference.readUInt32LE(ordinal * 12), reference.readUInt32LE(ordinal * 12 + 4), reference.readUInt32LE(ordinal * 12 + 8)];
  for (let i = 0; i < total; i++) { const [role, offset, length] = tupleAt(i); checkedPc4Read(artifacts[role], offset, length); }
  const digest = createHash('sha256'), pending = new Map(), examples = [];
  let count = 0, cursor = 0, held = 0, heldBytes = 0, mismatches = 0;
  const hash = (tuple, bytes) => digest.update(`${artifacts[tuple[0]].path}:${tuple[1]}:${tuple[2]}\n`).update(bytes);
  return {
    observe(artifact, offset, length, bytes) {
      const role = artifacts.findIndex(a => a.path === artifact.path && a.byte_length === artifact.byte_length && a.content_identity === artifact.content_identity);
      if (role < 0 || bytes.length !== length) throw new Error('Trace artifact drift');
      const actual = [role, offset, length];
      checkedPc4Read(artifacts[role], offset, length);
      if (count >= total) {
        if (cursor !== total) throw new Error('Trace prefix has unmatched demands');
        hash(actual, bytes); count++; return;
      }
      const expected = tupleAt(count);
      if (expected.some((n, i) => n !== actual[i])) {
        mismatches++;
        if (examples.length < 16) examples.push({ ordinal: count + 1, expected, actual });
      }
      count++;
      if (held >= 128 || heldBytes + bytes.length > 8 * 1024 * 1024) throw new Error('Trace reorder window exceeded');
      const key = actual.join(':');
      if (!pending.has(key)) pending.set(key, []);
      pending.get(key).push(bytes.slice()); held++; heldBytes += bytes.length;
      while (cursor < total) {
        const next = tupleAt(cursor), nextKey = next.join(':'), available = pending.get(nextKey);
        if (!available?.length) break;
        const body = available.shift(); held--; heldBytes -= body.length;
        if (!available.length) pending.delete(nextKey);
        hash(next, body); cursor++;
      }
    },
    finish() {
      return { comparison_read_count: Math.min(count, total), comparison_covers_measurement: count <= total,
        comparison_mismatches: mismatches, mismatch_examples: examples,
        comparison_multiset_equal: cursor === Math.min(count, total) && held === 0,
        reference_ordered_demand_sha256: held === 0 ? digest.digest('hex') : null };
    }
  };
}
