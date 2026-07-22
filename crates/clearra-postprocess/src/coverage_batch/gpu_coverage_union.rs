use std::fmt;

use clearra_coverage::pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId};
use clearra_postprocess_gpu::{
    BackendFallbackPolicy, PostGpuResult, PostProcessGpuBackend, SearchBackendRequest,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostProcessCoverageUnion {
    coverage: Option<PatternBitSet>,
    backend: PostGpuResult,
}

impl PostProcessCoverageUnion {
    pub async fn run(
        rows: &[PatternBitSet],
        search_backend: SearchBackendRequest,
        fallback_policy: BackendFallbackPolicy,
    ) -> Result<Self, PostProcessCoverageUnionError> {
        let first = rows
            .first()
            .ok_or(PostProcessCoverageUnionError::EmptyRows)?;
        if let Some((row_index, actual)) = rows.iter().enumerate().find_map(|(index, row)| {
            (row.pattern_count() != first.pattern_count()).then_some((index, row.pattern_count()))
        }) {
            return Err(PostProcessCoverageUnionError::PatternUniverseMismatch {
                row_index,
                expected: first.pattern_count(),
                actual,
            });
        }

        let row_words = rows
            .iter()
            .map(|row| row.words().to_vec())
            .collect::<Vec<_>>();
        let backend = PostProcessGpuBackend::union_pattern_words(search_backend, &row_words)
            .await
            .map_err(|error| PostProcessCoverageUnionError::Backend(error.to_string()))?;

        if let Some(words) = backend.union_words() {
            return Ok(Self {
                coverage: Some(bitset_from_words(first.pattern_count(), words)?),
                backend,
            });
        }

        if matches!(backend, PostGpuResult::Unavailable { .. })
            && fallback_policy == BackendFallbackPolicy::AllowWithDiagnostic
        {
            let coverage = cpu_union(rows)?;
            return Ok(Self {
                coverage: Some(coverage),
                backend: backend.with_cpu_fallback(),
            });
        }

        Ok(Self {
            coverage: None,
            backend,
        })
    }

    pub fn coverage(&self) -> Option<&PatternBitSet> {
        self.coverage.as_ref()
    }

    pub fn backend(&self) -> &PostGpuResult {
        &self.backend
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PostProcessCoverageUnionError {
    EmptyRows,
    PatternUniverseMismatch {
        row_index: usize,
        expected: usize,
        actual: usize,
    },
    Backend(String),
    InvalidGpuWord,
}

impl fmt::Display for PostProcessCoverageUnionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for PostProcessCoverageUnionError {}

fn cpu_union(rows: &[PatternBitSet]) -> Result<PatternBitSet, PostProcessCoverageUnionError> {
    let mut union = rows[0].clone();
    for row in &rows[1..] {
        union.union_with(row).map_err(|_| {
            PostProcessCoverageUnionError::PatternUniverseMismatch {
                row_index: 0,
                expected: union.pattern_count(),
                actual: row.pattern_count(),
            }
        })?;
    }
    Ok(union)
}

fn bitset_from_words(
    pattern_count: usize,
    words: &[u64],
) -> Result<PatternBitSet, PostProcessCoverageUnionError> {
    if words.len() != pattern_count.div_ceil(64) {
        return Err(PostProcessCoverageUnionError::InvalidGpuWord);
    }
    let mut bitset = PatternBitSet::new(pattern_count);
    for pattern_index in 0..pattern_count {
        if words[pattern_index / 64] & (1_u64 << (pattern_index % 64)) != 0 {
            bitset
                .insert(PatternId::new(pattern_index))
                .map_err(|_| PostProcessCoverageUnionError::InvalidGpuWord)?;
        }
    }
    Ok(bitset)
}
