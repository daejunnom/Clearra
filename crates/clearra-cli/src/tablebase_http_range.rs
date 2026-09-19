//! SRP: bounded immutable HTTP windows. No file persistence, offline fallback,
//! graph interpretation or user-visible result reduction.
use super::{Artifact, Result};
use std::collections::VecDeque;

#[cfg(test)]
#[path = "tablebase_http_range_tests.rs"]
mod tests;

pub(super) struct HttpReply {
    pub status: u16,
    pub content_range: String,
    pub bytes: Vec<u8>,
}
impl HttpReply {
    pub fn from_curl_output(mut bytes: Vec<u8>) -> Result<Self> {
        let marker = b"\nCLEARRA-PC4-HTTP\n";
        let at = bytes
            .windows(marker.len())
            .rposition(|v| v == marker)
            .ok_or("pc4_online_response_receipt_missing")?;
        let receipt = std::str::from_utf8(&bytes[at + marker.len()..])
            .map_err(|_| "pc4_online_response_receipt_invalid")?;
        let mut lines = receipt.split('\n');
        let status = lines
            .next()
            .and_then(|s| s.parse().ok())
            .ok_or("pc4_online_response_receipt_invalid")?;
        let content_range = lines
            .next()
            .ok_or("pc4_online_response_receipt_invalid")?
            .to_owned();
        if lines.next() != Some("") || lines.next().is_some() {
            return Err("pc4_online_response_receipt_invalid");
        }
        bytes.truncate(at);
        Ok(Self {
            status,
            content_range,
            bytes,
        })
    }
    pub(super) fn validate(self, artifact: &Artifact, offset: u64, length: u64) -> Result<Vec<u8>> {
        match self.status {
            206 => {}
            200 => return Err("pc4_online_whole_content_rejected"),
            429 => return Err("pc4_online_rate_limited"),
            416 => return Err("pc4_online_range_unsatisfiable"),
            _ => return Err("pc4_online_range_response_invalid"),
        }
        if self.content_range != content_range(offset, length, artifact.size) {
            return Err("pc4_online_range_response_invalid");
        }
        if self.bytes.len() as u64 != length {
            return Err("pc4_online_truncated_range");
        }
        Ok(self.bytes)
    }
}

pub(super) fn content_range(offset: u64, length: u64, total: u64) -> String {
    format!("bytes {}-{}/{}", offset, offset + length - 1, total)
}

struct Window {
    role: usize,
    offset: u64,
    bytes: Vec<u8>,
}
pub(super) struct OnlineRangeReader<F> {
    fetch: F,
    files: Vec<Artifact>,
    windows: VecDeque<Window>,
    retained: usize,
    reserved: u64,
    requests: u64,
}
impl<F: FnMut(&Artifact, u64, u64) -> Result<HttpReply>> OnlineRangeReader<F> {
    pub fn new(files: Vec<Artifact>, fetch: F) -> Self {
        Self {
            files,
            fetch,
            windows: VecDeque::new(),
            retained: 0,
            reserved: 0,
            requests: 0,
        }
    }
    pub fn read(&mut self, role: usize, offset: u64, length: u64) -> Result<Vec<u8>> {
        self.validate_span(role, offset, length)?;
        if let Some(bytes) = self.cached(role, offset, length) {
            // The real graph demand is about to enter App's decoded cache.
            // Release only an exact one-record prefetch; keep unread siblings
            // of a larger coalesced span until normal bounded eviction.
            if role == 2
                && self.windows.back().is_some_and(|w| {
                    w.role == role && w.offset == offset && w.bytes.len() as u64 == length
                })
            {
                self.retained -= self.windows.pop_back().unwrap().bytes.len();
            }
            return Ok(bytes);
        }
        let artifact = &self.files[role];
        let end = offset + length;
        // Reuse FHID/GOFF index pages; exact graph records are already retained
        // decoded by App. Uniform 16 KiB graph windows exhaust the 64 MiB
        // transport budget after only a small prefix of the P7P4 reference.
        // FILES pins the role order (fields, offsets, graph) for this reader.
        let direct = role == 2 || artifact.size <= 4_096;
        // A single bounded window replaces repeated tiny FHID/GOFF processes.
        // A crossing large demand stays one request; no read exceeds 64 KiB.
        let mut start = if direct {
            offset
        } else {
            offset / 4_096 * 4_096
        };
        let mut stop = if direct {
            end
        } else {
            (start + 4_096).min(artifact.size).max(end)
        };
        if stop - start > 65_536 {
            start = offset;
            stop = end;
        }
        let bytes = self.exact_span(role, start, stop - start, role != 2)?;
        let within = (offset - start) as usize;
        Ok(bytes[within..within + length as usize].to_vec())
    }

    pub fn read_cached(
        &mut self,
        role: usize,
        offset: u64,
        length: u64,
    ) -> Result<Option<Vec<u8>>> {
        self.validate_span(role, offset, length)?;
        Ok(self.cached(role, offset, length))
    }

    /// Reserve a finite native transport batch before starting any of its
    /// physical requests. The external owner is allowed to overlap I/O, but it
    /// must spend the same request/byte budget as scalar `exact_span` reads.
    /// Validation is all-or-nothing so one malformed sibling starts no I/O.
    pub fn reserve_external(&mut self, spans: &[(usize, u64, u64)]) -> Result<()> {
        if spans.is_empty() || spans.len() > 16 {
            return Err("pc4_online_batch_invalid");
        }
        let mut bytes = 0_u64;
        for &(role, offset, length) in spans {
            self.validate_span(role, offset, length)?;
            bytes = bytes
                .checked_add(length)
                .ok_or("pc4_online_transfer_limit")?;
        }
        let requests = u64::try_from(spans.len()).map_err(|_| "pc4_online_batch_invalid")?;
        if self
            .requests
            .checked_add(requests)
            .is_none_or(|v| v > 100_000)
            || self
                .reserved
                .checked_add(bytes)
                .is_none_or(|v| v > 64 * 1024 * 1024)
        {
            return Err("pc4_online_transfer_limit");
        }
        self.requests += requests;
        self.reserved += bytes;
        Ok(())
    }

    /// Only explicit, already known byte intervals may be merged. Cached
    /// demands are removed before planning so overlapping frontiers do not
    /// repeatedly transfer their already retained records.
    pub fn read_many(
        &mut self,
        role: usize,
        demands: &[(u64, u64)],
        max_gap: u64,
    ) -> Result<Vec<Vec<u8>>> {
        if demands.len() > 512 || max_gap > 4096 {
            return Err("pc4_online_batch_invalid");
        }
        for &(offset, length) in demands {
            self.validate_span(role, offset, length)?;
        }
        let mut result = vec![Vec::new(); demands.len()];
        let mut missing = Vec::new();
        for (index, &(offset, length)) in demands.iter().enumerate() {
            if let Some(bytes) = self.cached(role, offset, length) {
                result[index] = bytes;
            } else {
                missing.push((offset, length, index));
            }
        }
        missing.sort_unstable();
        let mut cursor = 0;
        while cursor < missing.len() {
            let start = missing[cursor].0;
            let mut end = start + missing[cursor].1;
            let mut stop = cursor + 1;
            while let Some(&(offset, length, _)) = missing.get(stop) {
                let merged_end = end.max(offset + length);
                if offset > end.saturating_add(max_gap) || merged_end - start > 65_536 {
                    break;
                }
                end = merged_end;
                stop += 1;
            }
            // Explicit graph batches use the same bounded cache as index
            // pages. They confer no graph/solution admission authority.
            let bytes = self.exact_span(role, start, end - start, true)?;
            for &(offset, length, index) in &missing[cursor..stop] {
                let within = (offset - start) as usize;
                result[index] = bytes[within..within + length as usize].to_vec();
            }
            cursor = stop;
        }
        Ok(result)
    }

    fn validate_span(&self, role: usize, offset: u64, length: u64) -> Result<()> {
        let artifact = self.files.get(role).ok_or("pc4_online_artifact_invalid")?;
        if length == 0
            || length > 65_536
            || offset
                .checked_add(length)
                .is_none_or(|end| end > artifact.size)
        {
            return Err("pc4_online_range_request_invalid");
        }
        Ok(())
    }

    fn cached(&mut self, role: usize, offset: u64, length: u64) -> Option<Vec<u8>> {
        let at = self.windows.iter().position(|w| {
            w.role == role
                && w.offset <= offset
                && w.offset + w.bytes.len() as u64 >= offset + length
        })?;
        let window = self.windows.remove(at)?;
        let start = (offset - window.offset) as usize;
        let bytes = window.bytes[start..start + length as usize].to_vec();
        self.windows.push_back(window);
        Some(bytes)
    }

    fn exact_span(
        &mut self,
        role: usize,
        start: u64,
        amount: u64,
        retain: bool,
    ) -> Result<Vec<u8>> {
        self.validate_span(role, start, amount)?;
        if let Some(bytes) = self.cached(role, start, amount) {
            return Ok(bytes);
        }
        if self.requests >= 100_000 || self.reserved + amount > 64 * 1024 * 1024 {
            return Err("pc4_online_transfer_limit");
        }
        self.requests += 1;
        self.reserved += amount;
        let artifact = &self.files[role];
        let bytes = (self.fetch)(artifact, start, amount)?.validate(artifact, start, amount)?;
        if !retain {
            return Ok(bytes);
        }
        while self.retained + bytes.len() > 8 * 1024 * 1024 || self.windows.len() >= 2_048 {
            self.retained -= self.windows.pop_front().unwrap().bytes.len();
        }
        let result = bytes.clone();
        self.retained += bytes.len();
        self.windows.push_back(Window {
            role,
            offset: start,
            bytes,
        });
        Ok(result)
    }
}
