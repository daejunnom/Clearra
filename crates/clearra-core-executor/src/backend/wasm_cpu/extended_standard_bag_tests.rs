//! The small graphs below isolate the supply product; the final test also
//! constructs and completes an actual full-height physical Build request.
//! Dense coverage and the original physical witness walk remain independent
//! references for the canonical kind-language product.

use std::collections::BTreeSet;

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    piece::{piece_kind::PieceKind, rotation::RotationState},
};
use clearra_coverage::pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    RequestedSearchBackend, SupplyWindowSize,
};
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, ProblemCompiler, SearchProblem,
};
use clearra_supply::{
    pattern_universe::{
        materialized_pattern_universe::UniformCompactPatternSource,
        MaterializedPatternUniverseStructure, PatternPiecePositionIndex,
    },
    queue::queue_pattern_expression::QueuePatternExpression,
    QueueObservationPolicy,
};

use crate::{
    backend::wasm_cpu::{
        buildup::{representative_pattern_path, BuildOrderGraph, BuildOrderNodeSpec},
        coverage_product::CoverageProductEvaluator,
        extended_geometry::ExtendedGeometrySearch,
        piece_order_language::PieceOrderLanguageCache,
        standard_bag_coverage::StandardBagCoverage,
    },
    CoreExecutionResult, CorePathStep,
};

use super::{BuildProbabilityAdvance, ExtendedBuildProbabilitySession};

fn source_problem(
    draws: usize,
    locks: usize,
    initial_hold: Option<PieceKind>,
    queue: PcQueueInput,
    max_patterns: usize,
) -> SearchProblem {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(8, 0),
        queue,
        PieceWindow::new(locks),
    )
    .with_exact_pieces(Some(locks))
    .with_supply_window_size(SupplyWindowSize::new(draws))
    .with_allow_hold(true)
    .with_hold_piece(initial_hold)
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_queue_observation_policy(QueueObservationPolicy::FullQueueOracle)
    .with_execution_policy(
        PcExecutionPolicy::default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_allow_backend_fallback(false)
            .with_workers(1)
            .with_max_patterns(max_patterns),
    );
    ProblemCompiler::compile_scenario_pc(&query).expect("small typed supply problem")
}

fn word_graph(word: &[PieceKind]) -> BuildOrderGraph {
    let mut specs = Vec::new();
    let mut edges = Vec::new();
    for (depth, &piece) in word.iter().enumerate() {
        specs.push(BuildOrderNodeSpec {
            edge_start: depth as u32,
            edge_count: 1,
            depth: depth as u8,
            accepting: false,
        });
        edges.push(BuildOrderGraph::edge(
            depth as u32 + 1,
            depth as u8,
            piece,
            RotationState::Zero,
            depth as i8,
            0,
            0,
        ));
    }
    specs.push(BuildOrderNodeSpec {
        edge_start: edges.len() as u32,
        edge_count: 0,
        depth: word.len() as u8,
        accepting: true,
    });
    BuildOrderGraph::from_topological_parts(specs, edges, 0, 0, true).expect("ranked word graph")
}

fn dense_coverage(problem: &SearchProblem, graph: &BuildOrderGraph) -> PatternBitSet {
    let universe = problem.piece_source().materialized_universe().unwrap();
    let index = PatternPiecePositionIndex::compile(universe).unwrap();
    CoverageProductEvaluator::default()
        .evaluate(
            graph,
            &index,
            problem.initial_hold(),
            problem.supply().hold_enabled(),
            problem.supply().projects_unplaced_lookahead(),
            problem.supply().projects_standard_bag_lookahead(),
            false,
            false,
            &ExecutionControl::default(),
        )
        .unwrap()
        .coverage_bits
}

fn standard_coverage(problem: &SearchProblem) -> StandardBagCoverage {
    StandardBagCoverage::for_universe(
        problem.piece_source().materialized_universe().unwrap(),
        problem.initial_hold(),
        problem.supply().hold_enabled(),
        problem.supply().projects_unplaced_lookahead(),
    )
    .unwrap()
    .expect("actual standard-bag product")
}

fn assert_product_parity(
    problem: &SearchProblem,
    graph: &BuildOrderGraph,
) -> (PatternBitSet, Vec<CorePathStep>) {
    let universe = problem.piece_source().materialized_universe().unwrap();
    let dense = dense_coverage(problem, graph);
    let mut languages = PieceOrderLanguageCache::default();
    let root = languages.canonicalize(graph).unwrap();
    let mut coverage = standard_coverage(problem);
    let symbolic = coverage
        .cover_language(&languages, root, &ExecutionControl::default())
        .unwrap();
    let materialized = coverage.materialize_root(symbolic.root).unwrap();
    assert_eq!(materialized, dense);
    assert_eq!(symbolic.covers_any_pattern, !dense.is_empty());
    assert_eq!(symbolic.covered_pattern_count, dense.count_ones() as usize);
    assert_eq!(
        symbolic.witness_pattern_id,
        dense.first_pattern().map(|pattern| pattern.index() as u32),
    );

    // This walks the original operation edges for every original ordinal.
    // It also detects a terminal-projection discrepancy hidden by an equal
    // aggregate count or by choosing only one successful queue.
    for ordinal in 0..universe.pattern_count() {
        let sequence = universe.sequence_at(ordinal);
        let path = representative_pattern_path(problem, graph, sequence.as_ref(), None);
        assert_eq!(
            path.len() == graph.max_depth(),
            dense.contains(PatternId::new(ordinal)),
            "physical witness membership for original ordinal {ordinal}",
        );
    }
    let first = dense
        .first_pattern()
        .expect("fixture has a successful queue");
    let sequence = universe.sequence(first);
    let path = representative_pattern_path(problem, graph, sequence.as_ref(), None);
    coverage.merge_global(symbolic.root).unwrap();
    assert_eq!(coverage.materialize_global().unwrap(), dense);
    (dense, path)
}

#[test]
fn extended_standard_bag_terminal_windows_match_dense_original_ids_and_witnesses() {
    use PieceKind::{I, J, L, O, S, T, Z};

    let cases: &[(usize, Option<PieceKind>, &[PieceKind], usize, &str)] = &[
        // Original IO is rearranged to OI; the last I is released at EOF.
        (2, None, &[O, I], 0, "release-held-at-terminal"),
        // Original IOTSZJ determines L, which is placed while I stays held.
        (6, None, &[O, T, S, Z, J, L], 0, "use-unplaced-lookahead"),
        // The initial I adds one lock without adding an original draw.
        (2, Some(I), &[O, T, I], 7, "release-held-at-terminal"),
    ];
    for &(draws, held, word, first_ordinal, last_action) in cases {
        let problem = source_problem(draws, word.len(), held, PcQueueInput::standard_7_bag(), 0);
        assert!(problem.supply().projects_unplaced_lookahead());
        assert!(problem.supply().projects_standard_bag_lookahead());
        assert_eq!(draws + usize::from(held.is_some()), word.len());
        assert!(ExtendedBuildProbabilitySession::uses_standard_bag_coverage(
            &problem,
            BuildProbabilityAggregation::Buildability,
            false,
        )
        .unwrap());
        let graph = word_graph(word);
        let (covered, path) = assert_product_parity(&problem, &graph);
        assert_eq!(covered.first_pattern().unwrap().index(), first_ordinal);
        assert_eq!(
            path.iter()
                .map(CorePathStep::piece)
                .collect::<Vec<_>>()
                .as_slice(),
            word
        );
        assert_eq!(path.last().unwrap().hold(), last_action);
    }
}

fn kind_words(language: &PieceOrderLanguageCache, root: u32) -> BTreeSet<Vec<u8>> {
    fn visit(
        language: &PieceOrderLanguageCache,
        node: u32,
        prefix: &mut Vec<u8>,
        words: &mut BTreeSet<Vec<u8>>,
    ) {
        if language.node(node).unwrap().accepting {
            words.insert(prefix.clone());
            return;
        }
        for index in 0..language.node_edge_count(node).unwrap() {
            let edge = language.edge_at(node, index).unwrap();
            prefix.push(edge.piece_code());
            visit(language, edge.child(), prefix, words);
            prefix.pop();
        }
    }
    let mut words = BTreeSet::new();
    visit(language, root, &mut Vec::new(), &mut words);
    words
}

#[test]
fn extended_standard_bag_keeps_same_kind_nfa_children_and_only_deduplicates_boolean_paths() {
    use PieceKind::{I, O, T};

    let specs = vec![
        BuildOrderNodeSpec {
            edge_start: 0,
            edge_count: 3,
            depth: 0,
            accepting: false,
        },
        BuildOrderNodeSpec {
            edge_start: 3,
            edge_count: 1,
            depth: 1,
            accepting: false,
        },
        BuildOrderNodeSpec {
            edge_start: 4,
            edge_count: 1,
            depth: 1,
            accepting: false,
        },
        BuildOrderNodeSpec {
            edge_start: 5,
            edge_count: 0,
            depth: 2,
            accepting: true,
        },
    ];
    let edges = vec![
        BuildOrderGraph::edge(1, 0, I, RotationState::Zero, 0, 0, 0),
        BuildOrderGraph::edge(1, 1, I, RotationState::Zero, 4, 0, 0),
        BuildOrderGraph::edge(2, 2, I, RotationState::Zero, 6, 0, 0),
        BuildOrderGraph::edge(3, 3, O, RotationState::Zero, 0, 0, 0),
        BuildOrderGraph::edge(3, 4, T, RotationState::Zero, 0, 0, 0),
    ];
    let graph = BuildOrderGraph::from_topological_parts(specs, edges, 0, 0, false).unwrap();
    assert_eq!(graph.edges(0).len(), 3);
    assert_eq!(graph.piece_edges(0).len(), 2);
    let mut languages = PieceOrderLanguageCache::default();
    let root = languages.canonicalize(&graph).unwrap();
    assert_eq!(languages.edges_for_piece(root, 1).unwrap().len(), 2);
    assert_eq!(
        kind_words(&languages, root),
        BTreeSet::from([vec![1, 2], vec![1, 3]])
    );

    let problem = source_problem(2, 2, None, PcQueueInput::standard_7_bag(), 0);
    let (covered, _) = assert_product_parity(&problem, &graph);
    let index =
        PatternPiecePositionIndex::compile(problem.piece_source().materialized_universe().unwrap())
            .unwrap();
    let counted = CoverageProductEvaluator::default()
        .evaluate(
            &graph,
            &index,
            problem.initial_hold(),
            true,
            true,
            true,
            true,
            false,
            &ExecutionControl::default(),
        )
        .unwrap();
    assert_eq!(counted.coverage_bits, covered);
    assert!(counted.count_complete);
    assert!(counted.path_count > u128::from(covered.count_ones()));
}

#[test]
fn extended_standard_bag_recycled_kind_ids_preserve_global_original_union() {
    use PieceKind::{I, J, O, T};

    let problem = source_problem(2, 2, None, PcQueueInput::standard_7_bag(), 0);
    let first = word_graph(&[O, I]);
    let second = word_graph(&[T, J]);
    let first_bits = dense_coverage(&problem, &first);
    let second_bits = dense_coverage(&problem, &second);
    assert!(!first_bits.is_empty());
    assert!(!second_bits.is_empty());
    assert_ne!(first_bits, second_bits);
    let mut expected = first_bits.clone();
    expected.union_with(&second_bits).unwrap();
    let control = ExecutionControl::default();
    let mut languages = PieceOrderLanguageCache::default();
    let mut coverage = standard_coverage(&problem);

    let first_root = languages.canonicalize(&first).unwrap();
    let result = coverage
        .cover_language(&languages, first_root, &control)
        .unwrap();
    assert_eq!(coverage.materialize_root(result.root).unwrap(), first_bits);
    coverage.merge_global(result.root).unwrap();
    coverage.flush_and_recycle_local_cache().unwrap();
    assert_eq!(coverage.materialize_global().unwrap(), first_bits);
    languages.clear_retain_capacity();

    let second_root = languages.canonicalize(&second).unwrap();
    assert_eq!(first_root, second_root, "exercise reused numeric kind IDs");
    let result = coverage
        .cover_language(&languages, second_root, &control)
        .unwrap();
    assert_eq!(coverage.materialize_root(result.root).unwrap(), second_bits);
    assert_eq!(
        result.witness_pattern_id,
        second_bits
            .first_pattern()
            .map(|pattern| pattern.index() as u32),
    );
    coverage.merge_global(result.root).unwrap();
    coverage.flush_and_recycle_local_cache().unwrap();
    languages.clear_retain_capacity();

    let duplicate_root = languages.canonicalize(&first).unwrap();
    let duplicate = coverage
        .cover_language(&languages, duplicate_root, &control)
        .unwrap();
    coverage.merge_global(duplicate.root).unwrap();
    let actual = coverage.materialize_global().unwrap();
    assert_eq!(actual, expected);
    assert_eq!(actual.count_ones(), 4);
    assert_eq!(actual.first_pattern(), first_bits.first_pattern());
}

#[test]
fn extended_standard_bag_gate_rejects_truncated_and_factorized_original_axes() {
    use PieceKind::{O, T};

    let complete = source_problem(2, 2, None, PcQueueInput::standard_7_bag(), 0);
    assert!(ExtendedBuildProbabilitySession::uses_standard_bag_coverage(
        &complete,
        BuildProbabilityAggregation::Buildability,
        false,
    )
    .unwrap());
    let probability_evidence = complete.clone().with_pc_chance_probability_v2_evidence();
    assert!(
        !ExtendedBuildProbabilitySession::uses_standard_bag_coverage(
            &probability_evidence,
            BuildProbabilityAggregation::Buildability,
            false,
        )
        .unwrap()
    );
    let typed_probability = probability_evidence
        .with_output_policy(clearra_problem::SearchOutputPolicy::CoverageSummary);
    assert!(ExtendedBuildProbabilitySession::uses_standard_bag_coverage(
        &typed_probability,
        BuildProbabilityAggregation::Buildability,
        false,
    )
    .unwrap());
    let truncated = source_problem(2, 2, None, PcQueueInput::standard_7_bag(), 1);
    let universe = truncated.piece_source().materialized_universe().unwrap();
    assert!(!universe.complete());
    assert!(universe.truncation_reason().is_some());
    assert_eq!(universe.pattern_count(), 1);
    assert_eq!(universe.total_possible_pattern_count(), 42);
    assert!(
        !ExtendedBuildProbabilitySession::uses_standard_bag_coverage(
            &truncated,
            BuildProbabilityAggregation::Buildability,
            false,
        )
        .unwrap()
    );

    // A nonempty full-bag DD is not proof that its truncated original prefix
    // has a witness. This is the concrete reason for the complete-axis gate.
    let graph = word_graph(&[T, O]);
    assert!(dense_coverage(&truncated, &graph).is_empty());
    let mut language = PieceOrderLanguageCache::default();
    let root = language.canonicalize(&graph).unwrap();
    let mut coverage = standard_coverage(&truncated);
    let outside_prefix = coverage
        .cover_language(&language, root, &ExecutionControl::default())
        .unwrap();
    assert!(!outside_prefix.covers_any_pattern);
    assert_eq!(outside_prefix.covered_pattern_count, 0);
    assert_eq!(outside_prefix.witness_pattern_id, None);
    assert!(coverage
        .materialize_root(outside_prefix.root)
        .unwrap()
        .is_empty());

    let factorized = source_problem(
        7,
        7,
        None,
        PcQueueInput::pattern_expression(QueuePatternExpression::parse("P7", 0).unwrap()),
        0,
    );
    let universe = factorized.piece_source().materialized_universe().unwrap();
    assert!(universe.complete());
    assert_eq!(universe.pattern_count(), 5_040);
    assert!(matches!(
        universe.structure(),
        MaterializedPatternUniverseStructure::FactorizedQueueExpression { sequence_len: 7 },
    ));
    assert!(matches!(
        universe.uniform_compact_source(),
        Some(UniformCompactPatternSource::FactorizedExpression(_)),
    ));
    assert!(
        !ExtendedBuildProbabilitySession::uses_standard_bag_coverage(
            &factorized,
            BuildProbabilityAggregation::Buildability,
            false,
        )
        .unwrap()
    );
}

fn complete_physical_fixture(
    problem: &SearchProblem,
    field: BuildProbabilityField,
    symbolic: bool,
) -> CoreExecutionResult {
    let mut session = ExtendedBuildProbabilitySession::new(
        problem,
        field,
        BuildProbabilityAggregation::Buildability,
    )
    .unwrap();
    assert!(session.standard_bag_coverage.is_some());
    if !symbolic {
        // Select the independent existing product without changing process
        // environment, source rows, Hold permissions, or physical producer.
        session.standard_bag_coverage = None;
        let universe = problem.piece_source().materialized_universe().unwrap();
        let family = universe.packing_multiset_family_for_execution(
            field.target_piece_count(),
            problem.initial_hold(),
            problem.supply().hold_enabled(),
            crate::backend::wasm_cpu::packing_hold_projection(problem),
        );
        session.geometry =
            ExtendedGeometrySearch::new(universe, &family, &session.catalog).unwrap();
    }
    let control = ExecutionControl::default();
    for _ in 0..4_096 {
        match session.advance(16, &control).unwrap() {
            BuildProbabilityAdvance::Pending => {}
            BuildProbabilityAdvance::Completed(result) => return result,
            BuildProbabilityAdvance::Cancelled => panic!("unexpected fixture cancellation"),
        }
    }
    panic!("two-piece physical Build fixture did not complete")
}

#[test]
fn extended_standard_bag_complete_full_height_build_matches_dense_result() {
    let problem = source_problem(2, 2, None, PcQueueInput::standard_7_bag(), 0);
    // A bottom-row horizontal I and a grounded O in separate components.
    // Neither operation clears a row, and each component has one tiling.
    let field =
        BuildProbabilityField::from_words_preserving_height(8, [0; 4], [0x0003_00cf, 0, 0, 0])
            .unwrap();
    assert!(!field.is_compact());
    let dense = complete_physical_fixture(&problem, field, false);
    let symbolic = complete_physical_fixture(&problem, field, true);
    assert_eq!(symbolic.coverage_pattern_words(), &[0x41]); // Original IO and OI.
    assert_eq!(
        symbolic.coverage_pattern_words(),
        dense.coverage_pattern_words()
    );
    assert_eq!(symbolic.normalized_solution_keys().len(), 1);
    assert_eq!(
        symbolic.normalized_solution_keys(),
        dense.normalized_solution_keys()
    );
    assert_eq!(symbolic.path_steps().len(), 2);
    assert_eq!(symbolic.path_steps(), dense.path_steps());
    for key in [
        "solution_found",
        "count_complete",
        "probability_complete",
        "packing_candidate_count",
        "packing_candidate_set_digest",
        "geometry_candidate_family_count",
        "unique_solution_count",
        "normalized_solution_set_hash",
        "witnessed_pattern_count",
        "covered_pattern_count",
        "coverage_probability",
        "coverage_probability_denominator",
        "representative_pattern_id",
        "representative_candidate_ordinal",
    ] {
        assert!(dense.field(key).is_some(), "reference field {key} exists");
        assert_eq!(symbolic.field(key), dense.field(key), "{key}");
    }
    assert_eq!(symbolic.bool_field("solution_found"), Some(true));
    assert_eq!(symbolic.bool_field("count_complete"), Some(true));
    assert_eq!(symbolic.bool_field("probability_complete"), Some(true));
}

#[test]
fn typed_chance_extended_standard_bag_rows_match_dense_pc_source_product() {
    use clearra_core_domain::board::standard_pc_board::Board256Mask;
    use clearra_objectives::policy::{
        objective_policy::ObjectivePolicy, score_objective_policy::SpinProfileSelection,
    };
    use clearra_problem::SearchOutputPolicy;

    let target = (0..8_u16).fold(Board256Mask::EMPTY, |mask, row| {
        mask.union(Board256Mask::singleton(row * 10 + u16::from(row >= 4)).unwrap())
    });
    let initial = Board256Mask::all_cells(80).unwrap().without(target);
    let field =
        BuildProbabilityField::from_words_preserving_height(8, initial.words(), target.words())
            .unwrap();
    for spin in [
        None,
        Some(SpinProfileSelection::TSpins),
        Some(SpinProfileSelection::TSpinsPlus),
        Some(SpinProfileSelection::AllSpin),
        Some(SpinProfileSelection::AllSpinPlus),
        Some(SpinProfileSelection::AllMini),
        Some(SpinProfileSelection::AllMiniPlus),
    ] {
        for (hold, held) in [(false, None), (true, None), (true, Some(PieceKind::I))] {
            let query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10_from_words(8, initial.words()).unwrap(),
                PcQueueInput::standard_7_bag(),
                PieceWindow::new(2),
            )
            .with_exact_pieces(Some(2))
            .with_supply_window_size(SupplyWindowSize::new(2))
            .with_allow_hold(hold)
            .with_hold_piece(held)
            .with_rule(clearra_rules::profile::builtin_rules::srs_plus())
            .with_count_policy(PcCountPolicy::CountUnique)
            .with_objective(spin.map_or_else(ObjectivePolicy::unique, |profile| {
                ObjectivePolicy::unique().with_back_to_back_preservation(profile)
            }));
            let problem = ProblemCompiler::compile_scenario_pc(&query)
                .unwrap()
                .with_pc_chance_probability_v2_evidence()
                .with_output_policy(SearchOutputPolicy::CoverageSummary);
            let run = |symbolic: bool| {
                let bound =
                    crate::resource::ExecutionMemoryBound::unbounded_for_problem(&problem).unwrap();
                let mut session =
                    ExtendedBuildProbabilitySession::new_pc_family(&problem, field, bound, 0)
                        .unwrap();
                assert!(session.standard_bag_coverage.is_some());
                if !symbolic {
                    // Exercise the previous position-index product against
                    // the same full source IDs, weights and physical graph.
                    session.standard_bag_coverage = None;
                    let universe = problem.piece_source().materialized_universe().unwrap();
                    let family = universe.packing_multiset_family_for_execution(
                        2,
                        problem.initial_hold(),
                        problem.supply().hold_enabled(),
                        crate::backend::wasm_cpu::packing_hold_projection(&problem),
                    );
                    session.geometry =
                        ExtendedGeometrySearch::new(universe, &family, &session.catalog).unwrap();
                }
                for _ in 0..64 {
                    match session.advance(16, &ExecutionControl::default()).unwrap() {
                        BuildProbabilityAdvance::Pending => {}
                        BuildProbabilityAdvance::Completed(result) => return result,
                        BuildProbabilityAdvance::Cancelled => {
                            panic!("uncancelled two-lock fixture")
                        }
                    }
                }
                panic!("two-lock PC fixture did not complete")
            };
            let dense = run(false);
            let symbolic = run(true);
            for name in [
                "covered_pattern_count",
                "coverage_probability",
                "coverage_pattern_count",
                "count_complete",
                "probability_complete",
                "execution_constraint_materialized",
                "b2b_preservation_evaluation_basis",
            ] {
                assert_eq!(
                    symbolic.field(name),
                    dense.field(name),
                    "{name}, spin={spin:?}, hold={hold}, held={held:?}"
                );
            }
            assert_eq!(
                symbolic.coverage_pattern_words(),
                dense.coverage_pattern_words()
            );
            let rows = |result: &CoreExecutionResult| {
                let evidence = result.pc_chance_coverage_evidence().unwrap();
                assert!(evidence.complete());
                assert!(evidence.problem().matches_search_problem(&problem));
                let mut rows = evidence
                    .rows()
                    .iter()
                    .map(|row| (row.candidate_id(), row.coverage_bits().to_owned_words()))
                    .collect::<Vec<_>>();
                rows.sort();
                rows
            };
            assert_eq!(rows(&symbolic), rows(&dense));
            assert_eq!(
                symbolic.field("solution_found"),
                Some(if held.is_some() { "true" } else { "false" })
            );
            assert!(symbolic.spin_coverage_execution_batches().is_empty());
            assert!(symbolic.postprocess_pattern_weights().is_empty());
        }
    }
}

fn full_draw_component_fixture(duplicate_s: bool) -> (SearchProblem, BuildProbabilityField) {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(12, 0),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(7),
    )
    .with_exact_pieces(Some(7))
    .with_supply_window_size(SupplyWindowSize::new(7))
    .with_allow_hold(true)
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_queue_observation_policy(QueueObservationPolicy::FullQueueOracle)
    .with_execution_policy(PcExecutionPolicy::default().with_workers(1));
    let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
    let mut base = [0_u64; 4];
    let mut target = [0_u64; 4];
    let insert = |words: &mut [u64; 4], x: usize, y: usize| {
        let cell = y * 10 + x;
        words[cell / 64] |= 1_u64 << (cell % 64);
    };
    // Fixed platforms separate the four-cell components. The middle columns
    // remain open so lower pockets can be entered horizontally from the sky.
    for y in [2, 5, 8] {
        for x in 0..3 {
            insert(&mut base, x, y);
        }
    }
    for y in [2, 5] {
        for x in 6..9 {
            insert(&mut base, x, y);
        }
    }
    for cells in [
        [(0, 0), (1, 0), (2, 0), (3, 0)],   // I
        [(6, 0), (7, 0), (6, 1), (7, 1)],   // O
        [(0, 3), (1, 3), (2, 3), (1, 4)],   // T
        [(6, 3), (7, 3), (7, 4), (8, 4)],   // S
        [(0, 6), (1, 6), (2, 6), (0, 7)],   // J
        [(0, 9), (0, 10), (0, 11), (1, 9)], // L
        if duplicate_s {
            [(6, 6), (7, 6), (7, 7), (8, 7)]
        } else {
            [(7, 6), (8, 6), (6, 7), (7, 7)] // Z
        },
    ] {
        for (x, y) in cells {
            insert(&mut target, x, y);
        }
    }
    let field = BuildProbabilityField::from_words_preserving_height(12, base, target).unwrap();
    assert_eq!(field.target_piece_count(), 7);
    (problem, field)
}

#[test]
fn extended_full_draw_multiset_preserves_candidate_inventory_and_physical_results() {
    for duplicate_s in [false, true] {
        let (problem, field) = full_draw_component_fixture(duplicate_s);
        assert_eq!(
            problem
                .piece_source()
                .materialized_universe()
                .unwrap()
                .pattern_count(),
            5040
        );
        let run = |prune: bool| {
            let mut session = ExtendedBuildProbabilitySession::new(
                &problem,
                field,
                BuildProbabilityAggregation::Buildability,
            )
            .unwrap();
            assert_eq!(session.full_draw_required_piece_counts, Some([1; 7]));
            if !prune {
                // Keep the identical source owner, geometry producer, graph
                // kernel and product. Disable only this necessary condition.
                session.full_draw_required_piece_counts = None;
            }
            for _ in 0..4096 {
                match session.advance(16, &ExecutionControl::default()).unwrap() {
                    BuildProbabilityAdvance::Pending => {}
                    BuildProbabilityAdvance::Completed(result) => {
                        return (result, session.searched_build_nodes);
                    }
                    BuildProbabilityAdvance::Cancelled => panic!("fixture was not cancelled"),
                }
            }
            panic!("separated seven-piece fixture did not complete")
        };
        let (reference, reference_nodes) = run(false);
        let (filtered, filtered_nodes) = run(true);
        assert_eq!(
            filtered.coverage_pattern_words(),
            reference.coverage_pattern_words()
        );
        assert_eq!(
            filtered.normalized_solution_keys(),
            reference.normalized_solution_keys()
        );
        assert_eq!(filtered.path_steps(), reference.path_steps());
        for key in [
            "solution_found",
            "count_complete",
            "probability_complete",
            "packing_candidate_count",
            "packing_candidate_set_digest",
            "geometry_candidate_family_count",
            "unique_solution_count",
            "normalized_solution_set_hash",
            "witnessed_pattern_count",
            "covered_pattern_count",
            "coverage_probability",
            "representative_pattern_id",
            "representative_candidate_ordinal",
        ] {
            assert!(reference.field(key).is_some(), "reference field {key}");
            assert_eq!(filtered.field(key), reference.field(key), "{key}");
        }
        assert_eq!(filtered.bool_field("count_complete"), Some(true));
        assert_eq!(filtered.bool_field("solution_found"), Some(!duplicate_s));
        assert!(reference_nodes > 0);
        if duplicate_s {
            // The relaxed packing family retains this physically buildable
            // S/S/no-Z candidate, but no full seven-bag queue can supply it.
            assert_eq!(filtered_nodes, 0);
        } else {
            assert_eq!(filtered.path_steps().len(), 7);
            assert_eq!(filtered_nodes, reference_nodes);
        }
    }
}

#[test]
fn extended_full_draw_multiset_certificate_keeps_held_and_partial_bag_boundaries() {
    let field = |locks| {
        BuildProbabilityField::from_words_preserving_height(
            8,
            [0; 4],
            [(1_u64 << (locks * 4)) - 1, 0, 0, 0],
        )
        .unwrap()
    };
    for (draws, locks, held, expected) in [
        (7, 7, None, Some([1; 7])),
        (7, 8, Some(PieceKind::I), Some([2, 1, 1, 1, 1, 1, 1])),
        (6, 6, None, None), // Known seventh kind may itself be placed.
        (7, 6, None, None), // Not every original draw must become a lock.
    ] {
        let problem = source_problem(draws, locks, held, PcQueueInput::standard_7_bag(), 0);
        let coverage = standard_coverage(&problem);
        assert_eq!(
            ExtendedBuildProbabilitySession::new_full_draw_required_piece_counts(
                &problem,
                field(locks),
                Some(&coverage),
            )
            .unwrap(),
            expected
        );
        assert_eq!(
            ExtendedBuildProbabilitySession::new_full_draw_required_piece_counts(
                &problem,
                field(locks),
                None,
            )
            .unwrap(),
            None
        );
    }
    // The existing independent dense/original-witness regression above uses
    // N=6 and O,T,S,Z,J,L while I remains held. It is a concrete reason not to
    // extend the complete-bag certificate to every projected EOF window.
}

#[test]
fn extended_standard_bag_boolean_gate_preserves_richer_execution_contracts() {
    use clearra_pc_graph::request::PcSolutionProbabilityPolicy;

    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(8, 0),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(2),
    )
    .with_exact_pieces(Some(2))
    .with_supply_window_size(SupplyWindowSize::new(2))
    .with_allow_hold(true)
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_queue_observation_policy(QueueObservationPolicy::FullQueueOracle);
    for unsupported in [
        query.clone().with_count_policy(PcCountPolicy::CountAll),
        query
            .clone()
            .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven),
        query
            .clone()
            .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include),
    ] {
        let problem = ProblemCompiler::compile_scenario_pc(&unsupported).unwrap();
        assert!(
            !ExtendedBuildProbabilitySession::uses_standard_bag_coverage(
                &problem,
                BuildProbabilityAggregation::Buildability,
                false,
            )
            .unwrap()
        );
    }
    let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
    assert!(
        !ExtendedBuildProbabilitySession::uses_standard_bag_coverage(
            &problem,
            BuildProbabilityAggregation::Buildability,
            true,
        )
        .unwrap()
    );
}

#[test]
#[cfg(all(feature = "parallel", not(target_family = "wasm")))]
fn extended_standard_bag_native_workers_materialize_original_ids_before_merge() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(8, 0),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(2),
    )
    .with_exact_pieces(Some(2))
    .with_supply_window_size(SupplyWindowSize::new(2))
    .with_allow_hold(true)
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_queue_observation_policy(QueueObservationPolicy::FullQueueOracle)
    .with_execution_policy(
        PcExecutionPolicy::default()
            .with_use_all_logical_processors(true)
            .with_workers(2),
    );
    let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
    let field =
        BuildProbabilityField::from_words_preserving_height(8, [0; 4], [0x0003_00cf, 0, 0, 0])
            .unwrap();
    let dense = complete_physical_fixture(&problem, field, false);
    let mut session = ExtendedBuildProbabilitySession::new(
        &problem,
        field,
        BuildProbabilityAggregation::Buildability,
    )
    .unwrap();
    assert!(session.standard_bag_coverage.is_some());
    let parallel = session
        .execute_pc_family_parallel(2, &ExecutionControl::default())
        .unwrap()
        .expect("nonempty native Build family uses the requested worker pool");
    assert_eq!(session.workers_used, 2);
    assert_eq!(parallel.coverage_pattern_words(), &[0x41]);
    assert_eq!(
        parallel.coverage_pattern_words(),
        dense.coverage_pattern_words()
    );
    assert_eq!(
        parallel.normalized_solution_keys(),
        dense.normalized_solution_keys()
    );
    assert_eq!(parallel.path_steps(), dense.path_steps());
    for key in [
        "count_complete",
        "probability_complete",
        "unique_solution_count",
        "packing_candidate_count",
        "packing_candidate_set_digest",
        "coverage_probability",
        "representative_pattern_id",
        "representative_candidate_ordinal",
    ] {
        assert_eq!(parallel.field(key), dense.field(key), "{key}");
    }
}
