//! SRP: byte-layout read-ahead for already queued graph IDs. No traversal,
//! candidate admission, profile qualification, full-file scan or retry.
use super::{
    http_range::{HttpReply, OnlineRangeReader},
    Artifact, Result,
};

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
    let mut ids = Vec::new();
    for &id in frontier {
        if !ids.contains(&id) {
            ids.push(id);
        }
    }
    let offsets: Vec<_> = ids.iter().map(|&id| (16 + u64::from(id) * 4, 8)).collect();
    let pairs = reader.read_many(1, &offsets)?;
    let mut records = Vec::new();
    for (&id, pair) in ids.iter().zip(pairs) {
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
        records.push((start, end - start));
    }
    reader.read_many(2, &records)?;
    Ok(())
}
