//! Feature-off application owner for one online PC4 lookup session.
//!
//! This module deliberately owns no HTTP client and starts no offline solver.
//! Hosts satisfy the emitted Range request, while a fallback remains a typed
//! signal until a product surface has obtained explicit user authorization and
//! starts a separate exact-search request.

use clearra_pc4_tablebase::{
    LookupFailure, LookupHit, LookupMachine, LookupSessionId, LookupStartError, LookupStep,
    Pc4RuleProfile, Pc4TargetLines, Pc4TerminalUseCase, PinnedPc4Generation,
    QualifiedPc4TargetIdentity, RangeAdmissionAttempt, RangeAdmissionError, RangeAdmissionGuard,
    RangeAdmissionInput, RangeAdmissionLimits, RangeAdmissionOutcome, RangeAdmissionSession,
    RangeRequest, RangeTransportFailure, SupplyError,
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
    field: Pc4OnlineLookupField,
    range_limits: RangeAdmissionLimits,
    offline_fallback: Pc4OfflineFallbackAuthorization,
}

/// Exact identity used to enter the qualified graph.
///
/// Hash lookup is the public board-entry path. ID lookup is reserved for
/// following an already-decoded graph edge: it is admitted only when the
/// manifest explicitly qualifies graph IDs as field-index ordinals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4OnlineLookupField {
    Hash(u64),
    Id(u32),
}

impl Pc4OnlineLookupField {
    pub const fn field_hash(self) -> Option<u64> {
        match self {
            Self::Hash(field_hash) => Some(field_hash),
            Self::Id(_) => None,
        }
    }

    pub const fn field_id(self) -> Option<u32> {
        match self {
            Self::Hash(_) => None,
            Self::Id(field_id) => Some(field_id),
        }
    }
}

impl Pc4OnlineLookupRequest {
    pub const fn new(
        lookup_session: LookupSessionId,
        target: QualifiedPc4TargetIdentity,
        field_hash: u64,
        range_limits: RangeAdmissionLimits,
        offline_fallback: Pc4OfflineFallbackAuthorization,
    ) -> Self {
        Self {
            lookup_session,
            target,
            field: Pc4OnlineLookupField::Hash(field_hash),
            range_limits,
            offline_fallback,
        }
    }

    pub const fn from_field_id(
        lookup_session: LookupSessionId,
        target: QualifiedPc4TargetIdentity,
        field_id: u32,
        range_limits: RangeAdmissionLimits,
        offline_fallback: Pc4OfflineFallbackAuthorization,
    ) -> Self {
        Self {
            lookup_session,
            target,
            field: Pc4OnlineLookupField::Id(field_id),
            range_limits,
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

    pub const fn field(&self) -> Pc4OnlineLookupField {
        self.field
    }

    pub const fn field_hash(&self) -> Option<u64> {
        self.field.field_hash()
    }

    pub const fn field_id(&self) -> Option<u32> {
        self.field.field_id()
    }

    pub const fn range_limits(&self) -> RangeAdmissionLimits {
        self.range_limits
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
    field: Pc4OnlineLookupField,
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

    pub const fn field(&self) -> Pc4OnlineLookupField {
        self.field
    }

    pub const fn field_hash(&self) -> Option<u64> {
        self.field.field_hash()
    }

    pub const fn field_id(&self) -> Option<u32> {
        self.field.field_id()
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

/// One admitted host response after the HTTP boundary has been checked.
///
/// A non-partial outcome also terminates the pending lookup request. It does
/// not start an offline search; the original request owner must separately
/// inspect `offline_fallback_disposition` and obtain explicit authorization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppOnlinePc4RangeDisposition {
    PartialContentSupplied,
    RangeNotSatisfiable { complete_length: u64 },
    TransportFailure(RangeTransportFailure),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppOnlinePc4RangeError {
    LookupNotAwaitingRange,
    Admission(RangeAdmissionError),
    Supply(SupplyError),
}

impl AppOnlinePc4RangeError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::LookupNotAwaitingRange => "pc4_online_lookup_not_awaiting_range",
            Self::Admission(error) => error.reason(),
            Self::Supply(error) => error.reason(),
        }
    }
}

impl From<RangeAdmissionError> for AppOnlinePc4RangeError {
    fn from(value: RangeAdmissionError) -> Self {
        Self::Admission(value)
    }
}

impl From<SupplyError> for AppOnlinePc4RangeError {
    fn from(value: SupplyError) -> Self {
        Self::Supply(value)
    }
}

pub struct AppOnlinePc4LookupSession {
    generation: PinnedPc4Generation,
    request: Pc4OnlineLookupRequest,
    machine: LookupMachine,
    range_admission: RangeAdmissionSession,
}

impl AppOnlinePc4LookupSession {
    pub fn start(
        generation: PinnedPc4Generation,
        request: Pc4OnlineLookupRequest,
    ) -> Result<Self, AppOnlinePc4LookupStartError> {
        let snapshot = generation.activated_snapshot();
        if request.target().snapshot() != snapshot.qualified_identity() {
            return Err(AppOnlinePc4LookupStartError::TargetSnapshotMismatch);
        }
        let machine = match request.field() {
            Pc4OnlineLookupField::Hash(field_hash) => LookupMachine::start(
                snapshot,
                request.profile(),
                field_hash,
                request.lookup_session(),
            ),
            Pc4OnlineLookupField::Id(field_id) => LookupMachine::start_by_field_id(
                snapshot,
                request.profile(),
                field_id,
                request.lookup_session(),
            ),
        }
        .map_err(AppOnlinePc4LookupStartError::Lookup)?;
        let range_admission = RangeAdmissionSession::new(
            request.lookup_session(),
            request.target().snapshot().clone(),
            request.range_limits(),
        );
        Ok(Self {
            generation,
            request,
            machine,
            range_admission,
        })
    }

    pub const fn generation(&self) -> &PinnedPc4Generation {
        &self.generation
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

    pub const fn range_usage(&self) -> clearra_pc4_tablebase::RangeAdmissionUsage {
        self.range_admission.usage()
    }

    /// Validates and consumes one host-observed Range result transactionally.
    ///
    /// There is intentionally no public raw `RangeResponse` supply path. The
    /// current pending request is selected inside this owner, so a caller also
    /// cannot substitute an older request from another lookup session.
    pub fn admit_range<G>(
        &mut self,
        attempt: RangeAdmissionAttempt,
        input: RangeAdmissionInput,
        guard: &G,
    ) -> Result<AppOnlinePc4RangeDisposition, AppOnlinePc4RangeError>
    where
        G: RangeAdmissionGuard + ?Sized,
    {
        let LookupStep::NeedRange(request) = self.machine.step() else {
            return Err(AppOnlinePc4RangeError::LookupNotAwaitingRange);
        };
        match self
            .range_admission
            .admit(&request, attempt, input, guard)?
        {
            RangeAdmissionOutcome::PartialContent(response) => {
                self.machine.supply(response)?;
                Ok(AppOnlinePc4RangeDisposition::PartialContentSupplied)
            }
            RangeAdmissionOutcome::RangeNotSatisfiable { complete_length } => {
                self.machine
                    .reject_range(&request, RangeTransportFailure::Unavailable)?;
                Ok(AppOnlinePc4RangeDisposition::RangeNotSatisfiable { complete_length })
            }
            RangeAdmissionOutcome::TransportFailure(failure) => {
                self.machine.reject_range(&request, failure)?;
                Ok(AppOnlinePc4RangeDisposition::TransportFailure(failure))
            }
        }
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
                    field: self.request.field(),
                    cause,
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        num::{NonZeroU16, NonZeroU32, NonZeroU64},
        sync::Arc,
    };

    use super::*;
    use clearra_pc4_tablebase::{
        ActivatedSnapshot, ArtifactDescriptor, DatasetSnapshotManifest, DatasetSnapshotVerifier,
        GraphTargetEncoding, ManifestContentIdentity, Pc4ArtifactRole, Pc4CurrentGeneration,
        Pc4GenerationRegistry, Pc4GenerationRetentionLimit, Pc4GenerationStageOutcome,
        Pc4ProfileManifest, ProfileAvailability, ProfileQualification,
        ProfileTargetCompletenessQualification, RangeAdmissionBinding, RangeHttpResponse,
        RangeResponse, RangeResponseKind, SnapshotIdentity, SnapshotVerificationAttestation,
        SnapshotVerificationFailure, SnapshotVerificationRequest,
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

    fn range_limits() -> RangeAdmissionLimits {
        RangeAdmissionLimits::new(
            NonZeroU64::new(64).expect("response bytes"),
            NonZeroU64::new(512).expect("session bytes"),
            NonZeroU32::new(32).expect("request count"),
            NonZeroU16::new(1).expect("active requests"),
            60,
        )
    }

    fn attempt(ordinal: u32) -> RangeAdmissionAttempt {
        RangeAdmissionAttempt::new(
            NonZeroU32::new(ordinal).expect("request ordinal"),
            NonZeroU16::new(1).expect("one active request"),
        )
    }

    struct LiveRangeGuard;

    impl RangeAdmissionGuard for LiveRangeGuard {
        fn is_cancelled(&self) -> bool {
            false
        }

        fn is_current_snapshot(
            &self,
            _expected: &clearra_pc4_tablebase::QualifiedSnapshotIdentity,
        ) -> bool {
            true
        }
    }

    fn response_for(request: &RangeRequest) -> RangeResponse {
        RangeResponse {
            lookup_session: request.lookup_session(),
            request_id: request.request_id(),
            snapshot: request.snapshot().clone(),
            profile: request.profile(),
            artifact: request.artifact(),
            artifact_content_identity: request.artifact_descriptor().content_identity().to_owned(),
            kind: RangeResponseKind::PartialContent,
            offset: request.offset(),
            complete_length: request.artifact_descriptor().byte_len(),
            bytes: vec![0; request.length() as usize],
        }
    }

    fn partial_http(request: &RangeRequest, response: RangeResponse) -> RangeAdmissionInput {
        let end = request.end_exclusive() - 1;
        RangeAdmissionInput::http(RangeHttpResponse::new(
            206,
            Some(format!(
                "bytes {}-{end}/{}",
                request.offset(),
                request.artifact_descriptor().byte_len()
            )),
            None,
            Some(response),
        ))
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
                    clearra_pc4_tablebase::FieldIdIndexRelation::RecordOrdinal,
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

    fn pinned_generation(generation: &str) -> PinnedPc4Generation {
        let mut registry = Pc4GenerationRegistry::new(
            Pc4GenerationRetentionLimit::new(1).expect("one retained generation"),
        );
        let token = match registry
            .stage(registry.version(), Arc::new(snapshot(generation)))
            .expect("stage synthetic generation")
        {
            Pc4GenerationStageOutcome::Staged { token, .. } => token,
            unexpected => panic!("expected staged generation, got {unexpected:?}"),
        };
        registry.promote(&token).expect("promote generation");
        match registry.pin_current() {
            Pc4CurrentGeneration::Current(current) => current,
            Pc4CurrentGeneration::NoCurrent => panic!("promoted generation must be current"),
        }
    }

    fn new_session(
        id: u64,
        authorization: Pc4OfflineFallbackAuthorization,
    ) -> AppOnlinePc4LookupSession {
        let generation = pinned_generation("generation-a");
        let target = generation
            .activated_snapshot()
            .qualified_target(
                Pc4RuleProfile::Srs,
                Pc4TerminalUseCase::PcSearch,
                Pc4TargetLines::new(4).expect("4L target"),
            )
            .expect("qualified PC target");
        AppOnlinePc4LookupSession::start(
            generation,
            Pc4OnlineLookupRequest::new(
                lookup_session(id),
                target,
                15,
                range_limits(),
                authorization,
            ),
        )
        .expect("lookup session")
    }

    fn new_session_by_field_id(
        id: u64,
        field_id: u32,
        authorization: Pc4OfflineFallbackAuthorization,
    ) -> AppOnlinePc4LookupSession {
        let generation = pinned_generation("generation-a");
        let target = generation
            .activated_snapshot()
            .qualified_target(
                Pc4RuleProfile::Srs,
                Pc4TerminalUseCase::PcSearch,
                Pc4TargetLines::new(4).expect("4L target"),
            )
            .expect("qualified PC target");
        AppOnlinePc4LookupSession::start(
            generation,
            Pc4OnlineLookupRequest::from_field_id(
                lookup_session(id),
                target,
                field_id,
                range_limits(),
                authorization,
            ),
        )
        .expect("direct field-ID lookup session")
    }

    #[test]
    fn target_from_another_generation_cannot_start_a_lookup() {
        let old_generation = pinned_generation("generation-a");
        let old_target = old_generation
            .activated_snapshot()
            .qualified_target(
                Pc4RuleProfile::Srs,
                Pc4TerminalUseCase::PcSearch,
                Pc4TargetLines::new(4).expect("4L target"),
            )
            .expect("old qualified target");
        let current_generation = pinned_generation("generation-b");
        let error = match AppOnlinePc4LookupSession::start(
            current_generation,
            Pc4OnlineLookupRequest::new(
                lookup_session(99),
                old_target,
                15,
                range_limits(),
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
            session.generation().qualified_identity(),
            session.request().target().snapshot()
        );
        assert_eq!(
            session.offline_fallback_disposition(),
            Pc4OfflineFallbackDisposition::NotAvailable
        );
    }

    #[test]
    fn fallback_requires_explicit_authorization_and_never_starts_inside_session_owner() {
        let mut session = new_session(1, Pc4OfflineFallbackAuthorization::NotAuthorized);
        let AppOnlinePc4LookupStep::NeedRange(_) = session.step() else {
            panic!("Range request")
        };
        assert_eq!(
            session
                .admit_range(
                    attempt(1),
                    RangeAdmissionInput::TransportFailure(RangeTransportFailure::Offline),
                    &LiveRangeGuard,
                )
                .expect("transport failure"),
            AppOnlinePc4RangeDisposition::TransportFailure(RangeTransportFailure::Offline)
        );
        assert_eq!(
            session.offline_fallback_disposition(),
            Pc4OfflineFallbackDisposition::RequiresExplicitAuthorization {
                cause: Pc4FallbackCause::LookupFailure(LookupFailure::Offline),
            }
        );

        let mut authorized = new_session(2, Pc4OfflineFallbackAuthorization::ExplicitlyAuthorized);
        let AppOnlinePc4LookupStep::NeedRange(_) = authorized.step() else {
            panic!("Range request")
        };
        let rate_limited = RangeTransportFailure::RateLimited {
            retry_after_seconds: Some(30),
        };
        assert_eq!(
            authorized
                .admit_range(
                    attempt(1),
                    RangeAdmissionInput::TransportFailure(rate_limited),
                    &LiveRangeGuard,
                )
                .expect("rate-limit failure"),
            AppOnlinePc4RangeDisposition::TransportFailure(rate_limited)
        );
        let Pc4OfflineFallbackDisposition::Authorized(signal) =
            authorized.offline_fallback_disposition()
        else {
            panic!("explicitly authorized signal")
        };
        assert_eq!(signal.lookup_session(), lookup_session(2));
        assert_eq!(signal.profile(), Pc4RuleProfile::Srs);
        assert_eq!(signal.use_case(), Pc4TerminalUseCase::PcSearch);
        assert_eq!(signal.target_lines().get(), 4);
        assert_eq!(signal.field_hash(), Some(15));
        assert_eq!(signal.field_id(), None);
        assert_eq!(signal.cause().reason(), "pc4_online_rate_limited");
    }

    #[test]
    fn direct_field_id_request_preserves_exact_identity_in_fallback_signal() {
        let mut session =
            new_session_by_field_id(3, 0, Pc4OfflineFallbackAuthorization::ExplicitlyAuthorized);
        assert_eq!(session.request().field_hash(), None);
        assert_eq!(session.request().field_id(), Some(0));
        let AppOnlinePc4LookupStep::NeedRange(_) = session.step() else {
            panic!("field-index header request")
        };
        session
            .admit_range(
                attempt(1),
                RangeAdmissionInput::TransportFailure(RangeTransportFailure::Offline),
                &LiveRangeGuard,
            )
            .expect("transport failure");
        let Pc4OfflineFallbackDisposition::Authorized(signal) =
            session.offline_fallback_disposition()
        else {
            panic!("authorized exact-identity signal")
        };
        assert_eq!(signal.field(), Pc4OnlineLookupField::Id(0));
        assert_eq!(signal.field_hash(), None);
        assert_eq!(signal.field_id(), Some(0));
    }

    #[test]
    fn a_delayed_failure_from_another_session_cannot_end_the_current_app_session() {
        let old = new_session(1, Pc4OfflineFallbackAuthorization::NotAuthorized);
        let AppOnlinePc4LookupStep::NeedRange(old_request) = old.step() else {
            panic!("old Range request")
        };
        let mut current = new_session(2, Pc4OfflineFallbackAuthorization::NotAuthorized);
        let AppOnlinePc4LookupStep::NeedRange(current_request) = current.step() else {
            panic!("current Range request")
        };
        let stale_response = response_for(&old_request);
        assert_eq!(
            current.admit_range(
                attempt(1),
                partial_http(&current_request, stale_response),
                &LiveRangeGuard,
            ),
            Err(AppOnlinePc4RangeError::Admission(
                RangeAdmissionError::ResponseBindingDrift {
                    binding: RangeAdmissionBinding::LookupSession,
                }
            ))
        );
        assert_eq!(current.range_usage(), Default::default());
        assert!(matches!(
            current.step(),
            AppOnlinePc4LookupStep::NeedRange(_)
        ));
    }

    #[test]
    fn whole_content_has_no_raw_supply_bypass_and_does_not_consume_budget() {
        let mut session = new_session(1, Pc4OfflineFallbackAuthorization::NotAuthorized);
        let AppOnlinePc4LookupStep::NeedRange(request) = session.step() else {
            panic!("Range request")
        };
        let input = RangeAdmissionInput::http(RangeHttpResponse::new(
            200,
            None,
            None,
            Some(response_for(&request)),
        ));
        assert_eq!(
            session.admit_range(attempt(1), input, &LiveRangeGuard),
            Err(AppOnlinePc4RangeError::Admission(
                RangeAdmissionError::WholeContentRejected
            ))
        );
        assert_eq!(session.range_usage(), Default::default());
        assert!(matches!(
            session.step(),
            AppOnlinePc4LookupStep::NeedRange(_)
        ));
    }

    #[test]
    fn unsatisfied_range_is_typed_and_ends_the_lookup_without_starting_fallback() {
        let mut session = new_session(1, Pc4OfflineFallbackAuthorization::NotAuthorized);
        let AppOnlinePc4LookupStep::NeedRange(request) = session.step() else {
            panic!("Range request")
        };
        let complete_length = request.artifact_descriptor().byte_len();
        let input = RangeAdmissionInput::http(RangeHttpResponse::new(
            416,
            Some(format!("bytes */{complete_length}")),
            None,
            None,
        ));
        assert_eq!(
            session
                .admit_range(attempt(1), input, &LiveRangeGuard)
                .expect("typed unsatisfied range"),
            AppOnlinePc4RangeDisposition::RangeNotSatisfiable { complete_length }
        );
        assert_eq!(session.range_usage().request_count(), 1);
        assert_eq!(session.range_usage().admitted_bytes(), 0);
        assert!(matches!(
            session.step(),
            AppOnlinePc4LookupStep::Failed(LookupFailure::DatasetUnavailable)
        ));
        assert_eq!(
            session.offline_fallback_disposition(),
            Pc4OfflineFallbackDisposition::RequiresExplicitAuthorization {
                cause: Pc4FallbackCause::LookupFailure(LookupFailure::DatasetUnavailable),
            }
        );
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
