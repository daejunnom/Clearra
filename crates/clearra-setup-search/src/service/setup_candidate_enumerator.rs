use std::collections::BTreeSet;

use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::query::SetupSearchQuery;

use super::{
    setup_pattern_source::{SetupPatternSource, SetupSourcePattern},
    setup_shape_packer::pack_piece_sequence,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SetupBuildCandidate {
    pub(crate) pattern_index: usize,
    pub(crate) occupied_shape: u64,
    pub(crate) placed_pieces: Vec<PieceKind>,
    pub(crate) final_hold: Option<PieceKind>,
    pub(crate) remaining_queue: Vec<PieceKind>,
}

pub(crate) fn enumerate_build_candidates(
    query: &SetupSearchQuery,
    source: &SetupPatternSource,
) -> Vec<SetupBuildCandidate> {
    let mut candidates = Vec::new();
    for pattern in &source.patterns {
        push_normal_prefix_candidates(query, pattern, &mut candidates);
        push_hold_first_candidates(query, pattern, &mut candidates);
        push_initial_hold_candidates(query, pattern, &mut candidates);
    }
    dedupe_candidates(candidates)
}

fn push_normal_prefix_candidates(
    query: &SetupSearchQuery,
    pattern: &SetupSourcePattern,
    candidates: &mut Vec<SetupBuildCandidate>,
) {
    let max_len = query
        .piece_budget()
        .max_piece_count()
        .min(pattern.queue_pieces.len() as u8) as usize;
    for len in 1..=max_len {
        let placed = pattern.queue_pieces[..len].to_vec();
        if let Some(occupied_shape) = pack_piece_sequence(&placed) {
            candidates.push(SetupBuildCandidate {
                pattern_index: pattern.pattern_index,
                occupied_shape,
                placed_pieces: placed,
                final_hold: query.hold_policy().initial_piece(),
                remaining_queue: pattern.queue_pieces[len..].to_vec(),
            });
        }
    }
}

fn push_hold_first_candidates(
    query: &SetupSearchQuery,
    pattern: &SetupSourcePattern,
    candidates: &mut Vec<SetupBuildCandidate>,
) {
    if !query.hold_policy().is_enabled() || pattern.queue_pieces.len() < 2 {
        return;
    }

    let max_len = query
        .piece_budget()
        .max_piece_count()
        .min((pattern.queue_pieces.len() - 1) as u8) as usize;
    for len in 1..=max_len {
        let placed = pattern.queue_pieces[1..=len].to_vec();
        if let Some(occupied_shape) = pack_piece_sequence(&placed) {
            candidates.push(SetupBuildCandidate {
                pattern_index: pattern.pattern_index,
                occupied_shape,
                placed_pieces: placed,
                final_hold: Some(pattern.queue_pieces[0]),
                remaining_queue: pattern.queue_pieces[len + 1..].to_vec(),
            });
        }
    }
}

fn push_initial_hold_candidates(
    query: &SetupSearchQuery,
    pattern: &SetupSourcePattern,
    candidates: &mut Vec<SetupBuildCandidate>,
) {
    let Some(held_piece) = query.hold_policy().initial_piece() else {
        return;
    };

    let max_len = query
        .piece_budget()
        .max_piece_count()
        .min((pattern.queue_pieces.len() + 1) as u8) as usize;
    for len in 1..=max_len {
        let queue_consumed = len.saturating_sub(1);
        let mut placed = Vec::with_capacity(len);
        placed.push(held_piece);
        placed.extend(pattern.queue_pieces.iter().take(queue_consumed).copied());
        if let Some(occupied_shape) = pack_piece_sequence(&placed) {
            candidates.push(SetupBuildCandidate {
                pattern_index: pattern.pattern_index,
                occupied_shape,
                placed_pieces: placed,
                final_hold: None,
                remaining_queue: pattern.queue_pieces[queue_consumed..].to_vec(),
            });
        }
    }
}

fn dedupe_candidates(candidates: Vec<SetupBuildCandidate>) -> Vec<SetupBuildCandidate> {
    let mut seen = BTreeSet::new();
    candidates
        .into_iter()
        .filter(|candidate| {
            seen.insert((
                candidate.pattern_index,
                candidate.occupied_shape,
                candidate.placed_pieces.clone(),
                candidate.final_hold,
                candidate.remaining_queue.clone(),
            ))
        })
        .collect()
}
