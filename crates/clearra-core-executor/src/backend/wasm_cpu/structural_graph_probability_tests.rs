//! Independent physical qualification of the structural-order shortcut.
//! The oracle enumerates concrete operation subsets, interprets ordered kicks
//! with the reference BFS, and clears rows without CandidateProjection.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, MutexGuard};

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PcSolutionProbabilityPolicy, PieceWindow, RequestedSearchBackend, SupplyWindowSize,
};
use clearra_problem::{ProblemCompiler, SearchOutputPolicy};
use clearra_rules::kicks::KickTableProfileId;
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::{
    catalog::GeometryCatalog, geometry::GeometryCandidate,
    piece_order_language::PieceOrderLanguageCache, placement_dependency, placement_poset,
    reachability::ReachabilityWorkspace, structural_order,
};
use crate::conditioned_local_relation::LocalRelationRowFrame;
use crate::reachability_reference::reference_spawn_lock_anchors;
use crate::{CoreExecutionResult, WasmCpuSearchBackend};

static PRODUCT_LOCK: Mutex<()> = Mutex::new(());

fn product_guard() -> MutexGuard<'static, ()> {
    PRODUCT_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn fixture(height: u8, placements: &[(PieceKind, u64)]) -> (GeometryCatalog, GeometryCandidate) {
    let required = placements.iter().fold(0_u64, |bits, (_, mask)| bits | mask);
    let full = (1_u64 << (10 * height)) - 1;
    let catalog = GeometryCatalog::compile_for_required_cells_on_dimensions(
        10,
        height,
        full & !required,
        required,
    )
    .expect("real inverse-lock-clear catalog");
    let rows: Vec<_> = placements
        .iter()
        .map(|&(piece, mask)| {
            catalog
                .skeleton_id(piece, mask)
                .expect("fixture placement belongs to catalog")
        })
        .collect();
    let candidate = GeometryCandidate::from_rows(&catalog, 0, &rows).expect("exact candidate");
    (catalog, candidate)
}

fn clear_rows(board: u64, width: u8, height: u8) -> (u64, u16) {
    let row_mask = (1_u64 << width) - 1;
    let mut output = 0;
    let mut output_y = 0;
    let mut removed = 0;
    for y in 0..height {
        let row = (board >> (y * width)) & row_mask;
        if row == row_mask {
            removed |= 1_u16 << y;
        } else {
            output |= row << (output_y * width);
            output_y += 1;
        }
    }
    (output, removed)
}

fn subset_state(
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    subset: usize,
) -> (u64, u16) {
    let logical =
        candidate
            .row_ids()
            .iter()
            .enumerate()
            .fold(catalog.initial_board(), |board, (i, &row)| {
                if subset & (1 << i) == 0 {
                    board
                } else {
                    board | catalog.skeleton(row).cells
                }
            });
    clear_rows(logical, catalog.width(), catalog.height())
}

fn original_deleted_rows(height: u8, previous: u16, current: u16) -> u16 {
    let mut result = previous;
    let mut physical_y = 0;
    for original_y in 0..height {
        if previous & (1 << original_y) != 0 {
            continue;
        }
        if current & (1 << physical_y) != 0 {
            result |= 1 << original_y;
        }
        physical_y += 1;
    }
    result
}

fn physical_transitions(
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    profile: KickTableProfileId,
) -> Vec<u16> {
    let n = candidate.row_ids().len();
    let mut transitions = vec![0; 1 << n];
    let mut locks = BTreeMap::new();
    for (subset, allowed) in transitions.iter_mut().enumerate() {
        let (board, deleted) = subset_state(catalog, candidate, subset);
        for (operation, &row_id) in candidate.row_ids().iter().enumerate() {
            if subset & (1 << operation) != 0 {
                continue;
            }
            let row = catalog.skeleton(row_id);
            let anchors = locks.entry((board, row.piece)).or_insert_with(|| {
                reference_spawn_lock_anchors(
                    catalog.width(),
                    catalog.height(),
                    board,
                    row.piece,
                    profile,
                )
                .expect("independent full-field ordered-kick traversal")
            });
            let child = subset | (1 << operation);
            let expected = subset_state(catalog, candidate, child);
            for &raw in catalog.realizations(row_id) {
                let Some(lock) = catalog.instantiate_realization(row.cells, raw, deleted) else {
                    continue;
                };
                if lock.lock_mask & board != 0
                    || lock.x < 0
                    || lock.lock_y < 0
                    || lock.x >= catalog.width() as i8
                    || lock.lock_y >= catalog.height() as i8
                {
                    continue;
                }
                let anchor = lock.lock_y as usize * catalog.width() as usize + lock.x as usize;
                if anchors[lock.rotation.quarter_turns() as usize] & (1_u64 << anchor) == 0 {
                    continue;
                }
                let (next_board, new_deleted) =
                    clear_rows(board | lock.lock_mask, catalog.width(), catalog.height());
                if (
                    next_board,
                    original_deleted_rows(catalog.height(), deleted, new_deleted),
                ) == expected
                {
                    *allowed |= 1 << operation;
                    break;
                }
            }
        }
    }
    transitions
}

fn operation_words(pieces: &[PieceKind], transitions: &[u16]) -> BTreeSet<Vec<PieceKind>> {
    fn visit(
        pieces: &[PieceKind],
        transitions: &[u16],
        subset: usize,
        path: &mut Vec<PieceKind>,
        words: &mut BTreeSet<Vec<PieceKind>>,
    ) {
        if subset + 1 == transitions.len() {
            words.insert(path.clone());
            return;
        }
        for (i, &piece) in pieces.iter().enumerate() {
            if transitions[subset] & (1 << i) != 0 {
                path.push(piece);
                visit(pieces, transitions, subset | (1 << i), path, words);
                path.pop();
            }
        }
    }
    let mut words = BTreeSet::new();
    visit(pieces, transitions, 0, &mut Vec::new(), &mut words);
    words
}

fn language_words(cache: &PieceOrderLanguageCache, root: u32) -> BTreeSet<Vec<PieceKind>> {
    fn visit(
        cache: &PieceOrderLanguageCache,
        node: u32,
        path: &mut Vec<PieceKind>,
        words: &mut BTreeSet<Vec<PieceKind>>,
    ) {
        if cache.node(node).unwrap().accepting {
            words.insert(path.clone());
        }
        for (index, piece) in PieceKind::STANDARD_TETROMINOES.into_iter().enumerate() {
            for edge in cache.edges_for_piece(node, index as u8 + 1).unwrap() {
                path.push(piece);
                visit(cache, edge.child(), path, words);
                path.pop();
            }
        }
    }
    let mut words = BTreeSet::new();
    visit(cache, root, &mut Vec::new(), &mut words);
    words
}

#[test]
fn symbolic_prefix_matches_all_concrete_edges_and_words_without_buildup() {
    use super::symbolic_prefix_domain::PrefixDomain;
    use super::symbolic_prefix_orders::{
        compile_frame_domains, occupancy_planes, with_packed_upper_target, with_target_reverse,
        with_upper_bound, SymbolicPrefixOrders, SymbolicPrefixRefusal, SymbolicPrefixWork,
    };
    use PieceKind::{I, J, L, O, T};

    let o = 3 | (3 << 10);
    for (height, placements) in [
        (2, &[(O, o), (O, o << 2)][..]),
        (3, &[(I, 0x01e0_0000), (J, 0xe020), (T, 0x1c02)][..]),
        (
            4,
            &[
                (I, 0x07_8000_0000),
                (T, 0x1c02),
                (O, 0x6018),
                (J, 0x60_0802_0000),
                (L, 0x08_0380_0000),
            ][..],
        ),
        (
            4,
            &[
                (O, o),
                (O, o << 2),
                (O, o << 4),
                (O, o << 20),
                (O, o << 22),
                (O, o << 24),
            ][..],
        ),
    ] {
        let (catalog, candidate) = fixture(height, placements);
        let pieces: Vec<_> = candidate
            .row_ids()
            .iter()
            .map(|&row| catalog.skeleton(row).piece)
            .collect();
        let geometry =
            placement_poset::CandidateGeometry::for_symbolic_prefix(&catalog, &candidate).unwrap();
        let mut projection_work = SymbolicPrefixWork::default();
        let domain = PrefixDomain::new(pieces.len()).unwrap();
        let owners = domain.word_owners(0);
        let active = domain.nonterminal_lanes(0);
        let frames = compile_frame_domains(&geometry, &owners, active, &mut projection_work);
        // Qualification reuses the entire first partition. Restricting it
        // must agree with a fresh partition for any surviving lanes.
        for selected in [0, active, active & 0x5555_5555_5555_5555] {
            let restricted =
                compile_frame_domains(&geometry, &owners, selected, &mut projection_work);
            assert_eq!(restricted, frames.map(|lanes| lanes & selected));
        }
        let mut checked = 0_u64;
        for (frame, lanes) in frames
            .into_iter()
            .enumerate()
            .filter(|(_, lanes)| *lanes != 0)
        {
            assert_eq!(checked & lanes, 0, "a prefix must own exactly one frame");
            checked |= lanes;
            let (initial, placements) = geometry.symbolic_frame(frame as u16);
            let planes = occupancy_planes(
                initial,
                &placements[..pieces.len()],
                &owners,
                lanes,
                &mut projection_work,
            );
            let mut prefixes = lanes;
            while prefixes != 0 {
                let prefix = prefixes.trailing_zeros() as usize;
                prefixes &= prefixes - 1;
                let board = planes.iter().enumerate().fold(0, |board, (cell, word)| {
                    board | (((word >> prefix) & 1) << cell)
                });
                assert_eq!(
                    (board, frame as u16),
                    subset_state(&catalog, &candidate, prefix)
                );
            }
        }
        assert_eq!(checked, active);
        for profile in [
            KickTableProfileId::Srs90,
            KickTableProfileId::SrsPlus,
            KickTableProfileId::SrsX,
            KickTableProfileId::Jstris180,
            KickTableProfileId::NoKick,
        ] {
            let oracle = physical_transitions(&catalog, &candidate, profile);
            for reverse in [false, true] {
                let mut work = SymbolicPrefixWork::default();
                let control = ExecutionControl::default();
                let compile = || {
                    SymbolicPrefixOrders::compile(
                        &catalog, &candidate, profile, &control, &mut work,
                    )
                    .unwrap()
                };
                let relation = if reverse {
                    with_target_reverse(compile)
                } else {
                    let mut compile = compile;
                    compile()
                };
                for (prefix, &allowed) in oracle.iter().enumerate() {
                    for operation in 0..pieces.len() {
                        assert_eq!(
                            relation.local_edge_permission(prefix, operation),
                            Some(allowed & (1 << operation) != 0),
                            "height={height} profile={profile:?} reverse={reverse} prefix={prefix:#x} op={operation}",
                        );
                    }
                }
                let mut cache = PieceOrderLanguageCache::default();
                let words = relation
                    .compile_language(&mut cache, &mut work)
                    .unwrap()
                    .map_or_else(BTreeSet::new, |root| language_words(&cache, root));
                assert_eq!(words, operation_words(&pieces, &oracle));
                assert_eq!(work.represented_prefixes, (1 << pieces.len()) - 1);
                assert!(work.concrete_guards > 0);
                assert_eq!(
                    work.pose.completed_relations as usize,
                    work.frame_kind_groups + work.target_operations
                );
                if profile == KickTableProfileId::Srs90 {
                    eprintln!(
                        "strict prefix, {height}L, {} pieces, reverse={reverse}: {work:?}",
                        pieces.len()
                    );
                }
            }
            let mut bounded_work = SymbolicPrefixWork::default();
            let bounded = with_target_reverse(|| {
                with_upper_bound(|| {
                    SymbolicPrefixOrders::compile(
                        &catalog,
                        &candidate,
                        profile,
                        &ExecutionControl::default(),
                        &mut bounded_work,
                    )
                })
            })
            .unwrap();
            assert_eq!(bounded.local_edge_permission(0, 0), None);
            let mut cache = PieceOrderLanguageCache::default();
            let bounded_words = bounded
                .compile_language(&mut cache, &mut bounded_work)
                .unwrap()
                .map_or_else(BTreeSet::new, |root| language_words(&cache, root));
            assert_eq!(bounded_words, operation_words(&pieces, &oracle));
            let mut packed_work = SymbolicPrefixWork::default();
            let packed = with_packed_upper_target(|| {
                SymbolicPrefixOrders::compile(
                    &catalog,
                    &candidate,
                    profile,
                    &ExecutionControl::default(),
                    &mut packed_work,
                )
            })
            .unwrap();
            assert_eq!(packed.local_edge_permission(0, 0), None);
            let mut cache = PieceOrderLanguageCache::default();
            let packed_words = packed
                .compile_language(&mut cache, &mut packed_work)
                .unwrap()
                .map_or_else(BTreeSet::new, |root| language_words(&cache, root));
            assert_eq!(packed_words, bounded_words);
            let cancelled = ExecutionControl::default();
            cancelled.cancellation.handle().cancel();
            let mut refused = SymbolicPrefixWork::default();
            assert!(matches!(
                SymbolicPrefixOrders::compile(
                    &catalog,
                    &candidate,
                    profile,
                    &cancelled,
                    &mut refused
                ),
                Err(SymbolicPrefixRefusal::Cancelled),
            ));
            assert_eq!(refused.represented_prefixes, 0);
        }
    }
    let eleven_os = [
        (O, o),
        (O, o << 2),
        (O, o << 4),
        (O, o << 6),
        (O, o << 8),
        (O, o << 20),
        (O, o << 22),
        (O, o << 24),
        (O, o << 26),
        (O, o << 28),
        (O, o << 40),
    ];
    let (catalog, candidate) = fixture(6, &eleven_os);
    assert_eq!(
        eleven_os.len(),
        super::symbolic_prefix_domain::MAX_OPERATIONS
    );
    let oracle = physical_transitions(&catalog, &candidate, KickTableProfileId::Srs90);
    for reverse in [false, true] {
        let mut work = SymbolicPrefixWork::default();
        let control = ExecutionControl::default();
        let compile = || {
            SymbolicPrefixOrders::compile(
                &catalog,
                &candidate,
                KickTableProfileId::Srs90,
                &control,
                &mut work,
            )
            .unwrap()
        };
        let relation = if reverse {
            with_target_reverse(compile)
        } else {
            let mut compile = compile;
            compile()
        };
        for (prefix, &allowed) in oracle.iter().enumerate() {
            for operation in 0..eleven_os.len() {
                assert_eq!(
                    relation.local_edge_permission(prefix, operation),
                    Some(allowed & (1 << operation) != 0),
                    "maximum operations reverse={reverse} prefix={prefix:#x} op={operation}",
                );
            }
        }
        let mut cache = PieceOrderLanguageCache::default();
        let words = relation
            .compile_language(&mut cache, &mut work)
            .unwrap()
            .map_or_else(BTreeSet::new, |root| language_words(&cache, root));
        // Filling each lower pair of rows, then the last O-shaped hole,
        // supplies a legal complete order. All operations have the same kind,
        // so its exact piece language is this single word.
        assert_eq!(words, BTreeSet::from([vec![O; eleven_os.len()]]));
        assert_eq!(work.represented_prefixes, (1 << eleven_os.len()) - 1);
    }
    let mut twelve_os = eleven_os.to_vec();
    twelve_os.push((O, o << 42));
    let (catalog, candidate) = fixture(6, &twelve_os);
    let mut work = SymbolicPrefixWork::default();
    assert!(matches!(
        SymbolicPrefixOrders::compile(
            &catalog,
            &candidate,
            KickTableProfileId::Srs90,
            &ExecutionControl::default(),
            &mut work,
        ),
        Err(SymbolicPrefixRefusal::Unsupported),
    ));
    assert_eq!(work.represented_prefixes, 0);
}

#[test]
fn symbolic_prefix_jstris_p7p4_one_canonical_identity_preserves_exact_queues() {
    use clearra_core_domain::solution::normalized_tiling_solution::{
        PiecePlacementMask, StandardBoard64TilingIdentity,
    };
    use clearra_rules::profile::builtin_rules::jstris_180;
    use PieceKind::{I, J, L, O, S, T, Z};

    let _guard = product_guard();
    // Constructed empty-origin tiling with a no-hold hard-drop witness
    // IOTZSLJZTI from two bags. Field identities admit every other exact
    // order and hold action, not just that witness.
    let placements = [
        (I, 0xf),
        (I, 0xf0),
        (O, 0xc0300),
        (T, 0x201c00),
        (Z, 0xc06000),
        (Z, 0x1006008000),
        (S, 0x18030000),
        (L, 0x1c0100000),
        (T, 0xe01000000),
        (J, 0xe020000000),
    ];
    let identity = StandardBoard64TilingIdentity::from_placements(
        0,
        placements.map(|(piece, cells)| PiecePlacementMask::new(piece, cells)),
    )
    .unwrap();
    let problem = ProblemCompiler::compile_scenario_pc(
        &PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(10),
        )
        .with_rule(jstris_180())
        .with_allow_hold(true)
        .with_exact_pieces(Some(10))
        .with_supply_window_size(SupplyWindowSize::new(11))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
        .with_retained_trace_limit(0)
        .with_execution_policy(policy()),
    )
    .unwrap()
    .with_output_policy(SearchOutputPolicy::CoverageSummary);
    let run = || {
        // Seed just the known canonical geometry identity, not a complete
        // claim over the unenumerated empty-origin geometry family.
        WasmCpuSearchBackend::execute_complete_precomputed_geometry_candidates_with_control(
            &problem,
            &[identity],
            &ExecutionControl::default(),
        )
        .unwrap()
    };
    let reference = run();
    let (strict, work) = super::symbolic_prefix_orders::with_target_reverse(|| {
        super::symbolic_prefix_orders::with_strict_prefix_order(run)
    });
    assert_parity(&reference, &strict);
    let [entry] = strict.solution_coverages() else {
        panic!("the supplied Jstris identity must be solvable");
    };
    assert_eq!(entry.identity(), identity);
    assert_eq!(entry.covered_patterns().count_ones(), 113_216);
    assert_eq!(
        strict.usize_field("coverage_pattern_count"),
        Some(4_233_600)
    );
    assert_eq!(strict.usize_field("total_build_order_nodes"), Some(0));
    assert_eq!(strict.usize_field("total_reachability_states"), Some(0));
    assert_eq!(
        (work.candidates_attempted, work.candidates_certified),
        (1, 1)
    );
    assert_eq!(
        (work.assignment_words, work.represented_prefixes),
        (16, 1023)
    );
    eprintln!(
        "strict 10-lock Jstris single identity: queues={}, baseline_nodes={:?}, baseline_poses={:?}, strict={work:?}",
        entry.covered_patterns().count_ones(),
        reference.usize_field("total_build_order_nodes"),
        reference.usize_field("total_reachability_states")
    );
    let (upper, bounded_work) = super::symbolic_prefix_orders::with_target_reverse(|| {
        super::symbolic_prefix_orders::with_upper_bound(|| {
            super::symbolic_prefix_orders::with_strict_prefix_order(run)
        })
    });
    assert_parity(&reference, &upper);
    assert_eq!(upper.usize_field("total_build_order_nodes"), Some(0));
    assert_eq!(upper.usize_field("total_reachability_states"), Some(0));
    assert_eq!(bounded_work.candidates_certified, 1);
    assert_eq!(bounded_work.language_states, work.language_states);
    assert_eq!(
        bounded_work.upper_pruned_unknown_edges + bounded_work.upper_remaining_unknown_edges,
        2360
    );
    assert!(
        bounded_work.upper_pruned_unknown_edges > 0,
        "{bounded_work:?}"
    );
    assert!(
        bounded_work.pose.reached_pose_lanes < work.pose.reached_pose_lanes,
        "{bounded_work:?}"
    );
    eprintln!("strict upper-bound Jstris single identity: {bounded_work:?}");
    let (packed, packed_work) = super::symbolic_prefix_orders::with_packed_upper_target(|| {
        super::symbolic_prefix_orders::with_strict_prefix_order(run)
    });
    assert_parity(&upper, &packed);
    assert_eq!(packed.usize_field("total_build_order_nodes"), Some(0));
    assert_eq!(packed.usize_field("total_reachability_states"), Some(0));
    assert_eq!(packed_work.candidates_certified, 1);
    assert_eq!(packed_work.pose.lane_boards, bounded_work.pose.lane_boards);
    assert_eq!(packed_work.language_states, bounded_work.language_states);
    assert!(
        packed_work.pose.completed_relations < bounded_work.pose.completed_relations,
        "{packed_work:?}"
    );
    eprintln!("strict packed upper-bound Jstris single identity: {packed_work:?}");
}

#[test]
fn symbolic_prefix_p7_complete_candidate_and_queue_parity() {
    use clearra_rules::profile::builtin_rules::{jstris_180, no_kick, srs, srs_plus, srs_x};

    use super::symbolic_prefix_orders::{with_packed_upper_target, with_strict_prefix_order};

    // CTK3 ctk3_w0kCQBjwwAMPPAD37g (not the smaller parity probe or the
    // horizontal mirror): 16 initial cells, six locks, seven original draws.
    const BOARD: u64 = 0x3c0f03c0f;
    const PATTERNS: usize = 5_040;
    let _guard = product_guard();
    for (label, rule, hold, historic_count) in [
        ("Jstris 180 with hold", jstris_180(), true, Some(246)),
        ("SRS90 with hold", srs(), true, Some(245)),
        ("SRS+ with hold", srs_plus(), true, Some(246)),
        ("SRS-X with hold", srs_x(), true, Some(289)),
        ("No kick with hold", no_kick(), true, Some(175)),
        ("Jstris 180 without hold", jstris_180(), false, None),
    ] {
        let problem = ProblemCompiler::compile_scenario_pc(
            &PcScenarioQuery::new(
                PcScenarioBoard::standard_10(4, BOARD),
                PcQueueInput::standard_7_bag(),
                PieceWindow::new(6),
            )
            .with_rule(rule)
            .with_allow_hold(hold)
            .with_exact_pieces(Some(6))
            .with_supply_window_size(SupplyWindowSize::new(7))
            .with_count_policy(PcCountPolicy::CountUnique)
            .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
            .with_retained_trace_limit(0)
            .with_execution_policy(policy()),
        )
        .unwrap()
        .with_output_policy(SearchOutputPolicy::CoverageSummary);
        assert_eq!(problem.supply().hold_enabled(), hold, "{label}");
        assert!(!hold || problem.initial_hold().hold_empty(), "{label}");
        assert_eq!(
            problem
                .piece_source()
                .materialized_universe()
                .expect("full P7 draw universe")
                .pattern_count(),
            PATTERNS,
            "{label}"
        );

        let run = || {
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .unwrap_or_else(|error| panic!("{label}: {error:?}"))
        };
        let reference = run();
        let (packed, work) = with_packed_upper_target(|| with_strict_prefix_order(run));
        #[cfg(all(
            feature = "local-search-ab",
            feature = "parallel",
            not(target_family = "wasm")
        ))]
        {
            let (legacy, legacy_work) =
                super::symbolic_prefix_orders::with_legacy_upper(true, || {
                    with_packed_upper_target(|| with_strict_prefix_order(run))
                });
            assert_parity(&legacy, &packed);
            assert_eq!(legacy.solution_coverages(), packed.solution_coverages());
            assert_eq!(
                legacy_work.upper_pruned_unknown_edges,
                work.upper_pruned_unknown_edges
            );
            assert_eq!(
                legacy_work.upper_remaining_unknown_edges,
                work.upper_remaining_unknown_edges
            );
            assert_eq!(
                legacy_work.harddrop_certificates,
                work.harddrop_certificates
            );
            assert!(work.occupancy_plane_terms < legacy_work.occupancy_plane_terms);
        }
        let reference_rows = reference.solution_coverages();
        let packed_rows = packed.solution_coverages();
        assert_eq!(reference_rows.len(), packed_rows.len(), "{label}");
        assert!(!reference_rows.is_empty(), "{label}");
        if let Some(expected) = historic_count {
            assert_eq!(
                reference_rows.len(),
                expected,
                "{label}: historical hold-enabled PC count"
            );
        }
        // CoverageSummary deliberately omits display identities and probabilities.
        // Its exact rows retain the canonical identity sequence and the full
        // original-ordinal bitset for each identity, not a union/hash/sample.
        for (position, (left, right)) in reference_rows.iter().zip(packed_rows).enumerate() {
            assert_eq!(
                left.identity(),
                right.identity(),
                "{label} identity {position}"
            );
            assert_eq!(
                left.identity().initial_board_mask(),
                BOARD,
                "{label} origin"
            );
            assert_eq!(
                right.covered_patterns().pattern_count(),
                PATTERNS,
                "{label} queue universe"
            );
            assert_eq!(
                left.covered_patterns(),
                right.covered_patterns(),
                "{label} ordinals for identity {position}"
            );
        }
        assert_parity(&reference, &packed);
        let candidates = reference.usize_field("packing_candidate_count").unwrap();
        assert!(candidates >= reference_rows.len(), "{label}");
        assert_eq!(
            packed.usize_field("packing_candidate_count"),
            Some(candidates),
            "{label}"
        );
        assert_eq!(
            (work.candidates_attempted, work.candidates_certified),
            (candidates, candidates),
            "{label}: some Geometry candidates did not use the strict verifier"
        );
        assert!(
            reference.usize_field("total_build_order_nodes").unwrap() > 0,
            "{label}"
        );
        assert_eq!(
            packed.usize_field("total_build_order_nodes"),
            Some(0),
            "{label}"
        );
        assert_eq!(
            packed.usize_field("total_reachability_states"),
            Some(0),
            "{label}"
        );
        assert_eq!(
            packed.usize_field("coverage_pattern_count"),
            Some(PATTERNS),
            "{label}"
        );
        assert_eq!(packed.usize_field("workers_used"), Some(1), "{label}");
        eprintln!(
            "strict P7 {label}: candidates={candidates}, identities={}, queues={}/{PATTERNS}, identity_queue_pairs={}, packed_relations={}",
            packed_rows.len(),
            packed.usize_field("covered_pattern_count").unwrap(),
            packed_rows.iter().map(|row| u64::from(row.covered_patterns().count_ones())).sum::<u64>(),
            work.pose.completed_relations,
        );
    }
}

#[test]
fn symbolic_prefix_refuses_worker_and_partition_escape_before_execution() {
    use clearra_core_domain::execution_cancellation::ExecutionPartition;
    use clearra_pc_graph::request::WorkerPolicy;

    let problem = |execution_policy| {
        ProblemCompiler::compile_scenario_pc(
            &PcScenarioQuery::new(
                PcScenarioBoard::standard_10(2, 0),
                PcQueueInput::standard_7_bag(),
                PieceWindow::new(5),
            )
            .with_exact_pieces(Some(5))
            .with_execution_policy(execution_policy),
        )
        .unwrap()
        .with_output_policy(SearchOutputPolicy::CoverageSummary)
    };
    let partition = ExecutionPartition::new(0, 2).expect("one of two parts");
    let clamped = policy().with_workers(2).with_worker_hardware_limit(1);
    assert_eq!(clamped.worker_policy(), WorkerPolicy::Fixed(2));
    assert_eq!(clamped.workers(), 1);
    for (policy, control) in [
        (clamped, ExecutionControl::default()),
        (
            policy().with_workers(1),
            ExecutionControl::default().with_partition(partition),
        ),
    ] {
        let problem = problem(policy);
        for precomputed in [false, true] {
            let (result, work) = super::symbolic_prefix_orders::with_strict_prefix_order(|| {
                if precomputed {
                    // No candidate callback runs for an empty stream. Scope
                    // admission must happen before that stream reports success.
                    WasmCpuSearchBackend::execute_complete_precomputed_geometry_candidates_with_control(
                        &problem,
                        &[],
                        &control,
                    )
                } else {
                    WasmCpuSearchBackend::execute_with_control(&problem, &control)
                }
            });
            assert!(
                matches!(
                    result,
                    Err(crate::WasmCpuSearchError::Unsupported {
                        reason: "symbolic_prefix_requires_one_explicit_worker_and_partition"
                    })
                ),
                "strict experiment cannot delegate: precomputed={precomputed}, result={result:?}"
            );
            assert_eq!(work.candidates_attempted, 0);
        }
    }
}

#[test]
fn structural_certificates_match_every_reachable_subset_and_piece_word() {
    let o = 3_u64 | (3_u64 << 10);
    let fixtures = [
        fixture(1, &[(PieceKind::I, 15)]),
        fixture(2, &[(PieceKind::O, o), (PieceKind::O, o << 2)]),
        fixture(4, &[(PieceKind::O, o), (PieceKind::O, o << 20)]),
        fixture(
            4,
            &[
                (PieceKind::O, o),
                (PieceKind::O, o << 2),
                (PieceKind::O, o << 20),
                (PieceKind::O, o << 22),
            ],
        ),
    ];
    let mut checked_states = 0;
    let mut intermediate_clears = 0;
    for (catalog, candidate) in fixtures {
        let certificate =
            placement_poset::compile(&catalog, &candidate).expect("certifiable hard-drop fixture");
        let mut language = PieceOrderLanguageCache::default();
        let compiled = language
            .compile_poset_language(certificate.pieces(), certificate.predecessors(), 4096)
            .expect("valid language")
            .expect("bounded certificate language");
        let words = language_words(&language, compiled.root);
        for profile in [
            KickTableProfileId::Srs90,
            KickTableProfileId::SrsPlus,
            KickTableProfileId::SrsX,
            KickTableProfileId::Jstris180,
            KickTableProfileId::NoKick,
        ] {
            let transitions = physical_transitions(&catalog, &candidate, profile);
            let mut reachable = vec![false; transitions.len()];
            reachable[0] = true;
            for subset in 0..transitions.len() {
                if !reachable[subset] {
                    continue;
                }
                checked_states += 1;
                let mut expected = 0_u16;
                for (i, &required) in certificate.predecessors().iter().enumerate() {
                    if subset & (1 << i) == 0 && required & !(subset as u16) == 0 {
                        expected |= 1 << i;
                    }
                }
                assert_eq!(
                    transitions[subset], expected,
                    "profile={profile:?}, candidate={:?}, subset={subset:#x}",
                    candidate.identity
                );
                for i in 0..candidate.row_ids().len() {
                    if transitions[subset] & (1 << i) != 0 {
                        let child = subset | (1 << i);
                        reachable[child] = true;
                        if child + 1 != transitions.len()
                            && subset_state(&catalog, &candidate, subset).1
                                != subset_state(&catalog, &candidate, child).1
                        {
                            intermediate_clears += 1;
                        }
                    }
                }
            }
            assert!(reachable[transitions.len() - 1]);
            assert_eq!(
                words,
                operation_words(certificate.pieces(), &transitions),
                "profile={profile:?}"
            );
        }
    }
    assert!(checked_states > 50);
    assert!(intermediate_clears > 0, "must exercise changing row frames");
}

#[test]
fn disjunctive_grounding_is_unknown_and_never_an_infeasibility_prune() {
    let o = 3_u64 | (3_u64 << 10);
    let (catalog, candidate) = fixture(
        3,
        &[
            (PieceKind::O, o),
            (PieceKind::O, o << 2),
            (PieceKind::I, 15 << 20),
        ],
    );
    let pieces: Vec<_> = candidate
        .row_ids()
        .iter()
        .map(|&row| catalog.skeleton(row).piece)
        .collect();
    let transitions = physical_transitions(&catalog, &candidate, KickTableProfileId::Jstris180);
    let words = operation_words(&pieces, &transitions);
    assert!(words.contains(&vec![PieceKind::O, PieceKind::I, PieceKind::O]));
    assert!(words.contains(&vec![PieceKind::O, PieceKind::O, PieceKind::I]));
    assert!(!words.contains(&vec![PieceKind::I, PieceKind::O, PieceKind::O]));
    assert!(
        placement_poset::compile(&catalog, &candidate).is_none(),
        "either O can support I, which cannot be encoded by one predecessor conjunction"
    );
    let incomplete = GeometryCandidate::from_rows(&catalog, 0, &candidate.row_ids()[..2]).unwrap();
    assert!(placement_poset::compile(&catalog, &incomplete).is_none());

    // O can clear the roof above I, but the Jstris180 reference traversal also
    // admits I before O. An occupied vertical corridor does not prove exact
    // movement impossible: this hard-drop-only certificate must fall back.
    let (roof_catalog, roof_candidate) = fixture(2, &[(PieceKind::O, o), (PieceKind::I, 0x3c)]);
    let roof_pieces: Vec<_> = roof_candidate
        .row_ids()
        .iter()
        .map(|&row| roof_catalog.skeleton(row).piece)
        .collect();
    let roof_transitions = physical_transitions(
        &roof_catalog,
        &roof_candidate,
        KickTableProfileId::Jstris180,
    );
    let roof_words = operation_words(&roof_pieces, &roof_transitions);
    assert!(!roof_words.is_empty());
    assert!(roof_words.contains(&vec![PieceKind::O, PieceKind::I]));
    assert!(roof_words.contains(&vec![PieceKind::I, PieceKind::O]));
    assert!(placement_poset::compile(&roof_catalog, &roof_candidate).is_none());
}

#[test]
fn cached_positive_interval_certifies_roof_orders_without_new_primitive_states() {
    let ((primitive_states, guards, word_count), metrics) =
        placement_dependency::with_mode(true, || {
            // Compile the catalog inside this scope so observations are bound to
            // its actual placement-condition owner, not a synthetic cache entry.
            let o = 3_u64 | (3_u64 << 10);
            let (catalog, candidate) = fixture(2, &[(PieceKind::O, o), (PieceKind::I, 0x3c)]);
            let profile = KickTableProfileId::Jstris180;
            let board = catalog.initial_board();
            let frame = LocalRelationRowFrame::new(catalog.height(), 0).unwrap();
            let mut workspace = ReachabilityWorkspace::default();
            workspace.configure(candidate.row_ids().len());
            workspace.configure_kick_profile(profile, false);
            workspace.configure_conditioned_boolean_shortcuts(true);

            let cold = placement_poset::compile_with_guard(
                &catalog,
                &candidate,
                |piece, realization, deleted, minimum, maximum| {
                    workspace.proves_lock_for_interval(
                        &catalog,
                        minimum,
                        maximum,
                        piece,
                        profile,
                        realization,
                        LocalRelationRowFrame::new(catalog.height(), deleted),
                    )
                },
            );
            assert!(
                cold.is_none(),
                "a cold interval cache supplies no certificate"
            );
            assert_eq!(
                workspace.generated_state_count(),
                0,
                "a certificate miss must not start BFS"
            );

            let i_row = catalog.skeleton_id(PieceKind::I, 0x3c).unwrap();
            let mut observed_positive = false;
            for realization in catalog.instantiations(i_row, 0) {
                let reachable = workspace.lock_reachable_instantiated_in_frame(
                    &catalog,
                    board,
                    PieceKind::I,
                    realization,
                    Some(frame),
                );
                if reachable
                    && workspace.proves_lock_for_interval(
                        &catalog,
                        board,
                        board,
                        PieceKind::I,
                        profile,
                        realization,
                        Some(frame),
                    )
                {
                    observed_positive = true;
                    break;
                }
            }
            let primitive_states = workspace.generated_state_count();
            assert!(
                primitive_states > 0,
                "warmup must perform a real movement search"
            );
            assert!(
                observed_positive,
                "normal reachability must retain an actual positive guard"
            );

            let mut positive_interval_queries = 0;
            let certificate = placement_poset::compile_with_guard(
                &catalog,
                &candidate,
                |piece, realization, deleted, minimum, maximum| {
                    let proved = workspace.proves_lock_for_interval(
                        &catalog,
                        minimum,
                        maximum,
                        piece,
                        profile,
                        realization,
                        LocalRelationRowFrame::new(catalog.height(), deleted),
                    );
                    if proved {
                        positive_interval_queries += 1;
                        assert_eq!(piece, PieceKind::I);
                        assert_eq!(deleted, 0);
                        // Placing O clears the upper original row, so the only D=0
                        // ideal before I is empty. Tightening the interval must keep
                        // O out of its maximum while retaining the real cache proof.
                        assert_eq!((minimum, maximum), (board, board));
                    }
                    proved
                },
            )
            .expect("the observed target proof certifies the exact roof language");
            assert_eq!(workspace.generated_state_count(), primitive_states,
            "graph certification and interval lookup must not launch another primitive traversal");
            assert!(positive_interval_queries > 0);
            assert!(certificate.interval_guard_count() > 0);

            let transitions = physical_transitions(&catalog, &candidate, profile);
            let oracle = operation_words(certificate.pieces(), &transitions);
            assert_eq!(
                oracle,
                BTreeSet::from([
                    vec![PieceKind::I, PieceKind::O],
                    vec![PieceKind::O, PieceKind::I],
                ])
            );
            let mut language = PieceOrderLanguageCache::default();
            let direct = language
                .compile_poset_language(certificate.pieces(), certificate.predecessors(), 64)
                .unwrap()
                .expect("finite two-piece language");
            assert_eq!(language_words(&language, direct.root), oracle);
            assert_eq!(workspace.generated_state_count(), primitive_states);
            (
                primitive_states,
                certificate.interval_guard_count(),
                oracle.len(),
            )
        });
    assert!(
        metrics.generated_relations > 0,
        "a real observed relation was retained"
    );
    eprintln!("cached interval graph fixture: warmup_primitive_states={primitive_states}, observed_interval_guards={guards}, exact_piece_words={word_count}, certification_primitive_states=0");
}

fn assert_parity(a: &CoreExecutionResult, b: &CoreExecutionResult) {
    assert_eq!(
        a.normalized_solution_identities(),
        b.normalized_solution_identities()
    );
    assert_eq!(a.normalized_solution_keys(), b.normalized_solution_keys());
    assert_eq!(a.coverage_pattern_words(), b.coverage_pattern_words());
    assert_eq!(a.solution_coverages(), b.solution_coverages());
    assert_eq!(
        a.normalized_solution_coverages(),
        b.normalized_solution_coverages()
    );
    assert_eq!(a.solution_probabilities(), b.solution_probabilities());
    for key in [
        "normalized_unique_solution_count",
        "normalized_solution_set_hash",
        "coverage_pattern_count",
        "covered_pattern_count",
        "coverage_probability",
        "materialized_probability_mass",
        "build_variant_count",
        "build_variant_count_exact",
    ] {
        assert_eq!(a.field(key), b.field(key), "{key}");
    }
    for result in [a, b] {
        for key in [
            "count_complete",
            "probability_complete",
            "solution_probability_complete",
        ] {
            assert_eq!(result.bool_field(key), Some(true), "{key}");
        }
        assert_eq!(result.bool_field("resource_truncated"), Some(false));
    }
}

fn policy() -> PcExecutionPolicy {
    PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_workers(1)
        .with_exact_legal_board_enabled(false)
        .with_conditioned_reachability_enabled(false)
        .with_tablebase_requested(false)
        .with_max_memory_mib(Some(256))
}

#[test]
fn direct_structural_language_preserves_every_solution_queue_and_probability() {
    let _guard = product_guard();
    for (label, height, board, piece_count, hold, held, pattern_count, require_bypass) in [
        ("singleton I", 1, 0x3f0, 1, false, None, 7, true),
        (
            "repeated O with held O",
            2,
            0xfc3f0,
            2,
            true,
            Some(PieceKind::O),
            42,
            true,
        ),
        (
            "roof fallback hold off",
            2,
            0xff3c0,
            2,
            false,
            None,
            42,
            false,
        ),
        (
            "roof fallback empty hold",
            2,
            0xff3c0,
            2,
            true,
            None,
            42,
            false,
        ),
        (
            "roof fallback held O",
            2,
            0xff3c0,
            2,
            true,
            Some(PieceKind::O),
            42,
            false,
        ),
        (
            "mixed four-piece bag",
            4,
            0xfc3f_0fc3_f0,
            4,
            false,
            None,
            840,
            false,
        ),
    ] {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(height, board),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(piece_count),
        )
        .with_allow_hold(hold)
        .with_hold_piece(held)
        .with_exact_pieces(Some(piece_count))
        .with_supply_window_size(SupplyWindowSize::new(piece_count))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
        .with_retained_trace_limit(0)
        .with_execution_policy(policy());
        let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
        let run = || {
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .unwrap()
        };
        let ((baseline, off), _) =
            placement_dependency::with_mode(true, || structural_order::with_mode(false, run));
        let ((treatment, _trace_metrics), _) =
            placement_dependency::with_mode(true, || structural_order::with_mode(true, run));
        assert_parity(&baseline, &treatment);
        assert_eq!(off.skipped_build_graphs, 0);

        // Trace output retains the first representative even when its replay
        // budget is zero. Exercise the no-trace entry point explicitly while
        // retaining Include's real per-identity coverage evidence.
        let summary_problem = problem
            .clone()
            .with_output_policy(SearchOutputPolicy::CoverageSummary);
        assert!(!summary_problem
            .output_policy()
            .retains_representative_trace());
        let run_summary = || {
            WasmCpuSearchBackend::execute_with_control(
                &summary_problem,
                &ExecutionControl::default(),
            )
            .unwrap()
        };
        let ((summary_baseline, summary_off), _) = placement_dependency::with_mode(true, || {
            structural_order::with_mode(false, run_summary)
        });
        let ((summary_treatment, on), _) = placement_dependency::with_mode(true, || {
            structural_order::with_mode(true, run_summary)
        });
        assert_parity(&summary_baseline, &summary_treatment);
        assert_eq!(summary_off.skipped_build_graphs, 0);
        assert_eq!(
            summary_treatment.coverage_pattern_words(),
            baseline.coverage_pattern_words()
        );
        assert_eq!(
            summary_treatment.solution_coverages(),
            baseline.solution_coverages()
        );
        assert_eq!(
            summary_treatment.normalized_solution_coverages(),
            baseline.normalized_solution_coverages()
        );
        assert!(
            summary_treatment
                .normalized_solution_identities()
                .is_empty(),
            "CoverageSummary deliberately hides identities"
        );
        assert!(
            summary_treatment.solution_probabilities().is_empty(),
            "CoverageSummary has no materialized identity report surface"
        );
        assert!(
            !baseline.normalized_solution_identities().is_empty(),
            "must compare actual identities"
        );
        let reports = crate::solution_probability::probability_reports(
            baseline.normalized_solution_identities(),
            summary_treatment.solution_coverages(),
            problem
                .piece_source()
                .materialized_universe()
                .unwrap()
                .weights(),
            summary_treatment.bool_field("solution_probability_complete") == Some(true),
        );
        assert!(
            !reports.is_empty(),
            "per-solution probability proof must not be vacuous"
        );
        assert_eq!(reports, baseline.solution_probabilities());
        if require_bypass {
            assert!(
                on.skipped_build_graphs > 0,
                "{label}: must exercise structural producer"
            );
        }
        if label == "repeated O with held O" {
            assert!(
                on.exchangeable_operations_removed > 0,
                "same-kind twins must fold"
            );
        }
        assert_eq!(
            treatment.usize_field("coverage_pattern_count"),
            Some(pattern_count)
        );
        assert!(!treatment.solution_probabilities().is_empty());
        if label == "mixed four-piece bag" {
            assert_eq!(
                baseline.usize_field("normalized_unique_solution_count"),
                Some(135)
            );
            assert_eq!(baseline.usize_field("covered_pattern_count"), Some(173));
            assert_eq!(reports.len(), 135, "compare every solution's probability");
        }
        eprintln!(
            "structural graph fixture: {label}; solutions={}, covered={}/{pattern_count}, skipped_build_graphs={}, residual_states={}, twins_removed={}, cached_interval_guards={}, summary_build_nodes={}->{}, summary_reachability_states={}->{}",
            baseline.normalized_solution_identities().len(),
            baseline.field("covered_pattern_count").unwrap(),
            on.skipped_build_graphs,
            on.language_residual_states,
            on.exchangeable_operations_removed,
            on.cached_interval_guards,
            summary_baseline.field("total_build_order_nodes").unwrap(),
            summary_treatment.field("total_build_order_nodes").unwrap(),
            summary_baseline.field("total_reachability_states").unwrap(),
            summary_treatment.field("total_reachability_states").unwrap(),
        );
    }
}

#[test]
fn count_all_same_current_and_held_piece_keeps_the_operation_graph() {
    let _guard = product_guard();
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_allow_hold(true)
    .with_hold_piece(Some(PieceKind::I))
    .with_exact_pieces(Some(1))
    .with_count_policy(PcCountPolicy::CountAll)
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
    .with_retained_trace_limit(0)
    .with_execution_policy(policy());
    let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
    let run = || {
        WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default()).unwrap()
    };
    let (baseline, _) = structural_order::with_mode(false, run);
    let (treatment, metrics) = structural_order::with_conditional_order_mode(true, || {
        structural_order::with_ordered_kick_interval_mode(true, || {
            structural_order::with_first_kick_interval_mode(true, || {
                structural_order::with_movement_clear_mode(true, || {
                    structural_order::with_mode(true, run)
                })
            })
        })
    });
    assert_parity(&baseline, &treatment);
    assert_eq!(metrics.skipped_build_graphs, 0);
    assert_eq!(metrics.movement_clear, Default::default());
    assert_eq!(metrics.first_kick_interval, Default::default());
    assert_eq!(metrics.ordered_kick_interval, Default::default());
    assert_eq!(metrics.conditional_order, Default::default());
    assert_eq!(treatment.coverage_pattern_words(), &[1]);
    assert!(
        treatment
            .field("build_variant_count")
            .unwrap()
            .parse::<u128>()
            .unwrap()
            > 1
    );
}

#[test]
fn ordered_kick_certificate_reduces_primitive_search_for_a_later_raw_kick() {
    use clearra_core_domain::{
        piece::rotation::RotationState,
        solution::normalized_tiling_solution::{PiecePlacementMask, StandardBoard64TilingIdentity},
    };
    use clearra_coverage::pattern::pattern_id::PatternId;
    use clearra_rules::profile::builtin_rules::srs;

    let _guard = product_guard();
    use PieceKind::{L, T};
    // Preserve the exact later-kick roof at original row one. L leaves the
    // T Right entry column open, then fills it after T clears the bottom row.
    let placements = [(T, 0x201c), (L, 0x01c0_1000)];
    let (catalog, candidate) = fixture(3, &placements);
    let board = catalog.initial_board();
    assert_eq!(board, 0x3e3f_cfe3);
    let profile = KickTableProfileId::Srs90;
    let pieces: Vec<_> = candidate
        .row_ids()
        .iter()
        .map(|&id| catalog.skeleton(id).piece)
        .collect();
    let transitions = physical_transitions(&catalog, &candidate, profile);
    let words = operation_words(&pieces, &transitions);
    assert!(
        words.contains(&vec![T, L]),
        "the independent oracle can finish T then L"
    );

    let target = catalog
        .instantiations(catalog.skeleton_id(T, 0x201c).unwrap(), 0)
        .find(|pose| pose.rotation == RotationState::Zero && pose.x == 2 && pose.lock_y == 0)
        .unwrap();
    let mut workspace = ReachabilityWorkspace::default();
    workspace.configure_kick_profile(profile, false);
    let frame = LocalRelationRowFrame::new(3, 0);
    assert!(!workspace.lock_harddrop_reachable_instantiated(10, board, target));
    assert!(!workspace
        .try_prove_first_kick_lock_interval(&catalog, board, board, T, profile, target, frame,));
    assert!(workspace
        .try_prove_ordered_kick_lock_interval(&catalog, board, board, T, profile, target, frame,));
    assert_eq!(workspace.generated_state_count(), 0);

    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(3, board),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(2),
    )
    .with_rule(srs())
    .with_allow_hold(false)
    .with_exact_pieces(Some(2))
    .with_supply_window_size(SupplyWindowSize::new(2))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
    .with_retained_trace_limit(0)
    .with_execution_policy(policy());
    let problem = ProblemCompiler::compile_scenario_pc(&query)
        .unwrap()
        .with_output_policy(SearchOutputPolicy::CoverageSummary);
    let run = |first, ordered| {
        structural_order::with_ordered_kick_interval_mode(ordered, || {
            structural_order::with_first_kick_interval_mode(first, || {
                structural_order::with_conditional_order_mode(false, || {
                    structural_order::with_movement_clear_mode(false, || {
                        structural_order::with_mode(false, || {
                            placement_dependency::with_mode(false, || {
                                WasmCpuSearchBackend::execute_with_control(
                                    &problem,
                                    &ExecutionControl::default(),
                                )
                                .unwrap()
                            })
                            .0
                        })
                    })
                })
            })
        })
    };
    let (baseline, disabled) = run(false, false);
    let (first_only, first) = run(true, false);
    let (treatment, enabled) = run(false, true);
    assert_parity(&baseline, &first_only);
    assert_parity(&baseline, &treatment);
    assert_eq!(disabled.ordered_kick_interval, Default::default());
    assert_eq!(first.ordered_kick_interval, Default::default());
    assert_eq!(enabled.first_kick_interval, Default::default());
    let later = enabled.ordered_kick_interval;
    assert!(later.attempts >= later.proofs);
    assert!(later.proofs >= later.selected_locks && later.selected_locks > 0);
    assert!(
        treatment.usize_field("total_reachability_states").unwrap()
            < baseline.usize_field("total_reachability_states").unwrap(),
        "ordered-kick selection must replace actual primitive search",
    );

    let intended = StandardBoard64TilingIdentity::from_placements(
        board,
        placements.map(|(piece, cells)| PiecePlacementMask::new(piece, cells)),
    )
    .unwrap();
    let coverage = treatment
        .solution_coverages()
        .iter()
        .find(|entry| entry.identity() == intended)
        .unwrap()
        .covered_patterns();
    let universe = problem.piece_source().materialized_universe().unwrap();
    assert_eq!(universe.pattern_count(), 42);
    for ordinal in 0..universe.pattern_count() {
        assert_eq!(
            coverage.contains(PatternId::new(ordinal)),
            words.contains::<[PieceKind]>(universe.sequence_at(ordinal).as_ref()),
            "later-kick target queue {ordinal}",
        );
    }
}

#[test]
fn clear_can_close_a_translation_entry_and_requires_a_movement_predecessor() {
    use clearra_core_domain::piece::rotation::RotationState;

    let t_cells = 0x201c00;
    let l_cells = 0xc02008;
    let (catalog, candidate) = fixture(3, &[(PieceKind::T, t_cells), (PieceKind::L, l_cells)]);
    let initial = catalog.initial_board();
    assert_eq!(initial, 0x3f1fc3f7);
    let pieces: Vec<_> = candidate
        .row_ids()
        .iter()
        .map(|&row_id| catalog.skeleton(row_id).piece)
        .collect();
    let l_operation = pieces
        .iter()
        .position(|&piece| piece == PieceKind::L)
        .unwrap();
    let (after_l, deleted) = subset_state(&catalog, &candidate, 1 << l_operation);
    assert_eq!((after_l, deleted), (0xff7f8, 1));
    let t_row = catalog.skeleton_id(PieceKind::T, t_cells).unwrap();
    let before_clear = catalog
        .instantiations(t_row, 0)
        .find(|lock| {
            lock.rotation == RotationState::Zero
                && lock.x == 0
                && lock.lock_y == 1
                && lock.lock_mask == t_cells
        })
        .unwrap();
    let after_clear = catalog
        .instantiations(t_row, deleted)
        .find(|lock| {
            lock.rotation == RotationState::Zero
                && lock.x == 0
                && lock.lock_y == 0
                && lock.lock_mask == 0x807
        })
        .unwrap();
    assert!(placement_poset::compile(&catalog, &candidate).is_none());

    // At D=0, L alone would complete original row zero, so that undeleted
    // row forces L out of the interval maximum. T can enter from x=1 and
    // slide left. L first clears that row and leaves a one-cell top entrance
    // above T's three-cell base; the fixed-orientation proof is now Unknown.
    // The full reference oracle also rejects T there under every profile.
    //
    // The exact language {TL} is representable by the ordinary poset T < L.
    // What is missing is inference of this necessary movement/clear edge,
    // beyond the compiler's current grounding and row-contributor facts.
    // A positive-only certificate miss must never invent that negative fact.
    for profile in [
        KickTableProfileId::Srs90,
        KickTableProfileId::SrsPlus,
        KickTableProfileId::SrsX,
        KickTableProfileId::Jstris180,
        KickTableProfileId::NoKick,
    ] {
        let transitions = physical_transitions(&catalog, &candidate, profile);
        assert_eq!(
            operation_words(&pieces, &transitions),
            BTreeSet::from([vec![PieceKind::T, PieceKind::L]]),
            "profile={profile:?}: exactly T before L"
        );
        let anchors = reference_spawn_lock_anchors(10, 3, after_l, PieceKind::T, profile).unwrap();
        assert_eq!(
            anchors[RotationState::Zero.quarter_turns() as usize] & 1,
            0,
            "profile={profile:?}: T cannot enter after L's clear"
        );

        let workspace = ReachabilityWorkspace::default();
        structural_order::with_translation_interval_mode(true, || {
            let initial_attempt = workspace
                .try_translation_lock_interval(
                    &catalog,
                    initial,
                    initial,
                    PieceKind::T,
                    profile,
                    before_clear,
                    LocalRelationRowFrame::new(3, 0),
                )
                .unwrap();
            assert!(initial_attempt.0);
            assert!(initial_attempt.1 > 0);
            let after_l_attempt = workspace
                .try_translation_lock_interval(
                    &catalog,
                    after_l,
                    after_l,
                    PieceKind::T,
                    profile,
                    after_clear,
                    LocalRelationRowFrame::new(3, deleted),
                )
                .unwrap();
            assert!(!after_l_attempt.0);
            assert!(after_l_attempt.1 > 0);

            let mut positive_t_guards = 0;
            let certificate = placement_poset::compile_with_guard(
                &catalog,
                &candidate,
                |piece, realization, deleted, minimum, maximum| {
                    let proven = workspace
                        .try_translation_lock_interval(
                            &catalog,
                            minimum,
                            maximum,
                            piece,
                            profile,
                            realization,
                            LocalRelationRowFrame::new(3, deleted),
                        )
                        .is_some_and(|(proven, _)| proven);
                    if proven && piece == PieceKind::T {
                        positive_t_guards += 1;
                    }
                    proven
                },
            );
            assert!(positive_t_guards > 0);
            assert!(certificate.is_none());
        });
        assert_eq!(
            workspace.generated_state_count(),
            0,
            "cold certificates must not run primitive traversal"
        );
    }
}

#[test]
fn movement_clear_certificate_matches_the_t_l_oracle_and_saves_primitive_search() {
    use clearra_rules::profile::builtin_rules::{jstris_180, no_kick, srs, srs_plus, srs_x};
    use placement_poset::MovementProof;

    let _guard = product_guard();
    let (catalog, candidate) = fixture(3, &[(PieceKind::T, 0x201c00), (PieceKind::L, 0xc02008)]);
    for (profile, rule) in [
        (KickTableProfileId::Srs90, srs()),
        (KickTableProfileId::SrsPlus, srs_plus()),
        (KickTableProfileId::SrsX, srs_x()),
        (KickTableProfileId::Jstris180, jstris_180()),
        (KickTableProfileId::NoKick, no_kick()),
    ] {
        let mut workspace = ReachabilityWorkspace::default();
        workspace.configure_kick_profile(profile, false);
        let mut negative_proofs = 0;
        let certificate = structural_order::with_translation_interval_mode(true, || {
            placement_poset::compile_with_movement_guard(&catalog, &candidate, true, |query| {
                let frame = LocalRelationRowFrame::new(3, query.deleted);
                match query.proof {
                    MovementProof::Reachable { maximum } => workspace
                        .try_translation_lock_interval(
                            &catalog,
                            query.minimum,
                            maximum,
                            query.piece,
                            profile,
                            query.realization,
                            frame,
                        )
                        .is_some_and(|(proved, _)| proved),
                    MovementProof::Closed => {
                        let proved = workspace.try_prove_lock_pose_unreachable_at_minimum(
                            &catalog,
                            query.minimum,
                            query.piece,
                            profile,
                            query.realization,
                            frame,
                        );
                        negative_proofs += usize::from(proved);
                        proved
                    }
                }
            })
        })
        .unwrap_or_else(|| panic!("{profile:?}: movement/clear evidence must close T/L"));
        let expected = operation_words(
            certificate.pieces(),
            &physical_transitions(&catalog, &candidate, profile),
        );
        let mut cache = PieceOrderLanguageCache::default();
        let language = cache
            .compile_poset_language(certificate.pieces(), certificate.predecessors(), 4096)
            .unwrap()
            .unwrap();
        assert_eq!(
            language_words(&cache, language.root),
            expected,
            "{profile:?}"
        );
        assert_eq!(expected, BTreeSet::from([vec![PieceKind::T, PieceKind::L]]));
        assert!(negative_proofs > 0, "{profile:?}");
        assert_eq!(workspace.generated_state_count(), 0, "{profile:?}");

        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(3, catalog.initial_board()),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(2),
        )
        .with_rule(rule)
        .with_allow_hold(false)
        .with_exact_pieces(Some(2))
        .with_supply_window_size(SupplyWindowSize::new(2))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
        .with_retained_trace_limit(0)
        .with_execution_policy(policy());
        let problem = ProblemCompiler::compile_scenario_pc(&query)
            .unwrap()
            .with_output_policy(SearchOutputPolicy::CoverageSummary);
        let run = |movement, translation| {
            structural_order::with_movement_clear_mode(movement, || {
                structural_order::with_translation_interval_mode(translation, || {
                    structural_order::with_mode(true, || {
                        placement_dependency::with_mode(false, || {
                            WasmCpuSearchBackend::execute_with_control(
                                &problem,
                                &ExecutionControl::default(),
                            )
                            .unwrap()
                        })
                        .0
                    })
                })
            })
        };
        let (baseline, old) = run(false, false);
        // Without a positive interval the full poset remains Unknown, but
        // the closed-pose proof must still avoid a real exact graph query.
        let (partial, fallback) = run(true, false);
        let (completed, new) = run(true, true);
        assert_parity(&baseline, &partial);
        assert_parity(&baseline, &completed);
        assert_eq!(old.movement_clear, Default::default());
        assert!(
            fallback.movement_clear.reused_closed_locks > 0,
            "{profile:?}"
        );
        assert!(
            new.skipped_build_graphs > fallback.skipped_build_graphs,
            "{profile:?}"
        );
        let states =
            |result: &CoreExecutionResult| result.usize_field("total_reachability_states").unwrap();
        assert!(states(&partial) < states(&baseline), "{profile:?}");
        assert!(states(&completed) < states(&baseline), "{profile:?}");
    }
}

#[test]
fn first_kick_certificate_saves_primitive_search_without_collapsing_support_or() {
    use clearra_core_domain::{
        piece::rotation::RotationState,
        solution::normalized_tiling_solution::{PiecePlacementMask, StandardBoard64TilingIdentity},
    };
    use clearra_coverage::pattern::pattern_id::PatternId;
    use clearra_rules::profile::builtin_rules::srs;

    let _guard = product_guard();
    use PieceKind::{I, O, T};
    let placements = [(I, 0x00f0_0000), (O, 0x0c03), (T, 0x7008)];
    let (catalog, candidate) = fixture(3, &placements);
    let board = catalog.initial_board();
    assert_eq!(board, 0x3f0f_83f4);
    let profile = KickTableProfileId::Srs90;
    let pieces: Vec<_> = candidate
        .row_ids()
        .iter()
        .map(|&id| catalog.skeleton(id).piece)
        .collect();
    let bit = |piece| 1_usize << pieces.iter().position(|&kind| kind == piece).unwrap();
    let transitions = physical_transitions(&catalog, &candidate, profile);
    assert_eq!(usize::from(transitions[0]) & bit(I), 0);
    for supporter in [O, T] {
        assert_ne!(usize::from(transitions[bit(supporter)]) & bit(I), 0);
    }
    let words = operation_words(&pieces, &transitions);
    assert_eq!(
        words,
        BTreeSet::from([vec![O, I, T], vec![O, T, I], vec![T, I, O], vec![T, O, I]])
    );
    assert!(
        placement_poset::compile(&catalog, &candidate).is_none(),
        "I requires O or T; do not force either as its mandatory predecessor"
    );

    let (maximum, deleted) = subset_state(&catalog, &candidate, bit(O));
    assert_eq!((maximum, deleted), (board | 0x0c03, 0));
    let target = catalog
        .instantiations(catalog.skeleton_id(T, 0x7008).unwrap(), deleted)
        .find(|lock| lock.rotation == RotationState::Two && lock.lock_mask == 0x7008)
        .unwrap();
    let mut workspace = ReachabilityWorkspace::default();
    workspace.configure_kick_profile(profile, false);
    for occupancy in [board, maximum] {
        assert!(!workspace.lock_harddrop_reachable_instantiated(10, occupancy, target));
    }
    assert!(workspace.try_prove_first_kick_lock_interval(
        &catalog,
        board,
        maximum,
        T,
        profile,
        target,
        LocalRelationRowFrame::new(3, 0),
    ));
    assert_eq!(workspace.generated_state_count(), 0);

    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(3, board),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(3),
    )
    .with_rule(srs())
    .with_allow_hold(false)
    .with_exact_pieces(Some(3))
    .with_supply_window_size(SupplyWindowSize::new(3))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
    .with_retained_trace_limit(0)
    .with_execution_policy(policy());
    let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
    let run = |problem: &clearra_problem::SearchProblem, first_kick, conditional| {
        structural_order::with_conditional_order_mode(conditional, || {
            structural_order::with_first_kick_interval_mode(first_kick, || {
                structural_order::with_movement_clear_mode(false, || {
                    structural_order::with_mode(false, || {
                        placement_dependency::with_mode(false, || {
                            WasmCpuSearchBackend::execute_with_control(
                                problem,
                                &ExecutionControl::default(),
                            )
                            .unwrap()
                        })
                        .0
                    })
                })
            })
        })
    };
    let (ordinary, _) = run(&problem, false, false);
    let intended = StandardBoard64TilingIdentity::from_placements(
        board,
        placements.map(|(piece, cells)| PiecePlacementMask::new(piece, cells)),
    )
    .unwrap();
    assert!(ordinary
        .normalized_solution_identities()
        .contains(&intended));
    let summary = problem
        .clone()
        .with_output_policy(SearchOutputPolicy::CoverageSummary);
    let (baseline, disabled) = run(&summary, false, false);
    let (candidate, enabled) = run(&summary, true, false);
    let (conditional, conditional_metrics) = run(&summary, false, true);
    assert_parity(&baseline, &candidate);
    assert_parity(&baseline, &conditional);
    assert_eq!(
        candidate.normalized_solution_coverages(),
        ordinary.normalized_solution_coverages()
    );
    let universe = problem.piece_source().materialized_universe().unwrap();
    assert_eq!(universe.pattern_count(), 210);
    let solution = candidate
        .solution_coverages()
        .iter()
        .find(|coverage| coverage.identity() == intended)
        .unwrap();
    for index in 0..universe.pattern_count() {
        assert_eq!(
            solution.covered_patterns().contains(PatternId::new(index)),
            words.contains::<[PieceKind]>(universe.sequence_at(index).as_ref()),
            "OR-support candidate queue {index}"
        );
    }
    let reports = crate::solution_probability::probability_reports(
        ordinary.normalized_solution_identities(),
        candidate.solution_coverages(),
        universe.weights(),
        true,
    );
    assert!(!reports.is_empty());
    assert_eq!(reports, ordinary.solution_probabilities());
    assert_eq!(disabled.first_kick_interval, Default::default());
    assert_eq!(disabled.conditional_order, Default::default());
    assert_eq!(enabled.conditional_order, Default::default());
    assert_eq!(conditional_metrics.first_kick_interval, Default::default());
    assert_eq!(enabled.skipped_build_graphs, 0);
    assert!(enabled.first_kick_interval.selected_locks > 0);
    assert!(enabled.first_kick_interval.attempts >= enabled.first_kick_interval.proofs);
    assert!(enabled.first_kick_interval.proofs >= enabled.first_kick_interval.selected_locks);
    assert!(
        candidate.usize_field("total_reachability_states").unwrap()
            < baseline.usize_field("total_reachability_states").unwrap(),
        "proof selection must reduce primitive traversal, not just report a hit",
    );
    let order = conditional_metrics.conditional_order;
    assert!(order.attempted_candidates > 0);
    assert!(order.checked_transitions > 0);
    assert!(order.checked_transitions <= order.attempted_candidates * 256);
    assert!(order.residual_states <= order.certified_candidates * 64);
    if order.certified_candidates != 0 {
        assert!(order.graph_bypasses > 0);
        assert!(
            conditional
                .usize_field("total_reachability_states")
                .unwrap()
                < baseline.usize_field("total_reachability_states").unwrap(),
            "completed conditional language must replace physical graph traversal",
        );
    } else {
        assert_eq!(order.graph_bypasses, 0);
        assert_eq!(
            order.unknown_candidates + order.budget_candidates,
            order.attempted_candidates
        );
    }
}

#[test]
fn conditional_ordered_negative_bypasses_graph_and_reduces_total_pose_work() {
    use clearra_core_domain::solution::normalized_tiling_solution::{
        PiecePlacementMask, StandardBoard64ColoredTilingIdentity, StandardBoard64TilingIdentity,
    };
    use clearra_coverage::pattern::pattern_id::PatternId;
    use clearra_rules::profile::builtin_rules::srs;
    use PieceKind::{I, J, L, T};

    let _guard = product_guard();
    for (placements, board, oracle_words, new_proof_poses, require_net_reduction) in [
        (
            &[(I, 0x01e0_0000), (J, 0xe020), (T, 0x1c02)][..],
            0x3e1f_03dd,
            vec![vec![T, I, J]],
            2,
            false,
        ),
        (
            &[
                (I, 0x03c0_0000),
                (T, 0x1c020),
                (J, 0x1c04_0000),
                (L, 0x3802),
            ][..],
            0x203a_07dd,
            vec![vec![T, I, J, L], vec![T, J, I, L], vec![J, T, I, L]],
            4,
            true,
        ),
    ] {
        let (catalog, candidate) = fixture(3, placements);
        assert_eq!(catalog.initial_board(), board);
        assert!(
            placement_poset::CandidateGeometry::for_conditional_order(&catalog, &candidate)
                .is_some()
        );
        let pieces: Vec<_> = candidate
            .row_ids()
            .iter()
            .map(|&row| catalog.skeleton(row).piece)
            .collect();
        let transitions = physical_transitions(&catalog, &candidate, KickTableProfileId::Srs90);
        let words = operation_words(&pieces, &transitions);
        assert_eq!(words, oracle_words.into_iter().collect::<BTreeSet<_>>());

        // The D=0 misses do not make these poses impossible after I's row clear.
        let identity = StandardBoard64TilingIdentity::from_placements(
            board,
            placements
                .iter()
                .map(|&(piece, cells)| PiecePlacementMask::new(piece, cells)),
        )
        .unwrap();
        let colored =
            StandardBoard64ColoredTilingIdentity::from_standard_board64_identity(identity);
        let problem = ProblemCompiler::compile_scenario_pc(
            &PcScenarioQuery::new(
                PcScenarioBoard::standard_10(3, board),
                PcQueueInput::standard_7_bag(),
                PieceWindow::new(placements.len()),
            )
            .with_rule(srs())
            .with_allow_hold(false)
            .with_exact_pieces(Some(placements.len()))
            .with_supply_window_size(SupplyWindowSize::new(placements.len()))
            .with_count_policy(PcCountPolicy::CountUnique)
            .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
            .with_allowed_colored_solution_identities([colored])
            .with_retained_trace_limit(0)
            .with_execution_policy(policy()),
        )
        .unwrap()
        .with_output_policy(SearchOutputPolicy::CoverageSummary);
        let run = |enabled, reuse| {
            structural_order::with_conditional_ordered_negative_reuse(reuse, || {
                structural_order::with_conditional_ordered_negative(enabled, || {
                    structural_order::with_conditional_order_mode(true, || {
                        structural_order::with_first_kick_interval_mode(false, || {
                            structural_order::with_ordered_kick_interval_mode(false, || {
                                structural_order::with_movement_clear_mode(false, || {
                                    structural_order::with_mode(false, || {
                                        placement_dependency::with_mode(false, || {
                                            WasmCpuSearchBackend::execute_with_control(
                                                &problem,
                                                &ExecutionControl::default(),
                                            )
                                            .unwrap()
                                        })
                                        .0
                                    })
                                })
                            })
                        })
                    })
                })
            })
        };
        let (baseline, before) = run(false, false);
        let (treatment, after) = run(true, false);
        let (reused, with_reuse) = run(true, true);
        assert_parity(&baseline, &treatment);
        assert_parity(&treatment, &reused);
        assert_eq!(before.conditional_ordered_negative, Default::default());
        assert_eq!(before.conditional_order.unknown_candidates, 1);
        assert_eq!(before.conditional_order.graph_bypasses, 0);
        assert_eq!(after.conditional_order.graph_bypasses, 1);
        assert_eq!(after.conditional_ordered_negative.proofs, new_proof_poses);
        assert_eq!(
            after.conditional_ordered_negative.checked_poses,
            new_proof_poses
        );
        assert!(baseline.usize_field("total_build_order_nodes").unwrap() > 0);
        assert_eq!(treatment.usize_field("total_build_order_nodes"), Some(0));
        assert_eq!(reused.usize_field("total_build_order_nodes"), Some(0));
        assert_eq!(with_reuse.conditional_order.graph_bypasses, 1);
        assert_eq!(with_reuse.conditional_ordered_negative.attempts, 1);
        assert_eq!(with_reuse.conditional_ordered_negative.proofs, 1);
        assert_eq!(with_reuse.conditional_ordered_negative.checked_poses, 1);
        assert_eq!(
            with_reuse.conditional_ordered_negative.reused,
            new_proof_poses - 1
        );
        let primitive_before = baseline.usize_field("total_reachability_states").unwrap();
        let primitive_after = treatment.usize_field("total_reachability_states").unwrap();
        let checked_poses =
            usize::try_from(after.conditional_ordered_negative.checked_poses).unwrap();
        if require_net_reduction {
            // The first blocked L aborts the old compiler before it can warm a
            // later T reachability witness. This branch eliminates more ordinary
            // poses than the new bounded proofs consume.
            assert!(
            primitive_after + checked_poses < primitive_before,
            "ordinary primitive poses {primitive_before} -> {primitive_after}, plus {checked_poses} ordered proof poses"
        );
        }
        eprintln!(
        "ordered negative {board:#x}: ordinary primitive poses {primitive_before} -> {primitive_after}; {checked_poses} ordered proof poses; conditional transitions {} -> {}",
        before.conditional_order.checked_transitions, after.conditional_order.checked_transitions
    );

        let universe = problem.piece_source().materialized_universe().unwrap();
        assert_eq!(
            universe.pattern_count(),
            if placements.len() == 3 { 210 } else { 840 }
        );
        let [coverage] = treatment.solution_coverages() else {
            panic!("the colored filter must admit exactly the intended solution");
        };
        assert_eq!(coverage.identity(), identity);
        assert_eq!(
            coverage.covered_patterns().count_ones() as usize,
            words.len()
        );
        let [reused_coverage] = reused.solution_coverages() else {
            panic!("reusing a bounded negative must preserve the admitted solution");
        };
        for ordinal in 0..universe.pattern_count() {
            let covered = words.contains::<[PieceKind]>(universe.sequence_at(ordinal).as_ref());
            assert_eq!(
                coverage
                    .covered_patterns()
                    .contains(PatternId::new(ordinal)),
                covered,
                "ordered-negative candidate queue ordinal {ordinal}"
            );
            assert_eq!(
                reused_coverage
                    .covered_patterns()
                    .contains(PatternId::new(ordinal)),
                covered,
                "reused ordered-negative candidate queue ordinal {ordinal}"
            );
        }
        if placements.len() == 4 {
            // Catalog order at the root is I,T,J,L. Finish these four real
            // queries (including the bounded L proof); the next callback hits
            // the compiler's normal BudgetExceeded branch in *both* arms.
            let l = pieces.iter().position(|&piece| piece == L).unwrap();
            assert_eq!(transitions[0] & (1_u16 << l), 0);
            let capped = |carry| {
                structural_order::with_conditional_fallback_work(4, carry, || run(true, true))
            };
            let ((discard, discard_order), discard_work) = capped(false);
            let ((carry, carry_order), carry_work) = capped(true);
            assert_parity(&baseline, &discard);
            assert_parity(&discard, &carry);
            for work in [discard_order, carry_order] {
                let order = work.conditional_order;
                assert_eq!(order.attempted_candidates, 1);
                assert_eq!(order.budget_candidates, 1);
                assert_eq!(order.unknown_candidates, 0);
                assert_eq!(order.graph_bypasses, 0);
                assert_eq!(order.checked_transitions, 4);
            }
            assert_eq!(
                discard_order.conditional_ordered_negative,
                carry_order.conditional_ordered_negative
            );
            assert_eq!(discard_work.offered, 1);
            assert_eq!(carry_work.offered, discard_work.offered);
            assert_eq!(discard_work.filtered_poses, 0);
            assert!(carry_work.filtered_poses > 0);
            assert_eq!(carry_work.root_filtered_poses, 1);
            assert_eq!(carry_work.conflicting_poses, 0);
            let field = |r: &CoreExecutionResult, key| r.usize_field(key).unwrap();
            let primitive_calls = |r: &CoreExecutionResult| {
                field(r, "reachability_partial_searches")
                    + field(r, "reachability_exhaustive_searches")
            };
            let graph_nodes = field(&discard, "total_build_order_nodes");
            assert!(graph_nodes > 0);
            assert_eq!(graph_nodes, field(&carry, "total_build_order_nodes"));
            assert!(
                field(&carry, "reachability_lock_queries")
                    < field(&discard, "reachability_lock_queries")
            );
            assert!(primitive_calls(&carry) < primitive_calls(&discard));
            assert!(
                field(&carry, "total_reachability_states")
                    <= field(&discard, "total_reachability_states")
            );
            let [row] = carry.solution_coverages() else {
                panic!("missing single fallback solution");
            };
            assert_eq!(row.identity(), identity);
            for ordinal in 0..840 {
                assert_eq!(
                    row.covered_patterns().contains(PatternId::new(ordinal)),
                    words.contains::<[PieceKind]>(universe.sequence_at(ordinal).as_ref()),
                    "bounded negative fallback original queue {ordinal}"
                );
            }
            eprintln!("bounded fallback: nodes={graph_nodes}, filtered={}, lock queries={}->{}, primitive calls={}->{}",
                carry_work.filtered_poses, field(&discard,"reachability_lock_queries"), field(&carry,"reachability_lock_queries"),
                primitive_calls(&discard), primitive_calls(&carry));
        }
    }
}

#[test]
fn conditional_support_or_created_by_one_clear_preserves_four_line_coverage() {
    use clearra_core_domain::solution::normalized_tiling_solution::{
        PiecePlacementMask, StandardBoard64ColoredTilingIdentity, StandardBoard64TilingIdentity,
    };
    use clearra_coverage::pattern::pattern_id::PatternId;
    use clearra_rules::profile::builtin_rules::srs;
    use PieceKind::{I, J, O, T};

    let _guard = product_guard();
    let placements = [
        (I, 0x07_8000_0000),
        (T, 0x1c02),
        (O, 0x6018),
        (J, 0x60_0802_0000),
    ];
    let (catalog, candidate) = fixture(4, &placements);
    let initial = catalog.initial_board();
    assert_eq!(initial, 0x98_77fd_83e5);
    let pieces: Vec<_> = candidate
        .row_ids()
        .iter()
        .map(|&id| catalog.skeleton(id).piece)
        .collect();
    let bit = |piece| 1_usize << pieces.iter().position(|&kind| kind == piece).unwrap();
    let transitions = physical_transitions(&catalog, &candidate, KickTableProfileId::Srs90);
    assert_eq!(usize::from(transitions[0]), bit(I) | bit(J));
    assert_eq!(subset_state(&catalog, &candidate, bit(I)).1, 0);
    let (after_j, deleted) = subset_state(&catalog, &candidate, bit(J));
    assert_eq!(deleted, 0b0100);
    assert_eq!(usize::from(transitions[bit(J)]), bit(T) | bit(O));
    let i_row = catalog.skeleton_id(I, placements[0].1).unwrap();
    let i_lock = catalog
        .instantiations(i_row, deleted)
        .next()
        .unwrap()
        .lock_mask;
    // The original row cleared by J included fixed initial cells grounding
    // I. In the new frame, neither the floor nor J grounds I.
    assert_ne!(initial & (placements[0].1 >> catalog.width()), 0);
    assert_eq!(i_lock & 0x3ff, 0);
    assert_eq!(after_j & (i_lock >> catalog.width()), 0);
    for supporter in [T, O] {
        let supported = bit(J) | bit(supporter);
        let (board, frame) = subset_state(&catalog, &candidate, supported);
        assert_eq!(frame, deleted);
        assert_ne!(board & (i_lock >> catalog.width()), 0);
        assert_ne!(usize::from(transitions[supported]) & bit(I), 0);
    }
    let words = operation_words(&pieces, &transitions);
    assert_eq!(
        words,
        BTreeSet::from([
            vec![I, J, T, O],
            vec![I, J, O, T],
            vec![J, T, I, O],
            vec![J, T, O, I],
            vec![J, O, I, T],
            vec![J, O, T, I],
        ])
    );
    assert!(
        placement_poset::compile(&catalog, &candidate).is_none(),
        "an unconditional predecessor graph cannot express the missing JI prefix"
    );
    let geometry = placement_poset::CandidateGeometry::for_conditional_order(&catalog, &candidate)
        .expect("the earlier D=0-only admission missed this one-operation clear");
    let mut reused_frame = false;
    let mut projected_new_frame = false;
    let mut simultaneous_delta = false;
    for executed in 0..(1_usize << pieces.len()) {
        let state = geometry.conditional_state(executed as u16);
        assert_eq!(
            (state.board, state.deleted),
            subset_state(&catalog, &candidate, executed)
        );
        for operation in 0..pieces.len() {
            let bit = 1_usize << operation;
            if executed & bit != 0 {
                continue;
            }
            let expected = subset_state(&catalog, &candidate, executed | bit);
            let (_, child) = geometry.conditional_operation(&state, operation);
            assert_eq!(child(), expected);
            reused_frame |= expected.1 == state.deleted;
            projected_new_frame |= expected.1 != state.deleted;
            if executed.count_ones() as usize + 2 <= pieces.len() {
                simultaneous_delta |= (expected.1 & !state.deleted).count_ones() >= 2;
            }
        }
    }
    assert!(reused_frame && projected_new_frame && simultaneous_delta);

    let identity = StandardBoard64TilingIdentity::from_placements(
        initial,
        placements.map(|(piece, cells)| PiecePlacementMask::new(piece, cells)),
    )
    .unwrap();
    let colored = StandardBoard64ColoredTilingIdentity::from_standard_board64_identity(identity);
    let problem = ProblemCompiler::compile_scenario_pc(
        &PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, initial),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(4),
        )
        .with_rule(srs())
        .with_allow_hold(false)
        .with_exact_pieces(Some(4))
        .with_supply_window_size(SupplyWindowSize::new(4))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
        // One piece per kind: the colored filter binds exactly this identity.
        // A graph bypass elsewhere cannot make this regression pass.
        .with_allowed_colored_solution_identities([colored])
        .with_retained_trace_limit(0)
        .with_execution_policy(policy()),
    )
    .unwrap()
    .with_output_policy(SearchOutputPolicy::CoverageSummary);
    let run = |conditional| {
        structural_order::with_conditional_order_mode(conditional, || {
            structural_order::with_first_kick_interval_mode(false, || {
                structural_order::with_movement_clear_mode(false, || {
                    structural_order::with_mode(false, || {
                        placement_dependency::with_mode(false, || {
                            WasmCpuSearchBackend::execute_with_control(
                                &problem,
                                &ExecutionControl::default(),
                            )
                            .unwrap()
                        })
                        .0
                    })
                })
            })
        })
    };
    let (ordinary, off) = run(false);
    let ((reference, reference_order), reference_work) =
        placement_poset::with_conditional_work_comparison(false, false, || run(true));
    let ((lock_only, lock_order), lock_work) =
        placement_poset::with_conditional_work_comparison(true, false, || run(true));
    let ((conditional, on), reused_work) =
        placement_poset::with_conditional_work_comparison(true, true, || run(true));
    assert_parity(&ordinary, &reference);
    assert_parity(&reference, &lock_only);
    assert_parity(&lock_only, &conditional);
    assert_eq!(
        reference_order.conditional_order,
        lock_order.conditional_order
    );
    assert_eq!(lock_order.conditional_order, on.conditional_order);
    for name in ["total_build_order_nodes", "total_reachability_states"] {
        let baseline = reference.usize_field(name).unwrap();
        assert_eq!(baseline, lock_only.usize_field(name).unwrap());
        assert_eq!(baseline, conditional.usize_field(name).unwrap());
    }
    assert_eq!(reference_work.reused_same_frame, 0);
    assert!(lock_work.reused_same_frame > 0 && lock_work.clear_reprojections > 0);
    assert_eq!(
        reference_work.clear_reprojections,
        lock_work.clear_reprojections
    );
    assert_eq!(
        reference_work.lock_projections,
        lock_work.lock_projections + lock_work.reused_same_frame,
    );
    assert_eq!(lock_work.lock_projections, reused_work.lock_projections);
    assert_eq!(
        lock_work.clear_reprojections,
        reused_work.clear_reprojections
    );
    assert_eq!(lock_work.reused_same_frame, reused_work.reused_same_frame);
    assert_eq!(lock_work.prepared_parents, 0);
    assert_eq!(lock_work.delta_reads, 0);
    assert!(reused_work.prepared_parents > 0 && reused_work.nonempty_delta_reads > 0);
    assert!(reused_work.direct_child_scans > 0);
    assert_eq!(
        lock_work.parent_group_visits,
        reused_work.parent_group_visits
    );
    assert_eq!(
        lock_work.direct_child_scans,
        reused_work.direct_child_scans + reused_work.delta_reads,
    );
    // The six allowed words alone give eleven distinct sibling-state edges;
    // each would rescan all four row groups before checking its realization.
    assert!(reused_work.delta_reads >= 11);
    assert!(lock_work.child_group_visits >= reused_work.child_group_visits + 44);
    eprintln!(
        "conditional geometry: reference={reference_work:?} lock={lock_work:?} lock+delta={reused_work:?}"
    );
    assert_eq!(off.conditional_order, Default::default());
    let metrics = on.conditional_order;
    assert!(metrics.graph_bypasses > 0);
    assert!(metrics.checked_transitions <= metrics.attempted_candidates * 256);
    let universe = problem.piece_source().materialized_universe().unwrap();
    assert_eq!(universe.pattern_count(), 840);
    assert_eq!(conditional.solution_coverages().len(), 1);
    let entry = conditional
        .solution_coverages()
        .iter()
        .find(|coverage| coverage.identity() == identity)
        .unwrap();
    assert_eq!(entry.covered_patterns().count_ones(), 6);
    for ordinal in 0..universe.pattern_count() {
        assert_eq!(
            entry.covered_patterns().contains(PatternId::new(ordinal)),
            words.contains::<[PieceKind]>(universe.sequence_at(ordinal).as_ref()),
            "clear-induced OR queue ordinal {ordinal}"
        );
    }
}

#[test]
fn conditional_joint_clear_support_or_preserves_2520_original_queues() {
    use clearra_core_domain::solution::normalized_tiling_solution::{
        PiecePlacementMask, StandardBoard64ColoredTilingIdentity, StandardBoard64TilingIdentity,
    };
    use clearra_coverage::pattern::pattern_id::PatternId;
    use clearra_rules::profile::builtin_rules::srs;
    use PieceKind::{I, J, L, O, T};

    let _guard = product_guard();
    let placements = [
        (I, 0x07_8000_0000),
        (T, 0x1c02),
        (O, 0x6018),
        (J, 0x60_0802_0000),
        (L, 0x08_0380_0000),
    ];
    let (catalog, candidate) = fixture(4, &placements);
    let initial = catalog.initial_board();
    assert_eq!(initial, 0x90_747d_83e5);
    let pieces: Vec<_> = candidate
        .row_ids()
        .iter()
        .map(|&row| catalog.skeleton(row).piece)
        .collect();
    let bit = |piece| 1_usize << pieces.iter().position(|&kind| kind == piece).unwrap();
    let joint = bit(J) | bit(L);
    let transitions = physical_transitions(&catalog, &candidate, KickTableProfileId::Srs90);
    let words = operation_words(&pieces, &transitions);
    // Every operation starts grounded; no original row has a singleton
    // contributor. Both J and L are executable in either order. Only their
    // combination deletes row two and removes I's floor-independent support.
    for (first, second) in [(J, L), (L, J)] {
        assert_ne!(usize::from(transitions[0]) & bit(first), 0);
        assert_ne!(usize::from(transitions[bit(first)]) & bit(second), 0);
        assert_eq!(subset_state(&catalog, &candidate, bit(first)).1, 0);
    }
    assert_eq!(subset_state(&catalog, &candidate, joint).1, 0b0100);
    assert_eq!(usize::from(transitions[joint]), bit(O) | bit(T));
    for supporter in [T, O] {
        assert_ne!(usize::from(transitions[joint | bit(supporter)]) & bit(I), 0);
    }
    assert!(words.contains(&vec![J, L, T, I, O]));
    assert!(words.contains(&vec![L, J, O, I, T]));
    assert!(!words.contains(&vec![J, L, I, T, O]));
    let admits = || {
        placement_poset::CandidateGeometry::for_conditional_order(&catalog, &candidate).is_some()
    };
    assert!(!admits());
    assert!(placement_poset::with_joint_clear_admission(true, admits));
    assert!(!admits());

    let identity = StandardBoard64TilingIdentity::from_placements(
        initial,
        placements.map(|(piece, cells)| PiecePlacementMask::new(piece, cells)),
    )
    .unwrap();
    let colored = StandardBoard64ColoredTilingIdentity::from_standard_board64_identity(identity);
    let problem = ProblemCompiler::compile_scenario_pc(
        &PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, initial),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(5),
        )
        .with_rule(srs())
        .with_allow_hold(false)
        .with_exact_pieces(Some(5))
        .with_supply_window_size(SupplyWindowSize::new(5))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
        .with_allowed_colored_solution_identities([colored])
        .with_retained_trace_limit(0)
        .with_execution_policy(policy()),
    )
    .unwrap()
    .with_output_policy(SearchOutputPolicy::CoverageSummary);
    let run = |joint_admission| {
        structural_order::with_mode(false, || {
            structural_order::with_conditional_order_mode(true, || {
                placement_poset::with_joint_clear_admission(joint_admission, || {
                    placement_dependency::with_mode(false, || {
                        WasmCpuSearchBackend::execute_with_control(
                            &problem,
                            &ExecutionControl::default(),
                        )
                        .unwrap()
                    })
                    .0
                })
            })
        })
    };
    let (baseline, before) = run(false);
    let (treatment, after) = run(true);
    assert_parity(&baseline, &treatment);
    assert_eq!(before.conditional_order.attempted_candidates, 0);
    let actual = after.conditional_order;
    assert_eq!(actual.attempted_candidates, 1);
    assert!((1..=256).contains(&actual.checked_transitions));
    assert!(actual.residual_states <= 64);
    assert_eq!(actual.budget_candidates, 0);
    assert_eq!(actual.certified_candidates + actual.unknown_candidates, 1);
    assert_eq!(actual.graph_bypasses, actual.certified_candidates);
    let old_nodes = baseline.usize_field("total_build_order_nodes").unwrap();
    let new_nodes = treatment.usize_field("total_build_order_nodes").unwrap();
    assert!(old_nodes > 0);
    assert_eq!(
        new_nodes,
        if actual.graph_bypasses == 1 {
            0
        } else {
            old_nodes
        }
    );
    eprintln!(
        "joint-clear support OR: certified={}, unknown={}, checked={}, graph nodes={old_nodes}->{new_nodes}, primitive poses={}->{}",
        actual.certified_candidates, actual.unknown_candidates, actual.checked_transitions,
        baseline.usize_field("total_reachability_states").unwrap(),
        treatment.usize_field("total_reachability_states").unwrap()
    );
    let universe = problem.piece_source().materialized_universe().unwrap();
    assert_eq!(universe.pattern_count(), 2520);
    let [entry] = treatment.solution_coverages() else {
        panic!("the colored filter must select only the five concrete placements");
    };
    for reverse in [false, true] {
        let run = || {
            super::symbolic_prefix_orders::with_strict_prefix_order(|| {
                WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                    .unwrap()
            })
        };
        let (strict, symbolic_work) = if reverse {
            super::symbolic_prefix_orders::with_target_reverse(run)
        } else {
            run()
        };
        assert_parity(&baseline, &strict);
        assert_eq!(strict.usize_field("workers_used"), Some(1));
        assert_eq!(strict.usize_field("total_build_order_nodes"), Some(0));
        assert_eq!(strict.usize_field("total_reachability_states"), Some(0));
        assert_eq!(symbolic_work.candidates_attempted, 1);
        assert_eq!(symbolic_work.candidates_certified, 1);
        eprintln!(
            "strict reverse={reverse} versus ordinary 4L joint-clear: baseline graph_nodes={old_nodes}, primitive_poses={:?}; relation and language costs: {symbolic_work:?}",
            baseline.usize_field("total_reachability_states"),
        );
    }
    assert_eq!(entry.identity(), identity);
    assert_eq!(entry.covered_patterns().count_ones() as usize, words.len());
    for ordinal in 0..universe.pattern_count() {
        assert_eq!(
            entry.covered_patterns().contains(PatternId::new(ordinal)),
            words.contains::<[PieceKind]>(universe.sequence_at(ordinal).as_ref()),
            "joint-clear OR original queue {ordinal}"
        );
    }
}

#[test]
fn identical_compact_board_changes_pending_i_lock_across_original_clear_frames() {
    use PieceKind::{I, O};

    let placements = [
        (O, 0x30_0c00), // x=0,1 above the two initial floor cells
        (O, 0x300c),
        (O, 0xc030),
        (O, 0x3_00c0),
        (O, 0xc_0300),
        (I, 0x03c0_0000), // original row two, x=2..5
        (I, 0x3c00_0000),
    ];
    let (catalog, candidate) = fixture(3, &placements);
    let profile = KickTableProfileId::Srs90;
    let operation = |piece, cells| {
        let row = catalog.skeleton_id(piece, cells).unwrap();
        candidate
            .row_ids()
            .iter()
            .position(|&id| id == row)
            .unwrap()
    };
    let transitions = physical_transitions(&catalog, &candidate, profile);
    let mut five_os = 0_usize;
    for &(piece, cells) in &placements[..5] {
        let bit = 1 << operation(piece, cells);
        assert_ne!(usize::from(transitions[five_os]) & bit, 0);
        assert_eq!(subset_state(&catalog, &candidate, five_os).1, 0);
        five_os |= bit;
    }
    let before = subset_state(&catalog, &candidate, 0);
    let after = subset_state(&catalog, &candidate, five_os);
    assert_eq!(before, (0x3, 0));
    assert_eq!(after, (before.0, 0b011));
    let target = operation(I, placements[5].1);
    let row_id = candidate.row_ids()[target];
    assert_eq!(usize::from(transitions[0]) & (1 << target), 0);
    assert_ne!(usize::from(transitions[five_os]) & (1 << target), 0);
    let pieces: Vec<_> = candidate
        .row_ids()
        .iter()
        .map(|&id| catalog.skeleton(id).piece)
        .collect();
    let words = operation_words(&pieces, &transitions);
    assert!(words.contains(&vec![O, O, O, O, O, I, I]));
    assert!(!words.contains(&vec![I, O, O, O, O, O, I]));

    let geometry = placement_poset::CandidateGeometry::for_conditional_order(&catalog, &candidate)
        .expect("the suspended I has two concrete support alternatives");
    let mut workspace = ReachabilityWorkspace::default();
    workspace.configure(pieces.len());
    workspace.configure_kick_profile(profile, false);
    // Repeat the floating pose to warm the ordinary exact-point cache. Then
    // revisit the exact same board *and catalog row*, after and before the
    // clear. The ILC frame determines which physical poses to ask about.
    for (visit, (executed, (board, deleted), mask, y)) in [
        (0, before, 0x03c0_0000, 2),
        (0, before, 0x03c0_0000, 2),
        (five_os, after, 0x3c, 0),
        (0, before, 0x03c0_0000, 2),
    ]
    .into_iter()
    .enumerate()
    {
        let projected = geometry.conditional_state(executed as u16);
        assert_eq!((projected.board, projected.deleted), (board, deleted));
        assert_eq!(geometry.conditional_lock_mask(target, &projected), mask);
        let expected = usize::from(transitions[executed]) & (1 << target) != 0;
        let hits = workspace.metrics().cache_unreachable_hits;
        let mut seen_rotations = 0_u8;
        for lock in catalog.instantiations(row_id, deleted) {
            assert_eq!((lock.lock_mask, lock.x, lock.lock_y), (mask, 2, y));
            seen_rotations |= 1 << lock.rotation.quarter_turns();
            assert_eq!(
                workspace.lock_reachable_instantiated_in_frame(
                    &catalog,
                    board,
                    I,
                    lock,
                    LocalRelationRowFrame::new(3, deleted),
                ),
                expected
            );
        }
        assert_eq!(seen_rotations, 0b0101, "both horizontal I realizations");
        if visit == 3 {
            assert!(
                workspace.metrics().cache_unreachable_hits > hits,
                "the return to the original physical pose must exercise a cached negative"
            );
        }
    }
}

#[test]
fn one_i_operation_keeps_opposite_ordered_kick_answers_for_its_two_realizations() {
    use clearra_core_domain::piece::rotation::RotationState::{Two, Zero};
    use PieceKind::I;

    let (catalog, candidate) = fixture(2, &[(I, 0xf), (I, 0x7800)]);
    let (board, deleted) = subset_state(&catalog, &candidate, 0);
    assert_eq!((board, deleted), (0xf87f0, 0));
    let frame = LocalRelationRowFrame::new(2, deleted).unwrap();
    let profile = KickTableProfileId::Srs90;
    let row = catalog.skeleton_id(I, 0xf).unwrap();
    let mut poses: Vec<_> = catalog.instantiations(row, deleted).collect();
    poses.sort_unstable_by_key(|pose| pose.rotation.quarter_turns());
    let [zero, two] = poses.as_slice() else {
        panic!("expected both horizontal I realizations of a single row");
    };
    assert_eq!((zero.rotation, two.rotation), (Zero, Two));
    for lock in &poses {
        assert_eq!((lock.lock_mask, lock.x, lock.lock_y), (0xf, 0, 0));
    }

    // Row one has holes at x=1..4. A vertical Left I can descend at x=3:
    // Left->Two's first three landings hit the roof; raw kick 3 enters
    // the floor cavity. Right->Zero instead takes an earlier sky landing,
    // and the alternative Left source at x=0 collides with the roof.
    let anchors = reference_spawn_lock_anchors(10, 2, board, I, profile).unwrap();
    let oracle = |rotation: clearra_core_domain::piece::rotation::RotationState| {
        anchors[rotation.quarter_turns() as usize] & 1 != 0
    };
    assert_eq!((oracle(Zero), oracle(Two)), (false, true));
    let transitions = physical_transitions(&catalog, &candidate, profile);
    let operation = candidate
        .row_ids()
        .iter()
        .position(|&id| id == row)
        .unwrap();
    let allowed = transitions[0] & (1 << operation) != 0;
    assert!(allowed, "one reachable realization admits the operation");
    assert_ne!(
        transitions[1 << operation],
        0,
        "bottom I first can finish the PC"
    );

    for order in [[*zero, *two], [*two, *zero]] {
        let mut workspace = ReachabilityWorkspace::default();
        workspace.configure(2);
        workspace.configure_kick_profile(profile, false);
        for lock in order {
            let expected = oracle(lock.rotation);
            assert!(!workspace.lock_harddrop_reachable_instantiated(10, board, lock));
            // Neither missing certificate supplies the opposite Boolean.
            if let (Some(bounds), _) = workspace
                .try_prove_ordered_lock_pose_closed_within(&catalog, board, I, profile, lock, frame)
            {
                assert!(bounds.excludes(board) && !expected);
            }
            if workspace.try_prove_ordered_kick_lock_interval(
                &catalog,
                board,
                board,
                I,
                profile,
                lock,
                Some(frame),
            ) {
                assert!(expected);
            }
        }
        for _ in 0..3 {
            let answers = order.map(|lock| {
                workspace.lock_reachable_instantiated_in_frame(
                    &catalog,
                    board,
                    I,
                    lock,
                    Some(frame),
                )
            });
            assert_eq!(answers, order.map(|lock| oracle(lock.rotation)));
            assert_eq!(answers.into_iter().any(|answer| answer), allowed);
        }
        let metrics = workspace.metrics();
        assert!(metrics.cache_unreachable_hits > 0 && metrics.cache_reachable_hits > 0);
    }
}

#[test]
fn conditional_and_strict_prefix_support_or_preserve_every_empty_hold_queue_ordinal() {
    use clearra_core_domain::solution::normalized_tiling_solution::{
        PiecePlacementMask, StandardBoard64TilingIdentity,
    };
    use clearra_coverage::pattern::pattern_id::PatternId;
    use clearra_rules::profile::builtin_rules::srs;
    use PieceKind::{I, O, T};

    // For a chosen physical word, independently try the current piece, a
    // swap with held, or storing into empty hold and using the next draw.
    // A finished word needs no additional draw, even with a piece held.
    fn plays_word(queue: &[PieceKind], held: Option<PieceKind>, word: &[PieceKind]) -> bool {
        let Some((&required, suffix)) = word.split_first() else {
            return true;
        };
        let Some((&current, remaining)) = queue.split_first() else {
            return false;
        };
        if current == required && plays_word(remaining, held, suffix) {
            return true;
        }
        match held {
            Some(piece) => piece == required && plays_word(remaining, Some(current), suffix),
            None => remaining.split_first().is_some_and(|(&next, after_next)| {
                next == required && plays_word(after_next, Some(current), suffix)
            }),
        }
    }

    fn four_draw_queues() -> Vec<[PieceKind; 4]> {
        fn visit(
            out: &mut Vec<[PieceKind; 4]>,
            queue: &mut [PieceKind; 4],
            depth: usize,
            used: u8,
        ) {
            if depth == queue.len() {
                out.push(*queue);
                return;
            }
            for (index, piece) in PieceKind::STANDARD_TETROMINOES.into_iter().enumerate() {
                let bit = 1_u8 << index;
                if used & bit == 0 {
                    queue[depth] = piece;
                    visit(out, queue, depth + 1, used | bit);
                }
            }
        }
        let mut out = Vec::new();
        visit(&mut out, &mut [PieceKind::I; 4], 0, 0);
        out
    }

    let _guard = product_guard();
    let placements = [(I, 0x00f0_0000), (O, 0x0c03), (T, 0x7008)];
    let (catalog, candidate) = fixture(3, &placements);
    let board = catalog.initial_board();
    let pieces: Vec<_> = candidate
        .row_ids()
        .iter()
        .map(|&id| catalog.skeleton(id).piece)
        .collect();
    let transitions = physical_transitions(&catalog, &candidate, KickTableProfileId::Srs90);
    let words = operation_words(&pieces, &transitions);
    assert_eq!(
        words,
        BTreeSet::from([vec![O, I, T], vec![O, T, I], vec![T, I, O], vec![T, O, I]])
    );

    let problem = ProblemCompiler::compile_scenario_pc(
        &PcScenarioQuery::new(
            PcScenarioBoard::standard_10(3, board),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(3),
        )
        .with_rule(srs())
        .with_allow_hold(true)
        .with_exact_pieces(Some(3))
        .with_supply_window_size(SupplyWindowSize::new(4))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
        .with_retained_trace_limit(0)
        .with_execution_policy(policy()),
    )
    .unwrap()
    .with_output_policy(SearchOutputPolicy::CoverageSummary);
    assert!(problem.supply().hold_enabled());
    assert!(problem.initial_hold().hold_empty());
    assert!(!problem.supply().projects_unplaced_lookahead());
    assert!(!problem
        .queue_observation_policy()
        .requires_observation_policy());

    let run = |conditional| {
        structural_order::with_conditional_order_mode(conditional, || {
            structural_order::with_first_kick_interval_mode(false, || {
                structural_order::with_movement_clear_mode(false, || {
                    structural_order::with_mode(false, || {
                        placement_dependency::with_mode(false, || {
                            WasmCpuSearchBackend::execute_with_control(
                                &problem,
                                &ExecutionControl::default(),
                            )
                            .unwrap()
                        })
                        .0
                    })
                })
            })
        })
    };
    let (baseline, _) = run(false);
    let (conditional, metrics) = run(true);
    assert_parity(&baseline, &conditional);
    assert!(metrics.conditional_order.attempted_candidates > 0);

    let (strict, strict_work) = super::symbolic_prefix_orders::with_strict_prefix_order(|| {
        WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default()).unwrap()
    });
    assert_parity(&baseline, &strict);
    assert_eq!(strict.usize_field("workers_used"), Some(1));
    assert_eq!(strict.usize_field("total_build_order_nodes"), Some(0));
    assert_eq!(strict.usize_field("total_reachability_states"), Some(0));
    assert!(baseline.usize_field("total_build_order_nodes").unwrap() > 0);
    assert!(strict_work.candidates_attempted > 0);
    assert_eq!(
        strict_work.candidates_attempted,
        strict_work.candidates_certified
    );
    let (strict_reverse, reverse_work) = super::symbolic_prefix_orders::with_target_reverse(|| {
        super::symbolic_prefix_orders::with_strict_prefix_order(|| {
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .unwrap()
        })
    });
    assert_parity(&strict, &strict_reverse);
    assert_eq!(
        strict_reverse.usize_field("total_build_order_nodes"),
        Some(0)
    );
    assert_eq!(
        strict_reverse.usize_field("total_reachability_states"),
        Some(0)
    );
    assert_eq!(
        reverse_work.candidates_certified,
        strict_work.candidates_certified
    );
    let (strict_upper, upper_work) = super::symbolic_prefix_orders::with_upper_bound(|| {
        super::symbolic_prefix_orders::with_target_reverse(|| {
            super::symbolic_prefix_orders::with_strict_prefix_order(|| {
                WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                    .unwrap()
            })
        })
    });
    assert_parity(&strict, &strict_upper);
    assert_eq!(strict_upper.usize_field("total_build_order_nodes"), Some(0));
    assert_eq!(
        upper_work.candidates_certified,
        strict_work.candidates_certified
    );

    let identity = StandardBoard64TilingIdentity::from_placements(
        board,
        placements.map(|(piece, cells)| PiecePlacementMask::new(piece, cells)),
    )
    .unwrap();
    let coverage = conditional
        .solution_coverages()
        .iter()
        .find(|coverage| coverage.identity() == identity)
        .expect("the O/T support-OR tiling is present")
        .covered_patterns();
    let strict_coverage = strict
        .solution_coverages()
        .iter()
        .find(|coverage| coverage.identity() == identity)
        .expect("the strict verifier emits the O/T support-OR tiling")
        .covered_patterns();
    let queues = four_draw_queues();
    let universe = problem.piece_source().materialized_universe().unwrap();
    assert_eq!(queues.len(), 840);
    assert_eq!(universe.pattern_count(), queues.len());
    let mut successes = 0;
    let mut hold_only_successes = 0;
    for (ordinal, queue) in queues.iter().enumerate() {
        assert_eq!(universe.sequence_at(ordinal).as_ref(), queue.as_slice());
        let expected = words.iter().any(|word| plays_word(queue, None, word));
        assert_eq!(
            coverage.contains(PatternId::new(ordinal)),
            expected,
            "conditional original supply ordinal {ordinal} ({queue:?})"
        );
        assert_eq!(
            strict_coverage.contains(PatternId::new(ordinal)),
            expected,
            "strict original supply ordinal {ordinal} ({queue:?})"
        );
        successes += usize::from(expected);
        hold_only_successes += usize::from(expected && !words.contains(&queue[..3]));
    }
    assert_eq!(successes, 80);
    assert_eq!(hold_only_successes, 64);
}
