use std::{cell::Cell, num::NonZeroU64};

use clearra_core_domain::{
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::{PiecePlacementMask, StandardBoard64TilingIdentity},
};
use clearra_pc4_tablebase::{
    ArtifactDescriptor, DatasetSnapshotManifest, DatasetSnapshotVerifier, GraphTargetEncoding,
    ManifestContentIdentity, Pc4ArtifactRole, Pc4ProfileManifest, ProfileAvailability,
    ProfileQualification, SnapshotIdentity, SnapshotVerificationAttestation,
    SnapshotVerificationFailure, SnapshotVerificationRequest,
};

use super::*;

struct SyntheticVerifier;

impl DatasetSnapshotVerifier for SyntheticVerifier {
    fn verify(
        &mut self,
        request: SnapshotVerificationRequest<'_>,
    ) -> Result<SnapshotVerificationAttestation, SnapshotVerificationFailure> {
        Ok(SnapshotVerificationAttestation::new(
            request.snapshot_identity().clone(),
            request.manifest_content_identity().clone(),
            "synthetic-candidate-boundary-verification",
        )
        .expect("synthetic verification attestation"))
    }
}

fn qualified_snapshot(generation: &str) -> QualifiedSnapshotIdentity {
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
            ProfileAvailability::qualified(
                Pc4ProfileManifest::new(
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
                .expect("profile manifest"),
            )
        })
        .collect();
    DatasetSnapshotManifest::new(
        identity,
        ManifestContentIdentity::new(format!("manifest-{generation}"))
            .expect("manifest content identity"),
        profiles,
    )
    .expect("manifest")
    .activate(&mut SyntheticVerifier)
    .expect("qualified snapshot")
    .qualified_identity()
    .clone()
}

fn session(value: u64) -> PcCandidateSessionId {
    PcCandidateSessionId::new(NonZeroU64::new(value).expect("non-zero session"))
}

fn offline_source(session_id: u64) -> PcCandidateSourceBinding {
    PcCandidateSourceBinding::offline_exact(
        session(session_id),
        PcCandidateRequestIdentity::from_sha256([1; 32]),
        PcCandidateSourceIdentity::from_sha256([2; 32]),
        Pc4RuleProfile::Srs,
        0,
    )
}

fn online_source(session_id: u64, generation: &str) -> PcCandidateSourceBinding {
    PcCandidateSourceBinding::online_pc4(
        session(session_id),
        PcCandidateRequestIdentity::from_sha256([1; 32]),
        PcCandidateSourceIdentity::from_sha256([2; 32]),
        Pc4RuleProfile::Srs,
        0,
        qualified_snapshot(generation),
    )
}

fn candidate(piece: PieceKind, mask: u64) -> StandardBoard64TilingIdentity {
    StandardBoard64TilingIdentity::from_placements(0, [PiecePlacementMask::new(piece, mask)])
        .expect("candidate")
}

fn candidates() -> Vec<StandardBoard64TilingIdentity> {
    let mut values = vec![
        candidate(PieceKind::T, 0b1111),
        candidate(PieceKind::I, 0b1111),
        candidate(PieceKind::O, 0b1111),
    ];
    values.sort_unstable();
    values
}

struct Guard {
    current: PcCandidateSourceBinding,
    cancelled: Cell<bool>,
    snapshot_current: Cell<bool>,
}

impl Guard {
    fn new(current: PcCandidateSourceBinding) -> Self {
        Self {
            current,
            cancelled: Cell::new(false),
            snapshot_current: Cell::new(true),
        }
    }
}

impl PcCandidatePageGuard for Guard {
    fn is_cancelled(&self) -> bool {
        self.cancelled.get()
    }

    fn is_current_source(&self, source: &PcCandidateSourceBinding) -> bool {
        source == &self.current
    }

    fn is_current_snapshot(&self, snapshot: &QualifiedSnapshotIdentity) -> bool {
        self.snapshot_current.get()
            && self
                .current
                .qualified_snapshot()
                .is_some_and(|current| current == snapshot)
    }
}

fn evidence(
    source: &PcCandidateSourceBinding,
    values: &[StandardBoard64TilingIdentity],
) -> PcCandidateCompletenessEvidence {
    PcCandidateCompletenessEvidence::from_verified_complete_source(
        source.clone(),
        values.len() as u64,
        PcCandidateSetDigest::calculate(values).expect("candidate digest"),
    )
}

#[test]
fn exact_offline_pages_seal_one_canonical_reducer_input() {
    let source = offline_source(1);
    let guard = Guard::new(source.clone());
    let all = candidates();
    let mut collector = PcCandidatePageCollector::new(source.clone());

    let first = PcConcreteCandidatePage::partial(
        source.clone(),
        collector.expected_cursor(),
        all[..2].to_vec(),
        false,
    )
    .expect("first page");
    let second_cursor = first.next_cursor();
    collector.accept(first, &guard).expect("accept first");
    let last = PcConcreteCandidatePage::complete(
        source.clone(),
        second_cursor,
        all[2..].to_vec(),
        evidence(&source, &all),
    )
    .expect("complete page");
    collector.accept(last, &guard).expect("accept complete");

    let collection = collector.finish().expect("terminal collection");
    assert_eq!(
        collection.completeness(),
        PcCandidateCollectionCompleteness::CompleteRequestUniverse
    );
    let reducer = collection
        .into_reducer_input()
        .expect("complete reducer input");
    assert_eq!(reducer.source(), &source);
    assert_eq!(reducer.candidates(), all);
}

#[test]
fn terminal_partial_tablebase_data_cannot_cross_the_reducer_seam() {
    let source = online_source(1, "generation-a");
    let guard = Guard::new(source.clone());
    let mut collector = PcCandidatePageCollector::new(source.clone());
    collector
        .accept(
            PcConcreteCandidatePage::partial(
                source,
                PcCandidatePageCursor::initial(),
                candidates(),
                true,
            )
            .expect("terminal partial page"),
            &guard,
        )
        .expect("collect known candidates");

    let collection = collector.finish().expect("partial terminal collection");
    assert_eq!(
        collection.completeness(),
        PcCandidateCollectionCompleteness::PartialKnownCandidates
    );
    assert_eq!(
        collection.into_reducer_input(),
        Err(PcCandidateBoundaryError::IncompleteCannotReduce)
    );
}

#[test]
fn mixed_provider_generation_request_and_source_pages_are_rejected() {
    let source = online_source(1, "generation-a");
    let guard = Guard::new(source.clone());
    for mixed in [
        offline_source(1),
        online_source(1, "generation-b"),
        PcCandidateSourceBinding::online_pc4(
            session(1),
            PcCandidateRequestIdentity::from_sha256([9; 32]),
            PcCandidateSourceIdentity::from_sha256([2; 32]),
            Pc4RuleProfile::Srs,
            0,
            qualified_snapshot("generation-a"),
        ),
        PcCandidateSourceBinding::online_pc4(
            session(1),
            PcCandidateRequestIdentity::from_sha256([1; 32]),
            PcCandidateSourceIdentity::from_sha256([9; 32]),
            Pc4RuleProfile::Srs,
            0,
            qualified_snapshot("generation-a"),
        ),
    ] {
        let mut collector = PcCandidatePageCollector::new(source.clone());
        let page = PcConcreteCandidatePage::partial(
            mixed,
            PcCandidatePageCursor::initial(),
            candidates(),
            true,
        )
        .expect("synthetic mixed page");
        assert_eq!(
            collector.accept(page, &guard),
            Err(PcCandidateBoundaryError::SourceBindingMismatch)
        );
        assert_eq!(
            collector.expected_cursor(),
            PcCandidatePageCursor::initial()
        );
    }
}

#[test]
fn duplicate_or_out_of_order_candidates_and_cursor_replay_are_rejected() {
    let source = offline_source(1);
    let guard = Guard::new(source.clone());
    let all = candidates();
    assert_eq!(
        PcConcreteCandidatePage::partial(
            source.clone(),
            PcCandidatePageCursor::initial(),
            vec![all[0], all[0]],
            true,
        ),
        Err(PcCandidateBoundaryError::CandidatesNotStrictlyCanonical)
    );

    let mut collector = PcCandidatePageCollector::new(source.clone());
    let first = PcConcreteCandidatePage::partial(
        source.clone(),
        collector.expected_cursor(),
        vec![all[0]],
        false,
    )
    .expect("first");
    let replay_cursor = first.cursor();
    collector.accept(first, &guard).expect("accept first");
    let duplicate = PcConcreteCandidatePage::partial(
        source.clone(),
        collector.expected_cursor(),
        vec![all[0]],
        true,
    )
    .expect("cross-page duplicate");
    assert_eq!(
        collector.accept(duplicate, &guard),
        Err(PcCandidateBoundaryError::CandidatesNotStrictlyCanonical)
    );
    assert_eq!(collector.expected_cursor().next_ordinal(), 1);
    let replay = PcConcreteCandidatePage::partial(source, replay_cursor, vec![all[1]], true)
        .expect("replayed cursor page");
    assert_eq!(
        collector.accept(replay, &guard),
        Err(PcCandidateBoundaryError::CursorMismatch)
    );
}

struct CancelsOnSecondObservation {
    current: PcCandidateSourceBinding,
    observations: Cell<u8>,
}

impl PcCandidatePageGuard for CancelsOnSecondObservation {
    fn is_cancelled(&self) -> bool {
        let next = self.observations.get().saturating_add(1);
        self.observations.set(next);
        next >= 2
    }

    fn is_current_source(&self, source: &PcCandidateSourceBinding) -> bool {
        source == &self.current
    }

    fn is_current_snapshot(&self, _snapshot: &QualifiedSnapshotIdentity) -> bool {
        true
    }
}

#[test]
fn cancellation_observed_after_validation_does_not_commit_page_state() {
    let source = offline_source(1);
    let guard = CancelsOnSecondObservation {
        current: source.clone(),
        observations: Cell::new(0),
    };
    let page = PcConcreteCandidatePage::partial(
        source.clone(),
        PcCandidatePageCursor::initial(),
        candidates(),
        true,
    )
    .expect("page");
    let mut collector = PcCandidatePageCollector::new(source);
    assert_eq!(
        collector.accept(page, &guard),
        Err(PcCandidateBoundaryError::Cancelled)
    );
    assert_eq!(
        collector.expected_cursor(),
        PcCandidatePageCursor::initial()
    );
    assert_eq!(
        collector.finish(),
        Err(PcCandidateBoundaryError::SourceNotTerminal)
    );
}

#[test]
fn false_complete_count_or_digest_is_rejected_without_mutating_cursor() {
    let source = offline_source(1);
    let guard = Guard::new(source.clone());
    let all = candidates();
    let wrong_count = PcCandidateCompletenessEvidence::from_verified_complete_source(
        source.clone(),
        99,
        PcCandidateSetDigest::calculate(&all).expect("candidate digest"),
    );
    let page = PcConcreteCandidatePage::complete(
        source.clone(),
        PcCandidatePageCursor::initial(),
        all.clone(),
        wrong_count,
    )
    .expect("count-mismatched page");
    let mut collector = PcCandidatePageCollector::new(source.clone());
    assert_eq!(
        collector.accept(page, &guard),
        Err(PcCandidateBoundaryError::CompletenessCountMismatch)
    );
    assert_eq!(
        collector.expected_cursor(),
        PcCandidatePageCursor::initial()
    );

    let wrong_digest = PcCandidateCompletenessEvidence::from_verified_complete_source(
        source.clone(),
        all.len() as u64,
        PcCandidateSetDigest([7; 32]),
    );
    let page = PcConcreteCandidatePage::complete(
        source,
        PcCandidatePageCursor::initial(),
        all,
        wrong_digest,
    )
    .expect("digest-mismatched page");
    assert_eq!(
        collector.accept(page, &guard),
        Err(PcCandidateBoundaryError::CompletenessDigestMismatch)
    );
    assert_eq!(
        collector.expected_cursor(),
        PcCandidatePageCursor::initial()
    );
}

#[test]
fn cancellation_stale_session_and_stale_generation_are_transactional() {
    let source = online_source(1, "generation-a");
    let guard = Guard::new(source.clone());
    let page = || {
        PcConcreteCandidatePage::partial(
            source.clone(),
            PcCandidatePageCursor::initial(),
            candidates(),
            true,
        )
        .expect("page")
    };

    let mut collector = PcCandidatePageCollector::new(source.clone());
    guard.cancelled.set(true);
    assert_eq!(
        collector.accept(page(), &guard),
        Err(PcCandidateBoundaryError::Cancelled)
    );
    guard.cancelled.set(false);
    guard.snapshot_current.set(false);
    assert_eq!(
        collector.accept(page(), &guard),
        Err(PcCandidateBoundaryError::StaleSnapshot)
    );
    assert_eq!(
        collector.expected_cursor(),
        PcCandidatePageCursor::initial()
    );

    let stale_guard = Guard::new(online_source(2, "generation-a"));
    assert_eq!(
        collector.accept(page(), &stale_guard),
        Err(PcCandidateBoundaryError::StaleSession)
    );
    assert_eq!(
        collector.expected_cursor(),
        PcCandidatePageCursor::initial()
    );
}

#[test]
fn pages_require_one_initial_board_and_nonterminal_progress() {
    let source = offline_source(1);
    assert_eq!(
        PcConcreteCandidatePage::partial(
            source.clone(),
            PcCandidatePageCursor::initial(),
            Vec::new(),
            false,
        ),
        Err(PcCandidateBoundaryError::EmptyNonTerminalPage)
    );
    let wrong_initial = StandardBoard64TilingIdentity::from_placements(
        1 << 20,
        [PiecePlacementMask::new(PieceKind::I, 0b1111)],
    )
    .expect("different initial board");
    assert_eq!(
        PcConcreteCandidatePage::partial(
            source,
            PcCandidatePageCursor::initial(),
            vec![wrong_initial],
            true,
        ),
        Err(PcCandidateBoundaryError::InitialBoardMismatch)
    );
}
