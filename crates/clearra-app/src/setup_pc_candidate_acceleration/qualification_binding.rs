// This pure value matcher is kept dependency-free so its exact fail-closed
// binding matrix can be executed without linking the very large clearra-app
// unit-test binary.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QualificationBindingRejection {
    RequestIdentity,
    Profile,
    InitialBoard,
    QualifiedRequestIdentity,
    QualifiedProfile,
    QualifiedInitialBoard,
    Target,
    Objective,
    CandidateSource,
    SnapshotGeneration,
    EvidenceIdentityMissing,
}

pub(crate) struct CandidateBinding<'a, Profile, Snapshot> {
    request_identity: &'a [u8; 32],
    source_identity: &'a [u8; 32],
    profile: &'a Profile,
    initial_board_mask: u64,
    snapshot: &'a Snapshot,
}

impl<'a, Profile, Snapshot> CandidateBinding<'a, Profile, Snapshot> {
    pub(crate) const fn new(
        request_identity: &'a [u8; 32],
        source_identity: &'a [u8; 32],
        profile: &'a Profile,
        initial_board_mask: u64,
        snapshot: &'a Snapshot,
    ) -> Self {
        Self {
            request_identity,
            source_identity,
            profile,
            initial_board_mask,
            snapshot,
        }
    }
}

pub(crate) struct RequestBinding<'a, Profile, Objective> {
    request_identity: &'a [u8; 32],
    profile: &'a Profile,
    initial_board_mask: u64,
    target_lines: u8,
    objective: &'a Objective,
}

impl<'a, Profile, Objective> RequestBinding<'a, Profile, Objective> {
    pub(crate) const fn new(
        request_identity: &'a [u8; 32],
        profile: &'a Profile,
        initial_board_mask: u64,
        target_lines: u8,
        objective: &'a Objective,
    ) -> Self {
        Self {
            request_identity,
            profile,
            initial_board_mask,
            target_lines,
            objective,
        }
    }
}

pub(crate) struct QualificationBinding<'a, Profile, Objective, Snapshot> {
    request_identity: &'a [u8; 32],
    source_identity: &'a [u8; 32],
    profile: &'a Profile,
    initial_board_mask: u64,
    target_lines: u8,
    objective: &'a Objective,
    snapshot: &'a Snapshot,
    evidence_identity: &'a str,
}

impl<'a, Profile, Objective, Snapshot> QualificationBinding<'a, Profile, Objective, Snapshot> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        request_identity: &'a [u8; 32],
        source_identity: &'a [u8; 32],
        profile: &'a Profile,
        initial_board_mask: u64,
        target_lines: u8,
        objective: &'a Objective,
        snapshot: &'a Snapshot,
        evidence_identity: &'a str,
    ) -> Self {
        Self {
            request_identity,
            source_identity,
            profile,
            initial_board_mask,
            target_lines,
            objective,
            snapshot,
            evidence_identity,
        }
    }
}

pub(crate) fn validate<Profile: Eq, Objective: Eq, Snapshot: Eq>(
    candidate: CandidateBinding<'_, Profile, Snapshot>,
    request: RequestBinding<'_, Profile, Objective>,
    qualification: QualificationBinding<'_, Profile, Objective, Snapshot>,
) -> Result<(), QualificationBindingRejection> {
    if candidate.request_identity != request.request_identity {
        return Err(QualificationBindingRejection::RequestIdentity);
    }
    if candidate.profile != request.profile {
        return Err(QualificationBindingRejection::Profile);
    }
    if candidate.initial_board_mask != request.initial_board_mask {
        return Err(QualificationBindingRejection::InitialBoard);
    }
    if qualification.request_identity != request.request_identity {
        return Err(QualificationBindingRejection::QualifiedRequestIdentity);
    }
    if qualification.profile != request.profile {
        return Err(QualificationBindingRejection::QualifiedProfile);
    }
    if qualification.initial_board_mask != request.initial_board_mask {
        return Err(QualificationBindingRejection::QualifiedInitialBoard);
    }
    if qualification.target_lines != request.target_lines {
        return Err(QualificationBindingRejection::Target);
    }
    if qualification.objective != request.objective {
        return Err(QualificationBindingRejection::Objective);
    }
    if qualification.source_identity != candidate.source_identity {
        return Err(QualificationBindingRejection::CandidateSource);
    }
    if qualification.snapshot != candidate.snapshot {
        return Err(QualificationBindingRejection::SnapshotGeneration);
    }
    if qualification.evidence_identity.trim().is_empty() {
        return Err(QualificationBindingRejection::EvidenceIdentityMissing);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Objective {
        Joint,
        Build,
    }

    fn validate_fixture(
        candidate_request: u8,
        candidate_source: u8,
        candidate_profile: u8,
        candidate_board: u64,
        candidate_snapshot: u8,
        request_identity: u8,
        request_profile: u8,
        request_board: u64,
        request_target: u8,
        request_objective: Objective,
        qualified_request: u8,
        qualified_source: u8,
        qualified_profile: u8,
        qualified_board: u64,
        qualified_target: u8,
        qualified_objective: Objective,
        qualified_snapshot: u8,
        evidence: &str,
    ) -> Result<(), QualificationBindingRejection> {
        let candidate_request = [candidate_request; 32];
        let candidate_source = [candidate_source; 32];
        let request_identity = [request_identity; 32];
        let qualified_request = [qualified_request; 32];
        let qualified_source = [qualified_source; 32];
        validate(
            CandidateBinding::new(
                &candidate_request,
                &candidate_source,
                &candidate_profile,
                candidate_board,
                &candidate_snapshot,
            ),
            RequestBinding::new(
                &request_identity,
                &request_profile,
                request_board,
                request_target,
                &request_objective,
            ),
            QualificationBinding::new(
                &qualified_request,
                &qualified_source,
                &qualified_profile,
                qualified_board,
                qualified_target,
                &qualified_objective,
                &qualified_snapshot,
                evidence,
            ),
        )
    }

    fn exact(target: u8) -> Result<(), QualificationBindingRejection> {
        validate_fixture(
            1,
            2,
            3,
            4,
            5,
            1,
            3,
            4,
            target,
            Objective::Joint,
            1,
            2,
            3,
            4,
            target,
            Objective::Joint,
            5,
            "differential-evidence",
        )
    }

    #[test]
    fn each_one_through_four_line_target_accepts_only_an_exact_binding() {
        for target in 1..=4 {
            assert_eq!(exact(target), Ok(()));
        }
    }

    #[test]
    fn every_bound_dimension_fails_closed() {
        let cases = [
            (
                validate_fixture(
                    9,
                    2,
                    3,
                    4,
                    5,
                    1,
                    3,
                    4,
                    4,
                    Objective::Joint,
                    1,
                    2,
                    3,
                    4,
                    4,
                    Objective::Joint,
                    5,
                    "evidence",
                ),
                QualificationBindingRejection::RequestIdentity,
            ),
            (
                validate_fixture(
                    1,
                    2,
                    9,
                    4,
                    5,
                    1,
                    3,
                    4,
                    4,
                    Objective::Joint,
                    1,
                    2,
                    3,
                    4,
                    4,
                    Objective::Joint,
                    5,
                    "evidence",
                ),
                QualificationBindingRejection::Profile,
            ),
            (
                validate_fixture(
                    1,
                    2,
                    3,
                    9,
                    5,
                    1,
                    3,
                    4,
                    4,
                    Objective::Joint,
                    1,
                    2,
                    3,
                    4,
                    4,
                    Objective::Joint,
                    5,
                    "evidence",
                ),
                QualificationBindingRejection::InitialBoard,
            ),
            (
                validate_fixture(
                    1,
                    2,
                    3,
                    4,
                    5,
                    1,
                    3,
                    4,
                    4,
                    Objective::Joint,
                    9,
                    2,
                    3,
                    4,
                    4,
                    Objective::Joint,
                    5,
                    "evidence",
                ),
                QualificationBindingRejection::QualifiedRequestIdentity,
            ),
            (
                validate_fixture(
                    1,
                    2,
                    3,
                    4,
                    5,
                    1,
                    3,
                    4,
                    4,
                    Objective::Joint,
                    1,
                    2,
                    9,
                    4,
                    4,
                    Objective::Joint,
                    5,
                    "evidence",
                ),
                QualificationBindingRejection::QualifiedProfile,
            ),
            (
                validate_fixture(
                    1,
                    2,
                    3,
                    4,
                    5,
                    1,
                    3,
                    4,
                    4,
                    Objective::Joint,
                    1,
                    2,
                    3,
                    9,
                    4,
                    Objective::Joint,
                    5,
                    "evidence",
                ),
                QualificationBindingRejection::QualifiedInitialBoard,
            ),
            (
                validate_fixture(
                    1,
                    2,
                    3,
                    4,
                    5,
                    1,
                    3,
                    4,
                    4,
                    Objective::Joint,
                    1,
                    2,
                    3,
                    4,
                    3,
                    Objective::Joint,
                    5,
                    "evidence",
                ),
                QualificationBindingRejection::Target,
            ),
            (
                validate_fixture(
                    1,
                    2,
                    3,
                    4,
                    5,
                    1,
                    3,
                    4,
                    4,
                    Objective::Joint,
                    1,
                    2,
                    3,
                    4,
                    4,
                    Objective::Build,
                    5,
                    "evidence",
                ),
                QualificationBindingRejection::Objective,
            ),
            (
                validate_fixture(
                    1,
                    2,
                    3,
                    4,
                    5,
                    1,
                    3,
                    4,
                    4,
                    Objective::Joint,
                    1,
                    9,
                    3,
                    4,
                    4,
                    Objective::Joint,
                    5,
                    "evidence",
                ),
                QualificationBindingRejection::CandidateSource,
            ),
            (
                validate_fixture(
                    1,
                    2,
                    3,
                    4,
                    5,
                    1,
                    3,
                    4,
                    4,
                    Objective::Joint,
                    1,
                    2,
                    3,
                    4,
                    4,
                    Objective::Joint,
                    9,
                    "evidence",
                ),
                QualificationBindingRejection::SnapshotGeneration,
            ),
            (
                validate_fixture(
                    1,
                    2,
                    3,
                    4,
                    5,
                    1,
                    3,
                    4,
                    4,
                    Objective::Joint,
                    1,
                    2,
                    3,
                    4,
                    4,
                    Objective::Joint,
                    5,
                    "  ",
                ),
                QualificationBindingRejection::EvidenceIdentityMissing,
            ),
        ];
        for (actual, expected) in cases {
            assert_eq!(actual, Err(expected));
        }
    }
}
