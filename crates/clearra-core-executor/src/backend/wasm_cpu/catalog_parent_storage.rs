//! Geometry projection collection and immutable per-row temporal parent storage.
use super::{Realization, SkeletonRow, WasmExactSearchError};
use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

#[cfg(any(test, feature = "minimum-physical-ab"))]
use super::super::inverse_parent::{inverse_parent_policy, InverseParentPolicy, InverseParentCursor, ParentAdvance};

#[cfg(any(test, feature = "minimum-physical-ab"))]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct LogicalProjection { piece: PieceKind, cells: u64, rotation: RotationState }

#[cfg(any(test, feature = "minimum-physical-ab"))]
pub(super) type DeferredParents = Vec<std::sync::OnceLock<[Realization; 4]>>;

pub(super) struct CompiledRows {
    pub skeletons: Vec<SkeletonRow>,
    pub realizations: Vec<Realization>,
    #[cfg(any(test, feature = "minimum-physical-ab"))]
    pub deferred: Option<DeferredParents>,
}

pub(super) enum ProjectionOutput {
    Eager(Vec<Realization>),
    #[cfg(any(test, feature = "minimum-physical-ab"))]
    Deferred(Vec<LogicalProjection>),
}

fn reserved<T>(capacity: usize) -> Result<Vec<T>, WasmExactSearchError> {
    let mut result = Vec::new();
    result.try_reserve_exact(capacity).map_err(|_| WasmExactSearchError::InvalidProblem(
        "wasm_geometry_catalog_realization_storage_unavailable"))?;
    Ok(result)
}

pub(super) fn instantiation_table_enabled() -> bool {
    #[cfg(any(test, feature = "minimum-physical-ab"))]
    { inverse_parent_policy() == InverseParentPolicy::EagerTable }
    #[cfg(not(any(test, feature = "minimum-physical-ab")))]
    { true }
}

pub(super) fn parent_storage_peak_bytes(count: u128) -> Option<u128> {
    #[cfg(any(test, feature = "minimum-physical-ab"))]
    if inverse_parent_policy() == InverseParentPolicy::Deferred {
        return count.checked_mul((core::mem::size_of::<LogicalProjection>()
            + core::mem::size_of::<std::sync::OnceLock<[Realization; 4]>>()) as u128);
    }
    count.checked_mul(2)?.checked_mul(core::mem::size_of::<Realization>() as u128)
}

impl ProjectionOutput {
    pub fn new(capacity: usize) -> Result<Self, WasmExactSearchError> {
        #[cfg(any(test, feature = "minimum-physical-ab"))]
        if inverse_parent_policy() == InverseParentPolicy::Deferred {
            return Ok(Self::Deferred(reserved(capacity)?));
        }
        Ok(Self::Eager(reserved(capacity)?))
    }

    pub fn push_projection(&mut self, piece: PieceKind, cells: u64, rotation: RotationState,
        x: i8, local_rows: &[u8], target_rows: &[u8; 4]) {
        #[cfg(any(test, feature = "minimum-physical-ab"))]
        if let Self::Deferred(rows) = self {
            rows.push(LogicalProjection { piece, cells, rotation });
            return;
        }
        let output = match self {
            Self::Eager(output) => output,
            #[cfg(any(test, feature = "minimum-physical-ab"))]
            Self::Deferred(_) => unreachable!(),
        };
        let mut required_deleted_rows = 0_u16;
        for index in 1..local_rows.len() {
            let first_deleted = target_rows[index - 1] + local_rows[index] - local_rows[index - 1];
            for row in first_deleted..target_rows[index] { required_deleted_rows |= 1_u16 << row; }
        }
        output.push(Realization { piece, cells, required_deleted_rows, rotation, x,
            target_anchor_y: target_rows[0] as i8 });
    }

    pub fn finish(self) -> Result<CompiledRows, WasmExactSearchError> {
        match self {
            Self::Eager(mut realizations) => {
                realizations.sort_unstable();
                realizations.dedup();
                let mut skeletons = reserved(realizations.len())?;
                let mut ordered = reserved(realizations.len())?;
                let mut cursor = 0;
                while cursor < realizations.len() {
                    let piece = realizations[cursor].piece;
                    let cells = realizations[cursor].cells;
                    let start = ordered.len();
                    while cursor < realizations.len() && realizations[cursor].piece == piece
                        && realizations[cursor].cells == cells {
                        ordered.push(realizations[cursor]); cursor += 1;
                    }
                    skeletons.push(SkeletonRow { piece, cells, realization_start: start as u32,
                        realization_count: (ordered.len() - start) as u16 });
                }
                Ok(CompiledRows { skeletons, realizations: ordered,
                    #[cfg(any(test, feature = "minimum-physical-ab"))] deferred: None })
            }
            #[cfg(any(test, feature = "minimum-physical-ab"))]
            Self::Deferred(mut rows) => {
                rows.sort_unstable(); rows.dedup();
                let mut skeletons = reserved(rows.len())?;
                let mut cursor = 0;
                while cursor < rows.len() {
                    let first = rows[cursor];
                    let start = cursor;
                    while cursor < rows.len() && rows[cursor].piece == first.piece
                        && rows[cursor].cells == first.cells { cursor += 1; }
                    let count = cursor - start;
                    if count == 0 || count > 4 { return Err(WasmExactSearchError::InvalidProblem(
                        "wasm_inverse_parent_rotation_domain_invalid")); }
                    skeletons.push(SkeletonRow { piece: first.piece, cells: first.cells,
                        realization_start: u32::try_from(start).map_err(|_| WasmExactSearchError::InvalidProblem(
                            "wasm_inverse_parent_index_overflow"))?, realization_count: count as u16 });
                }
                let mut deferred = reserved(skeletons.len())?;
                deferred.resize_with(skeletons.len(), std::sync::OnceLock::new);
                Ok(CompiledRows { skeletons, realizations: Vec::new(), deferred: Some(deferred) })
            }
        }
    }
}

#[cfg(any(test, feature = "minimum-physical-ab"))]
pub(super) fn materialize(width: u8, height: u8, skeleton: SkeletonRow) -> [Realization; 4] {
    let mut mask = skeleton.cells;
    let cells = std::array::from_fn(|_| {
        let cell = mask.trailing_zeros() as u16; mask &= mask - 1; cell
    });
    let mut cursor = InverseParentCursor::new(width, height, skeleton.piece, cells)
        .expect("logical projection has a valid fixed tetromino");
    let mut result = [Realization { piece: skeleton.piece, cells: skeleton.cells,
        required_deleted_rows: 0, rotation: RotationState::ALL[0], x: 0, target_anchor_y: 0 }; 4];
    let mut count = 0;
    loop {
        match cursor.advance() {
            ParentAdvance::Parent(parent) => {
                result[count] = Realization { piece: skeleton.piece, cells: skeleton.cells,
                    required_deleted_rows: u16::try_from(parent.required_deleted_rows)
                        .expect("Board64 catalog has at most 16 rows"),
                    rotation: parent.rotation, x: parent.x, target_anchor_y: parent.target_anchor_y };
                count += 1;
            }
            ParentAdvance::Pending => {}
            ParentAdvance::Complete => break,
        }
    }
    assert_eq!(count, skeleton.realization_count as usize,
        "an incomplete inverse parent domain must not become an empty complete family");
    result[..count].sort_unstable();
    result
}
