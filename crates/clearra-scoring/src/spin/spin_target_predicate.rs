use clearra_replay::ReplayTrace;

use crate::profile::ScoreProfile;

use super::{
    spin_result::{SpinKind, SpinResult},
    spin_target::{RequiredSpinKind, SpinMiniPolicy, SpinTarget},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpinTargetPredicateResult {
    satisfied: bool,
}

impl SpinTargetPredicateResult {
    pub fn new(satisfied: bool) -> Self {
        Self { satisfied }
    }
}
impl SpinTargetPredicateResult {
    pub fn satisfied(self) -> bool {
        self.satisfied
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpinTargetPredicate {
    target: SpinTarget,
}

impl SpinTargetPredicate {
    pub fn new(target: SpinTarget) -> Self {
        Self { target }
    }
}
impl SpinTargetPredicate {
    pub fn target(&self) -> &SpinTarget {
        &self.target
    }
}
impl SpinTargetPredicate {
    pub fn evaluate(
        &self,
        _trace: &ReplayTrace,
        spin_result: &SpinResult,
        profile: &ScoreProfile,
    ) -> SpinTargetPredicateResult {
        let profile_matches = self
            .target
            .required_score_profile()
            .is_none_or(|required| required == profile.id());
        SpinTargetPredicateResult::new(
            profile_matches
                && self
                    .target
                    .spin_piece_selector()
                    .matches(spin_result.piece())
                && self
                    .target
                    .clear_lines()
                    .matches(spin_result.cleared_lines())
                && mini_policy_matches(self.target.mini_policy(), *spin_result)
                && spin_kind_matches(self.target.spin_kind(), spin_result.spin_kind()),
        )
    }
}
impl SpinTargetPredicate {
    pub fn evaluate_result_only(&self, spin_result: &SpinResult, profile: &ScoreProfile) -> bool {
        let profile_matches = self
            .target
            .required_score_profile()
            .is_none_or(|required| required == profile.id());
        profile_matches
            && self
                .target
                .spin_piece_selector()
                .matches(spin_result.piece())
            && self
                .target
                .clear_lines()
                .matches(spin_result.cleared_lines())
            && mini_policy_matches(self.target.mini_policy(), *spin_result)
            && spin_kind_matches(self.target.spin_kind(), spin_result.spin_kind())
    }
}

fn spin_kind_matches(required: RequiredSpinKind, actual: SpinKind) -> bool {
    match required {
        RequiredSpinKind::RegularSpin => matches!(
            actual,
            SpinKind::RegularSpin | SpinKind::TSpin | SpinKind::AllSpin
        ),
        RequiredSpinKind::MiniSpin => {
            matches!(
                actual,
                SpinKind::MiniSpin | SpinKind::TSpinMini | SpinKind::AllSpinMini
            )
        }
        RequiredSpinKind::TSpin => matches!(actual, SpinKind::TSpin),
        RequiredSpinKind::TSpinMini => matches!(actual, SpinKind::TSpinMini),
        RequiredSpinKind::AllSpin => matches!(actual, SpinKind::AllSpin | SpinKind::TSpin),
        RequiredSpinKind::AllSpinMini => {
            matches!(actual, SpinKind::AllSpinMini | SpinKind::TSpinMini)
        }
        RequiredSpinKind::ProfileSpecific(expected) => {
            matches!(actual, SpinKind::ProfileSpecific(actual) if actual == expected)
        }
    }
}

fn mini_policy_matches(policy: SpinMiniPolicy, result: SpinResult) -> bool {
    match policy {
        SpinMiniPolicy::RegularOnly => !result.is_mini(),
        SpinMiniPolicy::MiniAllowed => true,
        SpinMiniPolicy::MiniOnly | SpinMiniPolicy::AllSpinAsMini => result.is_mini(),
    }
}
