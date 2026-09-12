// SRP rationale: this module has one change reason: projecting one verified PC4
// snapshot into manifest-private, adapter-safe capability slots.

use clearra_pc4_tablebase::{
    ActivatedSnapshot, Pc4ProfileManifest, Pc4RuleProfile, Pc4TargetLines, Pc4TerminalUseCase,
    ProfileAvailability, UnsupportedProfileReason,
};

/// Adapter-safe qualification of one exact PC4 rule profile.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4ProfileCapabilityStatus {
    ProfileQualified,
    ProfileNotQualified { reason: UnsupportedProfileReason },
}

/// Adapter-safe qualification of one exact use-case/target pair.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4TargetCapabilityStatus {
    TargetQualified,
    TargetNotQualified,
}

/// One independently projected PC or Setup target. The projection deliberately
/// retains no manifest, artifact, provenance, or known-answer identity string.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pc4TargetCapabilitySlot {
    use_case: Pc4TerminalUseCase,
    target_lines: Pc4TargetLines,
    status: Pc4TargetCapabilityStatus,
}

impl Pc4TargetCapabilitySlot {
    pub const fn use_case(&self) -> Pc4TerminalUseCase {
        self.use_case
    }

    pub const fn target_lines(&self) -> Pc4TargetLines {
        self.target_lines
    }

    pub const fn status(&self) -> Pc4TargetCapabilityStatus {
        self.status
    }
}

/// One of the five PC4 rule-profile slots. PC and Setup targets remain in
/// distinct fixed arrays so consumers cannot substitute one use case for the
/// other.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4ProfileCapabilitySlot {
    profile: Pc4RuleProfile,
    status: Pc4ProfileCapabilityStatus,
    pc_search_targets: [Pc4TargetCapabilitySlot; 4],
    setup_search_targets: [Pc4TargetCapabilitySlot; 4],
}

impl Pc4ProfileCapabilitySlot {
    pub const fn profile(&self) -> Pc4RuleProfile {
        self.profile
    }

    pub const fn status(&self) -> Pc4ProfileCapabilityStatus {
        self.status
    }

    pub const fn pc_search_targets(&self) -> &[Pc4TargetCapabilitySlot; 4] {
        &self.pc_search_targets
    }

    pub const fn setup_search_targets(&self) -> &[Pc4TargetCapabilitySlot; 4] {
        &self.setup_search_targets
    }

    pub fn target(
        &self,
        use_case: Pc4TerminalUseCase,
        target_lines: Pc4TargetLines,
    ) -> &Pc4TargetCapabilitySlot {
        let targets = match use_case {
            Pc4TerminalUseCase::PcSearch => &self.pc_search_targets,
            Pc4TerminalUseCase::SetupSearch => &self.setup_search_targets,
        };
        &targets[usize::from(target_lines.get() - Pc4TargetLines::MIN)]
    }
}

/// Complete five-profile capability view of one already activated snapshot.
///
/// It is intentionally not serializable and exposes no command, I/O, product
/// activation, artifact path, or manifest identity surface. Future adapters
/// may translate these typed slots into their own public presentation without
/// gaining authority to infer a missing profile or target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4ProfileCapabilityProjection {
    profiles: [Pc4ProfileCapabilitySlot; 5],
}

impl Pc4ProfileCapabilityProjection {
    pub const fn profiles(&self) -> &[Pc4ProfileCapabilitySlot; 5] {
        &self.profiles
    }

    pub const fn profile(&self, profile: Pc4RuleProfile) -> &Pc4ProfileCapabilitySlot {
        &self.profiles[profile_slot_index(profile)]
    }
}

/// Projects only claims present in the exact activated profile and exact
/// `(use_case, target_lines)` qualification. Missing claims remain missing;
/// this function performs no fallback or cross-profile/cross-use-case reuse.
pub fn project_pc4_profile_capabilities(
    snapshot: &ActivatedSnapshot,
) -> Pc4ProfileCapabilityProjection {
    Pc4ProfileCapabilityProjection {
        profiles: Pc4RuleProfile::ALL.map(|profile| project_profile(snapshot, profile)),
    }
}

fn project_profile(
    snapshot: &ActivatedSnapshot,
    profile: Pc4RuleProfile,
) -> Pc4ProfileCapabilitySlot {
    let (status, manifest) = match snapshot.profile_availability(profile) {
        ProfileAvailability::Qualified(manifest) => (
            Pc4ProfileCapabilityStatus::ProfileQualified,
            Some(&**manifest),
        ),
        ProfileAvailability::Unsupported { reason, .. } => (
            Pc4ProfileCapabilityStatus::ProfileNotQualified { reason: *reason },
            None,
        ),
    };
    Pc4ProfileCapabilitySlot {
        profile,
        status,
        pc_search_targets: project_targets(manifest, Pc4TerminalUseCase::PcSearch),
        setup_search_targets: project_targets(manifest, Pc4TerminalUseCase::SetupSearch),
    }
}

fn project_targets(
    manifest: Option<&Pc4ProfileManifest>,
    use_case: Pc4TerminalUseCase,
) -> [Pc4TargetCapabilitySlot; 4] {
    [1_u8, 2, 3, 4].map(|lines| {
        let target_lines = Pc4TargetLines::new(lines)
            .expect("the capability projection enumerates only the PC4 1..=4 target domain");
        let status = if manifest
            .and_then(|profile| profile.target_qualification(use_case, target_lines))
            .is_some()
        {
            Pc4TargetCapabilityStatus::TargetQualified
        } else {
            Pc4TargetCapabilityStatus::TargetNotQualified
        };
        Pc4TargetCapabilitySlot {
            use_case,
            target_lines,
            status,
        }
    })
}

const fn profile_slot_index(profile: Pc4RuleProfile) -> usize {
    match profile {
        Pc4RuleProfile::Srs => 0,
        Pc4RuleProfile::SrsPlus => 1,
        Pc4RuleProfile::SrsX => 2,
        Pc4RuleProfile::Jstris180 => 3,
        Pc4RuleProfile::NoKick => 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_pc4_tablebase::{
        ArtifactDescriptor, DatasetSnapshotManifest, DatasetSnapshotVerifier, FieldIdIndexRelation,
        GraphTargetEncoding, ManifestContentIdentity, Pc4ArtifactRole, Pc4TerminalFieldIdentity,
        ProfileQualification, ProfileTargetCompletenessQualification, SnapshotIdentity,
        SnapshotVerificationAttestation, SnapshotVerificationFailure, SnapshotVerificationRequest,
    };

    const UNAVAILABLE: [(Pc4RuleProfile, UnsupportedProfileReason); 4] = [
        (
            Pc4RuleProfile::SrsPlus,
            UnsupportedProfileReason::MissingProfileArtifacts,
        ),
        (
            Pc4RuleProfile::SrsX,
            UnsupportedProfileReason::MissingProfileSpecificIndex,
        ),
        (
            Pc4RuleProfile::Jstris180,
            UnsupportedProfileReason::MissingFormatSpecification,
        ),
        (
            Pc4RuleProfile::NoKick,
            UnsupportedProfileReason::MissingProvenance,
        ),
    ];

    struct SyntheticVerifier;

    impl DatasetSnapshotVerifier for SyntheticVerifier {
        fn verify(
            &mut self,
            request: SnapshotVerificationRequest<'_>,
        ) -> Result<SnapshotVerificationAttestation, SnapshotVerificationFailure> {
            SnapshotVerificationAttestation::new(
                request.snapshot_identity().clone(),
                request.manifest_content_identity().clone(),
                "synthetic-capability-verification",
            )
            .map_err(|_| SnapshotVerificationFailure::Rejected)
        }
    }

    fn activated_snapshot(qualified_targets: &[(Pc4TerminalUseCase, u8)]) -> ActivatedSnapshot {
        let qualified = qualified_profile(qualified_targets);
        let profiles = Pc4RuleProfile::ALL
            .into_iter()
            .map(|profile| {
                if profile == Pc4RuleProfile::Srs {
                    ProfileAvailability::qualified(qualified.clone())
                } else {
                    let reason = UNAVAILABLE
                        .iter()
                        .find_map(|(candidate, reason)| (*candidate == profile).then_some(*reason))
                        .expect("every non-SRS profile has an explicit unavailable reason");
                    ProfileAvailability::Unsupported { profile, reason }
                }
            })
            .collect();
        DatasetSnapshotManifest::new(
            SnapshotIdentity::new(
                "synthetic/repository",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "capability-generation",
            )
            .expect("snapshot identity"),
            ManifestContentIdentity::new("capability-manifest").expect("manifest identity"),
            profiles,
        )
        .expect("snapshot manifest")
        .activate(&mut SyntheticVerifier)
        .expect("activated snapshot")
    }

    fn qualified_profile(qualified_targets: &[(Pc4TerminalUseCase, u8)]) -> Pc4ProfileManifest {
        const FIELD_COUNT: u32 = 8;
        let descriptor = |role, path: &str, byte_len| {
            ArtifactDescriptor::new(role, path, byte_len, format!("identity-{path}"))
                .expect("artifact descriptor")
        };
        Pc4ProfileManifest::new(
            Pc4RuleProfile::Srs,
            FIELD_COUNT,
            GraphTargetEncoding::U24LittleEndian,
            FieldIdIndexRelation::RecordOrdinal,
            4_096,
            descriptor(Pc4ArtifactRole::FieldHashIndex, "srs/field.idx", 80),
            descriptor(Pc4ArtifactRole::GraphOffsets, "srs/offsets.idx", 52),
            descriptor(Pc4ArtifactRole::Graph, "srs/graph.bin", 1_024),
            ProfileQualification::new(
                "srs-index-spec",
                "srs-graph-spec",
                "srs-provenance",
                "srs-known-answers",
            )
            .expect("profile qualification"),
        )
        .expect("profile manifest")
        .with_target_qualifications(
            qualified_targets
                .iter()
                .copied()
                .map(|(use_case, lines)| target_qualification(use_case, lines))
                .collect(),
        )
        .expect("target qualifications")
    }

    fn target_qualification(
        use_case: Pc4TerminalUseCase,
        lines: u8,
    ) -> ProfileTargetCompletenessQualification {
        let target_lines = Pc4TargetLines::new(lines).expect("target lines");
        ProfileTargetCompletenessQualification::new(
            use_case,
            target_lines,
            Pc4TerminalFieldIdentity::full_rows(target_lines, 0),
            format!("{use_case:?}-{lines}-terminal"),
            format!("{use_case:?}-{lines}-outgoing"),
            format!("{use_case:?}-{lines}-known-answers"),
            format!("{use_case:?}-{lines}-offline-parity"),
        )
        .expect("target qualification")
    }

    fn status(
        projection: &Pc4ProfileCapabilityProjection,
        use_case: Pc4TerminalUseCase,
        lines: u8,
    ) -> Pc4TargetCapabilityStatus {
        projection
            .profile(Pc4RuleProfile::Srs)
            .target(use_case, Pc4TargetLines::new(lines).expect("target lines"))
            .status()
    }

    #[test]
    fn projection_enumerates_one_qualified_and_four_exact_unavailable_profiles() {
        let all_targets = [
            (Pc4TerminalUseCase::PcSearch, 1),
            (Pc4TerminalUseCase::PcSearch, 2),
            (Pc4TerminalUseCase::PcSearch, 3),
            (Pc4TerminalUseCase::PcSearch, 4),
            (Pc4TerminalUseCase::SetupSearch, 1),
            (Pc4TerminalUseCase::SetupSearch, 2),
            (Pc4TerminalUseCase::SetupSearch, 3),
            (Pc4TerminalUseCase::SetupSearch, 4),
        ];
        let projection = project_pc4_profile_capabilities(&activated_snapshot(&all_targets));

        assert_eq!(projection.profiles().len(), 5);
        assert_eq!(
            std::array::from_fn(|index| projection.profiles()[index].profile()),
            Pc4RuleProfile::ALL
        );
        assert_eq!(
            projection.profile(Pc4RuleProfile::Srs).status(),
            Pc4ProfileCapabilityStatus::ProfileQualified
        );
        for (profile, reason) in UNAVAILABLE {
            let slot = projection.profile(profile);
            assert_eq!(
                slot.status(),
                Pc4ProfileCapabilityStatus::ProfileNotQualified { reason }
            );
            assert!(slot
                .pc_search_targets()
                .iter()
                .chain(slot.setup_search_targets())
                .all(|target| target.status() == Pc4TargetCapabilityStatus::TargetNotQualified));
        }
    }

    #[test]
    fn projection_marks_every_missing_one_through_four_target_without_inference() {
        let projection = project_pc4_profile_capabilities(&activated_snapshot(&[(
            Pc4TerminalUseCase::PcSearch,
            2,
        )]));

        assert_eq!(
            [1_u8, 2, 3, 4].map(|lines| status(&projection, Pc4TerminalUseCase::PcSearch, lines)),
            [
                Pc4TargetCapabilityStatus::TargetNotQualified,
                Pc4TargetCapabilityStatus::TargetQualified,
                Pc4TargetCapabilityStatus::TargetNotQualified,
                Pc4TargetCapabilityStatus::TargetNotQualified,
            ]
        );
        assert!(projection
            .profile(Pc4RuleProfile::Srs)
            .setup_search_targets()
            .iter()
            .all(|target| target.status() == Pc4TargetCapabilityStatus::TargetNotQualified));
    }

    #[test]
    fn pc_and_setup_target_qualification_are_independent() {
        let projection = project_pc4_profile_capabilities(&activated_snapshot(&[
            (Pc4TerminalUseCase::PcSearch, 1),
            (Pc4TerminalUseCase::SetupSearch, 3),
        ]));

        assert_eq!(
            status(&projection, Pc4TerminalUseCase::PcSearch, 1),
            Pc4TargetCapabilityStatus::TargetQualified
        );
        assert_eq!(
            status(&projection, Pc4TerminalUseCase::SetupSearch, 1),
            Pc4TargetCapabilityStatus::TargetNotQualified
        );
        assert_eq!(
            status(&projection, Pc4TerminalUseCase::PcSearch, 3),
            Pc4TargetCapabilityStatus::TargetNotQualified
        );
        assert_eq!(
            status(&projection, Pc4TerminalUseCase::SetupSearch, 3),
            Pc4TargetCapabilityStatus::TargetQualified
        );
    }
}
