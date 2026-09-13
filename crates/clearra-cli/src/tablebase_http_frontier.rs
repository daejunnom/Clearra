//! SRP: byte-layout read-ahead for already queued graph IDs. No traversal,
//! candidate admission, profile qualification, full-file scan or retry.
use super::{
    http_range::{HttpReply, OnlineRangeReader},
    Artifact, Result,
};

// Same measured policy as the Web transport; this is not a graph/search bound.
const MAX_FRONTIER_GAP_BYTES: u64 = 4096;
const MAX_CONNECTED_ID_GAP: u32 = (MAX_FRONTIER_GAP_BYTES / 12 + 1) as u32;

#[cfg(test)]
#[path = "tablebase_http_frontier_tests.rs"]
mod tests;

pub(super) fn prefetch<F: FnMut(&Artifact, u64, u64) -> Result<HttpReply>>(
    reader: &mut OnlineRangeReader<F>,
    field_count: u32,
    graph_bytes: u64,
    offset: u64,
    length: u64,
    frontier: &[u32],
) -> Result<()> {
    if frontier.len() < 2 {
        return Ok(());
    }
    if frontier.len() > 32 || field_count == 0 || field_count > 1 << 24 {
        return Err("pc4_online_frontier_invalid");
    }
    // The caller binds the immutable offsets file. Only its real lookup-pair
    // phase triggers a batch; header/hash reads keep the original exact path.
    if offset < 16 || (offset - 16) % 4 != 0 || length != 8 {
        return Ok(());
    }
    let current = (offset - 16) / 4;
    if !frontier.iter().any(|&id| u64::from(id) == current)
        || frontier.iter().any(|&id| id >= field_count)
    {
        return Err("pc4_online_frontier_invalid");
    }
    let current = current as u32;
    // Minimum qualified record size is 12 bytes. An ID gap above 342 cannot
    // bridge a 4,096-byte transfer gap. Skip unrelated optional hints before
    // copying cached bytes; the required graph lookup remains unchanged.
    if !frontier
        .iter()
        .any(|&id| id != current && id.abs_diff(current) <= MAX_CONNECTED_ID_GAP)
    {
        return Ok(());
    }
    let mut ids = frontier.to_vec();
    ids.sort_unstable();
    ids.dedup();
    let at = ids.binary_search(&current).unwrap();
    let (mut lo, mut hi) = (at, at + 1);
    while lo > 0 && ids[lo] - ids[lo - 1] <= MAX_CONNECTED_ID_GAP {
        lo -= 1;
    }
    while hi < ids.len() && ids[hi] - ids[hi - 1] <= MAX_CONNECTED_ID_GAP {
        hi += 1;
    }
    let ids: Vec<_> = std::iter::once(current)
        .chain(ids[lo..hi].iter().copied().filter(|&id| id != current))
        .collect();
    let required_pair = reader.read(1, offset, length)?;
    let mut records = Vec::new();
    for id in ids {
        // Only the required offset pair may cause I/O. Reading all queued
        // siblings early lost index locality in the real P7P4 A/B.
        let pair = if id == current {
            Some(required_pair.clone())
        } else {
            reader.read_cached(1, 16 + u64::from(id) * 4, 8)?
        };
        let Some(pair) = pair else {
            continue;
        };
        let pair: [u8; 8] = pair.try_into().map_err(|_| "pc4_online_truncated_range")?;
        let start = u64::from(u32::from_le_bytes(pair[..4].try_into().unwrap()));
        let end = u64::from(u32::from_le_bytes(pair[4..].try_into().unwrap()));
        if end <= start
            || end - start > 65_536
            || end > graph_bytes
            || (id == 0 && start != 0)
            || (id == field_count - 1 && end != graph_bytes)
        {
            return Err("pc4_online_record_bounds");
        }
        if reader.read_cached(2, start, end - start)?.is_some() {
            if id == current {
                return Ok(());
            }
            continue;
        }
        records.push((start, end - start));
    }
    let required = records[0]; // the current record is uncached and always first
    records.sort_unstable();
    records.dedup();
    let mut begin = 0;
    while begin < records.len() {
        let start = records[begin].0;
        let mut end = start + records[begin].1;
        let mut stop = begin + 1;
        while let Some(&(offset, length)) = records.get(stop) {
            let merged_end = end.max(offset + length);
            if offset > end + MAX_FRONTIER_GAP_BYTES || merged_end - start > 65_536 {
                break;
            }
            end = merged_end;
            stop += 1;
        }
        let group = &records[begin..stop];
        if group.contains(&required) {
            // Never add a graph HTTP call just for a distant sibling. Only
            // enlarge the required transfer if it covers another known record.
            if group.len() > 1 {
                reader.read_many(2, group, MAX_FRONTIER_GAP_BYTES)?;
            }
            break;
        }
        begin = stop;
    }
    Ok(())
}
