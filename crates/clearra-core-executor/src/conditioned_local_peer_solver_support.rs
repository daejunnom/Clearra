//! SRP rationale: isolated in-process transport for solver integration tests.
//! It exercises the real bounded seed/query/reply codec without mutating any
//! process-wide asset registry or widening the production peer interface.

use super::*;
use crate::conditioned_local_product::QualifiedLocalRelationPack;

pub(crate) fn peer(
    owner: &QualifiedLocalRelationPack,
    authority: &VerifiedAcceleratorAuthority,
) -> Arc<QualifiedRelationPeer> {
    Arc::new(
        QualifiedRelationPeer::from_trusted_seed(
            &wire::encode_seed(owner).unwrap(),
            authority,
            MIN_RELATION_PEER_RESERVED_BYTES,
        )
        .unwrap(),
    )
}

pub(crate) fn exchange(
    owner: &QualifiedLocalRelationPack,
    peer: &QualifiedRelationPeer,
    corrupt: bool,
) -> Result<bool, LocalRelationPeerError> {
    let Some(query) = peer.drain()? else {
        return Ok(false);
    };
    let queries = wire::decode_queries(&query, PeerIdentity::owner(owner))?;
    let mut reply = wire::encode_reply(owner, &queries)?;
    if corrupt {
        *reply.last_mut().unwrap() ^= 1;
    }
    peer.import(&reply)?;
    let state = peer.state.lock().unwrap();
    assert!(state.bytes <= peer.cache_budget);
    assert!(state.records.len() <= MAX_RECORDS);
    assert!(state.pending.len() <= wire::MAX_RELATION_PEER_BATCH);
    assert!(state.sent.len() <= wire::MAX_RELATION_PEER_BATCH);
    Ok(true)
}
