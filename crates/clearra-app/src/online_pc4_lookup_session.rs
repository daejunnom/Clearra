//! Feature-off application owner for one online PC4 lookup session.
//!
//! This module deliberately owns no HTTP client and starts no offline solver.
//! Hosts satisfy the emitted Range request, while a fallback remains a typed
//! signal until a product surface has obtained explicit user authorization and
//! starts a separate exact-search request.

use clearra_pc4_tablebase::{
    ActivatedSnapshot, LookupFailure, LookupHit, LookupMachine, LookupSessionId, LookupStartError,
    LookupStep, Pc4RuleProfile, Pc4TargetLines, Pc4TerminalUseCase, QualifiedPc4TargetIdentity,
    RangeRequest, RangeResponse, RangeTransportFailure, SupplyError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4OfflineFallbackAuthorization {
    NotAuthorized,
    ExplicitlyAuthorized,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4OnlineLookupRequest {
    lookup_session: LookupSessionId,
    target: QualifiedPc4TargetIdentity,
    field_hash: u64,
    offline_fallback: Pc4OfflineFallbackAuthorization,
}

impl Pc4OnlineLookupRequest {
    pub const fn new(
        lookup_session: LookupSessionId,
        target: QualifiedPc4TargetIdentity,
        field_hash: u64,
        offline_fallback: Pc4OfflineFallbackAuthorization,
    ) -> Self {
        Self {
            lookup_session,
            target,
            field_hash,
            offline_fallback,
        }
    }

    pub const fn lookup_session(&self) -> LookupSessionId {
        self.lookup_session
    }

    pub const fn profile(&self) -> Pc4RuleProfile {
        self.target.profile()
    }

    pub const fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.target
    }

    pub const fn use_case(&self) -> Pc4TerminalUseCase {
        self.target.use_case()
    }

    pub const fn target_lines(&self) -> Pc4TargetLines {
        self.target.target_lines()
    }

    pub const fn field_hash(&self) -> u64 {
        self.field_hash
    }

    pub const fn offline_fallback(&self) -> Pc4OfflineFallbackAuthorization {
        self.offline_fallback
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4FallbackCause {
    LookupMiss,
    LookupFailure(LookupFailure),
}

impl Pc4FallbackCause {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::LookupMiss => "pc4_online_lookup_miss",
            Self::LookupFailure(failure) => failure.reason(),
        }
    }
}

/// Permission-bearing signal for the owner of the original exact-search
/// request. It contains no partial graph result and cannot execute a fallback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4OfflineFallbackSignal {
    lookup_session: LookupSessionId,
    target: QualifiedPc4TargetIdentity,
    field_hash: u64,
    cause: Pc4FallbackCause,
}

impl Pc4OfflineFallbackSignal {
    pub const fn lookup_session(&self) -> LookupSessionId {
        self.lookup_session
    }

    pub const fn profile(&self) -> Pc4RuleProfile {
        self.target.profile()
    }

    pub const fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.target
    }

    pub const fn use_case(&self) -> Pc4TerminalUseCase {
        self.target.use_case()
    }

    pub const fn target_lines(&self) -> Pc4TargetLines {
        self.target.target_lines()
    }

    pub const fn field_hash(&self) -> u64 {
        self.field_hash
    }

    pub const fn cause(&self) -> &Pc4FallbackCause {
        &self.cause
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4OfflineFallbackDisposition {
    NotAvailable,
    RequiresExplicitAuthorization { cause: Pc4FallbackCause },
    Authorized(Pc4OfflineFallbackSignal),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppOnlinePc4LookupStep {
    NeedRange(RangeRequest),
    Hit(AppQualifiedPc4LookupHit),
    Miss,
    Failed(LookupFailure),
    Cancelled,
}

/// Raw graph-record hit carried together with the exact target qualification
/// that authorized this lookup session. It is not candidate completeness:
/// traversal and exact placement materialization still have to finish.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppQualifiedPc4LookupHit {
    target: QualifiedPc4TargetIdentity,
    lookup: LookupHit,
}

impl AppQualifiedPc4LookupHit {
    pub const fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.target
    }

    pub const fn lookup(&self) -> &LookupHit {
        &self.lookup
    }

    pub fn into_parts(self) -> (QualifiedPc4TargetIdentity, LookupHit) {
        (self.target, self.lookup)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppOnlinePc4LookupStartError {
    TargetSnapshotMismatch,
    Lookup(LookupStartError),
}

impl AppOnlinePc4LookupStartError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::TargetSnapshotMismatch => "pc4_online_target_snapshot_mismatch",
            Self::Lookup(error) => error.reason(),
        }
    }
}

impl From<LookupStartError> for AppOnlinePc4LookupStartError {
    fn from(value: LookupStartError) -> Self {
        Self::Lookup(value)
    }
}

pub struct AppOnlinePc4LookupSession {
    request: Pc4OnlineLookupRequest,
    machine: LookupMachine,
}

impl AppOnlinePc4LookupSession {
    pub fn start(
        snapshot: &ActivatedSnapshot,
        request: Pc4OnlineLookupRequest,
    ) -> Result<Self, AppOnlinePc4LookupStartError> {
        if request.target().snapshot() != snapshot.qualified_identity() {
            return Err(AppOnlinePc4LookupStartError::TargetSnapshotMismatch);
        }
        let machine = LookupMachine::start(
            snapshot,
            request.profile(),
            request.field_hash(),
            request.lookup_session(),
        )
        .map_err(AppOnlinePc4LookupStartError::Lookup)?;
        Ok(Self { request, machine })
    }

    pub const fn request(&self) -> &Pc4OnlineLookupRequest {
        &self.request
    }

    pub fn step(&self) -> AppOnlinePc4LookupStep {
        match self.machine.step() {
            LookupStep::NeedRange(request) => AppOnlinePc4LookupStep::NeedRange(request),
            LookupStep::Hit(hit) => AppOnlinePc4LookupStep::Hit(AppQualifiedPc4LookupHit {
                target: self.request.target().clone(),
                lookup: hit,
            }),
            LookupStep::Miss => AppOnlinePc4LookupStep::Miss,
            LookupStep::Failed(failure) => AppOnlinePc4LookupStep::Failed(failure),
            LookupStep::Cancelled => AppOnlinePc4LookupStep::Cancelled,
        }
    }

    pub fn supply(&mut self, response: RangeResponse) -> Result<(), SupplyError> {
        self.machine.supply(response)
    }

    pub fn reject_range(
        &mut self,
        request: &RangeRequest,
        failure: RangeTransportFailure,
    ) -> Result<(), SupplyError> {
        self.machine.reject_range(request, failure)
    }

    pub fn cancel(&mut self) {
        self.machine.cancel();
    }

    /// Reports whether a separate offline exact request may be offered or
    /// started. Merely observing this value never starts computation.
    pub fn offline_fallback_disposition(&self) -> Pc4OfflineFallbackDisposition {
        let cause = match self.machine.step() {
            LookupStep::Miss => Pc4FallbackCause::LookupMiss,
            LookupStep::Failed(failure) => Pc4FallbackCause::LookupFailure(failure),
            LookupStep::NeedRange(_) | LookupStep::Hit(_) | LookupStep::Cancelled => {
                return Pc4OfflineFallbackDisposition::NotAvailable;
            }
        };
        match self.request.offline_fallback() {
            Pc4OfflineFallbackAuthorization::NotAuthorized => {
                Pc4OfflineFallbackDisposition::RequiresExplicitAuthorization { cause }
            }
            Pc4OfflineFallbackAuthorization::ExplicitlyAuthorized => {
                Pc4OfflineFallbackDisposition::Authorized(Pc4OfflineFallbackSignal {
                    lookup_session: self.request.lookup_session(),
                    target: self.request.target().clone(),
                    field_hash: self.request.field_hash(),
                    cause,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_pc4_tablebase::{
        ArtifactDescriptor, DatasetSnapshotManifest, DatasetSnapshotVerifier, GraphTargetEncoding,
        ManifestContentIdentity, Pc4ArtifactRole, Pc4ProfileManifest, ProfileAvailability,
        ProfileQualification, ProfileTargetCompletenessQualification, SnapshotIdentity,
        SnapshotVerificationAttestation, SnapshotVerificationFailure, SnapshotVerificationRequest,
    };

    struct SyntheticVerifier;

    impl DatasetSnapshotVerifier for SyntheticVerifier {
        fn verify(
            &mut self,
            request: SnapshotVerificationRequest<'_>,
        ) -> Result<SnapshotVerificationAttestation, SnapshotVerificationFailure> {
            Ok(SnapshotVerificationAttestation::new(
                request.snapshot_identity().clone(),
                request.manifest_content_identity().clone(),
                "synthetic-app-lookup-verification",
            )
            .expect("synthetic verification attestation"))
        }
    }

    fn lookup_session(value: u64) -> LookupSessionId {
        LookupSessionId::new(value).expect("non-zero lookup session")
    }

    fn snapshot(generation: &str) -> ActivatedSnapshot {
        let identity = SnapshotIdentity::new(
            "synthetic/repository",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            generation,
        )
        .expect("snapshot identity");
        let profiles = Pc4RuleProfile::ALL
            .into_iter()
            .map(|profile| {
                let prefix = profile.as_str();
                let descriptor = |role, suffix: &str, byte_len| {
                    ArtifactDescriptor::new(
                        role,
                        format!("{prefix}/{suffix}"),
                        byte_len,
                        format!("{prefix}-{suffix}-identity"),
                    )
                    .expect("artifact")
                };
                let manifest = Pc4ProfileManifest::new(
                    profile,
                    1,
                    GraphTargetEncoding::U24LittleEndian,
                    64,
                    descriptor(Pc4ArtifactRole::FieldHashIndex, "field.idx", 24),
                    descriptor(Pc4ArtifactRole::GraphOffsets, "offsets.idx", 24),
                    descriptor(Pc4ArtifactRole::Graph, "graph.bin", 64),
                    ProfileQualification::new(
                        format!("{prefix}-index-spec"),
                        format!("{prefix}-graph-spec"),
                        format!("{prefix}-provenance"),
                        format!("{prefix}-kat"),
                    )
                    .expect("qualification"),
                )
                .expect("profile manifest");
                let manifest = if profile == Pc4RuleProfile::Srs {
                    manifest
                        .with_target_qualifications(vec![
                            ProfileTargetCompletenessQualification::new(
                                Pc4TerminalUseCase::PcSearch,
                                Pc4TargetLines::new(4).expect("4L target"),
                                "synthetic-pc-terminal",
                                "synthetic-all-outgoing-edges",
                                "synthetic-pc-known-answer",
                                "synthetic-offline-exact-parity",
                            )
                            .expect("synthetic target qualification"),
                        ])
                        .expect("unique target qualification")
                } else {
                    manifest
                };
                ProfileAvailability::qualified(manifest)
            })
            .collect();
        DatasetSnapshotManifest::new(
            identity,
            ManifestContentIdentity::new(format!("synthetic-app-lookup-manifest:{generation}"))
                .expect("manifest content identity"),
            profiles,
        )
        .expect("manifest")
        .activate(&mut SyntheticVerifier)
        .expect("all profiles qualified")
    }

    fn new_session(
        id: u64,
        authorization: Pc4OfflineFallbackAuthorization,
    ) -> AppOnlinePc4LookupSession {
        let snapshot = snapshot("generation-a");
        let target = snapshot
            .qualified_target(
                Pc4RuleProfile::Srs,
                Pc4TerminalUseCase::PcSearch,
                Pc4TargetLines::new(4).expect("4L target"),
            )
            .expect("qualified PC target");
        AppOnlinePc4LookupSession::start(
            &snapshot,
            Pc4OnlineLookupRequest::new(lookup_session(id), target, 15, authorization),
        )
        .expect("lookup session")
    }

    #[test]
    fn target_from_another_generation_cannot_start_a_lookup() {
        let old_snapshot = snapshot("generation-a");
        let old_target = old_snapshot
            .qualified_target(
                Pc4RuleProfile::Srs,
                Pc4TerminalUseCase::PcSearch,
                Pc4TargetLines::new(4).expect("4L target"),
            )
            .expect("old qualified target");
        let current_snapshot = snapshot("generation-b");
        let error = match AppOnlinePc4LookupSession::start(
            &current_snapshot,
            Pc4OnlineLookupRequest::new(
                lookup_session(99),
                old_target,
                15,
                Pc4OfflineFallbackAuthorization::NotAuthorized,
            ),
        ) {
            Err(error) => error,
            Ok(_) => panic!("target generation mismatch must fail before Range I/O"),
        };

        assert_eq!(error, AppOnlinePc4LookupStartError::TargetSnapshotMismatch);
        assert_eq!(error.reason(), "pc4_online_target_snapshot_mismatch");
    }

    #[test]
    fn range_is_host_driven_and_fallback_is_unavailable_while_lookup_is_live() {
        let session = new_session(1, Pc4OfflineFallbackAuthorization::NotAuthorized);
        let AppOnlinePc4LookupStep::NeedRange(request) = session.step() else {
            panic!("host-driven Range request")
        };
        assert_eq!(request.lookup_session(), lookup_session(1));
        assert_eq!(
            session.offline_fallback_disposition(),
            Pc4OfflineFallbackDisposition::NotAvailable
        );
    }

    #[test]
    fn fallback_requires_explicit_authorization_and_never_starts_inside_session_owner() {
        let mut session = new_session(1, Pc4OfflineFallbackAuthorization::NotAuthorized);
        let AppOnlinePc4LookupStep::NeedRange(request) = session.step() else {
            panic!("Range request")
        };
        session
            .reject_range(&request, RangeTransportFailure::Offline)
            .expect("transport failure");
        assert_eq!(
            session.offline_fallback_disposition(),
            Pc4OfflineFallbackDisposition::RequiresExplicitAuthorization {
                cause: Pc4FallbackCause::LookupFailure(LookupFailure::Offline),
            }
        );

        let mut authorized = new_session(2, Pc4OfflineFallbackAuthorization::ExplicitlyAuthorized);
        let AppOnlinePc4LookupStep::NeedRange(request) = authorized.step() else {
            panic!("Range request")
        };
        authorized
            .reject_range(
                &request,
                RangeTransportFailure::RateLimited {
                    retry_after_seconds: Some(30),
                },
            )
            .expect("rate-limit failure");
        let Pc4OfflineFallbackDisposition::Authorized(signal) =
            authorized.offline_fallback_disposition()
        else {
            panic!("explicitly authorized signal")
        };
        assert_eq!(signal.lookup_session(), lookup_session(2));
        assert_eq!(signal.profile(), Pc4RuleProfile::Srs);
        assert_eq!(signal.use_case(), Pc4TerminalUseCase::PcSearch);
        assert_eq!(signal.target_lines().get(), 4);
        assert_eq!(signal.field_hash(), 15);
        assert_eq!(signal.cause().reason(), "pc4_online_rate_limited");
    }

    #[test]
    fn a_delayed_failure_from_another_session_cannot_end_the_current_app_session() {
        let old = new_session(1, Pc4OfflineFallbackAuthorization::NotAuthorized);
        let AppOnlinePc4LookupStep::NeedRange(old_request) = old.step() else {
            panic!("old Range request")
        };
        let mut current = new_session(2, Pc4OfflineFallbackAuthorization::NotAuthorized);
        assert_eq!(
            current.reject_range(&old_request, RangeTransportFailure::Timeout),
            Err(SupplyError::LookupSessionMismatch)
        );
        assert!(matches!(
            current.step(),
            AppOnlinePc4LookupStep::NeedRange(_)
        ));
    }

    #[test]
    fn cancellation_never_offers_or_authorizes_an_offline_fallback() {
        let mut session = new_session(1, Pc4OfflineFallbackAuthorization::ExplicitlyAuthorized);
        session.cancel();
        assert_eq!(session.step(), AppOnlinePc4LookupStep::Cancelled);
        assert_eq!(
            session.offline_fallback_disposition(),
            Pc4OfflineFallbackDisposition::NotAvailable
        );
    }
}
