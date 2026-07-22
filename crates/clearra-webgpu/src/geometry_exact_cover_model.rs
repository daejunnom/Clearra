use std::{fmt, sync::Arc};

use crate::WebGpuUnavailableResult;

pub(crate) const STATE_WORDS: usize = 4;
pub(crate) const TRACE_WORDS: usize = 2;
pub(crate) const COUNTER_WORDS: usize = 4;
pub(crate) const PARAM_WORDS: usize = 16;
pub(crate) const MAX_PACKING_OPERATIONS: usize = 15;
pub(crate) const CERTIFIED_CONSTRAINT_WORDS: usize = 4 + 7 * 64;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct WebGpuPlacementSkeleton {
    pub mask: u64,
    pub piece: u8,
    pub rotation: u8,
    pub x: i8,
    pub y: i8,
    pub operation_id: u16,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct WebGpuGeometryCatalogIdentity {
    words: [u64; 8],
}

impl WebGpuGeometryCatalogIdentity {
    pub const fn from_words(words: [u64; 8]) -> Self {
        Self { words }
    }

    pub const fn words(self) -> [u64; 8] {
        self.words
    }
}

/// Immutable exact-cover geometry shared by every multiset batch. Implementors
/// must keep the returned slices alive for the lifetime of the catalog owner.
pub trait WebGpuExactCoverCatalog: fmt::Debug + Send + Sync {
    fn identity(&self) -> WebGpuGeometryCatalogIdentity;

    fn skeleton_cell_masks(&self) -> &[u64];
    fn skeleton_piece_kinds(&self) -> &[u32];
    fn cell_support_offsets(&self) -> &[u32];
    fn cell_support_row_ids(&self) -> &[u32];

    fn certified_constraint_words(&self) -> &[u32] {
        &[]
    }
}

#[derive(Debug)]
struct OwnedWebGpuExactCoverCatalog {
    identity: WebGpuGeometryCatalogIdentity,
    skeleton_cell_masks: Box<[u64]>,
    skeleton_piece_kinds: Box<[u32]>,
    cell_support_offsets: Box<[u32]>,
    cell_support_row_ids: Box<[u32]>,
}

impl WebGpuExactCoverCatalog for OwnedWebGpuExactCoverCatalog {
    fn identity(&self) -> WebGpuGeometryCatalogIdentity {
        self.identity
    }

    fn skeleton_cell_masks(&self) -> &[u64] {
        &self.skeleton_cell_masks
    }

    fn skeleton_piece_kinds(&self) -> &[u32] {
        &self.skeleton_piece_kinds
    }

    fn cell_support_offsets(&self) -> &[u32] {
        &self.cell_support_offsets
    }

    fn cell_support_row_ids(&self) -> &[u32] {
        &self.cell_support_row_ids
    }
}

#[derive(Clone)]
pub struct WebGpuGeometryExactCoverBatch {
    geometry: Arc<WebGpuGeometryExactCoverGeometry>,
    pub(crate) desired_piece_counts: [u8; 7],
    pub(crate) frontier_capacity: u32,
}

#[derive(Debug)]
struct WebGpuGeometryExactCoverGeometry {
    width: u8,
    height: u8,
    initial_mask: u64,
    goal_mask: u64,
    required_fill_mask: u64,
    forbidden_mask: u64,
    catalog: Arc<dyn WebGpuExactCoverCatalog>,
}

impl fmt::Debug for WebGpuGeometryExactCoverBatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WebGpuGeometryExactCoverBatch")
            .field("geometry", &self.geometry)
            .field("desired_piece_counts", &self.desired_piece_counts)
            .field("frontier_capacity", &self.frontier_capacity)
            .finish()
    }
}

impl PartialEq for WebGpuGeometryExactCoverBatch {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.geometry, &other.geometry)
            && self.desired_piece_counts == other.desired_piece_counts
            && self.frontier_capacity == other.frontier_capacity
    }
}

impl Eq for WebGpuGeometryExactCoverBatch {}

impl WebGpuGeometryExactCoverBatch {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        width: u8,
        height: u8,
        initial_mask: u64,
        goal_mask: u64,
        required_fill_mask: u64,
        forbidden_mask: u64,
        desired_piece_counts: [u8; 7],
        operations: Vec<WebGpuPlacementSkeleton>,
        frontier_capacity: usize,
    ) -> Result<Self, WebGpuGeometryExactCoverInputError> {
        Self::new_shared_operations(
            width,
            height,
            initial_mask,
            goal_mask,
            required_fill_mask,
            forbidden_mask,
            desired_piece_counts,
            operations.into(),
            frontier_capacity,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_shared_operations(
        width: u8,
        height: u8,
        initial_mask: u64,
        goal_mask: u64,
        required_fill_mask: u64,
        forbidden_mask: u64,
        desired_piece_counts: [u8; 7],
        operations: Arc<[WebGpuPlacementSkeleton]>,
        frontier_capacity: usize,
    ) -> Result<Self, WebGpuGeometryExactCoverInputError> {
        let cell_count = u16::from(width)
            .checked_mul(u16::from(height))
            .filter(|count| *count > 0 && *count <= 64)
            .ok_or(WebGpuGeometryExactCoverInputError::InvalidBoard)?;
        let board_mask = if cell_count == 64 {
            u64::MAX
        } else {
            (1_u64 << cell_count) - 1
        };
        if initial_mask & !board_mask != 0
            || goal_mask & !board_mask != 0
            || required_fill_mask & !board_mask != 0
            || forbidden_mask & !board_mask != 0
            || required_fill_mask & !goal_mask != 0
            || initial_mask & forbidden_mask != 0
        {
            return Err(WebGpuGeometryExactCoverInputError::InvalidBoardMask);
        }
        if operations.is_empty() {
            return Err(WebGpuGeometryExactCoverInputError::EmptyOperationTable);
        }
        for operation in operations.iter() {
            if operation.piece == 0
                || operation.piece > 7
                || operation.mask == 0
                || operation.mask & !board_mask != 0
            {
                return Err(WebGpuGeometryExactCoverInputError::InvalidOperation);
            }
        }

        let mut masks = Vec::with_capacity(operations.len());
        let mut pieces = Vec::with_capacity(operations.len());
        for operation in operations.iter() {
            masks.push(operation.mask);
            pieces.push(u32::from(operation.piece));
        }
        let mut support_offsets = Vec::with_capacity(usize::from(cell_count) + 1);
        let mut support_operations = Vec::new();
        support_offsets.push(0);
        for cell in 0..cell_count {
            let cell_mask = 1_u64 << cell;
            for (index, operation) in operations.iter().enumerate() {
                if operation.mask & cell_mask != 0 {
                    support_operations.push(
                        u32::try_from(index)
                            .map_err(|_| WebGpuGeometryExactCoverInputError::DimensionOverflow)?,
                    );
                }
            }
            support_offsets.push(
                u32::try_from(support_operations.len())
                    .map_err(|_| WebGpuGeometryExactCoverInputError::DimensionOverflow)?,
            );
        }

        let identity =
            owned_catalog_identity(&masks, &pieces, &support_offsets, &support_operations);
        let catalog: Arc<dyn WebGpuExactCoverCatalog> = Arc::new(OwnedWebGpuExactCoverCatalog {
            identity,
            skeleton_cell_masks: masks.into_boxed_slice(),
            skeleton_piece_kinds: pieces.into_boxed_slice(),
            cell_support_offsets: support_offsets.into_boxed_slice(),
            cell_support_row_ids: support_operations.into_boxed_slice(),
        });
        Self::from_catalog(
            width,
            height,
            initial_mask,
            goal_mask,
            required_fill_mask,
            forbidden_mask,
            desired_piece_counts,
            catalog,
            frontier_capacity,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_catalog(
        width: u8,
        height: u8,
        initial_mask: u64,
        goal_mask: u64,
        required_fill_mask: u64,
        forbidden_mask: u64,
        desired_piece_counts: [u8; 7],
        catalog: Arc<dyn WebGpuExactCoverCatalog>,
        frontier_capacity: usize,
    ) -> Result<Self, WebGpuGeometryExactCoverInputError> {
        validate_board_masks(
            width,
            height,
            initial_mask,
            goal_mask,
            required_fill_mask,
            forbidden_mask,
        )?;
        validate_catalog(width, height, &*catalog)?;
        if catalog
            .skeleton_cell_masks()
            .iter()
            .any(|mask| *mask & !required_fill_mask != 0)
        {
            return Err(WebGpuGeometryExactCoverInputError::InvalidOperation);
        }
        let geometry = Arc::new(WebGpuGeometryExactCoverGeometry {
            width,
            height,
            initial_mask,
            goal_mask,
            required_fill_mask,
            forbidden_mask,
            catalog,
        });
        Self::from_geometry(geometry, desired_piece_counts, frontier_capacity)
    }

    pub fn from_shared_geometry(
        source: &Self,
        desired_piece_counts: [u8; 7],
        frontier_capacity: usize,
    ) -> Result<Self, WebGpuGeometryExactCoverInputError> {
        Self::from_geometry(
            Arc::clone(&source.geometry),
            desired_piece_counts,
            frontier_capacity,
        )
    }

    fn from_geometry(
        geometry: Arc<WebGpuGeometryExactCoverGeometry>,
        desired_piece_counts: [u8; 7],
        frontier_capacity: usize,
    ) -> Result<Self, WebGpuGeometryExactCoverInputError> {
        let target_depth = desired_piece_counts.iter().try_fold(0_u8, |total, count| {
            if *count > 15 {
                None
            } else {
                total.checked_add(*count)
            }
        });
        if target_depth.is_none() || target_depth == Some(0) || target_depth.unwrap_or(0) > 15 {
            return Err(WebGpuGeometryExactCoverInputError::InvalidPieceCounts);
        }
        let required_cell_count = geometry.required_fill_mask.count_ones();
        if u32::from(target_depth.unwrap_or(0))
            .checked_mul(4)
            .filter(|required_piece_cells| *required_piece_cells == required_cell_count)
            .is_none()
        {
            return Err(WebGpuGeometryExactCoverInputError::InvalidPieceCounts);
        }
        let frontier_capacity = if frontier_capacity == 0 {
            u32::MAX
        } else {
            u32::try_from(frontier_capacity)
                .map_err(|_| WebGpuGeometryExactCoverInputError::CapacityOverflow)?
        };
        Ok(Self {
            geometry,
            desired_piece_counts,
            frontier_capacity,
        })
    }

    pub fn skeleton_cell_masks(&self) -> &[u64] {
        self.geometry.catalog.skeleton_cell_masks()
    }

    pub fn skeleton_piece_kinds(&self) -> &[u32] {
        self.geometry.catalog.skeleton_piece_kinds()
    }

    pub fn target_depth(&self) -> u8 {
        self.desired_piece_counts.iter().copied().sum()
    }

    pub(crate) fn cell_count(&self) -> u32 {
        u32::from(self.geometry.width) * u32::from(self.geometry.height)
    }

    pub(crate) fn width(&self) -> u8 {
        self.geometry.width
    }

    pub(crate) fn initial_mask(&self) -> u64 {
        self.geometry.initial_mask
    }

    pub(crate) fn goal_mask(&self) -> u64 {
        self.geometry.goal_mask
    }

    pub(crate) fn required_fill_mask(&self) -> u64 {
        self.geometry.required_fill_mask
    }

    pub(crate) fn forbidden_mask(&self) -> u64 {
        self.geometry.forbidden_mask
    }

    pub(crate) fn desired_piece_counts(&self) -> [u8; 7] {
        self.desired_piece_counts
    }

    pub(crate) fn support_offsets(&self) -> &[u32] {
        self.geometry.catalog.cell_support_offsets()
    }

    pub(crate) fn support_operations(&self) -> &[u32] {
        self.geometry.catalog.cell_support_row_ids()
    }

    pub(crate) fn certified_constraint_words(&self) -> &[u32] {
        self.geometry.catalog.certified_constraint_words()
    }

    pub(crate) fn catalog(&self) -> &Arc<dyn WebGpuExactCoverCatalog> {
        &self.geometry.catalog
    }

    pub(crate) fn frontier_capacity(&self) -> u32 {
        self.frontier_capacity
    }

    pub(crate) fn can_share_family_dispatch(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.geometry, &other.geometry)
            && self.target_depth() == other.target_depth()
            && self.frontier_capacity == other.frontier_capacity
    }

    pub(crate) fn desired_counts_word(&self) -> u32 {
        self.desired_piece_counts
            .iter()
            .enumerate()
            .fold(0_u32, |word, (index, count)| {
                word | (u32::from(*count) << (index * 4))
            })
    }

    pub(crate) fn initial_state_words(&self) -> [u32; STATE_WORDS] {
        [
            self.initial_mask() as u32,
            (self.initial_mask() >> 32) as u32,
            0,
            self.desired_counts_word() << 4,
        ]
    }
}

fn validate_catalog(
    width: u8,
    height: u8,
    catalog: &dyn WebGpuExactCoverCatalog,
) -> Result<(), WebGpuGeometryExactCoverInputError> {
    let cell_count = usize::from(width) * usize::from(height);
    let masks = catalog.skeleton_cell_masks();
    let pieces = catalog.skeleton_piece_kinds();
    let offsets = catalog.cell_support_offsets();
    let rows = catalog.cell_support_row_ids();
    let constraints = catalog.certified_constraint_words();
    let cell_count_u32 =
        u32::try_from(cell_count).map_err(|_| WebGpuGeometryExactCoverInputError::InvalidBoard)?;
    let board_mask = if cell_count_u32 == 64 {
        u64::MAX
    } else {
        (1_u64 << cell_count_u32) - 1
    };
    if masks.is_empty() || masks.len() != pieces.len() {
        return Err(WebGpuGeometryExactCoverInputError::EmptyOperationTable);
    }
    if !constraints.is_empty() {
        let expected = 4_usize.saturating_add(7_usize.saturating_mul(width as usize));
        let safe_columns = u64::from(constraints.get(1).copied().unwrap_or(0))
            | (u64::from(constraints.get(2).copied().unwrap_or(0)) << 32);
        let valid_columns = if width == 64 {
            u64::MAX
        } else {
            (1_u64 << width) - 1
        };
        if constraints.len() != expected
            || constraints[0] & 1 == 0
            || constraints[3] != u32::from(width)
            || safe_columns & !valid_columns != 0
            || constraints[4..].iter().any(|word| {
                let minimum = word & 0xff;
                let maximum = (word >> 8) & 0xff;
                minimum > maximum || maximum > 4
            })
        {
            return Err(WebGpuGeometryExactCoverInputError::InvalidOperation);
        }
    }
    if offsets.len() != cell_count.saturating_add(1)
        || offsets.first() != Some(&0)
        || offsets.last().copied() != u32::try_from(rows.len()).ok()
        || offsets.windows(2).any(|pair| pair[0] > pair[1])
        || rows.iter().any(|row| *row as usize >= masks.len())
        || pieces.iter().any(|piece| !(1..=7).contains(piece))
        || masks
            .iter()
            .any(|mask| mask.count_ones() != 4 || *mask & !board_mask != 0)
    {
        return Err(WebGpuGeometryExactCoverInputError::InvalidOperation);
    }

    // The inverse support table is part of the exact-search authority. A
    // range-correct but incomplete table would silently remove every packing
    // that needs the omitted row, so confirm the full cell/row relation here.
    let mut seen = Vec::new();
    seen.try_reserve_exact(masks.len())
        .map_err(|_| WebGpuGeometryExactCoverInputError::CatalogValidationAllocation)?;
    seen.resize(masks.len(), 0_u8);
    for cell in 0..cell_count {
        seen.fill(0);
        let begin = offsets[cell] as usize;
        let end = offsets[cell + 1] as usize;
        let cell_mask = 1_u64 << cell;
        for row_id in rows[begin..end].iter().copied() {
            let row_index = row_id as usize;
            if seen[row_index] != 0 || masks[row_index] & cell_mask == 0 {
                return Err(WebGpuGeometryExactCoverInputError::InvalidOperation);
            }
            seen[row_index] = 1;
        }
        if masks
            .iter()
            .enumerate()
            .any(|(row_index, mask)| (*mask & cell_mask != 0) != (seen[row_index] != 0))
        {
            return Err(WebGpuGeometryExactCoverInputError::InvalidOperation);
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_board_masks(
    width: u8,
    height: u8,
    initial_mask: u64,
    goal_mask: u64,
    required_fill_mask: u64,
    forbidden_mask: u64,
) -> Result<(), WebGpuGeometryExactCoverInputError> {
    let cell_count = u16::from(width)
        .checked_mul(u16::from(height))
        .filter(|count| *count > 0 && *count <= 64)
        .ok_or(WebGpuGeometryExactCoverInputError::InvalidBoard)?;
    let board_mask = if cell_count == 64 {
        u64::MAX
    } else {
        (1_u64 << cell_count) - 1
    };
    if initial_mask & !board_mask != 0
        || goal_mask & !board_mask != 0
        || required_fill_mask & !board_mask != 0
        || forbidden_mask & !board_mask != 0
        || required_fill_mask != goal_mask & !initial_mask
        || initial_mask & forbidden_mask != 0
    {
        return Err(WebGpuGeometryExactCoverInputError::InvalidBoardMask);
    }
    Ok(())
}

fn owned_catalog_identity(
    masks: &[u64],
    pieces: &[u32],
    offsets: &[u32],
    rows: &[u32],
) -> WebGpuGeometryCatalogIdentity {
    fn mix(mut value: u64, next: u64) -> u64 {
        value ^= next;
        value.wrapping_mul(0x100_0000_01b3)
    }
    let mut mask_digest = 0xcbf2_9ce4_8422_2325;
    for value in masks {
        mask_digest = mix(mask_digest, *value);
    }
    let mut piece_digest = 0xcbf2_9ce4_8422_2325;
    for value in pieces {
        piece_digest = mix(piece_digest, u64::from(*value));
    }
    let mut support_digest = 0xcbf2_9ce4_8422_2325;
    for value in offsets.iter().chain(rows) {
        support_digest = mix(support_digest, u64::from(*value));
    }
    WebGpuGeometryCatalogIdentity::from_words([
        masks.len() as u64,
        rows.len() as u64,
        mask_digest,
        piece_digest,
        support_digest,
        1,
        0,
        0,
    ])
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebGpuPackingTrustState {
    NeedsCpuConfirm,
    TrustedCpuSampleConfirmed,
}

#[derive(Debug)]
pub struct WebGpuGeometryExactCoverConnected {
    pub(crate) solution_graph: crate::geometry_exact_cover_result::WebGpuGeometrySolutionGraph,
    pub(crate) shader_version: &'static str,
    pub(crate) shader_hash: String,
    pub(crate) peak_gpu_bytes: u64,
    pub(crate) peak_host_reduce_bytes: usize,
    pub(crate) timings: crate::geometry_exact_cover_timing::WebGpuGeometryExactCoverTimings,
    pub(crate) trust_state: WebGpuPackingTrustState,
    pub(crate) cpu_confirmed_dispatches: u32,
    pub(crate) cpu_confirmed_parents: u32,
}

impl WebGpuGeometryExactCoverConnected {
    pub fn solution_graph(
        &self,
    ) -> &crate::geometry_exact_cover_result::WebGpuGeometrySolutionGraph {
        &self.solution_graph
    }

    pub fn into_solution_graph(
        self,
    ) -> crate::geometry_exact_cover_result::WebGpuGeometrySolutionGraph {
        self.solution_graph
    }

    pub fn shader_version(&self) -> &'static str {
        self.shader_version
    }

    pub fn shader_hash(&self) -> &str {
        &self.shader_hash
    }

    pub fn peak_gpu_bytes(&self) -> u64 {
        self.peak_gpu_bytes
    }

    pub fn peak_host_reduce_bytes(&self) -> usize {
        self.peak_host_reduce_bytes
    }

    pub fn timings(&self) -> crate::geometry_exact_cover_timing::WebGpuGeometryExactCoverTimings {
        self.timings
    }

    pub fn trust_state(&self) -> WebGpuPackingTrustState {
        self.trust_state
    }

    pub fn can_claim_exact(&self) -> bool {
        self.trust_state == WebGpuPackingTrustState::TrustedCpuSampleConfirmed
            && self.cpu_confirmed_dispatches != 0
            && self.cpu_confirmed_parents != 0
    }

    pub const fn cpu_confirmed_dispatches(&self) -> u32 {
        self.cpu_confirmed_dispatches
    }

    pub const fn cpu_confirmed_parents(&self) -> u32 {
        self.cpu_confirmed_parents
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebGpuGeometryExactCoverIncomplete {
    pub(crate) generated_state_count: u32,
    pub(crate) capacity: u32,
}

impl WebGpuGeometryExactCoverIncomplete {
    pub fn generated_state_count(&self) -> u32 {
        self.generated_state_count
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebGpuCpuReferenceMismatchKind {
    BufferShape,
    EdgeCount,
    OperationIndex,
    ChildState,
}

impl WebGpuCpuReferenceMismatchKind {
    pub const fn diagnostic_reason(self) -> &'static str {
        match self {
            Self::BufferShape => "webgpu_trust_mismatch_buffer_shape",
            Self::EdgeCount => "webgpu_trust_mismatch_edge_count",
            Self::OperationIndex => "webgpu_trust_mismatch_operation_index",
            Self::ChildState => "webgpu_trust_mismatch_child_state",
        }
    }
}

#[derive(Debug)]
pub enum WebGpuGeometryExactCoverOutcome {
    Connected(WebGpuGeometryExactCoverConnected),
    Unavailable(WebGpuUnavailableResult),
    Cancelled,
    ResourceIncomplete(WebGpuGeometryExactCoverIncomplete),
    RejectedInvalidResult {
        candidate_index: usize,
    },
    RejectedTrustMismatch {
        parent_index: u32,
        mismatch_kind: WebGpuCpuReferenceMismatchKind,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WebGpuGeometryExactCoverInputError {
    InvalidBoard,
    InvalidBoardMask,
    InvalidPieceCounts,
    EmptyOperationTable,
    StaticBufferCache,
    LayerScratch,
    InvalidOperation,
    IncompatibleBatchFamily,
    CapacityOverflow,
    DimensionOverflow,
    DevicePoll,
    ReadbackFailed,
    ReadbackAlignment,
    CatalogValidationAllocation,
}

impl fmt::Display for WebGpuGeometryExactCoverInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for WebGpuGeometryExactCoverInputError {}
