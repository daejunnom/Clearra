pub mod all_mini_rule;
pub mod all_spin_rule;
pub mod kick_evidence_requirement;
pub mod kick_sensitive_spin_rule;
pub mod special_spin_case;
pub mod special_spin_case_id;
pub mod special_spin_case_registry;
pub mod spin_accuracy;
pub mod spin_classification;
pub mod spin_classification_input;
pub mod spin_classifier;
pub mod spin_classifier_registry;
pub mod spin_result;
pub mod spin_target;
pub mod spin_target_evidence;
pub mod spin_target_predicate;
pub mod t_spin_corner_rule;
pub mod verified_special_spin_profile;

pub use clearra_core_domain::ids::SpinTargetId;

pub use all_mini_rule::AllMiniRule;
pub use all_spin_rule::AllSpinRule;
pub use kick_evidence_requirement::KickEvidenceRequirement;
pub use kick_sensitive_spin_rule::KickSensitiveSpinRule;
pub use special_spin_case::{SpecialSpinCase, SpecialSpinVerificationState};
pub use special_spin_case_id::SpecialSpinCaseId;
pub use special_spin_case_registry::SpecialSpinCaseRegistry;
pub use spin_accuracy::{SpinAccuracy, TraceCompleteness};
pub use spin_classification::{ClassificationConfidence, SpinClassification};
pub use spin_classification_input::{
    BoardAnchor, KickEvidence, MovementInfo, RotationRequest, SpinClassificationInput,
};
pub use spin_classifier::SpinClassifier;
pub use spin_classifier_registry::{SpinClassifierDescriptor, SpinClassifierRegistry};
pub use spin_result::{SpinKind, SpinResult};
pub use spin_target::{
    RequiredClearKind, RequiredClearLines, RequiredSpinKind, SpinMiniPolicy, SpinPieceSelector,
    SpinTarget,
};
pub use spin_target_evidence::SpinTargetEvidence;
pub use spin_target_predicate::{SpinTargetPredicate, SpinTargetPredicateResult};
pub use t_spin_corner_rule::TSpinCornerRule;
pub use verified_special_spin_profile::{SpinClassifierCapability, VerifiedSpecialSpinProfile};
