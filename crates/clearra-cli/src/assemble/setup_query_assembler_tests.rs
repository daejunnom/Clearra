use super::*;

#[test]
fn assembles_residue_and_cycle_boundary_policy() {
    let query = SetupQueryAssembler::assemble(&SetupArgs::new("I,T,O", true)).expect("setup query");

    assert_eq!(query.residue().remaining_count(), 3);
    assert_eq!(query.residue().cycle(), Some(7));
    assert_eq!(
        query.cycle_reset_borrow_policy(),
        SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse
    );
}

#[test]
fn rejects_unknown_residue_piece() {
    assert_eq!(
        SetupQueryAssembler::assemble(&SetupArgs::new("IX", false)),
        Err(SetupQueryAssemblyError::UnknownPiece { value: 'X' })
    );
}

#[test]
fn preserves_setup_candidate_priority() {
    let args = SetupArgs::new("IOTS", false).with_candidate_priority(
        clearra_setup_search::query::SetupCandidatePriority::PcProbabilityFirst,
    );
    let query = SetupQueryAssembler::assemble(&args).expect("setup query");

    assert_eq!(
        query.candidate_priority(),
        clearra_setup_search::query::SetupCandidatePriority::PcProbabilityFirst
    );
}

#[test]
fn preserves_setup_length_preference() {
    let args = SetupArgs::new("IOTS", false)
        .with_length_preference(clearra_setup_search::query::SetupLengthPreference::Shorter);
    let query = SetupQueryAssembler::assemble(&args).expect("setup query");

    assert_eq!(
        query.length_preference(),
        clearra_setup_search::query::SetupLengthPreference::Shorter
    );
}

#[test]
fn preserves_selected_setup_kick_table() {
    let args = SetupArgs::new("IOTS", false).with_rule("srs-x");
    let query = SetupQueryAssembler::assemble(&args).expect("setup query");

    assert_eq!(query.rule().id().as_str(), "srs-x");
}

#[test]
fn preserves_selected_jstris_180_setup_kick_table() {
    let args = SetupArgs::new("IOTS", false).with_rule("jstris-180");
    let query = SetupQueryAssembler::assemble(&args).expect("setup query");

    assert_eq!(query.rule().id().as_str(), "jstris-180");
}

#[test]
fn rejects_unknown_setup_kick_table() {
    let args = SetupArgs::new("IOTS", false).with_rule("unknown");

    assert!(matches!(
        SetupQueryAssembler::assemble(&args),
        Err(SetupQueryAssemblyError::RuleProfile(
            RuleProfileAssemblyError::UnknownRuleProfile { .. }
        ))
    ));
}

#[test]
fn assembles_residue_and_observed_qb_separately() {
    let args = SetupArgs::new("TI", false).with_queue_based_pieces("OS");
    let query = SetupQueryAssembler::assemble(&args).expect("QB setup query");

    assert_eq!(
        query.search_mode(),
        clearra_setup_search::query::SetupSearchMode::QueueBased
    );
    assert_eq!(
        query.residue().pieces(),
        &[
            clearra_core_domain::piece::piece_kind::PieceKind::T,
            clearra_core_domain::piece::piece_kind::PieceKind::I,
        ]
    );
    assert_eq!(
        query
            .queue()
            .as_fixed_sequence()
            .expect("fixed queue")
            .pieces(),
        &[
            clearra_core_domain::piece::piece_kind::PieceKind::O,
            clearra_core_domain::piece::piece_kind::PieceKind::S,
        ]
    );
    assert!(query.next_cycle_remaining_pieces().is_none());
}

#[test]
fn assembles_next_cycle_inventory_without_changing_search_mode() {
    let args = SetupArgs::new("TI", false).with_next_cycle_remaining_pieces("OOSITZ");
    let query = SetupQueryAssembler::assemble(&args).expect("oracle setup query");

    assert_eq!(
        query.search_mode(),
        clearra_setup_search::query::SetupSearchMode::ShapeOracle
    );
    assert_eq!(
        query.next_cycle_remaining_pieces(),
        Some(
            &[
                clearra_core_domain::piece::piece_kind::PieceKind::O,
                clearra_core_domain::piece::piece_kind::PieceKind::O,
                clearra_core_domain::piece::piece_kind::PieceKind::S,
                clearra_core_domain::piece::piece_kind::PieceKind::I,
                clearra_core_domain::piece::piece_kind::PieceKind::T,
                clearra_core_domain::piece::piece_kind::PieceKind::Z,
            ][..]
        )
    );
}

#[test]
fn assembles_observed_qb_and_next_cycle_inventory_together() {
    let args = SetupArgs::new("TI", false)
        .with_queue_based_pieces("OS")
        .with_next_cycle_remaining_pieces("OOSITZ");
    let query = SetupQueryAssembler::assemble(&args).expect("combined setup query");

    assert_eq!(
        query
            .queue()
            .as_fixed_sequence()
            .expect("observed queue")
            .pieces(),
        &[
            clearra_core_domain::piece::piece_kind::PieceKind::O,
            clearra_core_domain::piece::piece_kind::PieceKind::S,
        ]
    );
    assert_eq!(
        query
            .next_cycle_remaining_pieces()
            .map(|pieces| pieces.len()),
        Some(6)
    );
}

#[test]
fn assembles_explicit_setup_initial_hold_without_ui_condition_expansion() {
    let args = SetupArgs::new("IOTS", false).with_initial_hold("s");
    let query = SetupQueryAssembler::assemble(&args).expect("setup query");

    assert_eq!(
        query.hold_policy(),
        clearra_setup_search::query::SetupHoldPolicy::EnabledWithPiece(
            clearra_core_domain::piece::piece_kind::PieceKind::S
        )
    );
}

#[test]
fn rejects_multi_piece_initial_hold() {
    let args = SetupArgs::new("IOTS", false).with_initial_hold("SZ");

    assert_eq!(
        SetupQueryAssembler::assemble(&args),
        Err(SetupQueryAssemblyError::InitialHoldInvalid)
    );
}

#[test]
fn queue_based_mode_requires_observed_pieces() {
    let args = SetupArgs::new("TI", false)
        .with_search_mode(clearra_setup_search::query::SetupSearchMode::QueueBased);

    assert_eq!(
        SetupQueryAssembler::assemble(&args),
        Err(SetupQueryAssemblyError::QueueBasedPiecesMissing)
    );
}

#[test]
fn assembles_selected_setup_and_hold_condition_for_exact_path_detail() {
    let args = SetupArgs::new("IOTS", false).with_path_detail(
        "setup-004011c4f9-0002-000000000000000000000000000001",
        "hold-T",
    );
    let query = SetupQueryAssembler::assemble(&args).expect("path detail query");
    let detail = query.path_detail().expect("path detail");

    assert_eq!(detail.board_mask(), 0x4011_c4f9);
    assert_eq!(detail.deleted_rows(), 2);
    assert_eq!(detail.placement_rows(), 1);
    assert_eq!(detail.condition_id(), "hold-T");
}

#[test]
fn rejects_noncanonical_setup_path_detail_id() {
    let args = SetupArgs::new("IOTS", false).with_path_detail("4011c4f9", "hold-T");

    assert_eq!(
        SetupQueryAssembler::assemble(&args),
        Err(SetupQueryAssemblyError::PathDetailInvalid)
    );
}
