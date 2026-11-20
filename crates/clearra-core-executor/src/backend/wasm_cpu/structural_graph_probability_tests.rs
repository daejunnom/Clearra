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
            assert!(certificate.observed_guard_count() > 0);

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
                certificate.observed_guard_count(),
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
    let (treatment, metrics) = structural_order::with_mode(true, run);
    assert_parity(&baseline, &treatment);
    assert_eq!(metrics.skipped_build_graphs, 0);
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
