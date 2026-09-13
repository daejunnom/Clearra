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
    fn validate(self, artifact: &Artifact, offset: u64, length: u64) -> Result<Vec<u8>> {
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
        let artifact = self.files.get(role).ok_or("pc4_online_artifact_invalid")?;
        let end = offset
            .checked_add(length)
            .ok_or("pc4_online_range_request_invalid")?;
        if length == 0 || length > 65_536 || end > artifact.size {
            return Err("pc4_online_range_request_invalid");
        }
        if let Some(at) = self.windows.iter().position(|w| {
            w.role == role && w.offset <= offset && w.offset + w.bytes.len() as u64 >= end
        }) {
            let window = self.windows.remove(at).unwrap();
            let start = (offset - window.offset) as usize;
            let bytes = window.bytes[start..start + length as usize].to_vec();
            self.windows.push_back(window);
            return Ok(bytes);
        }
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
        let amount = stop - start;
        if self.requests >= 100_000 || self.reserved + amount > 64 * 1024 * 1024 {
            return Err("pc4_online_transfer_limit");
        }
        self.requests += 1;
        self.reserved += amount;
        let bytes = (self.fetch)(artifact, start, amount)?.validate(artifact, start, amount)?;
        if direct {
            return Ok(bytes);
        }
        while self.retained + bytes.len() > 8 * 1024 * 1024 || self.windows.len() >= 2_048 {
            self.retained -= self.windows.pop_front().unwrap().bytes.len();
        }
        let within = (offset - start) as usize;
        let result = bytes[within..within + length as usize].to_vec();
        self.retained += bytes.len();
        self.windows.push_back(Window {
            role,
            offset: start,
            bytes,
        });
        Ok(result)
    }
}
