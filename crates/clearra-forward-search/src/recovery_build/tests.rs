use super::*;
use crate::CrossStageEarlyLimit;
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask,
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    piece::piece_kind::PieceKind,
};
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::profile::SpinProfileId;

fn mask(value: u64) -> Board256Mask {
    Board256Mask::from_words([value, 0, 0, 0])
}
fn pieces(value: &str) -> Vec<PieceKind> {
    value
        .chars()
        .map(|p| match p {
            'I' => PieceKind::I,
            'J' => PieceKind::J,
            'L' => PieceKind::L,
            'O' => PieceKind::O,
            'S' => PieceKind::S,
            'T' => PieceKind::T,
            'Z' => PieceKind::Z,
            _ => panic!("fixture piece"),
        })
        .collect()
}
fn query() -> RecoveryBuildFixedQuery {
    RecoveryBuildFixedQuery {
        fields: RecoveryBuildFields {
            height: 8,
            initial: mask(0),
            middle: mask(0xf),
            result: mask(0xc030),
        },
        first_supply: pieces("I"),
        second_supply: pieces("O"),
        early_limit: CrossStageEarlyLimit::Auto,
        allow_piece_exchange: false,
        hold_enabled: true,
        preserve_b2b: false,
        initial_b2b: true,
        rule_profile: RuleProfileId::SrsPlus,
        spin_profile: SpinProfileId::AllSpinPlus,
    }
}
#[test]
fn recovery_build_normal_matches_actual_combined_target() {
    let result = query().search(&ExecutionControl::default()).unwrap();
    assert_eq!(result.status, RecoveryBuildStatus::Normal);
    assert_eq!(result.steps.len(), 2);
    assert_eq!(result.terminal_board, [0xc03f, 0, 0, 0]);
    assert_eq!(result.exchange_balance, [0; 7]);
}
#[test]
fn recovery_build_late_middle_uses_different_real_piece_only_when_enabled() {
    // Start supplies O, but the middle shape is I. First O can build the result
    // to the right and later I fills the middle. No piece is transformed.
    let mut q = query();
    q.first_supply = pieces("O");
    q.second_supply = pieces("I");
    q.hold_enabled = false;
    assert_eq!(
        q.search(&ExecutionControl::default()).unwrap().status,
        RecoveryBuildStatus::NoPath
    );
    q.allow_piece_exchange = true;
    let result = q.search(&ExecutionControl::default()).unwrap();
    assert_eq!(result.status, RecoveryBuildStatus::Recovery);
    assert_eq!(result.actual_early, 1);
    assert_eq!(result.exchange_balance, [-1, 0, 0, 1, 0, 0, 0]);
    assert_eq!(result.steps[0].piece, PieceKind::O);
    assert!(result.steps[0].result_target);
    assert_eq!(result.steps[1].piece, PieceKind::I);
    assert!(!result.steps[1].result_target);
    q.early_limit = CrossStageEarlyLimit::AtMost(0);
    assert_eq!(
        q.search(&ExecutionControl::default()).unwrap().status,
        RecoveryBuildStatus::NoPath
    );
}
#[test]
fn recovery_build_cleared_rows_keep_logical_ownership_and_final_target() {
    let mut q = query();
    q.fields.initial = mask(0x3f0);
    q.fields.result = mask(0xc030);
    let result = q.search(&ExecutionControl::default()).unwrap();
    assert_eq!(result.status, RecoveryBuildStatus::Normal);
    assert_eq!(result.steps[0].cleared_lines, 1);
    assert_eq!(result.steps.last().unwrap().board_after, [0xc030, 0, 0, 0]);
}
#[test]
fn recovery_build_b2b_applies_to_deferred_middle_clears() {
    let mut q = query();
    q.fields.initial = mask(0x3f0);
    q.first_supply = pieces("O");
    q.second_supply = pieces("I");
    q.hold_enabled = false;
    q.allow_piece_exchange = true;
    // Result O is above the row which the late I clears; target is after clear.
    q.fields.result = mask(0xc030);
    let result = q.search(&ExecutionControl::default()).unwrap();
    assert_eq!(result.status, RecoveryBuildStatus::Recovery);
    assert_eq!(result.steps.last().unwrap().cleared_lines, 1);
    q.preserve_b2b = true;
    assert_eq!(
        q.search(&ExecutionControl::default()).unwrap().status,
        RecoveryBuildStatus::NoPath
    );
}
#[test]
fn recovery_build_pattern_product_has_one_event_per_supply_pair() {
    let q = query();
    let p = RecoveryBuildQuery {
        all_solutions: false,
        fields: q.fields,
        first_supply: "I".into(),
        second_supply: "O".into(),
        early_limit: q.early_limit,
        allow_piece_exchange: false,
        hold_enabled: true,
        preserve_b2b: false,
        initial_b2b: true,
        rule_profile: q.rule_profile,
        spin_profile: q.spin_profile,
    };
    let result = p.search(&ExecutionControl::default()).unwrap();
    assert_eq!(result.possible, 1);
    assert_eq!(result.evaluated, 1);
    assert_eq!(result.normal_count, 1);
    assert_eq!(result.normal_probability, 1.0);
    let mut p = p;
    p.first_supply = "P7".into();
    p.second_supply = "P7".into();
    let token = ExecutionCancellationToken::new();
    token.handle().cancel();
    assert_eq!(
        p.search(&ExecutionControl::new(token)),
        Err(RecoveryBuildError::Cancelled)
    );
}
#[test]
fn recovery_build_user_image_pair_keeps_fragmented_logical_s_and_z() {
    // These constants are transcribed cell-by-cell from the supplied screenshots.
    // S and Z cross rows cleared before they lock: static connected-tiling tests
    // would wrongly prune this exact drawing. Candidate roles are generated at
    // actual lock time, then lifted into the persistent Build coordinates.
    let mut q = query();
    q.fields = RecoveryBuildFields {
        height: 8,
        initial: mask(0xc0383f3fc7),
        middle: mask(0x3ff3fc7c0c038),
        result: mask(0x30483f07f3f8f),
    };
    q.first_supply = pieces("ITOLSZJ");
    q.second_supply = pieces("JTOSILZ");
    q.hold_enabled = false;
    q.allow_piece_exchange = true;
    let accept = |p: PieceKind, second: bool, rows: &[u16]| {
        let color: u64 = if second {
            match p {
                PieceKind::J => 0x3c0f,
                PieceKind::T => 0x40300400000,
                PieceKind::O => 0xc0300000,
                PieceKind::L => 0x3008020000000,
                PieceKind::S => 0x100c0200,
                PieceKind::Z => 0x30180,
                PieceKind::I => 0,
            }
        } else {
            match p {
                PieceKind::I => 0x3c0003c00000,
                PieceKind::T => 0x401804000000,
                PieceKind::O => 0x300c0000000,
                PieceKind::L => 0x3802000000000,
                PieceKind::S => 0x40000c020,
                PieceKind::Z => 0x300000018,
                PieceKind::J => 0,
            }
        };
        rows.iter().enumerate().all(|(row, &cells)| {
            let y = row.checked_sub(if second { 5 } else { 0 });
            let allowed = y
                .filter(|y| *y < 6)
                .map_or(0, |y| ((color as u64 >> (10 * y)) & 1023) as u16);
            cells & !allowed == 0
        })
    };
    let result = q
        .search_with_filter(&ExecutionControl::default(), &accept)
        .unwrap();
    assert_eq!(result.status, RecoveryBuildStatus::Recovery);
    assert_eq!(result.actual_early, 1);
    assert_eq!(result.exchange_balance, [-1, 1, 0, 0, 0, 0, 0]);
    assert_eq!(result.steps.len(), 14);
    assert_eq!(
        result.steps.last().unwrap().board_after,
        [0x30483f07f3f8f, 0, 0, 0]
    );
    q.allow_piece_exchange = false;
    assert_eq!(
        q.search_with_filter(&ExecutionControl::default(), &accept)
            .unwrap()
            .status,
        RecoveryBuildStatus::NoPath
    );
}
#[test]
fn recovery_build_invalid_geometry_is_not_no_path() {
    let mut q = query();
    q.fields.initial = mask(1);
    assert_eq!(
        q.search(&ExecutionControl::default()),
        Err(RecoveryBuildError::MiddleOverlapsStart)
    );
}

#[test]
fn recovery_build_auto_covers_two_and_three_early_pieces_without_a_one_piece_filter() {
    for count in [2_usize, 3] {
        let mut q = query();
        // Spaced one-column shafts require I pieces. Unlike the former
        // 0x3cff region, they contain no 2x2 square an early O could fill.
        let columns = (0..count).fold(0_u64, |bits, column| bits | (1_u64 << (2 * column)));
        q.fields.middle = mask((0..4).fold(0_u64, |bits, row| bits | (columns << (10 * row))));
        let result = (0..count * 2).fold(0_u64, |m, y| m | (0x300_u64 << (10 * y)));
        q.fields.result = mask(result);
        q.first_supply = vec![PieceKind::O; count];
        q.second_supply = vec![PieceKind::I; count];
        q.hold_enabled = false;
        q.allow_piece_exchange = true;
        for limit in 0..count {
            q.early_limit = CrossStageEarlyLimit::AtMost(limit);
            assert_eq!(
                q.search(&ExecutionControl::default()).unwrap().status,
                RecoveryBuildStatus::NoPath
            );
        }
        for limit in [
            CrossStageEarlyLimit::Auto,
            CrossStageEarlyLimit::AtMost(count),
        ] {
            q.early_limit = limit;
            let found = q.search(&ExecutionControl::default()).unwrap();
            assert_eq!(found.status, RecoveryBuildStatus::Recovery);
            assert_eq!(found.actual_early, count);
            assert!(found.steps[..count]
                .iter()
                .all(|step| step.result_target && step.piece == PieceKind::O));
            assert!(found.steps[count..]
                .iter()
                .all(|step| !step.result_target && step.piece == PieceKind::I));
        }
    }
}

#[test]
fn recovery_build_unconstrained_middle_may_use_an_o_instead_of_an_i() {
    // The old three-early fixture admitted a legal O in the middle. Keep
    // that positive behavior: occupancy goals must not silently fix a tiling.
    let mut q = query();
    q.fields.middle = mask(0x3cff);
    q.fields.result = mask((0..6).fold(0_u64, |bits, row| bits | (0x300_u64 << (10 * row))));
    q.first_supply = vec![PieceKind::O; 3];
    q.second_supply = vec![PieceKind::I; 3];
    q.hold_enabled = false;
    q.allow_piece_exchange = true;
    q.early_limit = CrossStageEarlyLimit::AtMost(2);
    let found = q.search(&ExecutionControl::default()).unwrap();
    assert_eq!(found.status, RecoveryBuildStatus::Recovery);
    assert!(found.actual_early <= 2);
    assert!(found
        .steps
        .iter()
        .any(|step| !step.result_target && step.piece == PieceKind::O));
}
