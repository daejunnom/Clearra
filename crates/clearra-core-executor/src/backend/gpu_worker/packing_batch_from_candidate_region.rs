use clearra_core_ffi::{
    gpu::CGpuPieceMultisetWindow,
    problem::{
        C_GPU_PIECE_SOURCE_BAG_ALIGNED_PATTERN, C_GPU_PIECE_SOURCE_FIXED_SEQUENCE,
        C_GPU_PIECE_SOURCE_OBSERVED_WINDOW,
    },
    supply::{
        C_PIECE_SOURCE_BAG_UNIVERSE, C_PIECE_SOURCE_FIXED_QUEUE, C_PIECE_SOURCE_OBSERVED_WINDOW,
    },
    CPackingProblem,
};

use super::{
    PackingBatchId, PackingBatchSource, PackingBatchSourceError, PackingBatchValidationError,
};

pub fn packing_batch_source_from_candidate_region(
    compact: &CPackingProblem,
    batch_id: Option<PackingBatchId>,
    pattern_universe_id: u64,
    pattern_weight_model_id: u64,
    candidate_capacity_override: Option<u32>,
) -> Result<PackingBatchSource, PackingBatchSourceError> {
    let piece_window = u8::try_from(compact.piece_window.max_pieces).map_err(|_| {
        PackingBatchValidationError::PieceCountExceedsPieceWindow {
            piece_count: u8::MAX,
            piece_window: u8::MAX,
        }
    })?;
    let piece_count = if compact.piece_window.has_exact_pieces != 0 {
        compact.piece_window.exact_pieces
    } else {
        compact.piece_window.max_pieces
    };
    let piece_count = u8::try_from(piece_count).map_err(|_| {
        PackingBatchValidationError::PieceCountExceedsPieceWindow {
            piece_count: u8::MAX,
            piece_window,
        }
    })?;
    let exact_piece_count = if compact.piece_window.has_exact_pieces != 0 {
        u8::try_from(compact.piece_window.exact_pieces).map_err(|_| {
            PackingBatchValidationError::ExactPieceCountExceedsPieceWindow {
                exact_piece_count: u8::MAX,
                piece_window,
            }
        })?
    } else {
        0
    };
    if compact.piece_multiset_window.total_count != piece_count {
        return Err(PackingBatchValidationError::MissingPieceMultisetWindow {
            piece_count,
            stored_len: u16::from(compact.piece_multiset_window.total_count),
        }
        .into());
    }
    let piece_source_kind = gpu_piece_source_kind(compact.piece_source.source_kind)?;
    let piece_multiset_window = gpu_piece_multiset_window(compact);

    let board_width = u8::try_from(compact.board.width).map_err(|_| {
        PackingBatchValidationError::BoardExceedsBoard64Limit {
            cell_count: u16::MAX,
        }
    })?;
    let board_height = u8::try_from(compact.board.visible_height).map_err(|_| {
        PackingBatchValidationError::BoardExceedsBoard64Limit {
            cell_count: u16::MAX,
        }
    })?;
    let active_packing_rows = u8::try_from(compact.board.visible_height).map_err(|_| {
        PackingBatchValidationError::BoardExceedsBoard64Limit {
            cell_count: u16::MAX,
        }
    })?;

    Ok(PackingBatchSource {
        batch_id: batch_id
            .unwrap_or_else(|| PackingBatchId::new(stable_nonzero_batch_hash(compact))),
        board_width,
        board_height,
        active_packing_rows,
        goal_clear_lines_hint: None,
        initial_board_mask: compact.board.initial_mask,
        piece_window,
        piece_count,
        exact_piece_count,
        piece_source_kind,
        piece_source_id: compact.piece_source.piece_source_id,
        piece_multiset_window,
        operation_table_id: stable_nonzero_hash(&format!(
            "operation-table:kind={};pieces={};rule={};kick={}",
            compact.problem_kind,
            compact.piece_window.max_pieces,
            compact.rule.rule_profile_id,
            compact.rule.kick_profile_id
        )),
        rule_profile_id: u64::from(compact.rule.rule_profile_id),
        kick_profile_id: u64::from(compact.rule.kick_profile_id),
        candidate_capacity: candidate_capacity_override.unwrap_or(compact.budget.max_results),
        max_frontier_states: if compact.budget.max_frontier_states == 0 {
            2_048
        } else {
            compact.budget.max_frontier_states
        },
        pattern_count: compact.piece_source.materialized_pattern_count.max(1),
        shape_hash_seed: stable_nonzero_hash(&format!(
            "shape-hash:mask={:016x};width={};height={};pieces={}",
            compact.board.initial_mask,
            compact.board.width,
            compact.board.visible_height,
            compact.piece_window.max_pieces
        )),
        pattern_universe_id,
        pattern_weight_model_id,
    })
}

fn gpu_piece_source_kind(source_kind: u32) -> Result<u8, PackingBatchValidationError> {
    match source_kind {
        C_PIECE_SOURCE_FIXED_QUEUE => Ok(C_GPU_PIECE_SOURCE_FIXED_SEQUENCE),
        C_PIECE_SOURCE_BAG_UNIVERSE => Ok(C_GPU_PIECE_SOURCE_BAG_ALIGNED_PATTERN),
        C_PIECE_SOURCE_OBSERVED_WINDOW => Ok(C_GPU_PIECE_SOURCE_OBSERVED_WINDOW),
        other => Err(PackingBatchValidationError::UnknownPieceSourceKind {
            piece_source_kind: other.try_into().unwrap_or(u8::MAX),
        }),
    }
}

fn gpu_piece_multiset_window(compact: &CPackingProblem) -> CGpuPieceMultisetWindow {
    CGpuPieceMultisetWindow {
        counts: compact.piece_multiset_window.counts,
        total_count: compact.piece_multiset_window.total_count,
        exact_count: compact.piece_multiset_window.exact_count,
        reserved: compact.piece_multiset_window.reserved,
    }
}

fn stable_nonzero_batch_hash(compact: &CPackingProblem) -> u64 {
    stable_nonzero_hash(&format!(
        "packing-batch:kind={};width={};height={};mask={:016x};pieces={};source-kind={};source-id={};provenance={};pattern-universe={};weight-model={};pattern-count={};multiset={:?};rule={};kick={};capacity={};frontier={}",
        compact.problem_kind,
        compact.board.width,
        compact.board.visible_height,
        compact.board.initial_mask,
        compact.piece_window.max_pieces,
        compact.piece_source.source_kind,
        compact.piece_source.piece_source_id,
        compact.piece_source.provenance_id,
        compact.piece_source.pattern_universe_id,
        compact.piece_source.pattern_weight_model_id,
        compact.piece_source.materialized_pattern_count.max(1),
        compact.piece_multiset_window.counts,
        compact.rule.rule_profile_id,
        compact.rule.kick_profile_id,
        compact.budget.max_results,
        compact.budget.max_frontier_states
    ))
}

fn stable_nonzero_hash(material: &str) -> u64 {
    const FNV_OFFSET: u64 = 14_695_981_039_346_656_037;
    const FNV_PRIME: u64 = 1_099_511_628_211;

    let mut hash = FNV_OFFSET;
    for byte in material.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    if hash == 0 {
        1
    } else {
        hash
    }
}
