//! Bounded Boolean-language dominance within one canonical candidate.
//!
//! A pending edge may be omitted when its optimistic suffix language is
//! covered by already proved same-kind alternatives from the same prefix.
//! This preserves full emitted kind words, not execution multiplicity,
//! poses, scoring, or replay. In particular, pending edges cannot certify
//! one another, and different canonical candidates never share this proof.

use clearra_core_domain::piece::piece_kind::PieceKind;

use super::symbolic_prefix_domain::{MAX_OPERATIONS, MAX_PREFIX_WORDS};

const LANES: usize = u64::BITS as usize;
const MAX_SUFFIX_DEPTH: u32 = 3;
const MAX_PROOFS_PER_CANDIDATE: usize = 24;
type OperationParents = [[u64; MAX_PREFIX_WORDS]; MAX_OPERATIONS];

// Visit only prefixes that leave a same-kind sibling possible and put the
// suffix in our exact three-letter encoder's supported depth.
const NEAR_TERMINAL: [[u64; MAX_PREFIX_WORDS]; MAX_OPERATIONS + 1] = {
    let mut result = [[0_u64; MAX_PREFIX_WORDS]; MAX_OPERATIONS + 1];
    let mut operations = 2;
    while operations <= MAX_OPERATIONS {
        let terminal = (1_usize << operations) - 1;
        let mut prefix = 0;
        while prefix < terminal {
            let remaining = (terminal ^ prefix).count_ones();
            if remaining >= 2 && remaining <= MAX_SUFFIX_DEPTH + 1 {
                result[operations][prefix / LANES] |= 1_u64 << (prefix % LANES);
            }
            prefix += 1;
        }
        operations += 1;
    }
    result
};

/// Remove only unresolved lanes for which upper-language inclusion in the
/// fixed lower language was proved. Both graphs belong to this candidate's
/// full terminal subset, and `upper` contains every potentially exact
/// complete path. `proven` is immutable for the entire comparison.
pub(super) fn elide_covered_suffix_edges(
    pieces: &[PieceKind],
    upper: &OperationParents,
    proven: &OperationParents,
    unresolved: &mut OperationParents,
) -> u64 {
    elide_with_limit(pieces, upper, proven, unresolved, MAX_PROOFS_PER_CANDIDATE)
}

fn elide_with_limit(
    pieces: &[PieceKind],
    upper: &OperationParents,
    proven: &OperationParents,
    unresolved: &mut OperationParents,
    mut proof_budget: usize,
) -> u64 {
    let operations = pieces.len();
    if !(2..=MAX_OPERATIONS).contains(&operations) || proof_budget == 0 {
        return 0;
    }

    let terminal = (1_usize << operations) - 1;
    let word_count = (terminal + 1).div_ceil(LANES);
    let mut same_kind = [0_usize; MAX_OPERATIONS];
    for (operation, &piece) in pieces.iter().enumerate() {
        // Add each unordered pair once.
        for (previous, &kind) in pieces[..operation].iter().enumerate() {
            if piece == kind {
                same_kind[operation] |= 1 << previous;
                same_kind[previous] |= 1 << operation;
            }
        }
    }

    let mut removed = 0;
    for (operation, words) in unresolved[..operations].iter_mut().enumerate() {
        let sibling_operations = same_kind[operation];
        if sibling_operations == 0 {
            continue;
        }
        let operation_bit = 1 << operation;
        for (word, pending) in words[..word_count].iter_mut().enumerate() {
            let near = NEAR_TERMINAL[operations][word];
            let eligible = *pending & near & upper[operation][word] & !proven[operation][word];
            if eligible == 0 {
                continue;
            }
            let siblings = set_bits(sibling_operations)
                .fold(0, |lanes, sibling| lanes | proven[sibling][word]);
            let mut lanes = eligible & siblings;
            while lanes != 0 {
                let lane = lanes.trailing_zeros() as usize;
                let lane_bit = 1_u64 << lane;
                lanes &= lanes - 1;
                let parent = word * LANES + lane;
                let remaining = terminal ^ parent;
                if remaining & operation_bit == 0 {
                    continue;
                }
                let alternatives = sibling_operations & remaining;
                if alternatives == 0 {
                    continue;
                }
                if proof_budget == 0 {
                    return removed;
                }
                proof_budget -= 1;

                // At most four pending operations, two of which have the
                // same kind. Give the remaining alphabet at most three
                // digits, identical for all sibling branches.
                let Some(labels) = local_labels(pieces, remaining) else {
                    continue;
                };
                let optimistic = suffix_words(
                    upper,
                    parent | operation_bit,
                    remaining ^ operation_bit,
                    &labels,
                    0,
                );
                if optimistic == 0 {
                    // Empty/missing upper evidence is not a certificate.
                    continue;
                }
                let mut certified = 0;
                for sibling in set_bits(alternatives) {
                    if proven[sibling][word] & lane_bit == 0 {
                        continue;
                    }
                    let sibling_bit = 1 << sibling;
                    certified |= suffix_words(
                        proven,
                        parent | sibling_bit,
                        remaining ^ sibling_bit,
                        &labels,
                        0,
                    );
                    if optimistic & !certified == 0 {
                        // Every exact suffix is in optimistic; each member
                        // of certified already has a complete physical path.
                        *pending &= !lane_bit;
                        removed += 1;
                        break;
                    }
                }
            }
        }
    }
    removed
}

fn local_labels(pieces: &[PieceKind], remaining: usize) -> Option<[u8; MAX_OPERATIONS]> {
    let mut labels = [0; MAX_OPERATIONS];
    let mut alphabet = [PieceKind::I; 3];
    let mut count = 0;
    for operation in set_bits(remaining) {
        let piece = pieces[operation];
        let label = if let Some(index) = alphabet[..count].iter().position(|&kind| kind == piece) {
            index
        } else {
            *alphabet.get_mut(count)? = piece;
            count += 1;
            count - 1
        };
        labels[operation] = label as u8;
    }
    Some(labels)
}

/// Encode the complete suffix language at a fixed depth (at most 3) as a
/// 27-bit set. Equal-length base-three words, including those with leading
/// zero digits, have distinct indices. Each recursion places one operation.
fn suffix_words(
    graph: &OperationParents,
    prefix: usize,
    remaining: usize,
    labels: &[u8; MAX_OPERATIONS],
    code: u8,
) -> u32 {
    if remaining == 0 {
        return 1_u32 << code;
    }
    let word = prefix / LANES;
    let lane = 1_u64 << (prefix % LANES);
    let mut result = 0;
    for operation in set_bits(remaining) {
        if graph[operation][word] & lane != 0 {
            let bit = 1 << operation;
            result |= suffix_words(
                graph,
                prefix | bit,
                remaining ^ bit,
                labels,
                3 * code + labels[operation],
            );
        }
    }
    result
}

fn set_bits(mut bits: usize) -> impl Iterator<Item = usize> {
    core::iter::from_fn(move || {
        if bits == 0 {
            return None;
        }
        let index = bits.trailing_zeros() as usize;
        bits &= bits - 1;
        Some(index)
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn add_edge(graph: &mut OperationParents, prefix: usize, operation: usize) {
        graph[operation][prefix / LANES] |= 1 << (prefix % LANES);
    }

    fn add_path(graph: &mut OperationParents, prefix: usize, operations: &[usize]) {
        let mut prefix = prefix;
        for &operation in operations {
            add_edge(graph, prefix, operation);
            prefix |= 1 << operation;
        }
    }

    fn difference(upper: &OperationParents, lower: &OperationParents) -> OperationParents {
        core::array::from_fn(|operation| {
            core::array::from_fn(|word| upper[operation][word] & !lower[operation][word])
        })
    }

    // Independent explicit word oracle; no encoded suffix sets or masks
    // representing an entire language.
    fn words(graph: &OperationParents, pieces: &[PieceKind]) -> BTreeSet<Vec<u8>> {
        fn visit(
            graph: &OperationParents,
            pieces: &[PieceKind],
            prefix: usize,
            word: &mut Vec<u8>,
            output: &mut BTreeSet<Vec<u8>>,
        ) {
            if word.len() == pieces.len() {
                output.insert(word.clone());
                return;
            }
            for (operation, piece) in pieces.iter().enumerate() {
                let bit = 1 << operation;
                if prefix & bit != 0
                    || graph[operation][prefix / LANES] & (1 << (prefix % LANES)) == 0
                {
                    continue;
                }
                word.push(if *piece == PieceKind::I { b'I' } else { b'J' });
                visit(graph, pieces, prefix | bit, word, output);
                word.pop();
            }
        }

        let mut result = BTreeSet::new();
        visit(graph, pieces, 0, &mut Vec::new(), &mut result);
        result
    }

    #[test]
    fn symbolic_prefix_subsumption_collective_certificate_preserves_every_exact_language() {
        use PieceKind::{I, J};
        let pieces = [I, I, I, J];
        let mut proven = [[0; MAX_PREFIX_WORDS]; MAX_OPERATIONS];
        add_path(&mut proven, 0, &[1, 0, 2, 3]); // alternative 1: IIIJ
        add_path(&mut proven, 0, &[2, 3, 0, 1]); // alternative 2: IJII
        let mut upper = proven;
        add_path(&mut upper, 0, &[0, 1, 2, 3]);
        add_path(&mut upper, 0, &[0, 3, 2, 1]); // pending: both words
        let pending = difference(&upper, &proven);
        let mut pruned = pending;
        assert!(elide_covered_suffix_edges(&pieces, &upper, &proven, &mut pruned) > 0);
        assert_eq!(pruned[0][0] & 1, 0, "remove the collectively covered edge");

        // All assignments of true/false to the unconfirmed physical edges
        // must preserve the exact language, not just the optimistic union.
        let trial_edges: Vec<_> = (0..(1 << pieces.len()))
            .flat_map(|prefix| (0..pieces.len()).map(move |operation| (prefix, operation)))
            .filter(|&(prefix, operation)| pending[operation][0] & (1 << prefix) != 0)
            .collect();
        assert!(trial_edges.len() < 10);
        for assignment in 0..(1 << trial_edges.len()) {
            let mut original = proven;
            let mut reduced = proven;
            for (index, &(prefix, operation)) in trial_edges.iter().enumerate() {
                if assignment & (1 << index) != 0 {
                    add_edge(&mut original, prefix, operation);
                    if pruned[operation][0] & (1 << prefix) != 0 {
                        add_edge(&mut reduced, prefix, operation);
                    }
                }
            }
            assert_eq!(words(&original, &pieces), words(&reduced, &pieces));
        }
    }

    #[test]
    fn symbolic_prefix_subsumption_incomparable_suffixes_stay_unresolved() {
        use PieceKind::{I, J};
        let pieces = [I, I, I, J];
        let mut proven = [[0; MAX_PREFIX_WORDS]; MAX_OPERATIONS];
        add_path(&mut proven, 0, &[1, 0, 2, 3]); // IIIJ, not IJII
        let mut upper = proven;
        add_path(&mut upper, 0, &[0, 1, 2, 3]);
        add_path(&mut upper, 0, &[0, 3, 2, 1]);
        add_path(&mut upper, 0, &[2, 3, 0, 1]); // not physically proved
        let mut pending = difference(&upper, &proven);
        elide_covered_suffix_edges(&pieces, &upper, &proven, &mut pending);
        assert_ne!(
            pending[0][0] & 1,
            0,
            "do not let pending siblings certify each other"
        );
        assert_ne!(pending[2][0] & 1, 0);
    }

    #[test]
    fn symbolic_prefix_subsumption_crosses_word_boundary_and_respects_proof_budget() {
        use PieceKind::{I, J};
        let pieces = [I, J, J, J, J, J, J, I];
        let prefix = 0b0111_1110;
        let mut proven = [[0; MAX_PREFIX_WORDS]; MAX_OPERATIONS];
        add_path(&mut proven, prefix, &[7, 0]);
        let mut upper = proven;
        add_path(&mut upper, prefix, &[0, 7]);
        let original = difference(&upper, &proven);
        let mut pending = original;
        assert_eq!(
            elide_with_limit(&pieces, &upper, &proven, &mut pending, 0),
            0
        );
        assert_eq!(pending, original);
        assert_eq!(
            elide_with_limit(&pieces, &upper, &proven, &mut pending, 1),
            1
        );
        assert_eq!(pending[0][prefix / LANES], 0);
    }
}
