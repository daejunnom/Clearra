// SRP: plan finite explicit byte demands into bounded contiguous transfers.
// No I/O, graph traversal, qualification or speculative dataset scanning.
const MAX_RANGE = 65_536;
const MAX_BATCH = 512;

export function checkedPc4Read(input, offset, length) {
  if (!Number.isSafeInteger(input?.byte_length) || input.byte_length <= 0 ||
      !Number.isSafeInteger(offset) || offset < 0 || !Number.isSafeInteger(length) || length < 1 ||
      length > MAX_RANGE || offset > input.byte_length - length ||
      !/^[A-Za-z0-9_.-]+\.bin$/.test(input.path) ||
      !/^sha256:[0-9a-f]{64}$/.test(input.content_identity)) throw new RangeError('pc4_online_range_invalid');
  return { artifact: { path: input.path, byte_length: input.byte_length, content_identity: input.content_identity },
    offset, length };
}

export function planPc4ReadBatch(demands, { maxGapBytes = 1_024 } = {}) {
  if (!Array.isArray(demands) || demands.length > MAX_BATCH || !Number.isSafeInteger(maxGapBytes) ||
      maxGapBytes < 0 || maxGapBytes > 4_096) throw new RangeError('pc4_online_batch_invalid');
  const checked = demands.map((d, index) => ({ ...checkedPc4Read(d?.artifact, d?.offset, d?.length), index }));
  const files = new Map();
  for (const demand of checked) {
    const a = demand.artifact;
    const identity = `${a.path}:${a.byte_length}:${a.content_identity}`;
    if (!files.has(identity)) files.set(identity, []);
    files.get(identity).push(demand);
  }
  const transfers = [];
  for (const fileDemands of files.values()) {
    fileDemands.sort((a, b) => a.offset - b.offset || a.length - b.length || a.index - b.index);
    let current;
    for (const d of fileDemands) {
      const end = d.offset + d.length;
      const mergedEnd = Math.max(current?.end ?? end, end);
      if (!current || d.offset > current.end + maxGapBytes || mergedEnd - current.offset > MAX_RANGE) {
        current = { artifact: d.artifact, offset: d.offset, end, demands: [] };
        transfers.push(current);
      } else current.end = mergedEnd;
      current.demands.push({ index: d.index, offset: d.offset, length: d.length });
    }
  }
  return transfers.map(({ end, ...transfer }) => ({ ...transfer, length: end - transfer.offset }));
}
