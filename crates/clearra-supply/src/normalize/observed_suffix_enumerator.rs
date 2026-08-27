use std::collections::HashMap;

use clearra_core_domain::{
    piece::piece_kind::PieceKind,
    probability::probability_value::{ProbabilityValue, ProbabilityValueError},
};

use crate::{
    bag::{bag_boundary::BagBoundaryCandidate, bag_profile::BagProfile},
    diagnostics::duplicate_witness::duplicate_for_boundary_offset_with_profile,
    queue::queue_pattern::QueuePattern,
};

pub(super) fn enumerate_visible_suffixes_with_profile(
    prefix: &mut Vec<PieceKind>,
    target_len: usize,
    boundary_candidate: BagBoundaryCandidate,
    bag_profile: &BagProfile,
    max_patterns: usize,
    patterns: &mut Vec<(BagBoundaryCandidate, QueuePattern)>,
    truncated: &mut bool,
) {
    if patterns.len() >= max_patterns {
        *truncated = true;
        return;
    }

    if prefix.len() == target_len {
        patterns.push((boundary_candidate, QueuePattern::new(prefix.clone())));
        return;
    }

    for piece in bag_profile.pieces() {
        prefix.push(piece);
        if duplicate_for_boundary_offset_with_profile(
            prefix,
            bag_profile,
            boundary_candidate.initial_offset(),
        )
        .is_none()
        {
            enumerate_visible_suffixes_with_profile(
                prefix,
                target_len,
                boundary_candidate,
                bag_profile,
                max_patterns,
                patterns,
                truncated,
            );
        }
        prefix.pop();

        if *truncated {
            return;
        }
    }
}

pub(super) fn total_visible_suffix_count_with_profile(
    prefix: &[PieceKind],
    target_len: usize,
    boundary_candidates: &[BagBoundaryCandidate],
    bag_profile: &BagProfile,
) -> Option<u128> {
    let mut total = 0_u128;
    for candidate in boundary_candidates {
        let count =
            count_visible_suffixes_for_candidate(prefix, target_len, *candidate, bag_profile)?;
        total = total.saturating_add(count);
    }
    (total > 0).then_some(total)
}

pub(super) fn materialized_probabilities(
    materialized_count: usize,
    total_pattern_count: u128,
) -> Result<Vec<ProbabilityValue>, ProbabilityValueError> {
    let uniform = 1.0 / total_pattern_count as f64;
    let complete = materialized_count as u128 == total_pattern_count;
    let mut probabilities = Vec::with_capacity(materialized_count);

    for index in 0..materialized_count {
        let value = if complete && index + 1 == materialized_count {
            1.0 - uniform * materialized_count.saturating_sub(1) as f64
        } else {
            uniform
        };
        let probability = ProbabilityValue::new(value)?;
        probabilities.push(probability);
    }

    Ok(probabilities)
}

fn count_visible_suffixes_for_candidate(
    prefix: &[PieceKind],
    target_len: usize,
    boundary_candidate: BagBoundaryCandidate,
    bag_profile: &BagProfile,
) -> Option<u128> {
    let (offset, used_counts) =
        boundary_state_after_prefix(prefix, boundary_candidate, bag_profile)?;
    let remaining = target_len.saturating_sub(prefix.len());
    let mut memo = HashMap::new();
    Some(count_suffixes(
        remaining,
        offset,
        used_counts,
        bag_profile,
        &mut memo,
    ))
}

fn boundary_state_after_prefix_with_profile(
    pieces: &[PieceKind],
    boundary_candidate: BagBoundaryCandidate,
    bag_profile: &BagProfile,
) -> Option<(usize, Vec<usize>)> {
    let mut offset = boundary_candidate.initial_offset();
    let mut used_counts = vec![0; bag_profile.entries().len()];
    let bag_size = bag_profile.bag_size();

    for piece in pieces.iter().copied() {
        if offset == bag_size {
            offset = 0;
            used_counts.fill(0);
        }
        let entry_index = bag_profile.entry_index(piece)?;
        used_counts[entry_index] += 1;
        if used_counts[entry_index] > bag_profile.entries()[entry_index].multiplicity() {
            return None;
        }
        offset += 1;
    }

    Some((offset, used_counts))
}

fn boundary_state_after_prefix(
    pieces: &[PieceKind],
    boundary_candidate: BagBoundaryCandidate,
    bag_profile: &BagProfile,
) -> Option<(usize, Vec<usize>)> {
    boundary_state_after_prefix_with_profile(pieces, boundary_candidate, bag_profile)
}

fn count_suffixes(
    remaining: usize,
    offset: usize,
    used_counts: Vec<usize>,
    bag_profile: &BagProfile,
    memo: &mut HashMap<(usize, usize, Vec<usize>), u128>,
) -> u128 {
    if remaining == 0 {
        return 1;
    }

    let (offset, used_counts) = normalized_bag_state(offset, used_counts, bag_profile);
    let key = (remaining, offset, used_counts.clone());
    if let Some(count) = memo.get(&key) {
        return *count;
    }

    let mut count = 0_u128;
    for (entry_index, entry) in bag_profile.entries().iter().enumerate() {
        if used_counts[entry_index] < entry.multiplicity() {
            let mut next_counts = used_counts.clone();
            next_counts[entry_index] += 1;
            count = count.saturating_add(count_suffixes(
                remaining - 1,
                offset + 1,
                next_counts,
                bag_profile,
                memo,
            ));
        }
    }

    memo.insert(key, count);
    count
}

fn normalized_bag_state(
    offset: usize,
    mut used_counts: Vec<usize>,
    bag_profile: &BagProfile,
) -> (usize, Vec<usize>) {
    if offset == bag_profile.bag_size() {
        used_counts.fill(0);
        (0, used_counts)
    } else {
        (offset, used_counts)
    }
}
