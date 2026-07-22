use std::fmt;

use clearra_webgpu::{WebGpuBackend, WebGpuBatchOutcome, WebGpuBitsetBatch};

use crate::{PostGpuResult, SearchBackendRequest};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PostProcessGpuBackend;

impl PostProcessGpuBackend {
    pub async fn union_pattern_words(
        search_backend_selected: SearchBackendRequest,
        rows: &[Vec<u64>],
    ) -> Result<PostGpuResult, PostProcessGpuError> {
        let gpu_rows = rows
            .iter()
            .map(|row| {
                row.iter()
                    .flat_map(|word| [*word as u32, (*word >> 32) as u32])
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let batch =
            WebGpuBitsetBatch::new(&gpu_rows).map_err(PostProcessGpuError::invalid_batch)?;

        match WebGpuBackend::run_bitset_union(&batch)
            .await
            .map_err(PostProcessGpuError::invalid_batch)?
        {
            WebGpuBatchOutcome::Connected(result) => {
                let words = result
                    .union_words()
                    .chunks_exact(2)
                    .map(|parts| u64::from(parts[0]) | (u64::from(parts[1]) << 32))
                    .collect();
                Ok(PostGpuResult::connected(
                    search_backend_selected,
                    words,
                    result.shader_hash().to_owned(),
                ))
            }
            WebGpuBatchOutcome::Unavailable(result) => Ok(PostGpuResult::unavailable(
                search_backend_selected,
                result.reason(),
            )),
            WebGpuBatchOutcome::RejectedMismatch(result) => Ok(PostGpuResult::rejected_mismatch(
                search_backend_selected,
                result.expected_digest(),
                result.actual_digest(),
            )),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostProcessGpuError {
    message: String,
}

impl PostProcessGpuError {
    fn invalid_batch(error: impl fmt::Display) -> Self {
        Self {
            message: format!("invalid postprocess GPU batch: {error}"),
        }
    }
}

impl fmt::Display for PostProcessGpuError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PostProcessGpuError {}
