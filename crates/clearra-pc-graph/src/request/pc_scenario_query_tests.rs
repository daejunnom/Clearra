use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::*;
use crate::request::RequestedSearchBackend;

const TARGET_FRAME_PARITY_FIXTURE: &str =
    include_str!("../../../../tests/fixtures/contracts/pc_target_frame_parity.v1.tsv");

#[test]
fn target_frames_preserve_all_four_words_through_twenty_four_lines() {
    for height in [6_u16, 7, 12, 13, 24] {
        let mut words = [0_u64; 4];
        // No completed row may normalize away the requested target. Use two
        // missing cells per row, with two additional
        // cells on the odd top row to keep the total divisible by four.
        let mut empty = 0;
        for y in 0..height {
            let filled_columns = if height % 2 == 1 && y == height - 1 {
                6
            } else {
                8
            };
            empty += 10 - filled_columns;
            for x in 0..filled_columns {
                let bit = y * 10 + x;
                words[usize::from(bit / 64)] |= 1_u64 << (bit % 64);
            }
        }
        let board = PcScenarioBoard::standard_10_from_words(height, words).unwrap();
        let frame = board.to_standard_target_frame(height as u8).unwrap();
        assert_eq!(frame.normalized_board().occupied_words(), words);
        assert_eq!(frame.required_pieces(), usize::from(empty / 4));
        assert_eq!(frame.initial_cleared_rows(), 0);
        assert_eq!(frame.normalized_board().visible_height(), height);
    }
}

#[test]
fn extended_initial_clear_compacts_rows_across_word_boundaries() {
    let mut words = [0_u64; 4];
    for row in [0_u16, 6, 12, 19] {
        for x in 0..10 {
            let bit = row * 10 + x;
            words[usize::from(bit / 64)] |= 1_u64 << (bit % 64);
        }
    }
    for (x, y) in [(0_u16, 5_u16), (9, 7), (3, 13), (7, 23)] {
        let bit = y * 10 + x;
        words[usize::from(bit / 64)] |= 1_u64 << (bit % 64);
    }
    let frame = PcScenarioBoard::standard_10_from_words(24, words)
        .unwrap()
        .to_standard_target_frame(24)
        .unwrap();
    assert_eq!(frame.initial_cleared_rows(), 4);
    assert_eq!(frame.required_pieces(), 59);
    let normalized = Board256Mask::from_words(frame.normalized_board().occupied_words());
    for (x, y) in [(0_u16, 4_u16), (9, 5), (3, 10), (7, 19)] {
        assert!(normalized.contains_index(y * 10 + x));
    }
    assert_eq!(normalized.count_ones(), 4);
    assert_eq!(
        frame.normalized_board().after_initial_line_clear(),
        *frame.normalized_board()
    );
}

#[test]
fn extended_input_rejects_out_of_domain_bits_without_truncation() {
    assert!(PcScenarioBoard::standard_10_from_words(25, [0; 4]).is_err());
    assert!(PcScenarioBoard::standard_10_from_words(24, [0, 0, 0, 1_u64 << 48]).is_err());
    assert!(PcScenarioBoard::standard_10_from_words(6, [0, 1, 0, 0]).is_err());
    let board = PcScenarioBoard::standard_10_from_words(24, [0, 0, 0, 1_u64 << 47]).unwrap();
    assert!(matches!(
        board.to_standard_target_frame(7),
        Err(PcScenarioTargetFrameError::ExtendedOccupancyAboveTarget { .. })
    ));
    assert!(matches!(
        PcScenarioBoard::standard_10(7, 1).to_standard_target_frame(7),
        Err(PcScenarioTargetFrameError::EmptyAreaNotTetrominoAligned { empty_cells: 69 })
    ));
}

#[derive(Clone, Copy, Debug)]
struct TargetFrameParityCase<'a> {
    id: &'a str,
    target_lines: u8,
    raw_mask: u64,
    normalized_mask: Option<u64>,
    initial_cleared_rows: u8,
    required_pieces: Option<usize>,
    outcome: &'a str,
}

fn target_frame_parity_cases() -> impl Iterator<Item = TargetFrameParityCase<'static>> {
    TARGET_FRAME_PARITY_FIXTURE
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let columns = line.split('\t').collect::<Vec<_>>();
            assert_eq!(columns.len(), 7, "fixture column count: {line}");
            let parse_hex = |value: &str| {
                u64::from_str_radix(value, 16)
                    .unwrap_or_else(|error| panic!("invalid fixture mask {value}: {error}"))
            };
            TargetFrameParityCase {
                id: columns[0],
                target_lines: columns[1].parse().expect("fixture target lines"),
                raw_mask: parse_hex(columns[2]),
                normalized_mask: (columns[3] != "-").then(|| parse_hex(columns[3])),
                initial_cleared_rows: columns[4].parse().expect("fixture cleared rows"),
                required_pieces: (columns[5] != "-")
                    .then(|| columns[5].parse().expect("fixture required pieces")),
                outcome: columns[6],
            }
        })
}

#[test]
fn explicit_one_through_six_line_target_frames_share_initial_clear_and_error_semantics() {
    for case in target_frame_parity_cases() {
        assert_ne!(
            case.raw_mask, 0,
            "{}: fixture must retain an input field",
            case.id
        );
        let board = PcScenarioBoard::standard_10(u16::from(case.target_lines), case.raw_mask);
        match case.outcome {
            "valid" => {
                let frame = board
                    .to_standard_target_frame(case.target_lines)
                    .unwrap_or_else(|error| {
                        panic!("{}: unexpected target-frame error: {error:?}", case.id)
                    });
                assert_eq!(
                    frame.normalized_board().visible_height(),
                    u16::from(case.target_lines),
                    "{}: normalization must preserve the user's target lines",
                    case.id
                );
                assert_eq!(
                    frame.normalized_board().occupied_mask(),
                    case.normalized_mask.expect("valid fixture normalized mask"),
                    "{}: normalized mask",
                    case.id
                );
                assert_eq!(
                    frame.initial_cleared_rows(),
                    case.initial_cleared_rows,
                    "{}: initial cleared rows",
                    case.id
                );
                assert_eq!(
                    frame.required_pieces(),
                    case.required_pieces.expect("valid fixture piece count"),
                    "{}: target-frame piece count",
                    case.id
                );
            }
            "target-lines-invalid" => assert!(
                matches!(
                    board.to_standard_target_frame(case.target_lines),
                    Err(PcScenarioTargetFrameError::TargetLinesOutsideProductDomain { .. })
                ),
                "{}",
                case.id
            ),
            "area-impossible" => assert!(
                matches!(
                    board.to_standard_target_frame(case.target_lines),
                    Err(PcScenarioTargetFrameError::EmptyAreaNotTetrominoAligned { .. })
                ),
                "{}",
                case.id
            ),
            "outside-target" => assert!(
                matches!(
                    board.to_standard_target_frame(case.target_lines),
                    Err(PcScenarioTargetFrameError::OccupancyOutsideDeclaredInitialField { .. })
                        | Err(PcScenarioTargetFrameError::OccupancyAboveTarget { .. })
                ),
                "{}",
                case.id
            ),
            outcome => panic!("{}: unknown fixture outcome {outcome}", case.id),
        }
    }
}

#[test]
fn scenario_query_owns_setup_completion_contract_without_pc_target() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0b1111),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I, PieceKind::O])),
        PieceWindow::new(2),
    )
    .with_hold_piece(Some(PieceKind::T))
    .with_count_policy(PcCountPolicy::CountUnique);

    assert_eq!(query.initial_board().width(), 10);
    assert_eq!(query.initial_board().visible_height(), 4);
    assert_eq!(query.initial_board().occupied_mask(), 0b1111);
    assert_eq!(query.remaining_queue().mode(), "fixed");
    assert_eq!(query.hold_state().piece(), Some(PieceKind::T));
    assert_eq!(query.piece_window().max_pieces(), 2);
    assert_eq!(query.exact_pieces(), None);
    assert_eq!(query.min_remaining_queue(), 0);
    assert!(query.allow_hold());
    assert!(!query.requires_180());
    assert_eq!(query.completion_goal(), PcCompletionGoal::ClearToEmpty);
    assert_eq!(query.completion_goal().as_str(), "clear-to-empty");
    assert_eq!(query.count_policy(), PcCountPolicy::CountUnique);
    assert!(query.verified_kick_profile().is_none());
    assert_eq!(
        query.execution_policy().requested_backend(),
        RequestedSearchBackend::Auto
    );
    assert!(query.execution_policy().deterministic());
    assert_eq!(
        query.retained_trace_limit(),
        SearchDefaults::MVP1.scenario_retained_trace_limit()
    );
}

#[test]
fn scenario_query_owns_completion_constraints() {
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
        ])),
        PieceWindow::new(4),
    )
    .with_exact_pieces(Some(3))
    .with_min_remaining_queue(1)
    .with_allow_hold(false)
    .with_requires_180(true)
    .with_retained_trace_limit(2);

    assert_eq!(query.exact_pieces(), Some(3));
    assert_eq!(query.min_remaining_queue(), 1);
    assert!(!query.allow_hold());
    assert!(query.requires_180());
    assert_eq!(query.retained_trace_limit(), 2);
}

#[test]
fn scenario_query_can_carry_verified_imported_kick_profile_override() {
    let verified =
        VerifiedKickTableProfile::try_new(clearra_rules::kicks::SrsKicks::srs_plus_profile())
            .expect("verified profile");
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_verified_kick_table_profile(verified.clone());

    assert_eq!(query.rule().id(), verified.profile().source_rule());
    assert_eq!(query.verified_kick_profile(), Some(&verified));
}

#[test]
fn build_probability_retained_capacity_accounts_supplied_identity_owner() {
    let verified =
        VerifiedKickTableProfile::try_new(clearra_rules::kicks::SrsKicks::srs_plus_profile())
            .expect("verified profile");
    let identity = StandardBoard64ColoredTilingIdentity::from_piece_masks(0, [0; 7])
        .expect("empty colored identity is structurally valid");
    let base = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    );

    assert_eq!(
        base.clone()
            .with_verified_kick_table_profile(verified)
            .checked_build_probability_retained_capacity_bytes(),
        None
    );
    let base_bytes = base
        .checked_build_probability_retained_capacity_bytes()
        .expect("queue owner is measurable");
    let selected_bytes = base
        .with_allowed_colored_solution_identities([identity])
        .checked_build_probability_retained_capacity_bytes()
        .expect("inline colored identities are measurable");
    assert_eq!(
        selected_bytes - base_bytes,
        core::mem::size_of::<StandardBoard64ColoredTilingIdentity>() as u128
    );
}

#[test]
fn scenario_query_can_carry_execution_policy() {
    let policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_workers(2)
        .with_max_frontier_states(128);
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(2, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
        PieceWindow::new(1),
    )
    .with_execution_policy(policy.clone());

    assert_eq!(query.execution_policy(), &policy);
}
