use clearra_coverage::universe::{
    pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
};

use super::{PieceSetId, PieceSourceId, PieceSourceKind};

pub(super) fn piece_source_id(
    kind: PieceSourceKind,
    piece_set_id: PieceSetId,
    provenance_id: u64,
    universe_id: Option<PatternUniverseId>,
    weight_model_id: Option<PatternWeightModelId>,
    count: u64,
    piece_hash: u64,
) -> PieceSourceId {
    let mut hash = 0xcbf29ce484222325u64;
    for value in [
        kind.as_str().as_bytes(),
        &piece_set_id.get().to_le_bytes(),
        &provenance_id.to_le_bytes(),
        &universe_id
            .map(PatternUniverseId::get)
            .unwrap_or(0)
            .to_le_bytes(),
        &weight_model_id
            .map(PatternWeightModelId::get)
            .unwrap_or(0)
            .to_le_bytes(),
        &count.to_le_bytes(),
        &piece_hash.to_le_bytes(),
    ] {
        for byte in value {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    PieceSourceId::new(hash.max(1))
}
