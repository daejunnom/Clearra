use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_piece_registry::registry::{MixedBagProfile, MixedPieceSet};
use clearra_profiles::pieces::piece_set_profile::PieceSetProfile;
use clearra_setup_search::query::PieceBudget;

use crate::diagnostic::diagnostic_report::DiagnosticReport;

use super::{
    piece_budget_validator::validate_budget_impl,
    piece_set_mixed_guard_validator::{
        validate_mixed_bag_profile_mvp3_guard_impl, validate_mixed_piece_set_mvp3_guard_impl,
    },
    piece_set_standard_validator::validate_pieces_impl,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PieceSetValidator;

impl PieceSetValidator {
    pub fn validate_pieces(pieces: &[PieceKind], location: &'static str) -> DiagnosticReport {
        validate_pieces_impl(pieces, location)
    }
}
impl PieceSetValidator {
    pub fn validate_profile(profile: PieceSetProfile) -> DiagnosticReport {
        Self::validate_pieces(profile.pieces(), "pieces.profile")
    }
}
impl PieceSetValidator {
    pub fn validate_budget(budget: &PieceBudget) -> DiagnosticReport {
        validate_budget_impl(budget)
    }
}
impl PieceSetValidator {
    pub fn validate_mixed_piece_set_mvp3_guard(piece_set: &MixedPieceSet) -> DiagnosticReport {
        validate_mixed_piece_set_mvp3_guard_impl(piece_set)
    }
}
impl PieceSetValidator {
    pub fn validate_mixed_bag_profile_mvp3_guard(
        piece_set: &MixedPieceSet,
        bag_profile: &MixedBagProfile,
    ) -> DiagnosticReport {
        validate_mixed_bag_profile_mvp3_guard_impl(piece_set, bag_profile)
    }
}

pub fn validate_piece_set(pieces: &[PieceKind]) -> DiagnosticReport {
    PieceSetValidator::validate_pieces(pieces, "pieces")
}

pub fn validate_piece_set_profile(profile: PieceSetProfile) -> DiagnosticReport {
    PieceSetValidator::validate_profile(profile)
}

pub fn validate_piece_budget(budget: &PieceBudget) -> DiagnosticReport {
    PieceSetValidator::validate_budget(budget)
}

pub fn validate_mixed_piece_set_mvp3_guard(piece_set: &MixedPieceSet) -> DiagnosticReport {
    PieceSetValidator::validate_mixed_piece_set_mvp3_guard(piece_set)
}

pub fn validate_mixed_bag_profile_mvp3_guard(
    piece_set: &MixedPieceSet,
    bag_profile: &MixedBagProfile,
) -> DiagnosticReport {
    PieceSetValidator::validate_mixed_bag_profile_mvp3_guard(piece_set, bag_profile)
}

#[cfg(test)]
#[path = "piece_set_validator_tests.rs"]
mod tests;
