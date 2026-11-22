//! Independent finite-source root restrictions and physical-language evidence.

use clearra_pc_graph::request::{
    PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    SupplyWindowSize,
};
use clearra_problem::ProblemCompiler;
use clearra_rules::{kicks::KickTableProfileId, profile::builtin_rules::srs_plus};
use clearra_supply::{
    pattern_universe::PatternUniverseMaterializer,
    queue::queue_pattern_expression::QueuePatternExpression,
};

use super::*;

const ORIGINAL_PATTERN_COUNT: usize = 177_811_200;
const FIRST_DRAW_BLOCK_SIZE: usize = 25_401_600;

fn query(expression: &str) -> PcScenarioQuery {
    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(6, 0),
        PcQueueInput::pattern_expression(QueuePatternExpression::parse(expression, 0).unwrap()),
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

struct PhysicalFixture {
    problem: SearchProblem,
    graph: BuildOrderGraph,
    workspace: BuildUpWorkspace,
    language_root: u32,
}

fn physical_fixture() -> PhysicalFixture {
    use PieceKind::{I, J, L, O, S, T, Z};
    let problem = ProblemCompiler::compile_scenario_pc(&query("P1P7P7")).unwrap();
    let catalog = GeometryCatalog::compile(&problem).unwrap();
    // This is the independently verified mixed physical tiling used by the
    // source-prefix graph regression. Its harddrop order is not a source
    // witness; all valid physical orders remain in the graph below.
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
    let candidate = GeometryCandidate::from_rows(&catalog, 0, &rows).unwrap();
    let mut workspace = BuildUpWorkspace::default();
    workspace
        .reachability
        .configure_kick_profile(KickTableProfileId::SrsPlus, false);
    let mut projection = CandidateProjection::compile(
        &catalog,
        &candidate,
        &mut workspace,
        BuildCompletion::ClearToEmpty,
    )
    .unwrap();
    let graph = BuildOrderGraph::build_with_legal_board_admission(
        &problem,
        &catalog,
        &candidate,
        &mut projection,
        &mut workspace,
        true,
        BuildCompletion::ClearToEmpty,
        None,
        BuildReachabilityMode::Existing,
        false,
        &CandidateMovementProofs::default(),
    )
    .unwrap();
    assert!(graph.is_live());
    let language_root = workspace
        .piece_order_languages
        .canonicalize(&graph)
        .unwrap();
    let mut candidate_inventory = [0_u8; 7];
    for &row_id in candidate.row_ids() {
        let piece = catalog.skeleton(row_id).piece;
        let kind = PieceKind::STANDARD_TETROMINOES
            .iter()
            .position(|&kind| kind == piece)
            .unwrap();
        candidate_inventory[kind] += 1;
    }
    assert_eq!(candidate_inventory, [3, 2, 2, 2, 2, 2, 2]);
    assert_eq!(
        accepted_inventory(&workspace.piece_order_languages, language_root),
        AcceptedInventory::Uniform(candidate_inventory),
        "every accepted physical kind word must consume this exact candidate"
    );
    PhysicalFixture {
        problem,
        graph,
        workspace,
        language_root,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AcceptedInventory {
    Empty,
    Uniform([u8; 7]),
    Mixed,
}

/// Visits the complete accepted DAG language, independently of candidate
/// masks and the root restriction. Every alternative contributes its child's
/// inventory plus its edge kind; different resulting inventories remain Mixed.
fn accepted_inventory(language: &PieceOrderLanguageCache, root: u32) -> AcceptedInventory {
    fn visit(
        language: &PieceOrderLanguageCache,
        root: u32,
        memo: &mut BTreeMap<u32, AcceptedInventory>,
    ) -> AcceptedInventory {
        if let Some(&value) = memo.get(&root) {
            return value;
        }
        let node = language.node(root).unwrap();
        let mut result = if node.accepting {
            assert_eq!(node.remaining_depth, 0);
            AcceptedInventory::Uniform([0; 7])
        } else {
            AcceptedInventory::Empty
        };
        for kind in 0..7 {
            for edge in language.edges_for_piece(root, kind as u8 + 1).unwrap() {
                let child = language.node(edge.child()).unwrap();
                assert_eq!(
                    child.remaining_depth.checked_add(1),
                    Some(node.remaining_depth)
                );
                let alternative = match visit(language, edge.child(), memo) {
                    AcceptedInventory::Empty => continue,
                    AcceptedInventory::Mixed => AcceptedInventory::Mixed,
                    AcceptedInventory::Uniform(mut counts) => {
                        counts[kind] = counts[kind].checked_add(1).unwrap();
                        AcceptedInventory::Uniform(counts)
                    }
                };
                result = match result {
                    AcceptedInventory::Empty => alternative,
                    existing if existing == alternative => existing,
                    _ => AcceptedInventory::Mixed,
                };
            }
        }
        memo.insert(root, result);
        result
    }
    visit(language, root, &mut BTreeMap::new())
}

fn mask_graph(masks: &[u8]) -> BuildOrderGraph {
    let mut specs = Vec::new();
    let mut edges = Vec::new();
    for (depth, &mask) in masks.iter().enumerate() {
        specs.push(BuildOrderNodeSpec {
            edge_start: edges.len() as u32,
            edge_count: mask.count_ones(),
            depth: depth as u8,
            accepting: false,
        });
        for (kind, piece) in PieceKind::STANDARD_TETROMINOES.iter().copied().enumerate() {
            if mask & (1 << kind) != 0 {
                edges.push(BuildOrderGraph::edge(
                    depth as u32 + 1,
                    depth as u8,
                    piece,
                    RotationState::Zero,
                    0,
                    0,
                    0,
                ));
            }
        }
    }
    specs.push(BuildOrderNodeSpec {
        edge_start: edges.len() as u32,
        edge_count: 0,
        depth: masks.len() as u8,
        accepting: true,
    });
    BuildOrderGraph::from_topological_parts(specs, edges, 0, 0, true).unwrap()
}

fn append_mask_language(language: &mut PieceOrderLanguageCache, masks: &[u8]) -> u32 {
    language.canonicalize(&mask_graph(masks)).unwrap()
}

fn exchange_i_and_o(graph: &BuildOrderGraph) -> BuildOrderGraph {
    let specs = graph
        .nodes
        .iter()
        .map(|node| BuildOrderNodeSpec {
            edge_start: node.edge_start,
            edge_count: node.edge_count,
            depth: node.depth,
            accepting: node.accepting,
        })
        .collect();
    let edges = graph
        .edges
        .iter()
        .map(|edge| {
            BuildOrderGraph::edge(
                edge.to,
                edge.operation_index,
                match edge.piece {
                    PieceKind::I => PieceKind::O,
                    PieceKind::O => PieceKind::I,
                    other => other,
                },
                edge.rotation,
                edge.x,
                edge.y,
                edge.cleared_lines,
            )
        })
        .collect();
    // This is a synthetic kind-language graph. Renaming piece kinds does not
    // claim that its unchanged cell coordinates form a physical O/I tiling.
    BuildOrderGraph::from_topological_parts(specs, edges, graph.root, 0, false).unwrap()
}

fn witness_path(fixture: &PhysicalFixture, ordinal: u32) -> Vec<CorePathStep> {
    let universe = fixture
        .problem
        .piece_source()
        .materialized_universe()
        .unwrap();
    let queue = universe.sequence_at(ordinal as usize);
    first_pattern_path(
        &fixture.graph,
        queue.as_ref(),
        fixture.problem.supply().hold_enabled(),
        fixture.problem.supply().projects_unplaced_lookahead(),
        fixture.problem.supply().projects_standard_bag_lookahead(),
        fixture.problem.initial_hold(),
        CoverageState {
            node: fixture.graph.root,
            cursor: fixture.problem.initial_hold().cursor(),
            hold: fixture.problem.initial_hold().hold_piece(),
        },
        None,
    )
}

fn product_for(problem: &SearchProblem) -> StandardBagCoverage {
    StandardBagCoverage::for_universe(
        problem.piece_source().materialized_universe().unwrap(),
        problem.initial_hold(),
        problem.supply().hold_enabled(),
        problem.supply().projects_unplaced_lookahead(),
    )
    .unwrap()
    .unwrap()
}

fn assert_same_coverage(actual: &PatternBitSet, expected: &PatternBitSet) {
    assert_eq!(actual.pattern_count(), ORIGINAL_PATTERN_COUNT);
    assert_eq!(actual.pattern_count(), expected.pattern_count());
    assert_eq!(actual.word_count(), expected.word_count());
    for word in 0..actual.word_count() {
        assert_eq!(
            actual.word_at(word),
            expected.word_at(word),
            "original word={word}"
        );
    }
}

fn assert_only_extra_i_block_is_covered(words: &PatternBitSet) {
    assert_eq!(words.pattern_count(), ORIGINAL_PATTERN_COUNT);
    assert_eq!(FIRST_DRAW_BLOCK_SIZE % 64, 0);
    let block_words = FIRST_DRAW_BLOCK_SIZE / 64;
    for first_draw_rank in 1..7 {
        for word in first_draw_rank * block_words..(first_draw_rank + 1) * block_words {
            assert_eq!(
                words.word_at(word),
                0,
                "wrong initial draw rank={first_draw_rank}, original word={word}"
            );
        }
    }
    assert_eq!(words.count_ones(), 5_846_037);
}

fn assert_fallback_was_unrestricted(product: &StandardBagCoverage) {
    let snapshot = product.first_draw_block_snapshot().unwrap().unwrap();
    assert_eq!(snapshot.root_restrictions, 0);
    assert_eq!(snapshot.root_draw_branches_skipped, 0);
}

#[test]
fn full6_full_draw_root_preserves_original_bitmap_minimum_witness_and_physical_path() {
    let mut fixture = physical_fixture();
    let token = fixture
        .graph
        .full_draw_source_root(&mut fixture.workspace.piece_order_languages)
        .unwrap()
        .expect("the actual full15 physical producer issues this root");
    assert_eq!(token.language_root(), fixture.language_root);
    let language = &fixture.workspace.piece_order_languages;
    let control = ExecutionControl::default();

    let mut reference = product_for(&fixture.problem);
    let expected = reference
        .cover_language(language, fixture.language_root, &control)
        .unwrap();
    assert!(expected.covers_any_pattern);
    assert_eq!(expected.covered_pattern_count, 5_846_037);
    assert_eq!(expected.witness_pattern_id, Some(0));
    let expected_words = reference.materialize_root(expected.root).unwrap();
    assert_only_extra_i_block_is_covered(&expected_words);
    assert_fallback_was_unrestricted(&reference);

    let mut restricted = product_for(&fixture.problem);
    let actual = restricted
        .cover_language_with_full_draw_root(language, fixture.language_root, &token, &control)
        .unwrap();
    assert_eq!(actual.covers_any_pattern, expected.covers_any_pattern);
    assert_eq!(actual.covered_pattern_count, expected.covered_pattern_count);
    assert_eq!(actual.witness_pattern_id, expected.witness_pattern_id);
    let actual_words = restricted.materialize_root(actual.root).unwrap();
    assert_same_coverage(&actual_words, &expected_words);
    assert_only_extra_i_block_is_covered(&actual_words);
    let expected_path = witness_path(&fixture, expected.witness_pattern_id.unwrap());
    let actual_path = witness_path(&fixture, actual.witness_pattern_id.unwrap());
    assert_eq!(expected_path.len(), 15);
    assert_eq!(actual_path, expected_path);

    let snapshot = restricted.first_draw_block_snapshot().unwrap().unwrap();
    assert_eq!(snapshot.root_restrictions, 1);
    assert_eq!(snapshot.root_draw_branches_skipped, 6);
    assert!(actual.product_states < expected.product_states);
    assert!(actual.edge_checks < expected.edge_checks);
    eprintln!(
        "full6 root inventory: product_states {} -> {}, edge_checks {} -> {}, covered={}, minimum={:?}, skipped_draws={}",
        expected.product_states,
        actual.product_states,
        expected.edge_checks,
        actual.edge_checks,
        actual.covered_pattern_count,
        actual.witness_pattern_id,
        snapshot.root_draw_branches_skipped,
    );

    // The restricted result is the exact ordinary product, so an ordinary
    // consumer of the shared memo must see every original queue unchanged.
    let memo_revisit = restricted
        .cover_language(language, fixture.language_root, &control)
        .unwrap();
    assert_eq!(
        memo_revisit.covered_pattern_count,
        expected.covered_pattern_count
    );
    assert_eq!(memo_revisit.witness_pattern_id, expected.witness_pattern_id);
    let memo_words = restricted.materialize_root(memo_revisit.root).unwrap();
    assert_same_coverage(&memo_words, &expected_words);
}

#[test]
fn full6_full_draw_root_other_and_mixed_language_fallback_cannot_poison_shared_memo() {
    let mut fixture = physical_fixture();
    let token = fixture
        .graph
        .full_draw_source_root(&mut fixture.workspace.piece_order_languages)
        .unwrap()
        .unwrap();
    // These words are concrete originals: O + bag + bag, or I + bag + bag.
    // Both accept without Hold, while their complete inventories differ.
    let mut masks = vec![1 << 1];
    for _ in 0..2 {
        masks.extend((0..7).map(|kind| 1 << kind));
    }
    let other_root = append_mask_language(&mut fixture.workspace.piece_order_languages, &masks);
    masks[0] |= 1;
    let mixed_root = append_mask_language(&mut fixture.workspace.piece_order_languages, &masks);
    let language = &fixture.workspace.piece_order_languages;
    assert_ne!(other_root, token.language_root());
    assert_ne!(mixed_root, token.language_root());
    assert_eq!(
        accepted_inventory(language, other_root),
        AcceptedInventory::Uniform([2, 3, 2, 2, 2, 2, 2])
    );
    assert_eq!(
        accepted_inventory(language, mixed_root),
        AcceptedInventory::Mixed
    );
    let control = ExecutionControl::default();
    let mut guarded = product_for(&fixture.problem);
    for (root, minimum) in [(other_root, FIRST_DRAW_BLOCK_SIZE as u32), (mixed_root, 0)] {
        let mut reference = product_for(&fixture.problem);
        let expected = reference.cover_language(language, root, &control).unwrap();
        assert!(expected.covers_any_pattern);
        assert_eq!(expected.witness_pattern_id, Some(minimum));
        let actual = guarded
            .cover_language_with_full_draw_root(language, root, &token, &control)
            .unwrap();
        assert_eq!(actual.covered_pattern_count, expected.covered_pattern_count);
        assert_eq!(actual.witness_pattern_id, expected.witness_pattern_id);
        assert_same_coverage(
            &guarded.materialize_root(actual.root).unwrap(),
            &reference.materialize_root(expected.root).unwrap(),
        );
        assert_fallback_was_unrestricted(&guarded);
    }
    // Invalid uses did not replace or narrow this actual root's product key.
    let original = guarded
        .cover_language_with_full_draw_root(language, fixture.language_root, &token, &control)
        .unwrap();
    assert_eq!(original.covered_pattern_count, 5_846_037);
    assert_eq!(original.witness_pattern_id, Some(0));
    assert_only_extra_i_block_is_covered(&guarded.materialize_root(original.root).unwrap());
    let after = guarded.first_draw_block_snapshot().unwrap().unwrap();
    assert_eq!(after.root_restrictions, 1);
    assert_eq!(after.root_draw_branches_skipped, 6);
}

#[test]
fn full6_full_draw_root_recycled_equal_numeric_id_has_no_old_inventory_authority() {
    let mut fixture = physical_fixture();
    let token = fixture
        .graph
        .full_draw_source_root(&mut fixture.workspace.piece_order_languages)
        .unwrap()
        .unwrap();
    let control = ExecutionControl::default();
    let mut guarded = product_for(&fixture.problem);
    let old_product = guarded
        .cover_language_with_full_draw_root(
            &fixture.workspace.piece_order_languages,
            fixture.language_root,
            &token,
            &control,
        )
        .unwrap();
    let old_words = guarded.materialize_root(old_product.root).unwrap();
    assert_only_extra_i_block_is_covered(&old_words);
    guarded.merge_global(old_product.root).unwrap();
    let old_arena = Arc::clone(fixture.workspace.piece_order_languages.arena_identity());
    let mut renamed = exchange_i_and_o(&fixture.graph);
    fixture
        .workspace
        .piece_order_languages
        .clear_retain_capacity();
    assert!(!Arc::ptr_eq(
        &old_arena,
        fixture.workspace.piece_order_languages.arena_identity()
    ));
    let replacement_root = fixture
        .workspace
        .piece_order_languages
        .canonicalize(&renamed)
        .unwrap();
    assert_eq!(
        replacement_root,
        token.language_root(),
        "bijective label renaming reuses the numeric root after reset"
    );
    assert_eq!(
        accepted_inventory(&fixture.workspace.piece_order_languages, replacement_root),
        AcceptedInventory::Uniform([2, 3, 2, 2, 2, 2, 2])
    );
    assert!(renamed
        .full_draw_source_root(&mut fixture.workspace.piece_order_languages)
        .unwrap()
        .is_none());
    assert!(fixture
        .graph
        .full_draw_source_root(&mut fixture.workspace.piece_order_languages)
        .unwrap()
        .is_none());
    let language = &fixture.workspace.piece_order_languages;
    let mut reference = product_for(&fixture.problem);
    let expected = reference
        .cover_language(language, replacement_root, &control)
        .unwrap();
    // The source's two complete bags are invariant under a kind bijection.
    // This synthetic language has the same coverage count in the O block.
    assert_eq!(expected.covered_pattern_count, 5_846_037);
    let minimum = expected.witness_pattern_id.unwrap() as usize;
    assert!((FIRST_DRAW_BLOCK_SIZE..2 * FIRST_DRAW_BLOCK_SIZE).contains(&minimum));
    // Reuse the same product owner, including its memo for this numeric root.
    // It must flush old exact coverage and recycle the memo before fallback.
    let actual = guarded
        .cover_language_with_full_draw_root(language, replacement_root, &token, &control)
        .unwrap();
    assert_eq!(actual.covered_pattern_count, expected.covered_pattern_count);
    assert_eq!(actual.witness_pattern_id, expected.witness_pattern_id);
    assert_same_coverage(
        &guarded.materialize_root(actual.root).unwrap(),
        &reference.materialize_root(expected.root).unwrap(),
    );
    let snapshot = guarded.first_draw_block_snapshot().unwrap().unwrap();
    assert_eq!(
        snapshot.root_restrictions, 1,
        "the stale token was not applied again"
    );
    assert_eq!(snapshot.root_draw_branches_skipped, 6);
    assert_eq!(snapshot.flushed_counts[0], 5_846_037);
    assert_eq!(snapshot.epoch_counts, [0; 7]);
    guarded.merge_global(actual.root).unwrap();
    let new_words = reference.materialize_root(expected.root).unwrap();
    let union = guarded.materialize_global().unwrap();
    assert_eq!(union.count_ones(), 2 * 5_846_037);
    for word in 0..union.word_count() {
        assert_eq!(
            union.word_at(word),
            old_words.word_at(word) | new_words.word_at(word),
            "flushed union word={word}"
        );
    }
}

#[test]
fn full6_full_draw_root_source_owner_and_synthetic_issuer_cannot_grant_authority() {
    let mut fixture = physical_fixture();
    let token = fixture
        .graph
        .full_draw_source_root(&mut fixture.workspace.piece_order_languages)
        .unwrap()
        .unwrap();
    let mut masks = vec![1];
    for _ in 0..2 {
        masks.extend((0..7).map(|kind| 1 << kind));
    }
    let mut synthetic = mask_graph(&masks);
    assert!(synthetic
        .full_draw_source_root(&mut fixture.workspace.piece_order_languages)
        .unwrap()
        .is_none());
    let language = &fixture.workspace.piece_order_languages;
    let universe = fixture
        .problem
        .piece_source()
        .materialized_universe()
        .unwrap();
    let identity = |initial, hold, projected| {
        super::super::standard_bag_coverage::StandardBagRequestIdentity::for_universe(
            universe, initial, hold, projected,
        )
    };
    assert_eq!(
        token.first_draw_for(
            language,
            fixture.language_root,
            identity(fixture.problem.initial_hold(), true, true)
        ),
        Some(0)
    );
    let mut initially_held = fixture.problem.initial_hold();
    initially_held.hold_piece = Some(PieceKind::I);
    initially_held.hold_empty = false;
    assert!(token
        .first_draw_for(
            language,
            fixture.language_root,
            identity(initially_held, true, true)
        )
        .is_none());
    assert!(token
        .first_draw_for(
            language,
            fixture.language_root,
            identity(fixture.problem.initial_hold(), true, false)
        )
        .is_none());
    let no_hold_problem =
        ProblemCompiler::compile_scenario_pc(&query("P1P7P7").with_allow_hold(false)).unwrap();
    assert!(token
        .first_draw_for(
            language,
            fixture.language_root,
            identity(no_hold_problem.initial_hold(), false, false)
        )
        .is_none());
    let same_size_other_layout = ProblemCompiler::compile_scenario_pc(&query("P7P1P7")).unwrap();
    let other_layout_identity =
        super::super::standard_bag_coverage::StandardBagRequestIdentity::for_universe(
            same_size_other_layout
                .piece_source()
                .materialized_universe()
                .unwrap(),
            same_size_other_layout.initial_hold(),
            same_size_other_layout.supply().hold_enabled(),
            same_size_other_layout
                .supply()
                .projects_unplaced_lookahead(),
        );
    assert!(token
        .first_draw_for(language, fixture.language_root, other_layout_identity)
        .is_none());
    let foreign_universe = PatternUniverseMaterializer::queue_pattern_expression(
        &QueuePatternExpression::parse("P1P7P7", 0).unwrap(),
        83,
    )
    .unwrap();
    let foreign_initial = HoldAutomatonState::with_contract(
        PieceSourceId::new(83),
        PieceSourceKind::MaterializedPatternUniverse,
        0,
        None,
        HoldPolicy::Allowed,
        0,
        0,
        SupplyObservationIdentity::full_queue_oracle(),
        SupplyProvenanceId(83),
    );
    assert_ne!(
        foreign_initial.piece_source_id(),
        fixture.problem.initial_hold().piece_source_id()
    );
    let make_foreign = || {
        StandardBagCoverage::for_universe(&foreign_universe, foreign_initial, true, true)
            .unwrap()
            .unwrap()
    };
    let control = ExecutionControl::default();
    let mut reference = make_foreign();
    let expected = reference
        .cover_language(language, fixture.language_root, &control)
        .unwrap();
    let mut guarded = make_foreign();
    let actual = guarded
        .cover_language_with_full_draw_root(language, fixture.language_root, &token, &control)
        .unwrap();
    assert_eq!(actual.covered_pattern_count, 5_846_037);
    assert_eq!(actual.covered_pattern_count, expected.covered_pattern_count);
    assert_eq!(actual.witness_pattern_id, expected.witness_pattern_id);
    assert_same_coverage(
        &guarded.materialize_root(actual.root).unwrap(),
        &reference.materialize_root(expected.root).unwrap(),
    );
    assert_fallback_was_unrestricted(&guarded);
}
