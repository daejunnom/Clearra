//! Feature-off input-disclosure boundary for online PC4 acceleration.
//!
//! This module decides only whether a normalized queue input contains the
//! exact bag remainder needed by the hidden-reveal state machine. It performs
//! no tablebase read, starts no offline search, and exposes no product UI. A
//! fixed concrete queue bypasses bag disclosure completely. Pattern and
//! hidden-queue requests can reach the separate lookup owner only through a
//! [`Pc4PreparedOnlineInput`].

use clearra_pc4_tablebase::{
    Pc4BagProfile, Pc4BagState, Pc4BagStateError, Pc4GraphPiece, Pc4RuleProfile, Pc4TargetLines,
    Pc4TerminalUseCase, QualifiedPc4TargetIdentity, PC4_BAG_PIECES,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4InputSurface {
    NonInteractiveCli,
    InteractiveCli,
    Gui,
    Discord,
}

impl Pc4InputSurface {
    pub const fn may_request_bag_remainder(self) -> bool {
        !matches!(self, Self::NonInteractiveCli)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4HiddenQueueSource {
    Pattern,
    HiddenQueue,
}

/// Exact observation/reveal boundary already normalized by the CLI/domain
/// parser. The initial visible queue is current piece plus preview, so its
/// length must be `preview_length + 1`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pc4HiddenRevealScope {
    visible_piece_count: usize,
    preview_length: usize,
    hidden_draws: usize,
    placement_count: usize,
}

impl Pc4HiddenRevealScope {
    pub fn new(
        visible_piece_count: usize,
        preview_length: usize,
        hidden_draws: usize,
        placement_count: usize,
    ) -> Result<Self, Pc4HiddenRevealScopeError> {
        let expected_visible_piece_count = preview_length
            .checked_add(1)
            .ok_or(Pc4HiddenRevealScopeError::VisibleLengthOverflow)?;
        if visible_piece_count != expected_visible_piece_count {
            return Err(Pc4HiddenRevealScopeError::InvalidVisiblePieceCount {
                expected: expected_visible_piece_count,
                actual: visible_piece_count,
            });
        }
        Ok(Self {
            visible_piece_count,
            preview_length,
            hidden_draws,
            placement_count,
        })
    }

    pub const fn visible_piece_count(self) -> usize {
        self.visible_piece_count
    }

    pub const fn preview_length(self) -> usize {
        self.preview_length
    }

    pub const fn hidden_draws(self) -> usize {
        self.hidden_draws
    }

    pub const fn placement_count(self) -> usize {
        self.placement_count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4HiddenRevealScopeError {
    VisibleLengthOverflow,
    InvalidVisiblePieceCount { expected: usize, actual: usize },
}

impl Pc4HiddenRevealScopeError {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::VisibleLengthOverflow => "pc4_input_visible_length_overflow",
            Self::InvalidVisiblePieceCount { .. } => "pc4_input_visible_piece_count_mismatch",
        }
    }
}

/// Piece counts use [`PC4_BAG_PIECES`] order. `None` means that exact field is
/// not yet known; zero is a known and valid remaining count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Pc4PartialBagRemainder {
    counts: [Option<u32>; 7],
}

impl Pc4PartialBagRemainder {
    pub const fn unknown() -> Self {
        Self { counts: [None; 7] }
    }

    pub const fn complete(counts: [u32; 7]) -> Self {
        Self {
            counts: [
                Some(counts[0]),
                Some(counts[1]),
                Some(counts[2]),
                Some(counts[3]),
                Some(counts[4]),
                Some(counts[5]),
                Some(counts[6]),
            ],
        }
    }

    pub const fn from_optional_counts(counts: [Option<u32>; 7]) -> Self {
        Self { counts }
    }

    pub fn with_remaining_count(mut self, piece: Pc4GraphPiece, count: u32) -> Self {
        self.counts[piece_index(piece)] = Some(count);
        self
    }

    pub fn remaining_count(self, piece: Pc4GraphPiece) -> Option<u32> {
        self.counts[piece_index(piece)]
    }

    pub const fn optional_counts(self) -> [Option<u32>; 7] {
        self.counts
    }

    fn complete_counts(self) -> Option<[u32; 7]> {
        let mut complete = [0_u32; 7];
        for (index, count) in self.counts.into_iter().enumerate() {
            let count = count?;
            complete[index] = count;
        }
        Some(complete)
    }

    fn missing_fields(self) -> Vec<Pc4BagDisclosureField> {
        PC4_BAG_PIECES
            .into_iter()
            .zip(self.counts)
            .filter_map(|(piece, count)| {
                count
                    .is_none()
                    .then_some(Pc4BagDisclosureField::RemainingCount(piece))
            })
            .collect()
    }
}

impl Default for Pc4PartialBagRemainder {
    fn default() -> Self {
        Self::unknown()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4BagDisclosure {
    Remaining(Pc4PartialBagRemainder),
    Refused,
    Cancelled,
}

impl Default for Pc4BagDisclosure {
    fn default() -> Self {
        Self::Remaining(Pc4PartialBagRemainder::unknown())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4BagDisclosureField {
    RemainingCount(Pc4GraphPiece),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4HiddenQueueDisclosure {
    source: Pc4HiddenQueueSource,
    visible_queue: Vec<Pc4GraphPiece>,
    scope: Pc4HiddenRevealScope,
    bag_profile: Pc4BagProfile,
    bag_epoch: u64,
    disclosure: Pc4BagDisclosure,
}

impl Pc4HiddenQueueDisclosure {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source: Pc4HiddenQueueSource,
        visible_queue: Vec<Pc4GraphPiece>,
        preview_length: usize,
        hidden_draws: usize,
        placement_count: usize,
        bag_profile: Pc4BagProfile,
        bag_epoch: u64,
        disclosure: Pc4BagDisclosure,
    ) -> Result<Self, Pc4HiddenRevealScopeError> {
        let scope = Pc4HiddenRevealScope::new(
            visible_queue.len(),
            preview_length,
            hidden_draws,
            placement_count,
        )?;
        Ok(Self {
            source,
            visible_queue,
            scope,
            bag_profile,
            bag_epoch,
            disclosure,
        })
    }

    pub const fn source(&self) -> Pc4HiddenQueueSource {
        self.source
    }

    pub fn visible_queue(&self) -> &[Pc4GraphPiece] {
        &self.visible_queue
    }

    pub const fn scope(&self) -> Pc4HiddenRevealScope {
        self.scope
    }

    pub const fn disclosure(&self) -> Pc4BagDisclosure {
        self.disclosure
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4QueueDisclosure {
    FixedExplicit(Vec<Pc4GraphPiece>),
    PatternOrHidden(Pc4HiddenQueueDisclosure),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4InputDisclosureRequest {
    target: QualifiedPc4TargetIdentity,
    surface: Pc4InputSurface,
    queue: Pc4QueueDisclosure,
}

impl Pc4InputDisclosureRequest {
    pub const fn new(
        target: QualifiedPc4TargetIdentity,
        surface: Pc4InputSurface,
        queue: Pc4QueueDisclosure,
    ) -> Self {
        Self {
            target,
            surface,
            queue,
        }
    }

    pub const fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.target
    }

    pub const fn surface(&self) -> Pc4InputSurface {
        self.surface
    }

    pub const fn queue(&self) -> &Pc4QueueDisclosure {
        &self.queue
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4PreparedQueueInput {
    FixedExplicit(Vec<Pc4GraphPiece>),
    PatternOrHidden {
        source: Pc4HiddenQueueSource,
        visible_queue: Vec<Pc4GraphPiece>,
        scope: Pc4HiddenRevealScope,
        /// `None` is valid only when `scope.hidden_draws() == 0`.
        bag_state: Option<Pc4BagState>,
    },
}

/// Input-only readiness token. It preserves the qualified profile, terminal
/// use case, and target. It does not qualify a dataset, prove a hit, or grant
/// permission to start an offline fallback.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4PreparedOnlineInput {
    target: QualifiedPc4TargetIdentity,
    surface: Pc4InputSurface,
    queue: Pc4PreparedQueueInput,
}

impl Pc4PreparedOnlineInput {
    pub const fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.target
    }

    pub const fn profile(&self) -> Pc4RuleProfile {
        self.target.profile()
    }

    pub const fn use_case(&self) -> Pc4TerminalUseCase {
        self.target.use_case()
    }

    pub const fn target_lines(&self) -> Pc4TargetLines {
        self.target.target_lines()
    }

    pub const fn surface(&self) -> Pc4InputSurface {
        self.surface
    }

    pub const fn queue(&self) -> &Pc4PreparedQueueInput {
        &self.queue
    }
}

/// Exact, display-safe remainder fields that one interactive surface may ask.
/// Internal graph IDs, bag epochs, and arbitrary reveal order are deliberately
/// absent from this API.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4BagDisclosureRequirement {
    target: QualifiedPc4TargetIdentity,
    surface: Pc4InputSurface,
    hidden: Pc4HiddenQueueDisclosure,
}

impl Pc4BagDisclosureRequirement {
    pub const fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.target
    }

    pub const fn profile(&self) -> Pc4RuleProfile {
        self.target.profile()
    }

    pub const fn use_case(&self) -> Pc4TerminalUseCase {
        self.target.use_case()
    }

    pub const fn target_lines(&self) -> Pc4TargetLines {
        self.target.target_lines()
    }

    pub const fn surface(&self) -> Pc4InputSurface {
        self.surface
    }

    pub const fn source(&self) -> Pc4HiddenQueueSource {
        self.hidden.source
    }

    pub fn visible_queue(&self) -> &[Pc4GraphPiece] {
        &self.hidden.visible_queue
    }

    pub const fn reveal_scope(&self) -> Pc4HiddenRevealScope {
        self.hidden.scope
    }

    pub fn missing_fields(&self) -> Vec<Pc4BagDisclosureField> {
        match self.hidden.disclosure {
            Pc4BagDisclosure::Remaining(remainder) => remainder.missing_fields(),
            Pc4BagDisclosure::Refused | Pc4BagDisclosure::Cancelled => Vec::new(),
        }
    }

    pub fn known_remaining_count(&self, piece: Pc4GraphPiece) -> Option<u32> {
        match self.hidden.disclosure {
            Pc4BagDisclosure::Remaining(remainder) => remainder.remaining_count(piece),
            Pc4BagDisclosure::Refused | Pc4BagDisclosure::Cancelled => None,
        }
    }

    pub fn provide_remainder(
        mut self,
        remainder: Pc4PartialBagRemainder,
    ) -> Pc4InputDisclosureRequest {
        self.hidden.disclosure = Pc4BagDisclosure::Remaining(remainder);
        Pc4InputDisclosureRequest::new(
            self.target,
            self.surface,
            Pc4QueueDisclosure::PatternOrHidden(self.hidden),
        )
    }

    pub fn refuse(mut self) -> Pc4InputDisclosureRequest {
        self.hidden.disclosure = Pc4BagDisclosure::Refused;
        Pc4InputDisclosureRequest::new(
            self.target,
            self.surface,
            Pc4QueueDisclosure::PatternOrHidden(self.hidden),
        )
    }

    pub fn cancel(mut self) -> Pc4InputDisclosureRequest {
        self.hidden.disclosure = Pc4BagDisclosure::Cancelled;
        Pc4InputDisclosureRequest::new(
            self.target,
            self.surface,
            Pc4QueueDisclosure::PatternOrHidden(self.hidden),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pc4InputDisclosureStopReason {
    UserRefused,
    Cancelled,
}

impl Pc4InputDisclosureStopReason {
    pub const fn reason(self) -> &'static str {
        match self {
            Self::UserRefused => "pc4_input_bag_disclosure_refused",
            Self::Cancelled => "pc4_input_bag_disclosure_cancelled",
        }
    }
}

/// Local stop signal only. Another owner may offer an offline action, but this
/// value neither authorizes nor starts it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pc4InputDisclosureStop {
    target: QualifiedPc4TargetIdentity,
    surface: Pc4InputSurface,
    source: Pc4HiddenQueueSource,
    scope: Pc4HiddenRevealScope,
    reason: Pc4InputDisclosureStopReason,
}

impl Pc4InputDisclosureStop {
    pub const fn target(&self) -> &QualifiedPc4TargetIdentity {
        &self.target
    }

    pub const fn surface(&self) -> Pc4InputSurface {
        self.surface
    }

    pub const fn source(&self) -> Pc4HiddenQueueSource {
        self.source
    }

    pub const fn reveal_scope(&self) -> Pc4HiddenRevealScope {
        self.scope
    }

    pub const fn stop_reason(&self) -> Pc4InputDisclosureStopReason {
        self.reason
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4InputDisclosureRejection {
    NonInteractiveBagDisclosureRequired(Box<Pc4BagDisclosureRequirement>),
    InvalidBagState {
        target: Box<QualifiedPc4TargetIdentity>,
        surface: Pc4InputSurface,
        source: Pc4HiddenQueueSource,
        scope: Pc4HiddenRevealScope,
        error: Pc4BagStateError,
    },
}

impl Pc4InputDisclosureRejection {
    pub fn target(&self) -> &QualifiedPc4TargetIdentity {
        match self {
            Self::NonInteractiveBagDisclosureRequired(requirement) => requirement.target(),
            Self::InvalidBagState { target, .. } => target.as_ref(),
        }
    }

    pub const fn surface(&self) -> Pc4InputSurface {
        match self {
            Self::NonInteractiveBagDisclosureRequired(requirement) => requirement.surface(),
            Self::InvalidBagState { surface, .. } => *surface,
        }
    }

    pub const fn reason(&self) -> &'static str {
        match self {
            Self::NonInteractiveBagDisclosureRequired(_) => {
                "pc4_input_noninteractive_bag_remainder_required"
            }
            Self::InvalidBagState { error, .. } => error.code(),
        }
    }

    pub fn into_requirement(self) -> Option<Pc4BagDisclosureRequirement> {
        match self {
            Self::NonInteractiveBagDisclosureRequired(requirement) => Some(*requirement),
            Self::InvalidBagState { .. } => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Pc4InputDisclosureDecision {
    Ready(Pc4PreparedOnlineInput),
    RequestBagRemainder(Pc4BagDisclosureRequirement),
    Stop(Pc4InputDisclosureStop),
}

/// Resolves only the input-disclosure gate. A non-ready outcome is guaranteed
/// to precede every tablebase read, and no outcome starts offline execution.
pub fn prepare_pc4_input_disclosure(
    request: Pc4InputDisclosureRequest,
) -> Result<Pc4InputDisclosureDecision, Pc4InputDisclosureRejection> {
    let Pc4InputDisclosureRequest {
        target,
        surface,
        queue,
    } = request;
    match queue {
        Pc4QueueDisclosure::FixedExplicit(queue) => {
            Ok(Pc4InputDisclosureDecision::Ready(Pc4PreparedOnlineInput {
                target,
                surface,
                queue: Pc4PreparedQueueInput::FixedExplicit(queue),
            }))
        }
        Pc4QueueDisclosure::PatternOrHidden(hidden) => {
            prepare_hidden_input(target, surface, hidden)
        }
    }
}

fn prepare_hidden_input(
    target: QualifiedPc4TargetIdentity,
    surface: Pc4InputSurface,
    hidden: Pc4HiddenQueueDisclosure,
) -> Result<Pc4InputDisclosureDecision, Pc4InputDisclosureRejection> {
    if hidden.scope.hidden_draws() == 0 {
        return Ok(ready_hidden(target, surface, hidden, None));
    }

    let remainder = match hidden.disclosure {
        Pc4BagDisclosure::Remaining(remainder) => remainder,
        Pc4BagDisclosure::Refused => {
            return Ok(stop_hidden(
                target,
                surface,
                &hidden,
                Pc4InputDisclosureStopReason::UserRefused,
            ));
        }
        Pc4BagDisclosure::Cancelled => {
            return Ok(stop_hidden(
                target,
                surface,
                &hidden,
                Pc4InputDisclosureStopReason::Cancelled,
            ));
        }
    };

    let Some(remainder) = remainder.complete_counts() else {
        let requirement = Pc4BagDisclosureRequirement {
            target,
            surface,
            hidden,
        };
        return if surface.may_request_bag_remainder() {
            Ok(Pc4InputDisclosureDecision::RequestBagRemainder(requirement))
        } else {
            Err(
                Pc4InputDisclosureRejection::NonInteractiveBagDisclosureRequired(Box::new(
                    requirement,
                )),
            )
        };
    };

    let bag_state =
        Pc4BagState::new(hidden.bag_profile, remainder, hidden.bag_epoch).map_err(|error| {
            Pc4InputDisclosureRejection::InvalidBagState {
                target: Box::new(target.clone()),
                surface,
                source: hidden.source,
                scope: hidden.scope,
                error,
            }
        })?;
    Ok(ready_hidden(target, surface, hidden, Some(bag_state)))
}

fn ready_hidden(
    target: QualifiedPc4TargetIdentity,
    surface: Pc4InputSurface,
    hidden: Pc4HiddenQueueDisclosure,
    bag_state: Option<Pc4BagState>,
) -> Pc4InputDisclosureDecision {
    Pc4InputDisclosureDecision::Ready(Pc4PreparedOnlineInput {
        target,
        surface,
        queue: Pc4PreparedQueueInput::PatternOrHidden {
            source: hidden.source,
            visible_queue: hidden.visible_queue,
            scope: hidden.scope,
            bag_state,
        },
    })
}

fn stop_hidden(
    target: QualifiedPc4TargetIdentity,
    surface: Pc4InputSurface,
    hidden: &Pc4HiddenQueueDisclosure,
    reason: Pc4InputDisclosureStopReason,
) -> Pc4InputDisclosureDecision {
    Pc4InputDisclosureDecision::Stop(Pc4InputDisclosureStop {
        target,
        surface,
        source: hidden.source,
        scope: hidden.scope,
        reason,
    })
}

fn piece_index(piece: Pc4GraphPiece) -> usize {
    PC4_BAG_PIECES
        .into_iter()
        .position(|candidate| candidate == piece)
        .expect("PC4 graph piece is a member of the canonical bag order")
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_pc4_tablebase::{
        ArtifactDescriptor, DatasetSnapshotManifest, DatasetSnapshotVerifier, FieldIdIndexRelation,
        GraphTargetEncoding, ManifestContentIdentity, Pc4ArtifactRole, Pc4ProfileManifest,
        ProfileAvailability, ProfileQualification, ProfileTargetCompletenessQualification,
        SnapshotIdentity, SnapshotVerificationAttestation, SnapshotVerificationFailure,
        SnapshotVerificationRequest,
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
                "synthetic-input-disclosure-verification",
            )
            .expect("synthetic attestation"))
        }
    }

    fn target(use_case: Pc4TerminalUseCase, lines: u8) -> QualifiedPc4TargetIdentity {
        let profiles = Pc4RuleProfile::ALL
            .into_iter()
            .map(|profile| {
                let descriptor = |role, suffix: &str, byte_len| {
                    ArtifactDescriptor::new(
                        role,
                        format!("{}/{suffix}", profile.as_str()),
                        byte_len,
                        format!("{}-{suffix}-identity", profile.as_str()),
                    )
                    .expect("artifact descriptor")
                };
                let manifest = Pc4ProfileManifest::new(
                    profile,
                    1,
                    GraphTargetEncoding::U24LittleEndian,
                    FieldIdIndexRelation::RecordOrdinal,
                    64,
                    descriptor(Pc4ArtifactRole::FieldHashIndex, "field.idx", 24),
                    descriptor(Pc4ArtifactRole::GraphOffsets, "offsets.idx", 24),
                    descriptor(Pc4ArtifactRole::Graph, "graph.bin", 64),
                    ProfileQualification::new(
                        format!("{}-index-spec", profile.as_str()),
                        format!("{}-graph-spec", profile.as_str()),
                        format!("{}-provenance", profile.as_str()),
                        format!("{}-kat", profile.as_str()),
                    )
                    .expect("profile qualification"),
                )
                .expect("profile manifest");
                let manifest = if profile == Pc4RuleProfile::Srs {
                    manifest
                        .with_target_qualifications(vec![
                            qualification(Pc4TerminalUseCase::PcSearch, 4),
                            qualification(Pc4TerminalUseCase::SetupSearch, 2),
                        ])
                        .expect("target qualifications")
                } else {
                    manifest
                };
                ProfileAvailability::qualified(manifest)
            })
            .collect();
        let snapshot = DatasetSnapshotManifest::new(
            SnapshotIdentity::new(
                "synthetic/repository",
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "generation-input-disclosure",
            )
            .expect("snapshot identity"),
            ManifestContentIdentity::new("synthetic-input-disclosure-manifest")
                .expect("content identity"),
            profiles,
        )
        .expect("manifest")
        .activate(&mut SyntheticVerifier)
        .expect("activated snapshot");
        snapshot
            .qualified_target(
                Pc4RuleProfile::Srs,
                use_case,
                Pc4TargetLines::new(lines).expect("qualified line target"),
            )
            .expect("qualified target")
    }

    fn qualification(
        use_case: Pc4TerminalUseCase,
        lines: u8,
    ) -> ProfileTargetCompletenessQualification {
        ProfileTargetCompletenessQualification::new(
            use_case,
            Pc4TargetLines::new(lines).expect("target lines"),
            format!("synthetic-{use_case:?}-{lines}-terminal"),
            format!("synthetic-{use_case:?}-{lines}-all-edges"),
            format!("synthetic-{use_case:?}-{lines}-kat"),
            format!("synthetic-{use_case:?}-{lines}-offline-parity"),
        )
        .expect("target qualification")
    }

    fn hidden(
        source: Pc4HiddenQueueSource,
        disclosure: Pc4BagDisclosure,
        hidden_draws: usize,
    ) -> Pc4QueueDisclosure {
        Pc4QueueDisclosure::PatternOrHidden(
            Pc4HiddenQueueDisclosure::new(
                source,
                vec![Pc4GraphPiece::T, Pc4GraphPiece::I],
                1,
                hidden_draws,
                5,
                Pc4BagProfile::standard_seven_bag(),
                3,
                disclosure,
            )
            .expect("normalized hidden input"),
        )
    }

    #[test]
    fn fixed_explicit_queue_is_ready_without_any_bag_question() {
        let decision = prepare_pc4_input_disclosure(Pc4InputDisclosureRequest::new(
            target(Pc4TerminalUseCase::PcSearch, 4),
            Pc4InputSurface::NonInteractiveCli,
            Pc4QueueDisclosure::FixedExplicit(vec![Pc4GraphPiece::T, Pc4GraphPiece::I]),
        ))
        .expect("fixed queue never needs bag disclosure");
        let Pc4InputDisclosureDecision::Ready(ready) = decision else {
            panic!("fixed queue must be ready")
        };
        assert_eq!(ready.profile(), Pc4RuleProfile::Srs);
        assert_eq!(ready.use_case(), Pc4TerminalUseCase::PcSearch);
        assert_eq!(ready.target_lines().get(), 4);
        assert_eq!(ready.surface(), Pc4InputSurface::NonInteractiveCli);
        assert_eq!(
            ready.queue(),
            &Pc4PreparedQueueInput::FixedExplicit(vec![Pc4GraphPiece::T, Pc4GraphPiece::I])
        );
    }

    #[test]
    fn interactive_surfaces_request_only_the_exact_missing_piece_counts() {
        for surface in [
            Pc4InputSurface::InteractiveCli,
            Pc4InputSurface::Gui,
            Pc4InputSurface::Discord,
        ] {
            let partial = Pc4PartialBagRemainder::complete([1, 1, 1, 1, 1, 1, 1]);
            let partial = Pc4PartialBagRemainder::from_optional_counts([
                partial.remaining_count(Pc4GraphPiece::I),
                partial.remaining_count(Pc4GraphPiece::O),
                None,
                partial.remaining_count(Pc4GraphPiece::S),
                partial.remaining_count(Pc4GraphPiece::Z),
                partial.remaining_count(Pc4GraphPiece::J),
                None,
            ]);
            let decision = prepare_pc4_input_disclosure(Pc4InputDisclosureRequest::new(
                target(Pc4TerminalUseCase::PcSearch, 4),
                surface,
                hidden(
                    Pc4HiddenQueueSource::Pattern,
                    Pc4BagDisclosure::Remaining(partial),
                    3,
                ),
            ))
            .expect("interactive requirement");
            let Pc4InputDisclosureDecision::RequestBagRemainder(requirement) = decision else {
                panic!("interactive surface must receive a requirement")
            };
            assert_eq!(requirement.surface(), surface);
            assert_eq!(requirement.profile(), Pc4RuleProfile::Srs);
            assert_eq!(requirement.use_case(), Pc4TerminalUseCase::PcSearch);
            assert_eq!(requirement.target_lines().get(), 4);
            assert_eq!(requirement.reveal_scope().hidden_draws(), 3);
            assert_eq!(
                requirement.missing_fields(),
                vec![
                    Pc4BagDisclosureField::RemainingCount(Pc4GraphPiece::T),
                    Pc4BagDisclosureField::RemainingCount(Pc4GraphPiece::L),
                ]
            );
            assert_eq!(requirement.known_remaining_count(Pc4GraphPiece::I), Some(1));
        }
    }

    #[test]
    fn noninteractive_cli_missing_remainder_is_a_typed_failure() {
        let error = prepare_pc4_input_disclosure(Pc4InputDisclosureRequest::new(
            target(Pc4TerminalUseCase::PcSearch, 4),
            Pc4InputSurface::NonInteractiveCli,
            hidden(
                Pc4HiddenQueueSource::HiddenQueue,
                Pc4BagDisclosure::Remaining(Pc4PartialBagRemainder::unknown()),
                2,
            ),
        ))
        .expect_err("noninteractive CLI must fail before lookup");
        assert_eq!(
            error.reason(),
            "pc4_input_noninteractive_bag_remainder_required"
        );
        assert_eq!(error.surface(), Pc4InputSurface::NonInteractiveCli);
        let requirement = error.into_requirement().expect("exact missing fields");
        assert_eq!(requirement.missing_fields().len(), PC4_BAG_PIECES.len());
        assert_eq!(requirement.reveal_scope().hidden_draws(), 2);
    }

    #[test]
    fn completed_remainder_resumes_with_the_same_qualified_setup_target() {
        let setup_target = target(Pc4TerminalUseCase::SetupSearch, 2);
        let decision = prepare_pc4_input_disclosure(Pc4InputDisclosureRequest::new(
            setup_target.clone(),
            Pc4InputSurface::Gui,
            hidden(
                Pc4HiddenQueueSource::Pattern,
                Pc4BagDisclosure::Remaining(Pc4PartialBagRemainder::unknown()),
                2,
            ),
        ))
        .expect("initial requirement");
        let Pc4InputDisclosureDecision::RequestBagRemainder(requirement) = decision else {
            panic!("missing remainder")
        };
        assert_eq!(requirement.target(), &setup_target);
        let resumed =
            requirement.provide_remainder(Pc4PartialBagRemainder::complete([1, 0, 1, 1, 0, 1, 1]));
        let Pc4InputDisclosureDecision::Ready(ready) =
            prepare_pc4_input_disclosure(resumed).expect("completed remainder")
        else {
            panic!("completed remainder must be ready")
        };
        assert_eq!(ready.target(), &setup_target);
        assert_eq!(ready.use_case(), Pc4TerminalUseCase::SetupSearch);
        assert_eq!(ready.target_lines().get(), 2);
        let Pc4PreparedQueueInput::PatternOrHidden {
            scope, bag_state, ..
        } = ready.queue()
        else {
            panic!("prepared hidden input")
        };
        assert_eq!(scope.hidden_draws(), 2);
        assert_eq!(bag_state.expect("exact bag state").remainder_total(), 5);
    }

    #[test]
    fn refusal_and_cancel_are_local_stops_not_lookup_or_fallback_results() {
        for (disclosure, expected) in [
            (
                Pc4BagDisclosure::Refused,
                Pc4InputDisclosureStopReason::UserRefused,
            ),
            (
                Pc4BagDisclosure::Cancelled,
                Pc4InputDisclosureStopReason::Cancelled,
            ),
        ] {
            let decision = prepare_pc4_input_disclosure(Pc4InputDisclosureRequest::new(
                target(Pc4TerminalUseCase::PcSearch, 4),
                Pc4InputSurface::Discord,
                hidden(Pc4HiddenQueueSource::Pattern, disclosure, 2),
            ))
            .expect("local stop");
            let Pc4InputDisclosureDecision::Stop(stop) = decision else {
                panic!("disclosure refusal/cancel must stop locally")
            };
            assert_eq!(stop.stop_reason(), expected);
            assert_eq!(stop.surface(), Pc4InputSurface::Discord);
            assert_eq!(stop.target().use_case(), Pc4TerminalUseCase::PcSearch);
        }
    }

    #[test]
    fn invalid_remainder_fails_before_a_ready_token_exists() {
        let error = prepare_pc4_input_disclosure(Pc4InputDisclosureRequest::new(
            target(Pc4TerminalUseCase::PcSearch, 4),
            Pc4InputSurface::Gui,
            hidden(
                Pc4HiddenQueueSource::Pattern,
                Pc4BagDisclosure::Remaining(Pc4PartialBagRemainder::complete([
                    2, 0, 0, 0, 0, 0, 0,
                ])),
                1,
            ),
        ))
        .expect_err("remainder exceeds the normalized bag profile");
        assert_eq!(error.reason(), "pc4_bag_remainder_exceeds_profile");
        assert!(matches!(
            error,
            Pc4InputDisclosureRejection::InvalidBagState { .. }
        ));
    }

    #[test]
    fn zero_hidden_draws_need_no_bag_remainder() {
        let decision = prepare_pc4_input_disclosure(Pc4InputDisclosureRequest::new(
            target(Pc4TerminalUseCase::PcSearch, 4),
            Pc4InputSurface::NonInteractiveCli,
            hidden(
                Pc4HiddenQueueSource::HiddenQueue,
                Pc4BagDisclosure::Remaining(Pc4PartialBagRemainder::unknown()),
                0,
            ),
        ))
        .expect("no hidden draw means no bag question");
        let Pc4InputDisclosureDecision::Ready(ready) = decision else {
            panic!("ready without hidden reveal")
        };
        let Pc4PreparedQueueInput::PatternOrHidden {
            scope, bag_state, ..
        } = ready.queue()
        else {
            panic!("prepared hidden input")
        };
        assert_eq!(scope.hidden_draws(), 0);
        assert_eq!(*bag_state, None);
    }

    #[test]
    fn reveal_scope_rejects_an_incomplete_current_plus_preview_prefix() {
        assert_eq!(
            Pc4HiddenRevealScope::new(2, 2, 1, 4),
            Err(Pc4HiddenRevealScopeError::InvalidVisiblePieceCount {
                expected: 3,
                actual: 2,
            })
        );
    }
}
