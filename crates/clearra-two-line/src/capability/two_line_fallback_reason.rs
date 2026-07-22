use clearra_profiles::{
    board::board_profile::BoardProfileId, pieces::piece_set_profile::PieceSetProfileId,
};
use clearra_rules::profile::rule_profile::RuleProfileId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TwoLineFallbackReason {
    ValidationFailed,
    FastPathTableUnavailable,
    FastPathRunnerUnavailable,
    UnsupportedBoardProfile { actual: BoardProfileId },
    UnsupportedBoardWidth { width: u16 },
    UnsupportedTargetLines { lines: u8 },
    UnsupportedHoldDisabled,
    UnsupportedPieceSet { actual: PieceSetProfileId },
    UnsupportedRuleProfile { actual: RuleProfileId },
}

impl TwoLineFallbackReason {
    pub fn code(self) -> &'static str {
        match self {
            Self::ValidationFailed => "validation_failed",
            Self::FastPathTableUnavailable => "two_line_table_unavailable",
            Self::FastPathRunnerUnavailable => "two_line_runner_unavailable",
            Self::UnsupportedBoardProfile { .. } => "unsupported_board_profile",
            Self::UnsupportedBoardWidth { .. } => "unsupported_board_width",
            Self::UnsupportedTargetLines { .. } => "unsupported_target_lines",
            Self::UnsupportedHoldDisabled => "unsupported_hold_disabled",
            Self::UnsupportedPieceSet { .. } => "unsupported_piece_set",
            Self::UnsupportedRuleProfile { .. } => "unsupported_rule_profile",
        }
    }
}
