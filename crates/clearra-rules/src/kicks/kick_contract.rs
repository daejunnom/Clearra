use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

use crate::{
    kicks::{
        kick_table::{KickOffset, KickOffsetSequence, KickTransition},
        kick_verification::KickVerificationCase,
        no_kick::NoKick,
        srs_kicks::{eight_direction_transitions, SrsKicks},
        KickProfileRegistry,
    },
    profile::{builtin_rules::srs_plus, rule_capability::RuleCapability},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KickContractReport {
    srs_jlstz_transition_count: usize,
    srs_i_transition_count: usize,
    o_piece_model: &'static str,
    no_kick_transition_count: usize,
    srs_plus_effective_kick_model: &'static str,
    srs_plus_extension_reason: Option<&'static str>,
    srs_plus_180_transition_count: usize,
    jstris_180_transition_count: usize,
    profile_registry_count: usize,
    verification_case_count: usize,
    verification_failure_count: usize,
    srs_profile_id: &'static str,
    no_kick_profile_id: &'static str,
    srs_plus_profile_id: &'static str,
    jstris_profile_id: &'static str,
}

impl KickContractReport {
    pub fn verify_builtin_contracts() -> Self {
        let srs_plus_capability = RuleCapability::from_rule(srs_plus());
        let srs_profile = SrsKicks::profile();
        let no_kick_profile = NoKick::profile();
        let srs_plus_profile = SrsKicks::srs_plus_profile();
        let jstris_profile = SrsKicks::jstris_180_profile();
        let verification_cases = builtin_verification_cases();
        let verification_failure_count = verification_cases
            .iter()
            .filter(|case| !case.verify(&srs_profile).is_passed())
            .count();
        let no_kick_failures = no_kick_verification_cases()
            .iter()
            .filter(|case| !case.verify(&no_kick_profile).is_passed())
            .count();
        let srs_plus_cases = srs_plus_verification_cases();
        let srs_plus_failures = srs_plus_cases
            .iter()
            .filter(|case| !case.verify(&srs_plus_profile).is_passed())
            .count();
        let jstris_cases = jstris_verification_cases();
        let jstris_failures = jstris_cases
            .iter()
            .filter(|case| !case.verify(&jstris_profile).is_passed())
            .count();
        Self {
            srs_jlstz_transition_count: srs_90_transition_count_for(&srs_profile, PieceKind::T),
            srs_i_transition_count: srs_90_transition_count_for(&srs_profile, PieceKind::I),
            o_piece_model: "clearra-internal-no-kick",
            no_kick_transition_count: no_kick_profile.entries().len(),
            srs_plus_effective_kick_model: srs_plus_capability.kick_model().as_str(),
            srs_plus_extension_reason: srs_plus_capability.extension_disabled_reason(),
            srs_plus_180_transition_count: srs_plus_profile
                .entries()
                .iter()
                .filter(|entry| entry.transition().is_180())
                .count(),
            jstris_180_transition_count: jstris_profile
                .entries()
                .iter()
                .filter(|entry| entry.transition().is_180())
                .count(),
            profile_registry_count: KickProfileRegistry::builtin_profiles().len(),
            verification_case_count: verification_cases.len()
                + no_kick_verification_cases().len()
                + srs_plus_cases.len()
                + jstris_cases.len(),
            verification_failure_count: verification_failure_count
                + no_kick_failures
                + srs_plus_failures
                + jstris_failures,
            srs_profile_id: srs_profile.id().as_str(),
            no_kick_profile_id: no_kick_profile.id().as_str(),
            srs_plus_profile_id: srs_plus_profile.id().as_str(),
            jstris_profile_id: jstris_profile.id().as_str(),
        }
    }
}
impl KickContractReport {
    pub fn srs_jlstz_transition_count(&self) -> usize {
        self.srs_jlstz_transition_count
    }
}
impl KickContractReport {
    pub fn srs_i_transition_count(&self) -> usize {
        self.srs_i_transition_count
    }
}
impl KickContractReport {
    pub fn o_piece_model(&self) -> &'static str {
        self.o_piece_model
    }
}
impl KickContractReport {
    pub fn no_kick_transition_count(&self) -> usize {
        self.no_kick_transition_count
    }
}
impl KickContractReport {
    pub fn srs_plus_effective_kick_model(&self) -> &'static str {
        self.srs_plus_effective_kick_model
    }
}
impl KickContractReport {
    pub fn srs_plus_extension_reason(&self) -> Option<&'static str> {
        self.srs_plus_extension_reason
    }
}
impl KickContractReport {
    pub fn srs_plus_180_transition_count(&self) -> usize {
        self.srs_plus_180_transition_count
    }
}
impl KickContractReport {
    pub fn jstris_180_transition_count(&self) -> usize {
        self.jstris_180_transition_count
    }
}
impl KickContractReport {
    pub fn profile_registry_count(&self) -> usize {
        self.profile_registry_count
    }
}
impl KickContractReport {
    pub fn verification_case_count(&self) -> usize {
        self.verification_case_count
    }
}
impl KickContractReport {
    pub fn verification_failure_count(&self) -> usize {
        self.verification_failure_count
    }
}
impl KickContractReport {
    pub fn srs_profile_id(&self) -> &'static str {
        self.srs_profile_id
    }
}
impl KickContractReport {
    pub fn no_kick_profile_id(&self) -> &'static str {
        self.no_kick_profile_id
    }
}
impl KickContractReport {
    pub fn srs_plus_profile_id(&self) -> &'static str {
        self.srs_plus_profile_id
    }
}
impl KickContractReport {
    pub fn jstris_profile_id(&self) -> &'static str {
        self.jstris_profile_id
    }
}

fn srs_90_transition_count_for(
    profile: &crate::kicks::KickTableProfile,
    piece: PieceKind,
) -> usize {
    profile
        .entries()
        .iter()
        .filter(|entry| entry.transition().piece() == piece && !entry.sequence().is_empty())
        .count()
}

fn builtin_verification_cases() -> Vec<KickVerificationCase> {
    let mut cases = Vec::new();
    for piece in [
        PieceKind::J,
        PieceKind::L,
        PieceKind::S,
        PieceKind::T,
        PieceKind::Z,
    ] {
        for (from, to, sequence) in jlstz_expected_sequences() {
            cases.push(KickVerificationCase::new(
                "srs-jlstz-90",
                KickTransition::new(piece, from, to),
                sequence,
            ));
        }
    }
    for (from, to, sequence) in i_expected_sequences() {
        cases.push(KickVerificationCase::new(
            "srs-i-90",
            KickTransition::new(PieceKind::I, from, to),
            sequence,
        ));
    }
    for (from, to) in eight_direction_transitions() {
        cases.push(KickVerificationCase::new(
            "srs-o-clearra-no-kick",
            KickTransition::new(PieceKind::O, from, to),
            KickOffsetSequence::no_kick(),
        ));
    }
    cases
}

fn no_kick_verification_cases() -> Vec<KickVerificationCase> {
    PieceKind::STANDARD_TETROMINOES
        .into_iter()
        .flat_map(|piece| {
            eight_direction_transitions()
                .into_iter()
                .map(move |(from, to)| {
                    KickVerificationCase::new(
                        "no-kick",
                        KickTransition::new(piece, from, to),
                        KickOffsetSequence::no_kick(),
                    )
                })
        })
        .collect()
}

fn srs_plus_verification_cases() -> Vec<KickVerificationCase> {
    let mut cases = Vec::new();
    for piece in [
        PieceKind::J,
        PieceKind::L,
        PieceKind::S,
        PieceKind::T,
        PieceKind::Z,
    ] {
        for (from, to, sequence) in jlstz_expected_sequences() {
            cases.push(KickVerificationCase::new(
                "srs-plus-jlstz-90",
                KickTransition::new(piece, from, to),
                sequence,
            ));
        }
    }
    for (from, to, sequence) in srs_plus_i_expected_sequences() {
        cases.push(KickVerificationCase::new(
            "srs-plus-i-90",
            KickTransition::new(PieceKind::I, from, to),
            sequence,
        ));
    }
    for (from, to) in eight_direction_transitions() {
        cases.push(KickVerificationCase::new(
            "srs-plus-o-90",
            KickTransition::new(PieceKind::O, from, to),
            KickOffsetSequence::no_kick(),
        ));
    }
    for piece in [
        PieceKind::J,
        PieceKind::L,
        PieceKind::S,
        PieceKind::T,
        PieceKind::Z,
    ] {
        for (from, to, sequence) in srs_plus_jlstz_180_expected_sequences() {
            cases.push(KickVerificationCase::new(
                "srs-plus-jlstz-180",
                KickTransition::new(piece, from, to),
                sequence,
            ));
        }
    }
    for (from, to, sequence) in srs_plus_i_180_expected_sequences() {
        cases.push(KickVerificationCase::new(
            "srs-plus-i-180",
            KickTransition::new(PieceKind::I, from, to),
            sequence,
        ));
    }
    cases
}

fn jlstz_expected_sequences() -> Vec<(RotationState, RotationState, KickOffsetSequence)> {
    use RotationState::{Left, Right, Two, Zero};

    vec![
        (
            Zero,
            Right,
            sequence([(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)]),
        ),
        (
            Right,
            Two,
            sequence([(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)]),
        ),
        (
            Two,
            Left,
            sequence([(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)]),
        ),
        (
            Left,
            Zero,
            sequence([(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)]),
        ),
        (
            Zero,
            Left,
            sequence([(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)]),
        ),
        (
            Left,
            Two,
            sequence([(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)]),
        ),
        (
            Two,
            Right,
            sequence([(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)]),
        ),
        (
            Right,
            Zero,
            sequence([(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)]),
        ),
    ]
}

fn i_expected_sequences() -> Vec<(RotationState, RotationState, KickOffsetSequence)> {
    use RotationState::{Left, Right, Two, Zero};

    vec![
        (
            Zero,
            Right,
            sequence([(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)]),
        ),
        (
            Right,
            Two,
            sequence([(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)]),
        ),
        (
            Two,
            Left,
            sequence([(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)]),
        ),
        (
            Left,
            Zero,
            sequence([(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)]),
        ),
        (
            Zero,
            Left,
            sequence([(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)]),
        ),
        (
            Left,
            Two,
            sequence([(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)]),
        ),
        (
            Two,
            Right,
            sequence([(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)]),
        ),
        (
            Right,
            Zero,
            sequence([(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)]),
        ),
    ]
}

fn srs_plus_i_expected_sequences() -> Vec<(RotationState, RotationState, KickOffsetSequence)> {
    use RotationState::{Left, Right, Two, Zero};

    vec![
        (
            Zero,
            Right,
            sequence([(0, 0), (1, 0), (-2, 0), (-2, -1), (1, 2)]),
        ),
        (
            Right,
            Two,
            sequence([(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)]),
        ),
        (
            Two,
            Left,
            sequence([(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)]),
        ),
        (
            Left,
            Zero,
            sequence([(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)]),
        ),
        (
            Zero,
            Left,
            sequence([(0, 0), (-1, 0), (2, 0), (2, -1), (-1, 2)]),
        ),
        (
            Left,
            Two,
            sequence([(0, 0), (1, 0), (-2, 0), (1, 2), (-2, -1)]),
        ),
        (
            Two,
            Right,
            sequence([(0, 0), (-2, 0), (1, 0), (-2, 1), (1, -2)]),
        ),
        (
            Right,
            Zero,
            sequence([(0, 0), (-1, 0), (2, 0), (-1, -2), (2, 1)]),
        ),
    ]
}

fn srs_plus_jlstz_180_expected_sequences() -> Vec<(RotationState, RotationState, KickOffsetSequence)>
{
    use RotationState::{Left, Right, Two, Zero};

    vec![
        (
            Zero,
            Two,
            sequence([(0, 0), (0, 1), (1, 1), (-1, 1), (1, 0), (-1, 0)]),
        ),
        (
            Right,
            Left,
            sequence([(0, 0), (1, 0), (1, 2), (1, 1), (0, 2), (0, 1)]),
        ),
        (
            Two,
            Zero,
            sequence([(0, 0), (0, -1), (-1, -1), (1, -1), (-1, 0), (1, 0)]),
        ),
        (
            Left,
            Right,
            sequence([(0, 0), (-1, 0), (-1, 2), (-1, 1), (0, 2), (0, 1)]),
        ),
    ]
}

fn srs_plus_i_180_expected_sequences() -> Vec<(RotationState, RotationState, KickOffsetSequence)> {
    use RotationState::{Left, Right, Two, Zero};

    vec![
        (Zero, Two, sequence([(0, 0), (0, 1)])),
        (Right, Left, sequence([(0, 0), (1, 0)])),
        (Two, Zero, sequence([(0, 0), (0, -1)])),
        (Left, Right, sequence([(0, 0), (-1, 0)])),
    ]
}

fn jstris_verification_cases() -> Vec<KickVerificationCase> {
    let mut cases = Vec::new();
    for piece in [
        PieceKind::J,
        PieceKind::L,
        PieceKind::S,
        PieceKind::T,
        PieceKind::Z,
    ] {
        for (from, to, sequence) in jlstz_expected_sequences() {
            cases.push(KickVerificationCase::new(
                "jstris-jlstz-90",
                KickTransition::new(piece, from, to),
                sequence,
            ));
        }
    }
    for (from, to, sequence) in i_expected_sequences() {
        cases.push(KickVerificationCase::new(
            "jstris-i-90",
            KickTransition::new(PieceKind::I, from, to),
            sequence,
        ));
    }
    for piece in [
        PieceKind::I,
        PieceKind::J,
        PieceKind::L,
        PieceKind::S,
        PieceKind::T,
        PieceKind::Z,
    ] {
        for (from, to, sequence) in srs_plus_i_180_expected_sequences() {
            cases.push(KickVerificationCase::new(
                "jstris-180",
                KickTransition::new(piece, from, to),
                sequence,
            ));
        }
    }
    cases
}

fn sequence<const N: usize>(values: [(i8, i8); N]) -> KickOffsetSequence {
    KickOffsetSequence::new(
        values
            .into_iter()
            .map(|(dx, dy)| KickOffset::new(dx, dy))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use crate::kicks::KickTableProfileId;

    use super::*;

    #[test]
    fn builtin_kick_contract_report_exposes_srs90_and_srs_plus_180_profile() {
        let report = KickContractReport::verify_builtin_contracts();

        assert_eq!(report.srs_jlstz_transition_count(), 8);
        assert_eq!(report.srs_i_transition_count(), 8);
        assert_eq!(report.o_piece_model(), "clearra-internal-no-kick");
        assert_eq!(
            report.no_kick_transition_count(),
            PieceKind::STANDARD_TETROMINOES.len() * eight_direction_transitions().len()
        );
        assert_eq!(report.srs_plus_effective_kick_model(), "srs-plus-180");
        assert_eq!(report.srs_plus_extension_reason(), None);
        assert_eq!(
            report.srs_plus_180_transition_count(),
            (PieceKind::STANDARD_TETROMINOES.len() - 1) * 4
        );
        assert_eq!(
            report.jstris_180_transition_count(),
            (PieceKind::STANDARD_TETROMINOES.len() - 1) * 4
        );
        assert!(report.profile_registry_count() >= 6);
        assert_eq!(report.srs_profile_id(), KickTableProfileId::Srs90.as_str());
        assert_eq!(
            report.no_kick_profile_id(),
            KickTableProfileId::NoKick.as_str()
        );
        assert_eq!(
            report.srs_plus_profile_id(),
            KickTableProfileId::SrsPlus.as_str()
        );
        assert_eq!(
            report.jstris_profile_id(),
            KickTableProfileId::Jstris180.as_str()
        );
        assert_eq!(
            report.verification_case_count(),
            builtin_verification_cases().len()
                + no_kick_verification_cases().len()
                + srs_plus_verification_cases().len()
                + jstris_verification_cases().len()
        );
        assert_eq!(report.verification_failure_count(), 0);
    }
}
