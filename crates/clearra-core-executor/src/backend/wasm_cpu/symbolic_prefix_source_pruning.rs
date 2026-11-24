//! Necessary source conditions, applied before physical prefix compilation.
//! The exact source/Hold product still decides every surviving language.

use clearra_core_domain::piece::piece_kind::PieceKind;

/// Starting with an empty Hold, d placements consume at most d+1 draws.
/// A fresh standard bag therefore cannot repeat a placed kind in the first
/// six placements (seven when Hold is disabled). This concerns the original
/// supply, independently of line clears or the physical board. An observed
/// bag remainder, fixed queue, initial held piece or advanced cursor does not
/// inherit the proof.
#[derive(Clone, Copy)]
pub(in super::super) struct SourcePrefixConstraint {
    distinct_prefix_len: usize,
    #[cfg(any(test, feature = "placement-dependency-pc"))]
    full_original_draws: Option<usize>,
}

const LOW_DEPTH_LANES: [u64; 8] = low_depth_lanes();

const fn low_depth_lanes() -> [u64; 8] {
    let mut result = [0; 8];
    let mut depth = 0;
    while depth < result.len() {
        let mut lane: u64 = 0;
        while lane < 64 {
            if lane.count_ones() as usize <= depth {
                result[depth] |= 1 << lane;
            }
            lane += 1;
        }
        depth += 1;
    }
    result
}

impl SourcePrefixConstraint {
    #[cfg(feature = "placement-dependency-pc")]
    pub(in super::super) fn for_problem(problem: &clearra_problem::SearchProblem) -> Option<Self> {
        Self::for_source(
            problem.piece_source().materialized_universe()?,
            problem.initial_hold(),
            problem.supply().hold_enabled(),
            problem.supply().projects_unplaced_lookahead(),
            problem.queue_observation_policy(),
        )
    }

    #[cfg(any(test, feature = "placement-dependency-pc"))]
    fn for_source(
        universe: &clearra_supply::pattern_universe::materialized_pattern_universe::MaterializedPatternUniverse,
        initial: clearra_supply::hold_automaton::HoldAutomatonState,
        hold_enabled: bool,
        projects_unplaced_lookahead: bool,
        observation: clearra_supply::QueueObservationPolicy,
    ) -> Option<Self> {
        use clearra_supply::bag::BagState;
        use clearra_supply::hold::hold_policy::HoldPolicy;
        use clearra_supply::pattern_universe::materialized_pattern_universe::UniformCompactPatternSource;

        if initial.cursor() != 0 || !initial.hold_empty() || initial.hold_piece().is_some() {
            return None;
        }
        // Inspect the authoritative compact storage view, not a descriptive
        // universe ID, an example queue or a guessed permutation syntax.
        let UniformCompactPatternSource::Standard7Bag {
            sequence_len,
            pattern_count,
        } = universe.uniform_compact_source()?
        else {
            return None;
        };
        // A clipped original universe can be globally covered without
        // containing the queue used by the constructive witness below.
        // Check the actual compact storage against the mathematical size;
        // `complete` and descriptive universe IDs alone are not authority.
        let full_original_draws = (universe.complete()
            && full_bag_pattern_count(sequence_len) == Some(pattern_count as u128)
            && universe.total_possible_pattern_count() == pattern_count as u128
            && initial.bag_epoch() == 0
            && initial.bag_remainder_key()
                == BagState::fresh_standard_7_bag().packed_remainder_key()
            && matches!(
                (hold_enabled, initial.hold_policy()),
                (true, HoldPolicy::Allowed) | (false, HoldPolicy::Allowed | HoldPolicy::Forbidden)
            )
            && !projects_unplaced_lookahead
            && observation == clearra_supply::QueueObservationPolicy::FullQueueOracle
            // The compiler tags even FullQueueOracle with this request's
            // provenance ID. Compare the semantic policy, not the ID-zero
            // convenience constructor. This value remains local to the same
            // original source/initial state used to compile the relation.
            && initial.observation().policy == observation)
            .then_some(sequence_len);
        Some(Self {
            distinct_prefix_len: if hold_enabled { 6 } else { 7 },
            full_original_draws,
        })
    }

    /// Valid only for a nonempty exact language compiled using this very
    /// constraint. Its first six (Hold) or seven (no Hold) placed kinds must
    /// be distinct. It proves existence in the full original source, never a
    /// particular source ordinal, coverage row, or path multiplicity.
    ///
    /// With Hold, write the first six distinct pieces as A..F and the missing
    /// kind as G. The first bag [G,A,..,F] stores G and places A..F. In the
    /// remaining <=5 positions only G can occur twice. If it does, replace
    /// the first G draw by a kind X absent from that suffix and swap held G;
    /// draw the second G normally. All second-bag draws are distinct. Without
    /// Hold, seven distinct first pieces and <=2 of each kind make the suffix
    /// itself a distinct second-bag prefix. Both constructions use only the
    /// guaranteed original draws and preserve an ordinary final Hold.
    #[cfg(any(test, feature = "placement-dependency-pc"))]
    pub(in super::super) fn certifies_every_word(
        self,
        operation_count: usize,
        pieces: impl IntoIterator<Item = PieceKind>,
    ) -> bool {
        let Some(draws) = self.full_original_draws else {
            return false;
        };
        // The filter constrains nonterminal parents. At exactly six/seven
        // locks the relevant terminal would not carry its proof.
        if operation_count <= self.distinct_prefix_len || operation_count > 11 {
            return false;
        }
        let hold_enabled = self.distinct_prefix_len == 6;
        if draws < operation_count + usize::from(hold_enabled) {
            return false;
        }
        let mut counts = [0_u8; 7];
        let mut observed = 0;
        for piece in pieces {
            observed += 1;
            if observed > operation_count {
                return false;
            }
            let count = &mut counts[super::super::piece_index(piece)];
            *count += 1;
            if *count > 2 {
                return false;
            }
        }
        observed == operation_count
    }

    pub(super) fn filter_lanes(self, pieces: &[PieceKind], word: usize, active: u64) -> u64 {
        let high_depth = word.count_ones() as usize;
        if high_depth > self.distinct_prefix_len || active == 0 {
            return active;
        }
        let early = LOW_DEPTH_LANES[self.distinct_prefix_len - high_depth] & active;
        if early == 0 {
            return active;
        }
        let mut seen = [0_u64; 7];
        let mut repeated = 0;
        for (operation, &piece) in pieces.iter().enumerate() {
            let owners = super::owner_lanes(1 << operation, word);
            let kind = super::super::piece_index(piece);
            repeated |= seen[kind] & owners;
            seen[kind] |= owners;
        }
        active & !(early & repeated)
    }
}

#[cfg(any(test, feature = "placement-dependency-pc"))]
fn full_bag_pattern_count(draws: usize) -> Option<u128> {
    const TAIL_COUNTS: [u128; 7] = [1, 7, 42, 210, 840, 2_520, 5_040];
    5_040_u128
        .checked_pow(u32::try_from(draws / 7).ok()?)?
        .checked_mul(TAIL_COUNTS[draws % 7])
}

#[cfg(test)]
#[path = "source_membership_certificate_tests.rs"]
mod membership_certificate_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_prefix_words_match_scalar_counts_across_high_operation_bits() {
        use PieceKind::*;
        for pieces in [
            vec![I, O, T, S, Z, J, L],
            vec![T, I, T, O, S, Z, J, L, I, O, S],
            vec![O; 11],
            vec![I, O, T, S, Z, J, L, I, O, T],
        ] {
            for limit in [6, 7] {
                let constraint = SourcePrefixConstraint {
                    distinct_prefix_len: limit,
                    full_original_draws: None,
                };
                let count = 1_usize << pieces.len();
                for word in 0..count.div_ceil(64) {
                    let active = u64::MAX >> (64 - (count - word * 64).min(64));
                    let actual = constraint.filter_lanes(&pieces, word, active);
                    for lane in 0..64 {
                        let subset = word * 64 + lane;
                        if subset >= count {
                            continue;
                        }
                        let mut counts = [0_u8; 7];
                        for (operation, &piece) in pieces.iter().enumerate() {
                            if subset & (1 << operation) != 0 {
                                counts[super::super::super::piece_index(piece)] += 1;
                            }
                        }
                        let expected = subset.count_ones() as usize > limit
                            || counts.into_iter().all(|count| count <= 1);
                        assert_eq!(actual & (1 << lane) != 0, expected);
                    }
                }
            }
        }
    }

    fn inspect_prefixes(
        queue: &[PieceKind],
        cursor: usize,
        held: Option<PieceKind>,
        placed: &mut Vec<PieceKind>,
        hold: bool,
        checked: &mut usize,
    ) {
        let constraint = SourcePrefixConstraint {
            distinct_prefix_len: if hold { 6 } else { 7 },
            full_original_draws: None,
        };
        if !placed.is_empty() {
            let subset = (1_usize << placed.len()) - 1;
            assert_ne!(
                constraint.filter_lanes(placed, subset / 64, 1 << (subset % 64)),
                0
            );
            *checked += 1;
        }
        if cursor >= queue.len() || placed.len() >= 7 {
            return;
        }
        placed.push(queue[cursor]);
        inspect_prefixes(queue, cursor + 1, held, placed, hold, checked);
        placed.pop();
        if hold {
            if let Some(piece) = held {
                placed.push(piece);
                inspect_prefixes(
                    queue,
                    cursor + 1,
                    Some(queue[cursor]),
                    placed,
                    hold,
                    checked,
                );
                placed.pop();
            } else if cursor + 1 < queue.len() {
                placed.push(queue[cursor + 1]);
                inspect_prefixes(
                    queue,
                    cursor + 2,
                    Some(queue[cursor]),
                    placed,
                    hold,
                    checked,
                );
                placed.pop();
            }
        }
    }

    #[test]
    fn source_prefix_constraint_keeps_every_empty_hold_execution_across_first_bag_boundary() {
        let first = PieceKind::STANDARD_TETROMINOES;
        let mut second = first;
        second.reverse();
        let queue = first.into_iter().chain(second).collect::<Vec<_>>();
        let mut checked = 0;
        for hold in [false, true] {
            inspect_prefixes(&queue, 0, None, &mut Vec::new(), hold, &mut checked);
        }
        assert!(checked > 100);
    }

    #[cfg(feature = "placement-dependency-pc")]
    #[test]
    fn source_prefix_admission_uses_actual_supply_and_initial_hold() {
        use clearra_pc_graph::request::{
            PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow, SupplyWindowSize,
        };
        use clearra_problem::ProblemCompiler;
        use clearra_supply::queue::fixed_sequence::FixedSequence;

        for (source, hold, held, expected) in [
            (PcQueueInput::standard_7_bag(), true, None, Some(6)),
            (PcQueueInput::standard_7_bag(), false, None, Some(7)),
            (
                PcQueueInput::standard_7_bag(),
                true,
                Some(PieceKind::O),
                None,
            ),
            (
                PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O; 2])),
                true,
                None,
                None,
            ),
        ] {
            let problem = ProblemCompiler::compile_scenario_pc(
                &PcScenarioQuery::new(
                    PcScenarioBoard::standard_10(2, 0xf3fcf),
                    source,
                    PieceWindow::new(1),
                )
                .with_allow_hold(hold)
                .with_hold_piece(held)
                .with_exact_pieces(Some(1))
                .with_supply_window_size(SupplyWindowSize::new(2)),
            )
            .unwrap();
            assert_eq!(
                SourcePrefixConstraint::for_problem(&problem)
                    .map(|value| value.distinct_prefix_len),
                expected
            );
        }
    }
}
