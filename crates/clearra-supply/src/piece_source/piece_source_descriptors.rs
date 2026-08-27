use clearra_core_domain::piece::piece_kind::PieceKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixedPieceSequence {
    pieces: Vec<PieceKind>,
}

impl FixedPieceSequence {
    pub fn new(pieces: Vec<PieceKind>) -> Self {
        Self { pieces }
    }
}
impl FixedPieceSequence {
    pub fn pieces(&self) -> &[PieceKind] {
        &self.pieces
    }
}
impl FixedPieceSequence {
    pub fn len(&self) -> usize {
        self.pieces.len()
    }

    /// Returns only the heap payload retained by the piece buffer, measured by
    /// allocation capacity. The inline descriptor is excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        checked_piece_capacity_bytes(&self.pieces)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BagUniverseDescriptor {
    pattern: Vec<PieceKind>,
}

impl BagUniverseDescriptor {
    pub fn new(pattern: Vec<PieceKind>) -> Self {
        Self { pattern }
    }
}
impl BagUniverseDescriptor {
    pub fn pattern(&self) -> &[PieceKind] {
        &self.pattern
    }

    /// Returns only the heap payload retained by the pattern buffer, measured
    /// by allocation capacity. The inline descriptor is excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        checked_piece_capacity_bytes(&self.pattern)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedWindowDescriptor {
    observed: Vec<PieceKind>,
    budget: usize,
}

impl ObservedWindowDescriptor {
    pub fn new(observed: Vec<PieceKind>, budget: usize) -> Self {
        Self { observed, budget }
    }
}
impl ObservedWindowDescriptor {
    pub fn observed(&self) -> &[PieceKind] {
        &self.observed
    }
}
impl ObservedWindowDescriptor {
    pub fn budget(&self) -> usize {
        self.budget
    }

    /// Returns only the heap payload retained by the observed-piece buffer,
    /// measured by allocation capacity. The inline descriptor is excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        checked_piece_capacity_bytes(&self.observed)
    }
}

fn checked_piece_capacity_bytes(pieces: &Vec<PieceKind>) -> Option<u128> {
    checked_count_bytes(
        pieces.capacity() as u128,
        core::mem::size_of::<PieceKind>() as u128,
    )
}

fn checked_count_bytes(count: u128, item_size: u128) -> Option<u128> {
    count.checked_mul(item_size)
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    use super::{
        checked_count_bytes, BagUniverseDescriptor, FixedPieceSequence, ObservedWindowDescriptor,
    };

    #[test]
    fn descriptor_retained_capacity_uses_allocated_piece_slots() {
        let mut fixed = Vec::with_capacity(64);
        fixed.push(PieceKind::I);
        let fixed_capacity = fixed.capacity();
        let fixed = FixedPieceSequence::new(fixed);

        let mut bag = Vec::with_capacity(32);
        bag.push(PieceKind::O);
        let bag_capacity = bag.capacity();
        let bag = BagUniverseDescriptor::new(bag);

        let mut observed = Vec::with_capacity(16);
        observed.push(PieceKind::T);
        let observed_capacity = observed.capacity();
        let observed = ObservedWindowDescriptor::new(observed, 1);
        let piece_size = core::mem::size_of::<PieceKind>() as u128;

        assert_eq!(
            fixed.checked_retained_capacity_bytes(),
            (fixed_capacity as u128).checked_mul(piece_size)
        );
        assert_eq!(
            bag.checked_retained_capacity_bytes(),
            (bag_capacity as u128).checked_mul(piece_size)
        );
        assert_eq!(
            observed.checked_retained_capacity_bytes(),
            (observed_capacity as u128).checked_mul(piece_size)
        );
    }

    #[test]
    fn descriptor_capacity_arithmetic_fails_closed_on_overflow() {
        assert_eq!(checked_count_bytes(u128::MAX, 2), None);
    }
}
