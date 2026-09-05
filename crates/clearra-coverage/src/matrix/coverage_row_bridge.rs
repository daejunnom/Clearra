#[cfg(test)]
use crate::matrix::coverage_row::CoverageRow as UntypedCoverageRow;
use crate::{
    pattern::{
        pattern_bitset::{PatternBitSet, PatternBitSetError},
        pattern_id::PatternId,
    },
    row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind},
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoverageRowBridgeError {
    EmptyPatternUniverse,
    MissingPieceSourceIdentity,
    MissingPatternUniverseIdentity,
    MissingPatternWeightModelIdentity,
    WordCountMismatch {
        expected: usize,
        actual: usize,
    },
    WordCountExceedsInput {
        word_count: usize,
        input_words: usize,
    },
    TailBitsOutsidePatternUniverse,
    CandidateIdOutOfRange {
        candidate_id: u64,
    },
    Pattern(PatternBitSetError),
}

#[cfg(test)]
pub fn coverage_row_from_raw_words_with_identity(
    candidate_id: u64,
    row_kind: CoverageRowKind,
    pattern_universe_id: u64,
    pattern_weight_model_id: u64,
    pattern_count: usize,
    word_count: usize,
    words: &[u64],
) -> Result<CoverageRow, CoverageRowBridgeError> {
    coverage_row_from_raw_words_with_identity_and_piece_source(
        candidate_id,
        row_kind,
        0,
        pattern_universe_id,
        pattern_weight_model_id,
        pattern_count,
        word_count,
        words,
    )
}

// This raw bridge deliberately keeps the ABI-shaped scalar fields explicit so
// callers cannot accidentally reuse a partially populated identity bundle.
#[allow(clippy::too_many_arguments)]
pub fn coverage_row_from_raw_words_with_identity_and_piece_source(
    candidate_id: u64,
    row_kind: CoverageRowKind,
    piece_source_id: u64,
    pattern_universe_id: u64,
    pattern_weight_model_id: u64,
    pattern_count: usize,
    word_count: usize,
    words: &[u64],
) -> Result<CoverageRow, CoverageRowBridgeError> {
    if piece_source_id == 0 {
        return Err(CoverageRowBridgeError::MissingPieceSourceIdentity);
    }
    if pattern_universe_id == 0 {
        return Err(CoverageRowBridgeError::MissingPatternUniverseIdentity);
    }
    if pattern_weight_model_id == 0 {
        return Err(CoverageRowBridgeError::MissingPatternWeightModelIdentity);
    }

    let bitset = pattern_bitset_from_raw_words(pattern_count, word_count, words)?;
    Ok(CoverageRow::new_with_piece_source(
        candidate_id,
        row_kind,
        piece_source_id,
        PatternUniverseId::new(pattern_universe_id),
        PatternWeightModelId::new(pattern_weight_model_id),
        bitset,
    ))
}

#[cfg(test)]
pub fn coverage_row_from_raw_words(
    candidate_id: u64,
    pattern_count: usize,
    word_count: usize,
    words: &[u64],
) -> Result<UntypedCoverageRow, CoverageRowBridgeError> {
    let bitset = pattern_bitset_from_raw_words(pattern_count, word_count, words)?;
    let candidate_id = usize::try_from(candidate_id)
        .map_err(|_| CoverageRowBridgeError::CandidateIdOutOfRange { candidate_id })?;
    Ok(UntypedCoverageRow::new(candidate_id, bitset))
}

fn pattern_bitset_from_raw_words(
    pattern_count: usize,
    word_count: usize,
    words: &[u64],
) -> Result<PatternBitSet, CoverageRowBridgeError> {
    if pattern_count == 0 {
        return Err(CoverageRowBridgeError::EmptyPatternUniverse);
    }

    let expected_word_count = pattern_count.div_ceil(64);
    if word_count != expected_word_count {
        return Err(CoverageRowBridgeError::WordCountMismatch {
            expected: expected_word_count,
            actual: word_count,
        });
    }
    if word_count > words.len() {
        return Err(CoverageRowBridgeError::WordCountExceedsInput {
            word_count,
            input_words: words.len(),
        });
    }
    if tail_bits_set(words, pattern_count, word_count) {
        return Err(CoverageRowBridgeError::TailBitsOutsidePatternUniverse);
    }

    let mut bitset = PatternBitSet::new(pattern_count);
    for (word_index, word) in words[..word_count].iter().copied().enumerate() {
        for bit_index in 0..64 {
            if (word & (1_u64 << bit_index)) == 0 {
                continue;
            }
            let pattern_index = word_index * 64 + bit_index;
            bitset
                .insert(PatternId::new(pattern_index))
                .map_err(CoverageRowBridgeError::Pattern)?;
        }
    }

    Ok(bitset)
}

fn tail_bits_set(words: &[u64], pattern_count: usize, word_count: usize) -> bool {
    if word_count == 0 {
        return false;
    }
    let used_bits_in_tail = pattern_count % 64;
    if used_bits_in_tail == 0 {
        return false;
    }
    let allowed_mask = (1_u64 << used_bits_in_tail) - 1;
    (words[word_count - 1] & !allowed_mask) != 0
}

#[cfg(test)]
#[path = "coverage_row_bridge_tests.rs"]
mod tests;
