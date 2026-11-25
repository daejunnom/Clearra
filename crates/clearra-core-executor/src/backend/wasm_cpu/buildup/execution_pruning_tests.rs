//! Differential oracles for request-local execution pruning. The reference
//! completes the physical language and applies the existing B2B predicate at
//! the end; graph allocation indexes are never solution identity.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use clearra_core_domain::solution::normalized_tiling_solution::PiecePlacementMask;
use clearra_objectives::policy::{
    objective_policy::ObjectivePolicy, score_objective_policy::SpinProfileSelection,
};
use clearra_pc_graph::request::{
    PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PcSolutionProbabilityPolicy,
    PieceWindow, RequestedSearchBackend, SupplyWindowSize,
};
use clearra_problem::{
    query::build_probability_query::{
        BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityFinesseRequest,
        FinessePatternKnowledge,
    },
    ProblemCompiler,
};
use clearra_rules::{
    kicks::{KickTableProfileId, NoKick},
    profile::{builtin_rules, rule_profile::RuleProfile},
    spawn::SpawnProfile,
};
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::super::build_probability::{BuildProbabilityAdvance, WasmBuildProbabilitySession};
use super::*;
use crate::{CoreExecutionResult, WasmCpuSearchBackend};

static PRODUCT_TEST_LOCK: Mutex<()> = Mutex::new(());

fn product_test_guard() -> MutexGuard<'static, ()> {
    // This lock only serializes resource-heavy fixtures; it owns no mutable
    // product state. The pruning TLS has its own unwind-safe restore guard.
    // A failed assertion must not prevent independent fixtures from running.
    PRODUCT_TEST_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

const SPIN_PROFILES: [SpinProfileSelection; 6] = [
    SpinProfileSelection::TSpins,
    SpinProfileSelection::TSpinsPlus,
    SpinProfileSelection::AllSpin,
    SpinProfileSelection::AllSpinPlus,
    SpinProfileSelection::AllMini,
    SpinProfileSelection::AllMiniPlus,
];

const RULES: [KickTableProfileId; 5] = [
    KickTableProfileId::Srs90,
    KickTableProfileId::SrsPlus,
    KickTableProfileId::SrsX,
    KickTableProfileId::Jstris180,
    KickTableProfileId::NoKick,
];

#[test]
fn typed_chance_constraint_survives_disabled_optional_semantic_pruning() {
    use clearra_problem::SearchOutputPolicy;
    use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

    for spin in SPIN_PROFILES {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, 0xfc3f0),
            PcQueueInput::pattern_expression(QueuePatternExpression::parse("II;OO", 2).unwrap()),
            PieceWindow::new(2),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(2))
        .with_supply_window_size(SupplyWindowSize::new(2))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_objective(ObjectivePolicy::unique().with_back_to_back_preservation(spin))
        .with_execution_policy(execution_policy());
        let problem = ProblemCompiler::compile_scenario_pc(&query)
            .unwrap()
            .with_pc_chance_probability_v2_evidence()
            .with_output_policy(SearchOutputPolicy::CoverageSummary);
        let result = with_semantic_pruning(false, || {
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .unwrap()
        });
        assert_eq!(result.coverage_pattern_words(), &[2], "{spin:?}");
        assert_eq!(
            result.field("coverage_probability"),
            Some("0.5"),
            "{spin:?}"
        );
        assert_eq!(
            result.field("execution_constraint_materialized"),
            Some("true")
        );
        assert!(result.pc_chance_coverage_evidence().unwrap().complete());
    }
}

fn rule(id: KickTableProfileId) -> RuleProfile {
    match id {
        KickTableProfileId::Srs90 => builtin_rules::srs(),
        KickTableProfileId::SrsPlus => builtin_rules::srs_plus(),
        KickTableProfileId::SrsX => builtin_rules::srs_x(),
        KickTableProfileId::Jstris180 => builtin_rules::jstris_180(),
        KickTableProfileId::NoKick => builtin_rules::no_kick(),
        _ => unreachable!("the test uses the five builtin ordered kick profiles"),
    }
}

fn execution_policy() -> PcExecutionPolicy {
    // A test-only TLS switch belongs to this thread. This is not a product
    // worker-policy change or a benchmark against Auto/all logical CPUs.
    PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_workers(1)
        .with_exact_legal_board_enabled(false)
        .with_tablebase_requested(false)
}

fn fixed_problem(
    height: u16,
    base: u64,
    pieces: Vec<PieceKind>,
    rule_id: KickTableProfileId,
    spin: SpinProfileSelection,
) -> SearchProblem {
    let locks = pieces.len();
    ProblemCompiler::compile_scenario_pc(
        &PcScenarioQuery::new(
            PcScenarioBoard::standard_10(height, base),
            PcQueueInput::fixed_sequence(FixedSequence::new(pieces)),
            PieceWindow::new(locks),
        )
        .with_exact_pieces(Some(locks))
        .with_allow_hold(false)
        .with_rule(rule(rule_id))
        .with_count_policy(PcCountPolicy::CountAll)
        .with_objective(ObjectivePolicy::all().with_back_to_back_preservation(spin))
        .with_execution_policy(execution_policy()),
    )
    .expect("fixed execution-pruning problem")
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct PhysicalStep {
    operation: u8,
    action: GeometryActionKey,
    cleared_lines: u8,
    perfect_clear: bool,
    blocked_t_corners: u8,
    blocked_t_front_corners: u8,
    lock: LockEvidenceKey,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct LockEvidenceKey {
    last_action_was_rotation: bool,
    used_kick: bool,
    used_180: bool,
    from_rotation: RotationState,
    rotation_request: u8,
    kick_index: u8,
    kick_dx: i8,
    kick_dy: i8,
    predecessor: (i8, i8),
    first_success_confirmed: bool,
    immobile_before_clear: bool,
}

impl From<ScoringLockEvidence> for LockEvidenceKey {
    fn from(evidence: ScoringLockEvidence) -> Self {
        Self {
            last_action_was_rotation: evidence.last_action_was_rotation(),
            used_kick: evidence.used_kick(),
            used_180: evidence.used_180(),
            from_rotation: evidence.from_rotation(),
            rotation_request: match evidence.rotation_request() {
                RotationRequest::None => 0,
                RotationRequest::Clockwise => 1,
                RotationRequest::CounterClockwise => 2,
                RotationRequest::HalfTurn => 3,
            },
            kick_index: evidence.kick_index(),
            kick_dx: evidence.kick_dx(),
            kick_dy: evidence.kick_dy(),
            predecessor: evidence.predecessor(),
            first_success_confirmed: evidence.first_success_confirmed(),
            immobile_before_clear: evidence.immobile_before_clear(),
        }
    }
}

type PhysicalPaths = BTreeMap<Vec<PhysicalStep>, usize>;

fn completed_paths(
    graph: Option<&ExactScoringExecutionGraph>,
    policy: Option<BackToBackEdgePolicy>,
) -> PhysicalPaths {
    fn visit(
        graph: &ExactScoringExecutionGraph,
        node_id: u32,
        policy: Option<BackToBackEdgePolicy>,
        path: &mut Vec<PhysicalStep>,
        output: &mut PhysicalPaths,
    ) {
        let node = graph.node(node_id).expect("valid graph node");
        if node.accepting() {
            *output.entry(path.clone()).or_default() += 1;
        }
        for edge in graph.edges(node) {
            assert!(edge.to() > node_id, "execution graph must remain acyclic");
            if policy.is_some_and(|policy| !policy.allows(*edge)) {
                continue;
            }
            path.push(PhysicalStep {
                operation: edge.operation_index(),
                action: GeometryActionKey::new(
                    edge.piece(),
                    edge.rotation(),
                    i16::from(edge.x()),
                    i16::from(edge.y()),
                ),
                cleared_lines: edge.cleared_lines(),
                perfect_clear: edge.perfect_clear(),
                blocked_t_corners: edge.blocked_t_corners(),
                blocked_t_front_corners: edge.blocked_t_front_corners(),
                lock: edge.lock_evidence().into(),
            });
            visit(graph, edge.to(), policy, path, output);
            path.pop();
        }
    }

    let mut output = PhysicalPaths::new();
    if let Some(graph) = graph {
        visit(graph, graph.root(), policy, &mut Vec::new(), &mut output);
    }
    output
}

fn scoring_graph(
    problem: &SearchProblem,
    base: u64,
    placements: &[(PieceKind, u64)],
    final_board: u64,
    pruning: bool,
) -> Option<ExactScoringExecutionGraph> {
    let target = placements.iter().fold(0, |cells, (_, mask)| cells | mask);
    let catalog = GeometryCatalog::compile_for_build_probability_on_board(problem, base, target)
        .expect("build catalog");
    let identity = StandardBoard64TilingIdentity::from_placements(
        base,
        placements
            .iter()
            .map(|&(piece, mask)| PiecePlacementMask::new(piece, mask)),
    )
    .expect("canonical fixed placement identity");
    with_semantic_pruning(pruning, || {
        exact_scoring_execution_graph_for_completion(
            problem,
            &catalog,
            identity,
            0,
            &mut BuildUpWorkspace::default(),
            BuildCompletion::ExactBoardAfterLineClears(final_board),
        )
        .expect("complete scoring language")
    })
}

#[test]
fn semantic_pruning_matches_completed_b2b_paths_for_pc_plain_and_spin_clears() {
    let _guard = product_test_guard();
    for rule_id in RULES {
        for spin in SPIN_PROFILES {
            let policy = BackToBackEdgePolicy::new(spin);
            for (label, height, base, placement, final_board) in [
                ("one-I PC", 2, 0x3f0, (PieceKind::I, 0xf), 0),
                // Keep the leftover block above x=9, outside the I's x=0..3
                // drop corridor. Putting it above x=0 roofs over the target
                // and makes the nonempty reference premise rule-dependent.
                ("one-I non-PC", 2, 0x803f0, (PieceKind::I, 0xf), 0x200),
                // A real SRS T mini single. From Left centered at (2, 1),
                // rotate clockwise to Zero. The first two tests collide
                // with upper-row blockers x=3 and x=0; (-1, -1) succeeds.
                // The two floor corners plus upper x=0 give three corners.
                ("T kick into pocket", 3, 0x27f8, (PieceKind::T, 0x807), 0xb),
            ] {
                let problem = fixed_problem(height, base, vec![placement.0], rule_id, spin);
                let reference = scoring_graph(&problem, base, &[placement], final_board, false);
                let pruned = scoring_graph(&problem, base, &[placement], final_board, true);
                let expected = completed_paths(reference.as_ref(), Some(policy));
                let actual = completed_paths(pruned.as_ref(), Some(policy));
                assert_eq!(actual, expected, "{label}, {rule_id:?}, {spin:?}");
                if label != "T kick into pocket" {
                    assert!(
                        !completed_paths(reference.as_ref(), None).is_empty(),
                        "{label}, {rule_id:?}, {spin:?}: the I has an unobstructed hard-drop path"
                    );
                }
                if label == "one-I PC" {
                    assert!(
                        !expected.is_empty(),
                        "{label}, {rule_id:?}, {spin:?}: PC exception must remain live"
                    );
                } else if label == "one-I non-PC"
                    && matches!(
                        spin,
                        SpinProfileSelection::TSpins | SpinProfileSelection::TSpinsPlus
                    )
                {
                    assert!(
                        expected.is_empty(),
                        "{label}, {rule_id:?}, {spin:?}: plain clear must fail B2B-save"
                    );
                    assert!(
                        pruned.is_some(),
                        "{label}, {rule_id:?}, {spin:?}: proven empty must be a complete graph"
                    );
                } else if label == "T kick into pocket" && rule_id == KickTableProfileId::Srs90 {
                    assert!(
                        !expected.is_empty(),
                        "{label}, {rule_id:?}, {spin:?}: spin-capable transition needs real evidence"
                    );
                    assert!(
                        expected.keys().any(|path| {
                            path.len() == 1
                                && path[0].blocked_t_corners == 3
                                && path[0].blocked_t_front_corners == 1
                                && path[0].lock.last_action_was_rotation
                                && path[0].lock.from_rotation == RotationState::Left
                                && path[0].lock.kick_index == 2
                                && path[0].lock.first_success_confirmed
                                && path[0].lock.kick_dx == -1
                                // Evidence stores normalized pose deltas:
                                // Left's origin is one row below its center,
                                // so SRS (-1,-1) is pose delta (-1,0).
                                && path[0].lock.kick_dy == 0
                        }),
                        "{label}, {rule_id:?}, {spin:?}: retain the actual SRS mini kick witness"
                    );
                }
            }
        }
    }
}

#[test]
fn semantic_pruning_rejected_finesse_group_clears_slots_without_a_query() {
    let board = FinesseBoard::new(Board64Layout::standard_10_by_lines(4).unwrap(), 0).unwrap();
    for pruning in [false, true] {
        let mut groups = FrozenFinesseTargetGroups::new();
        for index in [1, 0] {
            push_frozen_finesse_target(
                &mut groups,
                board,
                PieceKind::O,
                GroupedFinesseTarget::new(
                    index,
                    FinesseTarget::new(RotationState::Zero, 4, 0),
                    Some(TerminalEvidenceLabel::NoRotation),
                    false,
                ),
            );
        }
        let mut costs = vec![Some(37), Some(99)];
        let mut evidence = vec![Some(TerminalEvidenceLabel::NoRotation); 2];
        let queries = with_semantic_pruning(pruning, || {
            annotate_frozen_finesse_target_groups(
                groups,
                SpawnProfile::new(4, 2),
                &NoKick::profile(),
                &mut costs,
                &mut evidence,
                &ExecutionControl::default(),
                "pruning_test_movement_failed",
                "pruning_test_scoring_failed",
            )
            .unwrap()
        });
        assert_eq!(queries, usize::from(!pruning));
        assert_eq!(costs, [None, None]);
        assert_eq!(evidence, [None, None]);
    }
}

fn synthetic_finesse_graph(accepting: &[bool]) -> BuildOrderGraph {
    let edges = accepting
        .iter()
        .enumerate()
        .map(|(index, _)| BuildEdge {
            to: index as u32 + 1,
            operation_index: 0,
            piece: PieceKind::O,
            rotation: RotationState::Zero,
            x: 4,
            y: 0,
            cleared_lines: 0,
        })
        .collect::<Vec<_>>();
    let mut nodes = vec![BuildOrderNodeSpec {
        edge_start: 0,
        edge_count: edges.len() as u32,
        depth: 0,
        accepting: false,
    }];
    nodes.extend(accepting.iter().map(|&accepting| BuildOrderNodeSpec {
        edge_start: edges.len() as u32,
        edge_count: 0,
        depth: 1,
        accepting,
    }));
    BuildOrderGraph::from_topological_parts(nodes, edges, 0, 0, false).unwrap()
}

fn annotate_synthetic_finesse_graph(
    accepting: &[bool],
    pruning: bool,
) -> (PreparedFinesseLanguage, usize) {
    let problem = fixed_problem(
        4,
        0,
        vec![PieceKind::O],
        KickTableProfileId::NoKick,
        SpinProfileSelection::TSpins,
    );
    let catalog =
        GeometryCatalog::compile_for_build_probability_on_board(&problem, 0, 0xc030).unwrap();
    let row = catalog.skeleton_id(PieceKind::O, 0xc030).unwrap();
    let candidate = GeometryCandidate::from_rows(&catalog, 0, &[row]).unwrap();
    let mut workspace = BuildUpWorkspace::default();
    workspace.reachability.configure(1);
    workspace
        .reachability
        .configure_kick_profile(KickTableProfileId::NoKick, false);
    let mut projection = CandidateProjection::compile(
        &catalog,
        &candidate,
        &mut workspace,
        BuildCompletion::ExactBoardAfterLineClears(0xc030),
    )
    .unwrap();
    // This synthetic graph isolates suffix liveness and stable compaction;
    // duplicate physical actions represent separately labelled alternatives.
    let mut graph = synthetic_finesse_graph(accepting);
    let edge_pointer = graph.edges.as_ptr();
    let edge_capacity = graph.edges.capacity();
    let mut queries = usize::MAX;
    let language = with_semantic_pruning(pruning, || {
        annotate_and_prune_finesse_graph_with_query_count(
            &problem,
            &catalog,
            &mut projection,
            &mut graph,
            &mut workspace.reachability,
            false,
            &ExecutionControl::default(),
            Some(&mut queries),
        )
        .unwrap()
    });
    assert_eq!(
        graph.edges.as_ptr(),
        edge_pointer,
        "compact in the owned edge buffer"
    );
    assert_eq!(graph.edges.capacity(), edge_capacity);
    assert_eq!(
        graph.edges.len(),
        accepting.iter().filter(|&&value| value).count()
    );
    (language, queries)
}

#[test]
fn semantic_pruning_dead_suffix_starts_no_finesse_queries() {
    let (reference, reference_queries) = annotate_synthetic_finesse_graph(&[false], false);
    let (pruned, pruned_queries) = annotate_synthetic_finesse_graph(&[false], true);
    assert!(reference.edges.is_empty());
    assert!(pruned.edges.is_empty());
    assert_eq!(reference_queries, 1);
    assert_eq!(pruned_queries, 0);
}

#[test]
fn semantic_pruning_stable_finesse_compaction_preserves_costs_and_tie_order() {
    let (reference, _) = annotate_synthetic_finesse_graph(&[false, true, false, true], false);
    let (pruned, _) = annotate_synthetic_finesse_graph(&[false, true, false, true], true);
    let facts = |language: &PreparedFinesseLanguage| {
        language
            .edges
            .iter()
            .map(|edge| {
                (
                    edge.action_key,
                    edge.cost,
                    edge.transition_order,
                    edge.terminal_evidence,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(facts(&reference), facts(&pruned));
    assert_eq!(
        pruned
            .edges
            .iter()
            .map(|edge| edge.transition_order)
            .collect::<Vec<_>>(),
        [1, 3]
    );
    assert!(pruned.edges.iter().all(|edge| edge.cost > 0));
}

#[test]
fn semantic_pruning_compacts_multiple_source_nodes_with_original_edge_orders() {
    const LEFT_O: u64 = 0xc03;
    const CENTER_O: u64 = 0xc030;
    let problem = fixed_problem(
        4,
        0,
        vec![PieceKind::O, PieceKind::O],
        KickTableProfileId::NoKick,
        SpinProfileSelection::TSpins,
    );
    let catalog =
        GeometryCatalog::compile_for_build_probability_on_board(&problem, 0, LEFT_O | CENTER_O)
            .unwrap();
    let candidate = GeometryCandidate::from_rows(
        &catalog,
        0,
        &[
            catalog.skeleton_id(PieceKind::O, CENTER_O).unwrap(),
            catalog.skeleton_id(PieceKind::O, LEFT_O).unwrap(),
        ],
    )
    .unwrap();
    let operation = |mask| {
        candidate
            .row_ids()
            .iter()
            .position(|&row| catalog.skeleton(row).cells == mask)
            .unwrap() as u8
    };
    let edge = |to, mask, x| BuildEdge {
        to,
        operation_index: operation(mask),
        piece: PieceKind::O,
        rotation: RotationState::Zero,
        x,
        y: 0,
        cleared_lines: 0,
    };
    let spec = |edge_start, edge_count, depth, accepting| BuildOrderNodeSpec {
        edge_start,
        edge_count,
        depth,
        accepting,
    };
    for pruning in [false, true] {
        // Both operation orders reach the same physical completion. Dead
        // labelled alternatives precede live ones in three source ranges,
        // exercising writes that overwrite earlier ranges of the same Vec.
        let mut graph = BuildOrderGraph::from_topological_parts(
            vec![
                spec(0, 3, 0, false),
                spec(3, 0, 1, false),
                spec(3, 2, 1, false),
                spec(5, 2, 1, false),
                spec(7, 0, 2, false),
                spec(7, 0, 2, true),
            ],
            vec![
                edge(1, CENTER_O, 4),
                edge(2, CENTER_O, 4),
                edge(3, LEFT_O, 0),
                edge(4, LEFT_O, 0),
                edge(5, LEFT_O, 0),
                edge(4, CENTER_O, 4),
                edge(5, CENTER_O, 4),
            ],
            0,
            0,
            false,
        )
        .unwrap();
        let edge_pointer = graph.edges.as_ptr();
        let edge_capacity = graph.edges.capacity();
        let mut workspace = BuildUpWorkspace::default();
        workspace.reachability.configure(2);
        workspace
            .reachability
            .configure_kick_profile(KickTableProfileId::NoKick, false);
        let mut projection = CandidateProjection::compile(
            &catalog,
            &candidate,
            &mut workspace,
            BuildCompletion::ExactBoardAfterLineClears(LEFT_O | CENTER_O),
        )
        .unwrap();
        let mut queries = usize::MAX;
        let language = with_semantic_pruning(pruning, || {
            annotate_and_prune_finesse_graph_with_query_count(
                &problem,
                &catalog,
                &mut projection,
                &mut graph,
                &mut workspace.reachability,
                false,
                &ExecutionControl::default(),
                Some(&mut queries),
            )
            .unwrap()
        });
        assert_eq!(queries, 3);
        assert_eq!(graph.edges.as_ptr(), edge_pointer);
        assert_eq!(graph.edges.capacity(), edge_capacity);
        assert_eq!(graph.edges.len(), 4);
        let center = GeometryActionKey::new(PieceKind::O, RotationState::Zero, 4, 0);
        let left = GeometryActionKey::new(PieceKind::O, RotationState::Zero, 0, 0);
        // Center is one hard drop. Left needs DAS-left and hard drop. The
        // other O never blocks either route, regardless of operation order.
        let expected = [(1, center, 1), (2, left, 2), (4, left, 2), (6, center, 1)];
        assert_eq!(
            language
                .edges
                .iter()
                .map(|edge| (edge.transition_order, edge.action_key, edge.cost))
                .collect::<Vec<_>>(),
            expected
        );
        assert!(language
            .edges
            .iter()
            .all(|edge| edge.terminal_evidence.is_none()));
    }
}

fn assert_raw_result_parity(reference: &CoreExecutionResult, pruned: &CoreExecutionResult) {
    assert_eq!(
        reference.normalized_solution_identities(),
        pruned.normalized_solution_identities()
    );
    assert_eq!(
        reference.normalized_solution_keys(),
        pruned.normalized_solution_keys()
    );
    assert_eq!(
        reference.coverage_pattern_words(),
        pruned.coverage_pattern_words()
    );
    assert_eq!(reference.solution_coverages(), pruned.solution_coverages());
    assert_eq!(
        reference.normalized_solution_coverages(),
        pruned.normalized_solution_coverages()
    );
    assert_eq!(
        reference.solution_probabilities(),
        pruned.solution_probabilities()
    );
    assert_eq!(
        reference.postprocess_pattern_weights(),
        pruned.postprocess_pattern_weights()
    );
    for field in [
        "normalized_unique_solution_count",
        "build_variant_count",
        "build_variant_count_exact",
        "coverage_pattern_count",
        "covered_pattern_count",
        "coverage_probability",
        "count_complete",
        "probability_complete",
        "solution_probability_complete",
        "resource_truncated",
        "postprocess_execution_complete",
    ] {
        assert_eq!(reference.field(field), pruned.field(field), "{field}");
    }
}

fn assert_scoring_languages_equal(
    reference: &CoreExecutionResult,
    pruned: &CoreExecutionResult,
    profile: SpinProfileSelection,
) {
    let reference_batch = reference
        .exact_scoring_execution_batch()
        .expect("B2B execution evidence");
    let pruned_batch = pruned
        .exact_scoring_execution_batch()
        .expect("B2B execution evidence");
    assert!(reference_batch.complete());
    assert!(pruned_batch.complete());
    assert_eq!(reference_batch.patterns(), pruned_batch.patterns());
    assert_eq!(
        reference_batch.initial_cursor(),
        pruned_batch.initial_cursor()
    );
    assert_eq!(reference_batch.initial_hold(), pruned_batch.initial_hold());
    let pruned_graphs = pruned_batch
        .graphs()
        .iter()
        .map(|graph| (graph.identity(), graph))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(reference_batch.graphs().len(), pruned_graphs.len());
    for reference_graph in reference_batch.graphs() {
        let pruned_graph = pruned_graphs
            .get(&reference_graph.identity())
            .expect("same raw identity");
        let policy = Some(BackToBackEdgePolicy::new(profile));
        assert_eq!(
            completed_paths(Some(reference_graph), policy),
            completed_paths(Some(pruned_graph), policy)
        );
    }
}

#[test]
fn semantic_pruning_build_finesse_preserves_exact_target_results_and_reports() {
    let _guard = product_test_guard();
    for spin in [SpinProfileSelection::TSpins, SpinProfileSelection::AllSpin] {
        for (base, target, pieces) in [
            (0x3f0, 0xf, vec![PieceKind::I]),
            (0x803f0, 0xf, vec![PieceKind::I]),
            (0x27f8, 0x807, vec![PieceKind::T]),
            (0, 0xc03 | 0xc030, vec![PieceKind::O, PieceKind::O]),
            // Existing IIOOO fixture: several tilings share the same source.
            // Compare the complete reported optimum and witness, so the
            // per-candidate edge ordering reaches the shared MIN/tie consumer.
            (
                0,
                0xfffff,
                vec![
                    PieceKind::I,
                    PieceKind::I,
                    PieceKind::O,
                    PieceKind::O,
                    PieceKind::O,
                ],
            ),
        ] {
            let problem = fixed_problem(4, base, pieces, KickTableProfileId::Srs90, spin);
            let field = BuildProbabilityField::from_words_preserving_height(
                4,
                [base, 0, 0, 0],
                [target, 0, 0, 0],
            )
            .unwrap();
            let run = |pruning| {
                let mut session = WasmBuildProbabilitySession::new(
                    &problem,
                    field,
                    BuildProbabilityAggregation::Buildability,
                    BuildProbabilityFinesseRequest::Search {
                        pattern_knowledge: FinessePatternKnowledge::Oracle,
                    },
                )
                .unwrap_or_else(|error| {
                    panic!(
                        "BuildFinesse setup: base={base:#x}, target={target:#x}, rule=Srs90, spin={spin:?}, pruning={pruning}: {error:?}"
                    )
                });
                loop {
                    match session
                        .advance(1_024, &ExecutionControl::default())
                        .unwrap_or_else(|error| {
                            panic!(
                                "BuildFinesse advance: base={base:#x}, target={target:#x}, rule=Srs90, spin={spin:?}, pruning={pruning}: {error:?}"
                            )
                        })
                    {
                        BuildProbabilityAdvance::Pending => {}
                        BuildProbabilityAdvance::Completed(result) => break result,
                        BuildProbabilityAdvance::Cancelled => panic!(
                            "BuildFinesse cancelled without cancellation: base={base:#x}, target={target:#x}, rule=Srs90, spin={spin:?}, pruning={pruning}"
                        ),
                    }
                }
            };
            let reference = with_semantic_pruning(false, || run(false));
            let pruned = with_semantic_pruning(true, || run(true));
            assert_raw_result_parity(&reference, &pruned);
            assert_eq!(
                reference.finesse_report(),
                pruned.finesse_report(),
                "base={base:#x}, target={target:#x}, rule=Srs90, spin={spin:?}"
            );
            assert_eq!(
                reference.path_steps(),
                pruned.path_steps(),
                "base={base:#x}, target={target:#x}, rule=Srs90, spin={spin:?}"
            );
        }
    }
}

#[test]
#[ignore = "managed full P7 original-ordinal and completed-language differential matrix"]
fn semantic_pruning_p7_preserves_raw_coverage_counts_and_constrained_paths() {
    let _guard = product_test_guard();
    let mut cases = Vec::new();
    for rule in RULES {
        for hold in [false, true] {
            for count in [PcCountPolicy::CountUnique, PcCountPolicy::CountAll] {
                cases.push((rule, hold, count, SpinProfileSelection::TSpins));
            }
        }
    }
    for spin in SPIN_PROFILES.into_iter().skip(1) {
        cases.push((
            KickTableProfileId::SrsPlus,
            true,
            PcCountPolicy::CountAll,
            spin,
        ));
    }
    for (rule_id, hold, count, spin) in cases {
        let problem = ProblemCompiler::compile_scenario_pc(
            &PcScenarioQuery::new(
                PcScenarioBoard::standard_10(4, 0x3c0f03c0f),
                PcQueueInput::standard_7_bag(),
                PieceWindow::new(6),
            )
            .with_exact_pieces(Some(6))
            .with_supply_window_size(SupplyWindowSize::new(7))
            .with_allow_hold(hold)
            .with_rule(rule(rule_id))
            .with_count_policy(count)
            .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
            .with_retained_trace_limit(1)
            .with_objective(ObjectivePolicy::all().with_back_to_back_preservation(spin))
            .with_execution_policy(execution_policy()),
        )
        .unwrap();
        assert_eq!(problem.count_policy(), count);
        assert_eq!(
            problem
                .piece_source()
                .materialized_universe()
                .unwrap()
                .pattern_count(),
            5_040
        );
        let run = || {
            WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
                .unwrap()
        };
        let reference = with_semantic_pruning(false, run);
        let pruned = with_semantic_pruning(true, run);
        assert_eq!(reference.bool_field("count_complete"), Some(true));
        assert_eq!(reference.bool_field("probability_complete"), Some(true));
        assert_eq!(reference.usize_field("workers_used"), Some(1));
        assert_eq!(pruned.usize_field("workers_used"), Some(1));
        assert!(!reference.solution_coverages().is_empty());
        assert_raw_result_parity(&reference, &pruned);
        assert_scoring_languages_equal(&reference, &pruned, spin);
        assert_eq!(reference.path_steps(), pruned.path_steps());
        eprintln!("semantic_pruning_p7 rule={rule_id:?} hold={hold} count={count:?} spin={spin:?} identities={} covered={} variants={}", reference.solution_coverages().len(), reference.field("covered_pattern_count").unwrap(), reference.field("build_variant_count").unwrap());
    }
}
