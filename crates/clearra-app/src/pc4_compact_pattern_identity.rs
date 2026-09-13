//! Bind actual compact queue storage without enumerating its Cartesian space.
//! The supply-owned borrowed view is authoritative for this representation;
//! the original pattern text, coarse IDs or sampled queue rows are not proofs.
use clearra_supply::pattern_universe::materialized_pattern_universe::UniformCompactPatternSource;

use super::{hash_len, Pc4CompiledPatternError, Sha256};
use sha2::Digest;

pub(super) fn hash_compact_source<G: Fn() -> bool>(
    hasher: &mut Sha256,
    source: UniformCompactPatternSource<'_>,
    count: usize,
    length: usize,
    cancelled: &G,
) -> Result<(), Pc4CompiledPatternError> {
    let check = || {
        if cancelled() {
            Err(Pc4CompiledPatternError::Cancelled)
        } else {
            Ok(())
        }
    };
    check()?;
    hasher.update(b"compact-ranked-storage.v1\0");
    hasher.update((1.0 / count as f64).to_bits().to_be_bytes());
    match source {
        UniformCompactPatternSource::Standard7Bag {
            sequence_len,
            pattern_count,
        } => {
            if sequence_len != length || pattern_count != count {
                return Err(Pc4CompiledPatternError::InconsistentUniverse);
            }
            hasher.update([0]);
            hash_len(hasher, sequence_len)?;
            hash_len(hasher, pattern_count)?;
            for piece in clearra_core_domain::piece::piece_kind::PieceKind::STANDARD_TETROMINOES {
                hasher.update([super::piece_identity_byte(piece)]);
            }
        }
        UniformCompactPatternSource::FactorizedExpression(shape) => {
            if shape.visible_sequence_len() != length || shape.pattern_count() != count {
                return Err(Pc4CompiledPatternError::InconsistentUniverse);
            }
            hasher.update([1]);
            hash_len(hasher, shape.pattern_count())?;
            hash_len(hasher, shape.full_sequence_len())?;
            hash_len(hasher, shape.visible_sequence_len())?;
            hash_len(hasher, shape.atoms().len())?;
            for atom in shape.atoms() {
                check()?;
                hash_len(hasher, atom.draw_count())?;
                hash_len(hasher, atom.variant_count())?;
                hash_len(hasher, atom.choices().len())?;
                for piece in atom.choices() {
                    hasher.update([super::piece_identity_byte(*piece)]);
                }
            }
        }
    }
    check()
}
