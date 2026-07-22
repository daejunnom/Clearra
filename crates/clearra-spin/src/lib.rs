//! Spin recognition, resolution, and award contracts.
//!
//! This crate intentionally does not depend on `clearra-scoring`.
//! Spin recognition produces `SpinInterpretationSet`; scoring consumes that set.

pub mod award;
pub mod evidence;
pub mod recognition;
pub mod resolution;
pub mod special;
pub mod target;

pub use award::{SpinAwardClass, SpinAwardProfile, SpinAwardProfileId};
pub use evidence::{
    BoardAnchor, CornerEvidence, EvidenceRequirements, ImmobileEvidence, KickEvidence,
    KickTableProfileId, LastActionEvidence, SpecialSpinEvidence, SpinEvidence,
    VerifiedKickTableProfileId,
};
pub use recognition::{
    SpinRecognitionProfile, SpinRecognitionProfileId, SpinRecognizerId, UnknownSpinPolicy,
};
pub use resolution::{
    SpinInterpretation, SpinInterpretationSet, SpinResolutionProfile, SpinResolutionProfileId,
};
pub use special::{
    BoardSignaturePredicate, CornerRuleOverride, CustomSpecialSpinId, ImportedSpecialSpinId,
    KickSignature, SpecialSpinCase, SpecialSpinCaseId, SpecialSpinCaseRegistry,
    SpecialSpinCaseRegistryId, SpecialSpinVerificationState,
};
pub use target::PredicateResult;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
