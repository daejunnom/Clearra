//! Construct literal source queues and run their explicit actions through the
//! canonical public supply automaton. The oracle does not call the optimized
//! Boolean product, its memo, or the certificate to decide an execution.

use clearra_core_domain::probability::probability_value::ProbabilityValue;
use clearra_coverage::universe::{
    pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
};
use clearra_supply::{
    bag::BagState,
    execution_automaton::{SupplyBranchKind, SupplyExecutionAutomaton, SupplyExecutionState},
    hold::hold_policy::HoldPolicy,
    hold_automaton::SupplyProvenanceId,
    piece_source::PieceSourceId,
    queue::fixed_sequence::FixedSequence,
    MaterializedPatternUniverse, PatternUniverseMaterializer, QueueObservationPolicy,
};

use super::*;

const KINDS: [PieceKind; 7] = PieceKind::STANDARD_TETROMINOES;

fn initial(hold_enabled: bool) -> SupplyExecutionState {
    let mut state = SupplyExecutionState::new(
        PieceSourceId::new(0x55),
        0,
        None,
        0,
        BagState::fresh_standard_7_bag().packed_remainder_key(),
        SupplyProvenanceId(0x55),
    );
    if !hold_enabled {
        state.hold_policy = HoldPolicy::Forbidden;
    }
    state
}

fn constraint(draws: usize, hold_enabled: bool) -> SourcePrefixConstraint {
    let universe = PatternUniverseMaterializer::standard_7_bag(draws, 0, 0x55).unwrap();
    SourcePrefixConstraint::for_source(
        &universe,
        initial(hold_enabled),
        hold_enabled,
        false,
        QueueObservationPolicy::FullQueueOracle,
    )
    .unwrap()
}

fn prefix_is_kept(constraint: SourcePrefixConstraint, word: &[PieceKind]) -> bool {
    // The production relation filters each nonterminal parent, not the
    // terminal. Operation identities here follow this concrete word order.
    (0..word.len()).all(|depth| {
        let prefix = (1_usize << depth) - 1;
        constraint.filter_lanes(word, prefix / 64, 1_u64 << (prefix % 64)) != 0
    })
}

fn construct_and_check(word: &[PieceKind], hold_enabled: bool) {
    let mut queue = Vec::new();
    let mut actions = vec![SupplyBranchKind::Current; word.len()];
    if hold_enabled {
        let missing = KINDS
            .into_iter()
            .find(|piece| !word[..6].contains(piece))
            .unwrap();
        queue.push(missing);
        queue.extend_from_slice(&word[..6]);
        actions[0] = SupplyBranchKind::StoreCurrent;
        let suffix = &word[6..];
        if suffix.iter().filter(|&&piece| piece == missing).count() == 2 {
            let replacement = KINDS
                .into_iter()
                .find(|piece| !suffix.contains(piece))
                .expect("at most five suffix positions leave an unused kind");
            let first_missing = suffix.iter().position(|&piece| piece == missing).unwrap();
            for (index, &piece) in suffix.iter().enumerate() {
                if index == first_missing {
                    queue.push(replacement);
                    actions[6 + index] = SupplyBranchKind::SwapHeld;
                } else {
                    queue.push(piece);
                }
            }
        } else {
            queue.extend_from_slice(suffix);
        }
    } else {
        queue.extend_from_slice(word);
    }
    assert_eq!(queue.len(), word.len() + usize::from(hold_enabled));
    for bag in queue.chunks(7) {
        let mut seen = 0_u8;
        for &piece in bag {
            let bit = 1_u8 << super::super::super::piece_index(piece);
            assert_eq!(seen & bit, 0, "constructed queue repeats inside a bag");
            seen |= bit;
        }
    }
    let automaton = SupplyExecutionAutomaton::sequence();
    let mut state = initial(hold_enabled);
    for (&piece, action) in word.iter().zip(actions) {
        let cursor = usize::from(state.cursor);
        let step = automaton
            .transition(state, action, queue[cursor], queue.get(cursor + 1).copied())
            .expect("canonical source/Hold action");
        assert_eq!(step.used_piece, piece);
        state = step.next_state;
    }
    assert_eq!(usize::from(state.cursor), queue.len());
    assert_eq!(state.hold_piece.is_some(), hold_enabled);
}

fn visit_suffixes(
    word: &mut Vec<PieceKind>,
    counts: &mut [u8; 7],
    remaining: usize,
    visit: &mut impl FnMut(&[PieceKind]),
) {
    if remaining == 0 {
        visit(word);
        return;
    }
    for (kind, piece) in KINDS.into_iter().enumerate() {
        if counts[kind] == 2 {
            continue;
        }
        counts[kind] += 1;
        word.push(piece);
        visit_suffixes(word, counts, remaining - 1, visit);
        word.pop();
        counts[kind] -= 1;
    }
}

#[test]
fn source_certificate_constructed_queues_match_canonical_automaton() {
    let mut checked = 0;
    for hold_enabled in [false, true] {
        let prefix_len = if hold_enabled { 6 } else { 7 };
        // Full-bag piece names are symmetric. Rotate the missing kind and
        // reverse the distinct prefix as additional concrete label checks;
        // enumerate every bounded suffix for each selected prefix.
        for shift in 0..7 {
            for reversed in [false, true] {
                let mut prefix: Vec<_> = (0..prefix_len).map(|i| KINDS[(i + shift) % 7]).collect();
                if reversed {
                    prefix.reverse();
                }
                for total in prefix_len + 1..=11 {
                    let constraint = constraint(total + usize::from(hold_enabled), hold_enabled);
                    let mut counts = [0; 7];
                    for &piece in &prefix {
                        counts[super::super::super::piece_index(piece)] += 1;
                    }
                    visit_suffixes(
                        &mut prefix.clone(),
                        &mut counts,
                        total - prefix_len,
                        &mut |word| {
                            assert!(
                                constraint.certifies_every_word(word.len(), word.iter().copied())
                            );
                            assert!(prefix_is_kept(constraint, word));
                            construct_and_check(word, hold_enabled);
                            checked += 1;
                        },
                    );
                }
            }
        }
    }
    assert!(checked > 10_000);
}

#[test]
fn source_certificate_excludes_terminal_boundary_and_requires_actual_prefix_filter() {
    use PieceKind::*;
    let good = [I, O, T, S, Z, J, L, I, O, T];
    for hold_enabled in [false, true] {
        let constraint = constraint(11, hold_enabled);
        let terminal = if hold_enabled { 6 } else { 7 };
        assert!(!constraint.certifies_every_word(terminal, good[..terminal].iter().copied()));
        assert!(!constraint.certifies_every_word(12, good.iter().copied()));
        assert!(!constraint.certifies_every_word(10, good[..9].iter().copied()));
        assert!(constraint.certifies_every_word(10, good));
        assert!(prefix_is_kept(constraint, &good));
        let repeated_early = [I, I, O, T, S, Z, J, L, O, T];
        assert!(constraint.certifies_every_word(10, repeated_early));
        assert!(!prefix_is_kept(constraint, &repeated_early));
        assert!(!constraint.certifies_every_word(10, [I, I, I, O, T, S, Z, J, L, O]));
        let too_short = self::constraint(7, hold_enabled);
        assert!(!too_short.certifies_every_word(10, good));
    }
    assert_eq!(full_bag_pattern_count(7), Some(5_040));
    assert_eq!(full_bag_pattern_count(8), Some(35_280));
    assert_eq!(full_bag_pattern_count(11), Some(4_233_600));
    assert_eq!(full_bag_pattern_count(usize::MAX), None);
}

#[cfg(feature = "placement-dependency-pc")]
#[test]
fn source_certificate_admits_public_compiler_p7p4_with_request_observation_id() {
    use clearra_core_domain::pc::pc_target::PcTarget;
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;
    use clearra_pc_graph::request::{
        OpeningPcSearchQuery, PcExecutionPolicy, PcHoldPolicy, PcQueueInput,
        RequestedSearchBackend, WorkerPolicy,
    };
    use clearra_problem::{ProblemCompiler, SearchOutputPolicy};

    // Use the same public opening compiler as placement_graph_bench P7P4.
    // Compiling this compact request does not enumerate any geometry/source
    // search. Worker fields reproduce its request policy only.
    let policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_worker_policy(WorkerPolicy::Auto)
        .with_worker_hardware_limit(9)
        .with_use_all_logical_processors(true)
        .with_exact_legal_board_enabled(false)
        .with_conditioned_reachability_enabled(false)
        .with_tablebase_requested(false);
    let query = OpeningPcSearchQuery::new(PcTarget::four_lines())
        .with_rule(clearra_rules::profile::builtin_rules::jstris_180())
        .with_queue(PcQueueInput::standard_7_bag())
        .with_hold_policy(PcHoldPolicy::EnabledEmpty)
        .with_objective(ObjectivePolicy::unique())
        .with_execution_policy(policy);
    let problem = ProblemCompiler::compile_opening_pc(&query)
        .unwrap()
        .with_output_policy(SearchOutputPolicy::Summary);
    let initial = problem.initial_hold();
    assert_ne!(initial.observation().observation_id, 0);
    assert_eq!(initial.observation().observation_id, initial.provenance.0);
    assert_eq!(
        initial.observation().policy,
        QueueObservationPolicy::FullQueueOracle
    );
    let universe = problem.piece_source().materialized_universe().unwrap();
    assert!(universe.complete());
    assert_eq!(universe.pattern_count(), 4_233_600);
    assert_eq!(universe.total_possible_pattern_count(), 4_233_600);
    assert!(!problem.supply().projects_unplaced_lookahead());
    let constraint = SourcePrefixConstraint::for_problem(&problem).unwrap();
    let word = [
        KINDS[0], KINDS[1], KINDS[2], KINDS[3], KINDS[4], KINDS[5], KINDS[6], KINDS[0], KINDS[1],
        KINDS[2],
    ];
    assert!(prefix_is_kept(constraint, &word));
    assert!(constraint.certifies_every_word(word.len(), word));
    construct_and_check(&word, true);
}

#[test]
fn source_certificate_rejects_clipped_fixed_weighted_observed_and_nonfresh_scopes() {
    let word = [
        KINDS[0], KINDS[1], KINDS[2], KINDS[3], KINDS[4], KINDS[5], KINDS[6], KINDS[0],
    ];
    let full = PatternUniverseMaterializer::standard_7_bag(9, 0, 0x55).unwrap();
    let accepts = |universe: &MaterializedPatternUniverse, state, projected, observation| {
        SourcePrefixConstraint::for_source(universe, state, true, projected, observation)
            .is_some_and(|constraint| constraint.certifies_every_word(word.len(), word))
    };
    let oracle = QueueObservationPolicy::FullQueueOracle;
    assert!(accepts(&full, initial(true), false, oracle));
    let clipped = PatternUniverseMaterializer::standard_7_bag(9, 13, 0x55).unwrap();
    assert!(!accepts(&clipped, initial(true), false, oracle));
    let fixed =
        PatternUniverseMaterializer::fixed_sequence(&FixedSequence::new(word.to_vec()), 0x55);
    assert!(!accepts(&fixed, initial(true), false, oracle));
    let weighted = MaterializedPatternUniverse::from_sequences(
        PatternUniverseId::new(0x55),
        PatternWeightModelId::new(0x55),
        vec![word.to_vec(), word.iter().copied().rev().collect()],
        vec![
            ProbabilityValue::new(0.75).unwrap(),
            ProbabilityValue::new(0.25).unwrap(),
        ],
        2,
        true,
        None,
    )
    .unwrap();
    assert!(!accepts(&weighted, initial(true), false, oracle));
    assert!(!accepts(&full, initial(true), true, oracle));
    assert!(!accepts(
        &full,
        initial(true),
        false,
        QueueObservationPolicy::VisibleSeven
    ));
    for change in 0..7 {
        let mut state = initial(true);
        match change {
            0 => state.cursor = 1,
            1 => {
                state.hold_piece = Some(KINDS[0]);
                state.hold_empty = false;
            }
            2 => state.bag_epoch = 1,
            3 => state.bag_remainder_key = 0,
            4 => state.hold_policy = HoldPolicy::Required,
            5 => {
                state.observation = clearra_supply::SupplyObservationIdentity::new(
                    QueueObservationPolicy::VisibleSeven,
                    state.provenance.0,
                )
            }
            6 => state.hold_policy = HoldPolicy::Forbidden,
            _ => unreachable!(),
        }
        assert!(
            !accepts(&full, state, false, oracle),
            "scope change {change}"
        );
    }
}
