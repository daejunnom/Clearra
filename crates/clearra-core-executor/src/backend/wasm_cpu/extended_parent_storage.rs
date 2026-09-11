//! Extended logical projections and compact, complete temporal-parent caches.
use super::{ExtendedRealization, ExtendedSkeletonRow, WasmExactSearchError};
use super::super::extended_board::ExtendedBoard;
use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

#[cfg(any(test, feature = "minimum-physical-ab"))]
use super::super::inverse_parent::{inverse_parent_policy, InverseParentCursor,
    InverseParentPolicy, ParentAdvance, TemporalParent};

#[cfg(any(test, feature = "minimum-physical-ab"))]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct LogicalProjection {
    piece: PieceKind,
    cells: ExtendedBoard,
    rotation: RotationState,
}

#[cfg(any(test, feature = "minimum-physical-ab"))]
pub(super) type DeferredParents = Vec<std::sync::OnceLock<[TemporalParent; 4]>>;

pub(super) struct CompiledRows {
    pub skeletons: Vec<ExtendedSkeletonRow>,
    pub realizations: Vec<ExtendedRealization>,
    #[cfg(any(test, feature = "minimum-physical-ab"))]
    pub deferred: Option<DeferredParents>,
}

pub(super) enum ProjectionOutput {
    Eager(Vec<ExtendedRealization>),
    #[cfg(any(test, feature = "minimum-physical-ab"))]
    Deferred(Vec<LogicalProjection>),
}

fn unavailable() -> WasmExactSearchError {
    WasmExactSearchError::InvalidProblem("wasm_extended_catalog_parent_storage_unavailable")
}

fn reserved<T>(capacity: usize) -> Result<Vec<T>, WasmExactSearchError> {
    let mut rows = Vec::new();
    rows.try_reserve_exact(capacity).map_err(|_| unavailable())?;
    Ok(rows)
}

fn push<T>(rows: &mut Vec<T>, value: T) -> Result<(), WasmExactSearchError> {
    if rows.len() == rows.capacity() {
        rows.try_reserve(1).map_err(|_| unavailable())?;
    }
    rows.push(value);
    Ok(())
}

impl ProjectionOutput {
    pub fn new() -> Self {
        #[cfg(any(test, feature = "minimum-physical-ab"))]
        if inverse_parent_policy() == InverseParentPolicy::Deferred {
            return Self::Deferred(Vec::new());
        }
        // Extended execution already instantiates raw parents on demand;
        // EagerTable and EagerRaw therefore select the same existing path here.
        Self::Eager(Vec::new())
    }

    pub fn push_projection(&mut self, piece: PieceKind, cells: ExtendedBoard,
        rotation: RotationState, x: i8, local_rows: &[u8], target_rows: &[u8; 4])
        -> Result<(), WasmExactSearchError> {
        #[cfg(any(test, feature = "minimum-physical-ab"))]
        if let Self::Deferred(rows) = self {
            return push(rows, LogicalProjection { piece, cells, rotation });
        }
        let output = match self {
            Self::Eager(output) => output,
            #[cfg(any(test, feature = "minimum-physical-ab"))]
            Self::Deferred(_) => unreachable!(),
        };
        let mut required_deleted_rows = 0_u32;
        for index in 1..local_rows.len() {
            let first_deleted = target_rows[index - 1] + local_rows[index] - local_rows[index - 1];
            for row in first_deleted..target_rows[index] { required_deleted_rows |= 1_u32 << row; }
        }
        push(output, ExtendedRealization { piece, cells, required_deleted_rows,
            rotation, x, target_anchor_y: target_rows[0] as i8 })
    }

    pub fn finish(self) -> Result<CompiledRows, WasmExactSearchError> {
        match self {
            Self::Eager(mut rows) => {
                rows.sort_unstable(); rows.dedup();
                let mut skeletons = Vec::new();
                let mut ordered = reserved(rows.len())?;
                let mut cursor = 0;
                while cursor < rows.len() {
                    let first = rows[cursor];
                    let start = ordered.len();
                    while cursor < rows.len() && rows[cursor].piece == first.piece
                        && rows[cursor].cells == first.cells {
                        ordered.push(rows[cursor]); cursor += 1;
                    }
                    push(&mut skeletons, ExtendedSkeletonRow { piece: first.piece, cells: first.cells,
                        realization_start: u32::try_from(start).map_err(|_| unavailable())?,
                        realization_count: u32::try_from(ordered.len() - start).map_err(|_| unavailable())? })?;
                }
                Ok(CompiledRows { skeletons, realizations: ordered,
                    #[cfg(any(test, feature = "minimum-physical-ab"))] deferred: None })
            }
            #[cfg(any(test, feature = "minimum-physical-ab"))]
            Self::Deferred(mut rows) => {
                rows.sort_unstable(); rows.dedup();
                let mut skeletons = Vec::new();
                let mut cursor = 0;
                while cursor < rows.len() {
                    let first = rows[cursor];
                    let start = cursor;
                    while cursor < rows.len() && rows[cursor].piece == first.piece
                        && rows[cursor].cells == first.cells { cursor += 1; }
                    let count = cursor - start;
                    if count == 0 || count > 4 {
                        return Err(WasmExactSearchError::InvalidProblem(
                            "wasm_extended_inverse_parent_rotation_domain_invalid"));
                    }
                    push(&mut skeletons, ExtendedSkeletonRow { piece: first.piece, cells: first.cells,
                        realization_start: u32::try_from(start).map_err(|_| unavailable())?,
                        realization_count: count as u32 })?;
                }
                let mut deferred = reserved(skeletons.len())?;
                deferred.resize_with(skeletons.len(), std::sync::OnceLock::new);
                Ok(CompiledRows { skeletons, realizations: Vec::new(), deferred: Some(deferred) })
            }
        }
    }
}

pub(super) enum ParentIter<'a> {
    Eager(core::iter::Copied<core::slice::Iter<'a, ExtendedRealization>>),
    #[cfg(any(test, feature = "minimum-physical-ab"))]
    Deferred { skeleton: ExtendedSkeletonRow, parents: core::iter::Copied<core::slice::Iter<'a, TemporalParent>> },
}

impl Iterator for ParentIter<'_> {
    type Item = ExtendedRealization;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Eager(rows) => rows.next(),
            #[cfg(any(test, feature = "minimum-physical-ab"))]
            Self::Deferred { skeleton, parents } => parents.next().map(|parent| ExtendedRealization {
                piece: skeleton.piece, cells: skeleton.cells,
                required_deleted_rows: parent.required_deleted_rows,
                rotation: parent.rotation, x: parent.x, target_anchor_y: parent.target_anchor_y,
            }),
        }
    }
}

#[cfg(any(test, feature = "minimum-physical-ab"))]
pub(super) fn materialize(width: u8, height: u8, skeleton: ExtendedSkeletonRow) -> [TemporalParent; 4] {
    let mut cells = skeleton.cells.cells();
    let target = std::array::from_fn(|_| cells.next().expect("logical tetromino has four cells"));
    assert!(cells.next().is_none(), "logical tetromino cannot have a fifth cell");
    let mut cursor = InverseParentCursor::new(width, height, skeleton.piece, target)
        .expect("extended logical projection has a valid fixed tetromino");
    let mut parents = [TemporalParent { required_deleted_rows: 0,
        rotation: RotationState::ALL[0], x: 0, target_anchor_y: 0 }; 4];
    let mut count = 0;
    loop {
        match cursor.advance() {
            ParentAdvance::Parent(parent) => { parents[count] = parent; count += 1; }
            ParentAdvance::Pending => {}
            ParentAdvance::Complete => break,
        }
    }
    assert_eq!(count, skeleton.realization_count as usize,
        "an incomplete extended parent family cannot become an empty complete domain");
    parents[..count].sort_unstable();
    parents
}
