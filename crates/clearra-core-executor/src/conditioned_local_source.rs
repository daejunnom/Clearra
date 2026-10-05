//! SRP rationale: one solver-facing relation contract for full owners and
//! bounded in-app peers. Both return the SAME exact relation type and leave
//! composition, Boolean shortcuts and exact fallback to the existing solver.

use std::sync::Arc;

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_rules::kicks::KickTableProfileId;

use crate::conditioned_local_peer::{
    peer_snapshot, PeerRecord, PreparedPeerContext, QualifiedRelationPeer,
};
use crate::conditioned_local_product::{
    qualified_local_relation_snapshot, PreparedQualifiedLocalRelationContext,
    QualifiedLocalRelationPack,
};
use crate::conditioned_local_relation::{
    ConditionedPoseWindow, ExactConditionedLocalRelation, LocalRelationRowFrame,
};
use crate::conditioned_reachability::ConditionedReachabilityEntryPose;
use crate::legal_board::ProviderStatus;

pub(crate) enum RelationSource {
    Full(Arc<QualifiedLocalRelationPack>),
    Peer(Arc<QualifiedRelationPeer>),
}

impl From<Arc<QualifiedLocalRelationPack>> for RelationSource {
    fn from(pack: Arc<QualifiedLocalRelationPack>) -> Self {
        Self::Full(pack)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PreparedRelationContext {
    Full(PreparedQualifiedLocalRelationContext),
    Peer(PreparedPeerContext),
}

pub(crate) enum RelationRecord<'a> {
    Full(&'a ExactConditionedLocalRelation),
    Peer(PeerRecord<'a>),
}
impl core::ops::Deref for RelationRecord<'_> {
    type Target = ExactConditionedLocalRelation;
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Full(record) => record,
            Self::Peer(record) => record,
        }
    }
}

impl RelationSource {
    pub(crate) fn snapshot(profile: KickTableProfileId) -> Option<Self> {
        qualified_local_relation_snapshot(profile)
            .map(Self::Full)
            .or_else(|| peer_snapshot(profile).map(Self::Peer))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_context(
        &self,
        width: u8,
        height: u8,
        frame: LocalRelationRowFrame,
        piece: PieceKind,
        profile: KickTableProfileId,
        window: ConditionedPoseWindow,
        entries: &[ConditionedReachabilityEntryPose],
    ) -> Result<PreparedRelationContext, ProviderStatus> {
        match self {
            Self::Full(pack) => pack
                .prepare_context(width, height, frame, piece, profile, window, entries)
                .map(PreparedRelationContext::Full),
            Self::Peer(peer) => peer
                .prepare_context(width, height, frame, piece, profile, window, entries)
                .map(PreparedRelationContext::Peer),
        }
    }

    pub(crate) fn lookup_prepared_record_for_proven_entries(
        &self,
        board: u64,
        context: PreparedRelationContext,
    ) -> Result<RelationRecord<'_>, ProviderStatus> {
        match (self, context) {
            (Self::Full(pack), PreparedRelationContext::Full(context)) => pack
                .lookup_prepared_record_for_proven_entries(board, context)
                .map(RelationRecord::Full),
            (Self::Peer(peer), PreparedRelationContext::Peer(context)) => {
                peer.lookup(board, context).map(RelationRecord::Peer)
            }
            _ => Err(ProviderStatus::SnapshotMismatch),
        }
    }
}
