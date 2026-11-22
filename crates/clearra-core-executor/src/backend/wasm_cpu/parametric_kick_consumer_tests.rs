//! Exact imported table ownership across ordinary and shared PC consumers.
//! Expected queue bits come from the raw shape/kick interpreter, with concrete
//! row deletion after each lock, independently of dependency/catalog edges.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    piece::{piece_kind::PieceKind, rotation::RotationState},
};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    RequestedSearchBackend, SupplyWindowSize,
};
use clearra_piece_registry::standard::tetromino_registry::standard_tetromino_registry;
use clearra_problem::{ProblemCompiler, SearchOutputPolicy, SearchProblem};
use clearra_rules::{
    kicks::{
        KickOffsetSequence, KickTableEntry, KickTableProfile, KickTableProfileId, KickTransition,
        SrsKicks, VerifiedKickTableProfile,
    },
    profile::rule_profile::RuleProfileId,
};
use clearra_supply::QueueObservationPolicy;

use super::{
    catalog::{GeometryCatalog, InstantiatedRealization},
    compact_kick_authority::CompactKickAuthority,
    piece_index,
    reachability::{ReachabilityWorkspace, SharedReachabilityTemplates},
};
use crate::{
    reachability_reference::reference_spawn_locks_for_ordered_profile, WasmCpuSearchBackend,
};

fn profile(reordered: bool, id: KickTableProfileId) -> VerifiedKickTableProfile {
    let original = SrsKicks::profile();
    let selected = KickTransition::new(PieceKind::T, RotationState::Right, RotationState::Zero);
    let mut entries = original.entries().to_vec();
    if reordered {
        let entry = entries
            .iter_mut()
            .find(|entry| entry.transition() == selected)
            .unwrap();
        let mut offsets = entry.sequence().offsets().to_vec();
        offsets[..=2].rotate_right(1);
        *entry = KickTableEntry::new(selected, KickOffsetSequence::new(offsets));
    }
    VerifiedKickTableProfile::try_new(KickTableProfile::new(id, RuleProfileId::Srs, entries))
        .unwrap()
}

#[test]
fn same_tag_different_ordered_tables_cannot_replace_a_shared_movement_owner() {
    const INITIAL: u64 = 0x000f_83c7;
    let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(
        10,
        2,
        INITIAL,
        ((1 << 20) - 1) & !INITIAL,
    )
    .unwrap();
    let canonical = profile(false, KickTableProfileId::Srs90);
    let reordered = profile(true, KickTableProfileId::Srs90);
    let first = CompactKickAuthority::admit_profile(10, 2, RuleProfileId::Srs, &canonical).unwrap();
    let second =
        CompactKickAuthority::admit_profile(10, 2, RuleProfileId::Srs, &reordered).unwrap();
    assert_eq!(first.profile_id(), second.profile_id());
    assert_ne!(first, second);
    let mut a = ReachabilityWorkspace::default();
    let mut b = ReachabilityWorkspace::default();
    a.set_shared_templates(Arc::new(SharedReachabilityTemplates::with_authority(first)))
        .unwrap();
    b.set_shared_templates(Arc::new(SharedReachabilityTemplates::with_authority(
        second.clone(),
    )))
    .unwrap();
    assert!(a.has_local_import() && b.has_local_import());
    assert!(a.bind_local_import(second, &catalog).is_err());
    let lock = InstantiatedRealization {
        lock_mask: 0x4038,
        rotation: RotationState::Zero,
        x: 3,
        lock_y: 0,
    };
    for _ in 0..2 {
        // Reconfiguring by an equal nominal ID must preserve the bound table.
        a.configure_kick_profile(KickTableProfileId::Srs90, false);
        b.configure_kick_profile(KickTableProfileId::Srs90, false);
        let actual = [
            a.lock_reachable_instantiated(&catalog, INITIAL, PieceKind::T, lock),
            b.lock_reachable_instantiated(&catalog, INITIAL, PieceKind::T, lock),
        ];
        let expected = [&canonical, &reordered].map(|profile| {
            reference_spawn_locks_for_ordered_profile(
                10,
                2,
                INITIAL,
                PieceKind::T,
                profile.profile(),
            )
            .unwrap()[0]
                & (1 << 3)
                != 0
        });
        assert_eq!(expected, [false, true]);
        assert_eq!(actual, expected);
    }
}

fn problem(
    initial: u64,
    pieces: usize,
    table: &VerifiedKickTableProfile,
    observation: QueueObservationPolicy,
    workers: usize,
) -> SearchProblem {
    ProblemCompiler::compile_scenario_pc(
        &PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, initial),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(pieces),
        )
        .with_verified_kick_table_profile(table.clone())
        .with_allow_hold(false)
        .with_exact_pieces(Some(pieces))
        .with_supply_window_size(SupplyWindowSize::new(pieces))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_queue_observation_policy(observation)
        .with_retained_trace_limit(0)
        .with_execution_policy(
            PcExecutionPolicy::mvp_default()
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_allow_backend_fallback(false)
                .with_workers(workers)
                .with_tablebase_requested(false)
                .with_exact_legal_board_enabled(false)
                .with_conditioned_reachability_enabled(false)
                .with_precompute_build_dependencies(false),
        ),
    )
    .unwrap()
    .with_output_policy(SearchOutputPolicy::Summary)
}

fn clear_rows(board: u64) -> u64 {
    let mut next = 0_u64;
    let mut row = 0;
    for source in 0..2 {
        let cells = (board >> (10 * source)) & 0x3ff;
        if cells != 0x3ff {
            next |= cells << (10 * row);
            row += 1;
        }
    }
    next
}

type Moves = BTreeMap<(u64, u8), Vec<u64>>;

fn raw_next_boards(
    board: u64,
    piece: PieceKind,
    profile: &KickTableProfile,
    cache: &mut Moves,
) -> Vec<u64> {
    cache
        .entry((board, piece_index(piece) as u8))
        .or_insert_with(|| {
            let anchors =
                reference_spawn_locks_for_ordered_profile(10, 2, board, piece, profile).unwrap();
            let definition = standard_tetromino_registry().get(piece).unwrap();
            let mut next = BTreeSet::new();
            for rotation in RotationState::ALL {
                let mut remaining = anchors[rotation.quarter_turns() as usize];
                while remaining != 0 {
                    let anchor = remaining.trailing_zeros() as i16;
                    remaining &= remaining - 1;
                    let (x, y) = (anchor % 10, anchor / 10);
                    let mut footprint = 0_u64;
                    let mut valid = true;
                    for cell in definition.shape(rotation).cells() {
                        let column = x + i16::from(cell.x());
                        let row = y + i16::from(cell.y());
                        if !(0..10).contains(&column) || !(0..2).contains(&row) {
                            valid = false;
                            break;
                        }
                        footprint |= 1_u64 << (row * 10 + column);
                    }
                    if valid && footprint.count_ones() == 4 && board & footprint == 0 {
                        next.insert(clear_rows(board | footprint));
                    }
                }
            }
            next.into_iter().collect()
        })
        .clone()
}

fn raw_queue_succeeds(
    board: u64,
    queue: &[PieceKind],
    profile: &KickTableProfile,
    cache: &mut Moves,
) -> bool {
    let Some((&piece, suffix)) = queue.split_first() else {
        return board == 0;
    };
    raw_next_boards(board, piece, profile, cache)
        .into_iter()
        .any(|next| raw_queue_succeeds(next, suffix, profile, cache))
}

fn raw_coverage(problem: &SearchProblem, profile: &KickTableProfile) -> Vec<u64> {
    let universe = problem.piece_source().materialized_universe().unwrap();
    let mut result = vec![0; universe.pattern_count().div_ceil(64)];
    let mut moves = Moves::new();
    for ordinal in 0..universe.pattern_count() {
        if raw_queue_succeeds(
            problem.initial_board().occupied_mask(),
            universe.sequence_at(ordinal).as_ref(),
            profile,
            &mut moves,
        ) {
            result[ordinal / 64] |= 1_u64 << (ordinal % 64);
        }
    }
    result
}

#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
#[test]
fn supplied_ordered_profile_reaches_actual_serial_and_parallel_dependency_consumers() {
    let canonical = profile(false, KickTableProfileId::Imported);
    let reordered = profile(true, KickTableProfileId::Imported);
    let mut first_fixture_coverage = Vec::new();
    let mut observed_parallel = false;
    // The first board is the real order-sensitive T/I cavity. The second
    // has a larger exact-cover family while its complete supply has only
    // 7P3 queues, so a real parallel consumer can be checked against raw BFS.
    for (initial, pieces) in [(0x000f_83c7, 2), (0x3c0f, 3)] {
        for table in [&canonical, &reordered] {
            let reference_problem = problem(
                initial,
                pieces,
                table,
                QueueObservationPolicy::FullQueueOracle,
                1,
            );
            assert!(reference_problem.pc_placement_dependency_eligible());
            let expected = raw_coverage(&reference_problem, table.profile());
            if pieces == 3 {
                assert!(expected.iter().any(|&word| word != 0));
            }
            if pieces == 2 {
                first_fixture_coverage.push(expected.clone());
            }
            let mut identities = None;
            for (observation, workers) in [
                (QueueObservationPolicy::FullQueueOracle, 1),
                (QueueObservationPolicy::VisibleSeven, 1),
                (QueueObservationPolicy::VisibleSeven, 2),
            ] {
                let request = problem(initial, pieces, table, observation, workers);
                let result = WasmCpuSearchBackend::execute_with_control(
                    &request,
                    &ExecutionControl::default(),
                )
                .unwrap();
                assert_eq!(result.bool_field("count_complete"), Some(true));
                assert_eq!(result.bool_field("probability_complete"), Some(true));
                assert_eq!(result.bool_field("resource_truncated"), Some(false));
                assert_eq!(result.usize_field("total_build_order_nodes"), Some(0));
                assert_eq!(result.usize_field("total_reachability_states"), Some(0));
                assert_eq!(result.coverage_pattern_words(), expected.as_slice());
                if let Some(first) = &identities {
                    assert_eq!(result.normalized_solution_identities(), first);
                } else {
                    identities = Some(result.normalized_solution_identities().to_vec());
                }
                observed_parallel |= result.bool_field("cpu_parallel_execution") == Some(true)
                    && result.usize_field("workers_used") == Some(2);
            }
        }
    }
    assert_ne!(
        first_fixture_coverage[0], first_fixture_coverage[1],
        "raw table order must be observable"
    );
    assert!(
        observed_parallel,
        "at least one result must traverse the actual two-worker consumer"
    );
}
