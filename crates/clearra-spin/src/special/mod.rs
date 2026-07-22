pub mod special_spin_case;
pub mod special_spin_case_id;
pub mod special_spin_case_registry;

pub use special_spin_case::{
    BoardSignaturePredicate, CornerRuleOverride, KickSignature, SpecialSpinCase,
    SpecialSpinVerificationState,
};
pub use special_spin_case_id::{CustomSpecialSpinId, ImportedSpecialSpinId, SpecialSpinCaseId};
pub use special_spin_case_registry::{SpecialSpinCaseRegistry, SpecialSpinCaseRegistryId};
