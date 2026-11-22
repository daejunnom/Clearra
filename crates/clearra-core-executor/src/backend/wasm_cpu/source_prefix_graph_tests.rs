use clearra_pc_graph::request::{
    PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    SupplyWindowSize,
};
use clearra_problem::ProblemCompiler;
use clearra_rules::{kicks::KickTableProfileId, profile::builtin_rules::srs_plus};
use clearra_supply::queue::{
    fixed_sequence::FixedSequence, queue_pattern_expression::QueuePatternExpression,
};

use super::super::source_prefix::{self, FullDrawSourcePrefix};
use super::*;

fn query(queue: PcQueueInput) -> PcScenarioQuery {
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(6, 0),
        queue,
        PieceWindow::new(15),
    )
    .with_rule(srs_plus())
    .with_exact_pieces(Some(15))
    .with_supply_window_size(SupplyWindowSize::new(15))
    .with_hold_piece(None)
    .with_allow_hold(true)
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_queue_observation_policy(QueueObservationPolicy::FullQueueOracle)
    .with_retained_trace_limit(1)
    .with_execution_policy(
        PcExecutionPolicy::mvp_default()
            .with_max_patterns(0)
            .with_exact_legal_board_enabled(false)
            .with_conditioned_reachability_enabled(false),
    )
}

fn mixed_candidate(catalog: &GeometryCatalog) -> (GeometryCandidate, Vec<PieceKind>) {
    use PieceKind::{I, J, L, O, S, T, Z};
    // This tiling has an independently tested physical harddrop order. That
    // particular word violates P1P7P7's first-seven inventory; it is not a
    // source witness. The whole graph's source intersection is tested below.
    let ordered_cells = [
        (I, 0x4010_0401_u64),
        (T, 0x100e),
        (T, 0x8070),
        (Z, 0x0003_0180),
        (S, 0x0300_6000),
        (S, 0x0006_00c0_0000),
        (J, 0x0001_8020_0800),
        (Z, 0x0018_0c00_0000),
        (L, 0x0060_1004_0000),
        (I, 0x0080_2008_0200),
        (I, 0x001e << 40),
        (O, 0x0003_00c0 << 40),
        (O, 0x000c_0300 << 40),
        (L, 0x1c01 << 40),
        (J, 0xe020 << 40),
    ];
    let rows = ordered_cells
        .iter()
        .map(|&(piece, cells)| catalog.skeleton_id(piece, cells).unwrap())
        .collect::<Vec<_>>();
    let candidate = GeometryCandidate::from_rows(catalog, 0, &rows).unwrap();
    let physical_word = ordered_cells.iter().map(|&(piece, _)| piece).collect();
    (candidate, physical_word)
}

fn graph(
    problem: &SearchProblem,
    catalog: &GeometryCatalog,
    candidate: &GeometryCandidate,
    piece_language_projection_only: bool,
) -> BuildOrderGraph {
    let mut workspace = BuildUpWorkspace::default();
    workspace
        .reachability
        .configure_kick_profile(KickTableProfileId::SrsPlus, false);
    let mut projection = CandidateProjection::compile(
        catalog,
        candidate,
        &mut workspace,
        BuildCompletion::ClearToEmpty,
    )
    .unwrap();
    BuildOrderGraph::build_with_legal_board_admission(
        problem,
        catalog,
        candidate,
        &mut projection,
        &mut workspace,
        piece_language_projection_only,
        BuildCompletion::ClearToEmpty,
        None,
        BuildReachabilityMode::Existing,
        false,
        &CandidateMovementProofs::default(),
    )
    .unwrap()
}

fn path_for_queue(
    problem: &SearchProblem,
    graph: &BuildOrderGraph,
    queue: &[PieceKind],
) -> Vec<CorePathStep> {
    first_pattern_path(
        graph,
        queue,
        problem.supply().hold_enabled(),
        problem.supply().projects_unplaced_lookahead(),
        problem.supply().projects_standard_bag_lookahead(),
        problem.initial_hold(),
        CoverageState {
            node: graph.root,
            cursor: problem.initial_hold().cursor(),
            hold: problem.initial_hold().hold_piece(),
        },
        None,
    )
}

struct SourceSummary {
    covered_count: usize,
    witness: Option<u32>,
    words: Option<PatternBitSet>,
    path: Vec<CorePathStep>,
}

fn source_summary(problem: &SearchProblem, graph: &BuildOrderGraph) -> SourceSummary {
    if !graph.is_live() {
        // The product verifier also bypasses the source product for a graph
        // with no complete physical path. The unfiltered reference below is
        // live and still runs the exact source product before this comparison.
        return SourceSummary {
            covered_count: 0,
            witness: None,
            words: None,
            path: Vec::new(),
        };
    }
    let universe = problem.piece_source().materialized_universe().unwrap();
    let mut language = PieceOrderLanguageCache::default();
    let root = language.canonicalize(graph).unwrap();
    let mut product = StandardBagCoverage::for_universe(
        universe,
        problem.initial_hold(),
        problem.supply().hold_enabled(),
        problem.supply().projects_unplaced_lookahead(),
    )
    .unwrap()
    .unwrap();
    let covered = product
        .cover_language(&language, root, &ExecutionControl::default())
        .unwrap();
    assert_eq!(
        covered.covers_any_pattern,
        covered.covered_pattern_count != 0
    );
    assert_eq!(
        covered.witness_pattern_id.is_some(),
        covered.covers_any_pattern
    );
    // Exact count zero proves both accepted sets empty. Otherwise compare
    // every original ordinal bit, not only equal counts or a digest.
    let words = covered
        .covers_any_pattern
        .then(|| product.materialize_root(covered.root).unwrap());
    let path = covered.witness_pattern_id.map_or_else(Vec::new, |ordinal| {
        let queue = universe.sequence_at(ordinal as usize);
        let path = path_for_queue(problem, graph, queue.as_ref());
        assert_eq!(path.len(), 15);
        path
    });
    SourceSummary {
        covered_count: covered.covered_pattern_count,
        witness: covered.witness_pattern_id,
        words,
        path,
    }
}

#[test]
fn full6_source_prefix_preserves_exact_graph_source_intersection_and_witness() {
    let problem = ProblemCompiler::compile_scenario_pc(&query(PcQueueInput::pattern_expression(
        QueuePatternExpression::parse("P1P7P7", 0).unwrap(),
    )))
    .unwrap();
    let catalog = GeometryCatalog::compile(&problem).unwrap();
    let (candidate, _) = mixed_candidate(&catalog);
    assert!(FullDrawSourcePrefix::compile(&problem, &catalog, &candidate).is_some());
    let raw = source_prefix::with_disabled(|| {
        assert!(FullDrawSourcePrefix::compile(&problem, &catalog, &candidate).is_none());
        graph(&problem, &catalog, &candidate, true)
    });
    assert!(
        raw.is_live(),
        "the unfiltered physical candidate is buildable"
    );
    let raw_source = source_summary(&problem, &raw);
    let pruned = graph(&problem, &catalog, &candidate, true);
    let pruned_source = source_summary(&problem, &pruned);
    eprintln!(
        "full6 source prefix: raw_nodes={} pruned_nodes={} raw_edges={} pruned_edges={} raw_reachability={} pruned_reachability={} raw_covered={} pruned_covered={} raw_witness={:?} pruned_witness={:?}",
        raw.nodes.len(), pruned.nodes.len(), raw.edges.len(), pruned.edges.len(),
        raw.reachability_states, pruned.reachability_states,
        raw_source.covered_count, pruned_source.covered_count,
        raw_source.witness, pruned_source.witness,
    );
    assert!(pruned.nodes.len() < raw.nodes.len());
    assert!(pruned.edges.len() < raw.edges.len());
    assert_eq!(pruned_source.covered_count, raw_source.covered_count);
    assert_eq!(pruned_source.witness, raw_source.witness);
    assert_eq!(pruned_source.path, raw_source.path);
    match (&pruned_source.words, &raw_source.words) {
        (None, None) => assert_eq!(raw_source.covered_count, 0),
        (Some(actual), Some(expected)) => {
            assert_eq!(actual.pattern_count(), 177_811_200);
            assert_eq!(actual.pattern_count(), expected.pattern_count());
            assert_eq!(actual.word_count(), expected.word_count());
            for word in 0..actual.word_count() {
                assert_eq!(actual.word_at(word), expected.word_at(word), "word={word}");
            }
        }
        _ => panic!("source prefix changed original queue coverage availability"),
    }

    // Exact physical graph exports deliberately request every operation edge.
    // They must still accept the supplied fixed physical order even when that
    // order cannot occur in the P1P7P7 source.
    let exported = graph(&problem, &catalog, &candidate, false);
    let reference_export =
        source_prefix::with_disabled(|| graph(&problem, &catalog, &candidate, false));
    assert_eq!(exported.nodes.len(), reference_export.nodes.len());
    assert_eq!(exported.edges, reference_export.edges);
    assert_eq!(exported.piece_edges, reference_export.piece_edges);
}

#[test]
fn full6_source_prefix_keeps_arbitrary_fixed_queue_on_the_unfiltered_graph() {
    let factorized = ProblemCompiler::compile_scenario_pc(&query(
        PcQueueInput::pattern_expression(QueuePatternExpression::parse("P1P7P7", 0).unwrap()),
    ))
    .unwrap();
    let catalog = GeometryCatalog::compile(&factorized).unwrap();
    let (candidate, physical_word) = mixed_candidate(&catalog);
    let fixed = ProblemCompiler::compile_scenario_pc(&query(PcQueueInput::fixed_sequence(
        FixedSequence::new(physical_word.clone()),
    )))
    .unwrap();
    assert!(FullDrawSourcePrefix::compile(&fixed, &catalog, &candidate).is_none());
    let actual = graph(&fixed, &catalog, &candidate, true);
    assert!(actual.is_live());
    assert_eq!(path_for_queue(&fixed, &actual, &physical_word).len(), 15);
}
