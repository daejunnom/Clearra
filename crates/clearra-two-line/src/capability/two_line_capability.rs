use clearra_core_domain::pc::pc_target::PcTarget;
use clearra_profiles::{
    board::board_profile::{BoardProfile, BoardProfileId},
    pieces::piece_set_profile::{PieceSetProfile, PieceSetProfileId},
};
use clearra_rules::profile::rule_profile::RuleProfile;

use super::two_line_fallback_reason::TwoLineFallbackReason;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TwoLineCapabilityInput {
    board: BoardProfile,
    piece_set: PieceSetProfile,
    target: PcTarget,
    rule: RuleProfile,
    hold_enabled: bool,
    validation_passed: bool,
}

impl TwoLineCapabilityInput {
    pub fn new(
        board: BoardProfile,
        piece_set: PieceSetProfile,
        target: PcTarget,
        rule: RuleProfile,
        hold_enabled: bool,
        validation_passed: bool,
    ) -> Self {
        Self {
            board,
            piece_set,
            target,
            rule,
            hold_enabled,
            validation_passed,
        }
    }
}
impl TwoLineCapabilityInput {
    pub fn board(self) -> BoardProfile {
        self.board
    }
}
impl TwoLineCapabilityInput {
    pub fn piece_set(self) -> PieceSetProfile {
        self.piece_set
    }
}
impl TwoLineCapabilityInput {
    pub fn target(self) -> PcTarget {
        self.target
    }
}
impl TwoLineCapabilityInput {
    pub fn rule(self) -> RuleProfile {
        self.rule
    }
}
impl TwoLineCapabilityInput {
    pub fn hold_enabled(self) -> bool {
        self.hold_enabled
    }
}
impl TwoLineCapabilityInput {
    pub fn validation_passed(self) -> bool {
        self.validation_passed
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TwoLineCapability {
    input: TwoLineCapabilityInput,
    fallback_reason: Option<TwoLineFallbackReason>,
}

impl TwoLineCapability {
    pub fn evaluate(input: TwoLineCapabilityInput) -> Self {
        let fallback_reason = first_fallback_reason(input);
        Self {
            input,
            fallback_reason,
        }
    }
}
impl TwoLineCapability {
    pub fn input(self) -> TwoLineCapabilityInput {
        self.input
    }
}
impl TwoLineCapability {
    pub fn is_capable(self) -> bool {
        self.fallback_reason.is_none()
    }
}
impl TwoLineCapability {
    pub fn fallback_reason(self) -> Option<TwoLineFallbackReason> {
        self.fallback_reason
    }
}

fn first_fallback_reason(input: TwoLineCapabilityInput) -> Option<TwoLineFallbackReason> {
    if !input.validation_passed() {
        return Some(TwoLineFallbackReason::ValidationFailed);
    }

    if input.board().id() != BoardProfileId::Standard10 {
        return Some(TwoLineFallbackReason::UnsupportedBoardProfile {
            actual: input.board().id(),
        });
    }

    if input.board().size().width() != 10 {
        return Some(TwoLineFallbackReason::UnsupportedBoardWidth {
            width: input.board().size().width(),
        });
    }

    if input.target().lines() != 2 {
        return Some(TwoLineFallbackReason::UnsupportedTargetLines {
            lines: input.target().lines(),
        });
    }

    if !input.hold_enabled() {
        return Some(TwoLineFallbackReason::UnsupportedHoldDisabled);
    }

    if input.piece_set().id() != PieceSetProfileId::StandardTetrominoes {
        return Some(TwoLineFallbackReason::UnsupportedPieceSet {
            actual: input.piece_set().id(),
        });
    }

    if !input.rule().is_two_line_supported() {
        return Some(TwoLineFallbackReason::UnsupportedRuleProfile {
            actual: input.rule().id(),
        });
    }

    None
}

#[cfg(test)]
#[path = "two_line_capability_tests.rs"]
mod tests;
