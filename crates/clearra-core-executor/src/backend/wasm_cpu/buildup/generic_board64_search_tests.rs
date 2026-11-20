use super::*;

use clearra_core_domain::board::board_size::BoardSize;
use clearra_supply::{
    execution_automaton::SupplyProvenanceId, piece_source::PieceSourceId, SupplyObservationIdentity,
};

const LIMITS: GenericBuildUpLimits = GenericBuildUpLimits {
    max_states: 128,
    max_memo_entries: 64,
};

fn layout(width: u16, height: u16) -> Board64Layout {
    Board64Layout::new(BoardSize::new(width, height).unwrap()).unwrap()
}

fn source_state(policy: HoldPolicy) -> SupplyExecutionState {
    SupplyExecutionState::with_contract(
        PieceSourceId::new(1),
        PieceSourceKind::FixedQueue,
        0,
        None,
        policy,
        0,
        0,
        SupplyObservationIdentity::full_queue_oracle(),
        SupplyProvenanceId(1),
    )
}

fn source(queue: &[PieceKind], policy: HoldPolicy) -> GenericBuildUpSource<'_> {
    GenericBuildUpSource::Concrete {
        queue,
        initial: source_state(policy),
        release_held_at_terminal: false,
    }
}

fn target(piece: PieceKind, target_cells: u64) -> GenericBuildUpPlacement {
    GenericBuildUpPlacement {
        piece,
        target_cells,
        exact_pose: None,
    }
}

#[test]
fn generic_board64_builds_custom_two_row_candidate_with_remapped_original_rows() {
    use PieceKind::I;
    let mut worker =
        GenericBoard64BuildUpWorkspace::new(layout(4, 2), KickTableProfileId::SrsPlus).unwrap();
    // Put the floating operation first in canonical input order. The solver
    // must reject it at the root, and then remap it after the lower row clears.
    let entries = [target(I, 0xf0), target(I, 0x0f)];
    let report = worker
        .verify(
            0,
            &entries,
            source(&[I, I], HoldPolicy::Forbidden),
            LIMITS,
            &ExecutionControl::default(),
        )
        .unwrap();
    let GenericBuildUpOutcome::Buildable { steps } = report.outcome else {
        panic!("expected custom-width PC witness");
    };
    assert_eq!(
        steps
            .iter()
            .map(|step| (step.placement_index, step.y, step.cleared_original_rows))
            .collect::<Vec<_>>(),
        [(1, 0, 0b01), (0, 0, 0b10)]
    );
    assert!(steps.iter().all(|step| step.physical_cells == 0x0f));
    assert_eq!(steps[1].supply_after.cursor, 2);
}

#[test]
fn generic_board64_hold_and_source_exhaustion_are_distinct() {
    use PieceKind::{I, O};
    let mut worker =
        GenericBoard64BuildUpWorkspace::new(layout(4, 2), KickTableProfileId::NoKick).unwrap();
    let entries = [target(O, 0x33), target(O, 0xcc)];
    let control = ExecutionControl::default();
    let enabled = worker
        .verify(
            0,
            &entries,
            source(&[I, O, O], HoldPolicy::Allowed),
            LIMITS,
            &control,
        )
        .unwrap();
    let GenericBuildUpOutcome::Buildable { steps } = enabled.outcome else {
        panic!("expected StoreCurrent I followed by two O locks");
    };
    assert_eq!(
        steps[0].supply_action,
        GenericBuildUpSupplyAction::Queue(SupplyBranchKind::StoreCurrent)
    );
    assert_eq!(steps[1].supply_after.hold_piece, Some(I));
    let disabled = worker
        .verify(
            0,
            &entries,
            source(&[I, O, O], HoldPolicy::Forbidden),
            LIMITS,
            &control,
        )
        .unwrap();
    assert_eq!(disabled.outcome, GenericBuildUpOutcome::NotBuildable);
    let standard = worker
        .verify(
            0,
            &entries,
            GenericBuildUpSource::StandardBag {
                initial: source_state(HoldPolicy::Forbidden),
            },
            LIMITS,
            &control,
        )
        .unwrap();
    assert_eq!(standard.outcome, GenericBuildUpOutcome::NotBuildable);
    let mut worker =
        GenericBoard64BuildUpWorkspace::new(layout(4, 1), KickTableProfileId::NoKick).unwrap();
    let mut already_drawn = source_state(HoldPolicy::Allowed);
    already_drawn.cursor = 1;
    already_drawn.hold_piece = Some(I);
    already_drawn.hold_empty = false;
    let found = worker
        .verify(
            0,
            &[target(I, 0x0f)],
            GenericBuildUpSource::Concrete {
                queue: &[I],
                initial: already_drawn,
                release_held_at_terminal: true,
            },
            LIMITS,
            &control,
        )
        .unwrap();
    let GenericBuildUpOutcome::Buildable { steps } = found.outcome else {
        panic!("valid explicit terminal release");
    };
    assert_eq!(
        steps[0].supply_action,
        GenericBuildUpSupplyAction::ReleaseHeldAtTerminal
    );
    assert_eq!(steps[0].supply_after.cursor, 1);
    assert_eq!(steps[0].supply_after.hold_piece, None);
}

#[test]
fn generic_board64_truncation_and_cancellation_are_not_negative_results() {
    use PieceKind::I;
    let mut worker =
        GenericBoard64BuildUpWorkspace::new(layout(4, 2), KickTableProfileId::NoKick).unwrap();
    let entries = [target(I, 0xf0), target(I, 0x0f)];
    let pieces = [I, I];
    let source = source(&pieces, HoldPolicy::Forbidden);
    let control = ExecutionControl::default();
    let limit = GenericBuildUpLimits {
        max_states: 1,
        ..LIMITS
    };
    assert_eq!(
        worker.verify(0, &entries, source, limit, &control),
        Err(GenericBuildUpError::Resource(
            ResourceTruncationReason::FrontierBudgetExceeded
        ))
    );
    assert_eq!(
        worker.count_all(0, &entries, source, LIMITS, Some(3), &control),
        Err(GenericBuildUpError::Resource(
            ResourceTruncationReason::CandidateBudgetExceeded
        ))
    );
    assert_eq!(
        worker
            .count_all(0, &entries, source, LIMITS, Some(4), &control)
            .unwrap()
            .variants,
        4,
    );
    control.cancellation.handle().cancel();
    assert_eq!(
        worker.verify(0, &entries, source, LIMITS, &control),
        Err(GenericBuildUpError::Cancelled)
    );
}

#[test]
fn generic_board64_count_all_distinguishes_rotations_order_and_same_piece_hold() {
    use PieceKind::O;
    let mut worker =
        GenericBoard64BuildUpWorkspace::new(layout(4, 2), KickTableProfileId::NoKick).unwrap();
    let entries = [target(O, 0x33), target(O, 0xcc)];
    let control = ExecutionControl::default();
    for memo_limit in [0, 1, 64] {
        let limits = GenericBuildUpLimits {
            max_memo_entries: memo_limit,
            ..LIMITS
        };
        let count = worker
            .count_all(
                0,
                &entries,
                source(&[O, O, O], HoldPolicy::Allowed),
                limits,
                None,
                &control,
            )
            .unwrap();
        assert_eq!(count.variants, 2 * 4 * 4 * 4);
    }
    let exact = entries.map(|mut placement| {
        placement.exact_pose = Some(GenericBuildUpOriginalPose {
            rotation: RotationState::Two,
            x: if placement.target_cells == 0x33 { 0 } else { 2 },
            y: 0,
            required_deleted_rows: 0,
        });
        placement
    });
    assert_eq!(
        worker
            .count_all(
                0,
                &exact,
                source(&[O, O], HoldPolicy::Forbidden),
                LIMITS,
                None,
                &control
            )
            .unwrap()
            .variants,
        2,
        "two placement orderings, one exact true rotation per lock",
    );
}

#[test]
fn generic_board64_exact_pose_does_not_widen_the_c_clear_state_contract() {
    use PieceKind::I;
    let mut worker =
        GenericBoard64BuildUpWorkspace::new(layout(4, 2), KickTableProfileId::NoKick).unwrap();
    let base = GenericBuildUpOriginalPose {
        rotation: RotationState::Zero,
        x: 0,
        y: 0,
        required_deleted_rows: 0,
    };
    let mut entries = [
        GenericBuildUpPlacement {
            exact_pose: Some(base),
            ..target(I, 0x0f)
        },
        GenericBuildUpPlacement {
            exact_pose: Some(GenericBuildUpOriginalPose {
                y: 1,
                required_deleted_rows: 1,
                ..base
            }),
            ..target(I, 0xf0)
        },
    ];
    let control = ExecutionControl::default();
    let queue = [I, I];
    let source = source(&queue, HoldPolicy::Forbidden);
    assert_eq!(
        worker
            .count_all(0, &entries, source, LIMITS, None, &control)
            .unwrap()
            .variants,
        1
    );
    entries[1].exact_pose = entries[1].exact_pose.map(|mut pose| {
        pose.required_deleted_rows = 0;
        pose
    });
    assert_eq!(
        worker
            .count_all(0, &entries, source, LIMITS, None, &control)
            .unwrap()
            .variants,
        0
    );
}

#[test]
fn generic_board64_preferred_t_rotation_requires_the_concrete_kick_surface() {
    let without_j = [
        (0, 0),
        (1, 0),
        (0, 1),
        (0, 2),
        (3, 0),
        (4, 0),
        (2, 1),
        (3, 1),
        (3, 2),
        (4, 2),
        (2, 3),
        (3, 3),
        (4, 1),
        (4, 3),
    ]
    .into_iter()
    .fold(0_u64, |mask, (x, y)| mask | (1_u64 << (x + 5 * y)));
    let with_j = without_j | (1_u64 << 15) | (1_u64 << 20) | (1_u64 << 25) | (1_u64 << 26);
    let mut worker =
        GenericBoard64BuildUpWorkspace::new(layout(5, 6), KickTableProfileId::SrsPlus).unwrap();
    let lock = RotationState::Right;
    assert_eq!(
        worker.preferred_t_spin_rotation_before_clear(without_j, lock, 1, 1),
        Ok(None)
    );
    let evidence = worker
        .preferred_t_spin_rotation_before_clear(with_j, lock, 1, 1)
        .unwrap()
        .expect("J enables a legal final kick");
    assert!(evidence.last_action_was_rotation());
    assert!(evidence.first_success_confirmed());
    assert!(evidence.used_kick());
    let mut worker =
        GenericBoard64BuildUpWorkspace::new(layout(5, 6), KickTableProfileId::NoKick).unwrap();
    assert_eq!(
        worker.preferred_t_spin_rotation_before_clear(with_j, lock, 1, 1),
        Ok(None)
    );
}

#[test]
fn generic_board64_builds_cover_from_stream_without_fixing_a_representative_pose() {
    use crate::backend::generic_geometry::{
        stream_geometry, CoverSelection, GeometryEnd, GeometryLimits, GeometryRequest,
        ProjectedTerminal,
    };
    use clearra_supply::PieceMultisetKey;
    use PieceKind::I;

    let layout = layout(4, 2);
    let mut workspace =
        GenericBoard64BuildUpWorkspace::new(layout, KickTableProfileId::SrsPlus).unwrap();
    let control = ExecutionControl::default();
    let mut count = 0;
    let receipt = stream_geometry(
        GeometryRequest {
            layout,
            initial: 0,
            region: 0xff,
            required: 0xff,
            forbidden: 0,
            max_pieces: 2,
            exact_pieces: Some(2),
            inventory: PieceMultisetKey::from_pieces([I, I]),
            exact_family: None,
            terminal: ProjectedTerminal::BoardAfterLineClears(0),
            selection: CoverSelection::All,
        },
        GeometryLimits::default(),
        &control.cancellation,
        |cover| {
            let result = workspace
                .verify_geometry_cover(
                    0,
                    &cover,
                    source(&[I, I], HoldPolicy::Forbidden),
                    LIMITS,
                    &control,
                )
                .unwrap();
            assert!(matches!(
                result.outcome,
                GenericBuildUpOutcome::Buildable { .. }
            ));
            let count = workspace
                .count_all_geometry_cover(
                    0,
                    &cover,
                    source(&[I, I], HoldPolicy::Forbidden),
                    LIMITS,
                    None,
                    &control,
                )
                .unwrap();
            assert_eq!(count.variants, 4);
            count += 1;
            std::ops::ControlFlow::Continue(())
        },
    )
    .unwrap();
    assert_eq!(receipt.end, GeometryEnd::Complete);
    assert_eq!(count, 1);
}

#[cfg(feature = "native-c-core")]
#[test]
fn generic_board64_matches_native_build_up_for_a_real_custom_width() {
    use clearra_core_ffi::{
        problem::{C_PIECE_I, C_PIECE_O},
        CBuildUpProblemBuilder, CNativeBuildUpCountLimits, CPackingCandidate, CPackingOperation,
        CoreCNative,
    };
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::ProblemCompiler;
    use clearra_rules::profile::builtin_rules::srs_plus;
    use clearra_supply::queue::fixed_sequence::FixedSequence;
    use PieceKind::{I, O};

    let mut worker =
        GenericBoard64BuildUpWorkspace::new(layout(4, 2), KickTableProfileId::SrsPlus).unwrap();
    let i = [target(I, 0x0f), target(I, 0xf0)];
    let o = [target(O, 0x33), target(O, 0xcc)];
    let fixtures: [(&[PieceKind], _, _, _, _, _); 3] = [
        (
            &[I, I],
            &i,
            C_PIECE_I,
            [(0, 0), (0, 1)],
            HoldPolicy::Forbidden,
            4,
        ),
        (
            &[O, I],
            &i,
            C_PIECE_I,
            [(0, 0), (0, 1)],
            HoldPolicy::Forbidden,
            0,
        ),
        (
            &[O, O, O],
            &o,
            C_PIECE_O,
            [(0, 0), (2, 0)],
            HoldPolicy::Allowed,
            128,
        ),
    ];
    for (pieces, entries, c_piece, poses, hold, expected) in fixtures {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::new(4, 2, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(pieces.to_vec())),
            PieceWindow::new(2),
        )
        .with_exact_pieces(Some(2))
        .with_allow_hold(hold != HoldPolicy::Forbidden)
        .with_rule(srs_plus());
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("custom native input");
        let mut candidate = CPackingCandidate {
            candidate_id: 1,
            canonical_operation_set_id: 1,
            operation_count: 2,
            geometry_variant_domains: 0b11,
            ..Default::default()
        };
        for (index, operation) in candidate.operations.iter_mut().take(2).enumerate() {
            *operation = CPackingOperation {
                piece: c_piece,
                rotation: 0,
                x: poses[index].0,
                y: poses[index].1,
                operation_id: (index + 1) as u16,
                required_deleted_row_mask: 0,
                mask: entries[index].target_cells,
            };
        }
        let native_problem =
            CBuildUpProblemBuilder::from_packing_candidate(&problem, &candidate, 0, 0)
                .expect("C candidate lowering");
        let native = CoreCNative::count_buildup_variants(
            &native_problem,
            &CNativeBuildUpCountLimits::default(),
        )
        .unwrap();
        assert_eq!(
            native.report.search_complete, 1,
            "native status {}",
            native.status
        );
        assert_eq!(native.report.total_variant_count, expected);
        let rust = worker
            .count_all(
                0,
                entries,
                source(pieces, hold),
                LIMITS,
                None,
                &ExecutionControl::default(),
            )
            .unwrap();
        assert_eq!(rust.variants, expected);
    }
}
