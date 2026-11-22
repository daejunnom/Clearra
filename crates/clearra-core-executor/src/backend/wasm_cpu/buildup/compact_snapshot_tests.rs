use std::collections::BTreeSet;

use clearra_finesse::{CostedGeometryLanguage, QueueClassProductEvaluator};
use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
use clearra_problem::ProblemCompiler;
use clearra_rules::profile::builtin_rules;
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::*;

const O_LEFT: u64 = 0x0c03;
const O_RIGHT: u64 = 0x300c;
const TWO_O_HOLES: u64 = 0x0f_ffff & !(O_LEFT | O_RIGHT);
const PLACEMENTS: [PiecePlacementMask; 2] = [
    PiecePlacementMask::new(PieceKind::O, O_LEFT),
    PiecePlacementMask::new(PieceKind::O, O_RIGHT),
];

fn problem() -> SearchProblem {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, TWO_O_HOLES),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O, PieceKind::O])),
        PieceWindow::new(2),
    )
    .with_exact_pieces(Some(2))
    .with_allow_hold(false)
    .with_rule(builtin_rules::no_kick());
    ProblemCompiler::compile_scenario_pc(&query).expect("exact O/O problem")
}

type PhysicalEdge = (u64, u64, u64, u16, u16);

fn physical_edges(snapshot: &CompactBuildUpSnapshot) -> BTreeSet<PhysicalEdge> {
    let mut result = BTreeSet::new();
    for (index, node) in snapshot.nodes().iter().enumerate() {
        for edge in snapshot.edges_from(index).expect("node range") {
            let child = snapshot.nodes()[edge.child_index as usize];
            assert_eq!(
                child.placed_operations,
                node.placed_operations | (1_u16 << edge.operation_index)
            );
            assert_eq!(node.board_mask & edge.target_mask, 0);
            result.insert((
                node.board_mask,
                edge.target_mask,
                child.board_mask,
                edge.cleared_physical_rows,
                child.deleted_original_rows,
            ));
        }
    }
    result
}

fn exact_o_cost(problem: &SearchProblem, language: &CostedGeometryLanguage) -> Option<u32> {
    QueueClassProductEvaluator::new(language)
        .with_spawn_profile(problem.spawn_profile())
        .with_hold_enabled(false)
        .fixed_queue_cost(&[PieceKind::O, PieceKind::O], None)
        .expect("fixed queue cost")
}

#[test]
fn rust_compact_snapshot_is_immutable_across_prepare_and_rejects_partial_input() {
    let problem = problem();
    let mut workspace = RustCompactBuildUpWorkspace::new(&problem).expect("workspace");
    let control = ExecutionControl::default();
    let geometry = workspace
        .prepare(
            &PLACEMENTS,
            CompactBuildUpTransitionMode::GroundedGeometry,
            &control,
        )
        .expect("geometry only");
    let expected = BTreeSet::from([
        (TWO_O_HOLES, O_LEFT, TWO_O_HOLES | O_LEFT, 0, 0),
        (TWO_O_HOLES, O_RIGHT, TWO_O_HOLES | O_RIGHT, 0, 0),
        (TWO_O_HOLES | O_LEFT, O_RIGHT, 0, 3, 3),
        (TWO_O_HOLES | O_RIGHT, O_LEFT, 0, 3, 3),
    ]);
    assert_eq!(physical_edges(&geometry), expected);
    assert_eq!(geometry.nodes().len(), 4);
    assert_eq!(
        geometry
            .nodes()
            .iter()
            .filter(|node| node.accepting)
            .count(),
        1
    );
    let geometry_cost = geometry.classic_finesse(&control).expect("costing");
    assert!(exact_o_cost(&problem, &geometry_cost).is_some());

    let reachable = workspace
        .prepare(
            &[PLACEMENTS[1], PLACEMENTS[0]],
            CompactBuildUpTransitionMode::Reachable,
            &control,
        )
        .expect("reachable");
    assert_eq!(physical_edges(&reachable), expected);
    assert_eq!(geometry.operations(), reachable.operations());
    assert_eq!(
        geometry.classic_finesse(&control).expect("repeat costing"),
        geometry_cost
    );
    assert_eq!(physical_edges(&geometry), expected);
    assert!(matches!(
        reachable.classic_finesse(&control),
        Err(CompactBuildUpSnapshotError::GeometryOnlyRequired)
    ));

    assert!(matches!(
        workspace.prepare(
            &PLACEMENTS[..1],
            CompactBuildUpTransitionMode::Reachable,
            &control
        ),
        Err(CompactBuildUpSnapshotError::InvalidCandidate)
    ));
    assert!(matches!(
        workspace.prepare(
            &[PLACEMENTS[0]; 2],
            CompactBuildUpTransitionMode::Reachable,
            &control
        ),
        Err(CompactBuildUpSnapshotError::InvalidCandidate)
    ));
    control.cancellation.handle().cancel();
    assert!(matches!(
        workspace.prepare(
            &PLACEMENTS,
            CompactBuildUpTransitionMode::Reachable,
            &control
        ),
        Err(CompactBuildUpSnapshotError::Cancelled)
    ));
    assert!(matches!(
        geometry.classic_finesse(&control),
        Err(CompactBuildUpSnapshotError::Cancelled)
    ));
}

#[test]
fn rust_compact_snapshot_preserves_physical_and_original_clear_masks() {
    let lower = 0b1111_u64;
    let upper = 0b1111_u64 << 14;
    let initial = 0x0f_ffff & !(lower | upper);
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, initial),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I, PieceKind::I])),
        PieceWindow::new(2),
    )
    .with_exact_pieces(Some(2))
    .with_allow_hold(false);
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("exact I/I problem");
    let mut workspace = RustCompactBuildUpWorkspace::new(&problem).expect("workspace");
    let snapshot = workspace
        .prepare(
            &[
                PiecePlacementMask::new(PieceKind::I, lower),
                PiecePlacementMask::new(PieceKind::I, upper),
            ],
            CompactBuildUpTransitionMode::GroundedGeometry,
            &ExecutionControl::default(),
        )
        .expect("grounded relation");
    assert!(physical_edges(&snapshot).contains(&(
        initial,
        lower,
        0x03ff & !(upper >> 10),
        0b01,
        0b01,
    )));
    assert!(physical_edges(&snapshot).contains(&(
        0x03ff & !(upper >> 10),
        upper >> 10,
        0,
        0b01, // upper original row falls down after the first clear
        0b11,
    )));
}

#[test]
fn rust_compact_snapshot_requires_full_table_identity_beyond_fixed_pc_supply() {
    use clearra_pc_graph::request::{PcCountPolicy, PcExecutionPolicy, RequestedSearchBackend};
    use clearra_rules::kicks::{
        KickOffsetSequence, KickTableEntry, KickTableProfile, KickTransition, SrsKicks,
        VerifiedKickTableProfile,
    };

    let builtin = SrsKicks::profile();
    let selected = KickTransition::new(PieceKind::T, RotationState::Right, RotationState::Zero);
    for change_unqueued_t in [false, true] {
        let mut entries = builtin.entries().to_vec();
        if change_unqueued_t {
            let entry = entries
                .iter_mut()
                .find(|entry| entry.transition() == selected)
                .unwrap();
            let mut offsets = entry.sequence().offsets().to_vec();
            offsets[..=2].rotate_right(1);
            *entry = KickTableEntry::new(selected, KickOffsetSequence::new(offsets));
        }
        let verified = VerifiedKickTableProfile::try_new(KickTableProfile::new(
            builtin.id(),
            builtin.source_rule(),
            entries,
        ))
        .unwrap();
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, TWO_O_HOLES),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O, PieceKind::O])),
            PieceWindow::new(2),
        )
        .with_exact_pieces(Some(2))
        .with_allow_hold(false)
        .with_count_policy(PcCountPolicy::CountAll)
        .with_verified_kick_table_profile(verified)
        .with_execution_policy(
            PcExecutionPolicy::mvp_default()
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_workers(1)
                .with_tablebase_requested(false)
                .with_precompute_build_dependencies(false),
        );
        let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
        // Whole fixed PC only consumes O and may keep its existing narrow
        // playable-piece equivalence. Snapshot input is an independent set
        // of placements, so its builtin evidence needs full table identity.
        assert!(crate::backend::wasm_cpu::ensure_connected_kick_profile(&problem).is_ok());
        let snapshot = RustCompactBuildUpWorkspace::new(&problem);
        if change_unqueued_t {
            assert!(matches!(
                snapshot,
                Err(CompactBuildUpSnapshotError::Engine(
                    "wasm_exact_backend_requires_canonical_ordered_kick_profile"
                ))
            ));
        } else {
            assert!(snapshot.is_ok());
        }
    }
}

#[cfg(feature = "native-c-core")]
#[test]
fn rust_compact_snapshot_matches_native_physical_dag_and_classic_cost() {
    use clearra_core_ffi::{
        problem::C_PIECE_O, BuildUpGeometryTransitionMode, CBuildUpProblemBuilder,
        CPackingCandidate, CPackingOperation, NativeBuildUpWorkspace,
    };
    use clearra_rules::kicks::NoKick;

    use crate::buildup::native_compact_finesse_language::annotate_prepared_native_compact_finesse_language;

    let problem = problem();
    let mut rust = RustCompactBuildUpWorkspace::new(&problem).expect("Rust workspace");
    let mut native = NativeBuildUpWorkspace::new();
    let mut candidate = CPackingCandidate {
        candidate_id: 7,
        canonical_operation_set_id: 9,
        operation_count: 2,
        ..CPackingCandidate::default()
    };
    for (index, (x, mask)) in [(0, O_LEFT), (2, O_RIGHT)].into_iter().enumerate() {
        candidate.operations[index] = CPackingOperation {
            piece: C_PIECE_O,
            rotation: 0,
            x,
            y: 0,
            operation_id: 4,
            required_deleted_row_mask: 0,
            mask,
        };
    }
    let native_problem = CBuildUpProblemBuilder::from_packing_candidate(&problem, &candidate, 0, 0)
        .expect("native problem");
    let control = ExecutionControl::default();
    for (rust_mode, c_mode) in [
        (
            CompactBuildUpTransitionMode::GroundedGeometry,
            BuildUpGeometryTransitionMode::GeometryOnly,
        ),
        (
            CompactBuildUpTransitionMode::Reachable,
            BuildUpGeometryTransitionMode::Reachable,
        ),
    ] {
        let a = native
            .export_geometry_language_v2_with_cancellation(
                &native_problem,
                c_mode,
                &control.cancellation,
            )
            .expect("native v2 snapshot");
        let b = rust
            .prepare(&PLACEMENTS, rust_mode, &control)
            .expect("Rust selected candidate");
        assert!(a.complete());
        let mut a_edges = BTreeSet::new();
        for node in a.nodes().iter().copied() {
            for edge in &a.edges()[node.first_edge()..node.first_edge() + node.edge_count()] {
                let child = a.nodes()[edge.child_node_index()];
                a_edges.insert((
                    node.board_mask(),
                    edge.target_mask(),
                    child.board_mask(),
                    edge.cleared_row_mask(),
                    child.deleted_row_mask(),
                ));
            }
        }
        // The C input binds one declared rotation per operation. Rust
        // resolves every possible concrete rotation of each typed mask.
        // Compare exactly the physical transition projection and minimum
        // cost, without asserting the ABI index/rotation lists are equal.
        assert_eq!(physical_edges(&b), a_edges);
        if rust_mode == CompactBuildUpTransitionMode::GroundedGeometry {
            let a_cost = annotate_prepared_native_compact_finesse_language(
                &a,
                b.layout(),
                problem.spawn_profile(),
                &NoKick::profile(),
            )
            .expect("native snapshot cost");
            let b_cost = b.classic_finesse(&control).expect("Rust snapshot cost");
            let expected = exact_o_cost(&problem, &a_cost).expect("completed native fixed route");
            assert_eq!(exact_o_cost(&problem, &b_cost), Some(expected));
        }
    }
}
