//! Rust exact cover over generic Board64 target frames, distinct from physical
//! BuildUp reachability. Product PC keeps its specialized geometry scheduler.

use core::{mem::size_of, ops::ControlFlow};

use clearra_core_domain::{
    execution_cancellation::ExecutionCancellationToken,
    piece::{piece_kind::PieceKind, rotation::RotationState},
    resource::{ResourceReport, ResourceTruncationReason},
};
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_supply::PieceMultisetKey;

use super::{
    build_clear_rows::ClearRowDomain,
    catalog::{
        checked_realization_capacity, compile_inverse_clear_realizations,
        instantiate_realization_raw, Realization,
    },
    piece_index,
};

const MAX_PIECES: usize = 15;
const FAMILY_WORDS: usize = 4;
const FILL_PER_PLACEMENT: u32 = 4;

#[derive(Clone, Copy, Debug)]
pub struct GeometryRequest<'a> {
    pub layout: Board64Layout,
    pub initial: u64,
    pub region: u64,
    /// May include already occupied region cells. New pieces exactly cover
    /// the residual required cells, not the optional cells in the region.
    pub required: u64,
    pub forbidden: u64,
    pub max_pieces: u8,
    pub exact_pieces: Option<u8>,
    /// Componentwise supply upper bound.
    pub inventory: PieceMultisetKey,
    /// A supplied family is exact, even when explicitly empty.
    pub exact_family: Option<&'a [PieceMultisetKey]>,
    pub terminal: ProjectedTerminal,
    pub selection: CoverSelection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectedTerminal {
    Any,
    BoardAfterLineClears(u64),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoverSelection {
    First,
    All,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GeometryLimits {
    pub nodes: Option<usize>,
    pub candidates: Option<usize>,
    /// Bounds engine-owned allocation payloads. External retention belongs
    /// to the consumer; this is not an OS memory or stack limit.
    pub engine_bytes: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeometryError {
    HeightOutsideCompactDomain,
    InvalidMasks,
    InvalidPieceWindow,
    InvalidExactFamily,
    PieceCountOutsideCompactDomain,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeometryEnd {
    Complete,
    FirstSolution,
    Cancelled,
    SinkStopped,
    Resource(ResourceTruncationReason),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeometryReceipt {
    pub end: GeometryEnd,
    pub expanded_nodes: usize,
    pub emitted: usize,
    /// Conservative preallocation bound, not an observed peak.
    pub engine_bytes_upper_bound: Option<usize>,
    pub resource: ResourceReport,
}

impl GeometryReceipt {
    fn new() -> Self {
        let mut resource = ResourceReport::complete();
        resource.probability_complete = false;
        Self {
            end: GeometryEnd::Complete,
            expanded_nodes: 0,
            emitted: 0,
            engine_bytes_upper_bound: None,
            resource,
        }
    }

    fn finish(mut self, end: GeometryEnd) -> Self {
        self.end = end;
        match end {
            GeometryEnd::Complete => (),
            GeometryEnd::Resource(reason) => self.resource.mark_truncated(reason),
            GeometryEnd::Cancelled | GeometryEnd::FirstSolution | GeometryEnd::SinkStopped => {
                self.resource.mark_truncated_unknown();
            }
        }
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeometryRealization {
    pub piece: PieceKind,
    pub target_cells: u64,
    pub minimum_deleted_rows: u16,
    pub rotation: RotationState,
    pub target_x: i8,
    pub target_anchor_y: i8,
}

impl GeometryRealization {
    fn from_catalog(value: Realization) -> Self {
        Self {
            piece: value.piece,
            target_cells: value.cells,
            minimum_deleted_rows: value.required_deleted_rows,
            rotation: value.rotation,
            target_x: value.x,
            target_anchor_y: value.target_anchor_y,
        }
    }

    /// Resolves a template against the complete preceding clear state.
    /// The returned lock has not been tested against occupancy or movement.
    pub fn instantiate(self, layout: Board64Layout, deleted_rows: u16) -> Option<GeometryLock> {
        let width = layout.width() as u8;
        let height = layout.height();
        if height > 16
            || u32::from(deleted_rows) >> height != 0
            || self.target_cells & !layout.all_cells_mask() != 0
            || self.target_cells.count_ones() != FILL_PER_PLACEMENT
            || !(0..width as i8).contains(&self.target_x)
            || !(0..height as i8).contains(&self.target_anchor_y)
        {
            return None;
        }
        let realization = Realization {
            piece: self.piece,
            cells: self.target_cells,
            required_deleted_rows: self.minimum_deleted_rows,
            rotation: self.rotation,
            x: self.target_x,
            target_anchor_y: self.target_anchor_y,
        };
        let lock = instantiate_realization_raw(
            width,
            height as u8,
            self.target_cells,
            realization,
            deleted_rows,
        )?;
        Some(GeometryLock {
            mask: lock.lock_mask,
            rotation: lock.rotation,
            x: lock.x,
            y: lock.lock_y,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeometryLock {
    pub mask: u64,
    pub rotation: RotationState,
    pub x: i8,
    pub y: i8,
}

pub struct GeometryCover<'a> {
    realizations: &'a [Realization],
    rows: &'a [u32],
    pub projected_board: u64,
    pub projected_deleted_rows: u16,
}

impl GeometryCover<'_> {
    /// Canonical within a cover. Alternative rotations/inverse rows are
    /// available through `realizations`, not duplicate covers.
    pub fn representatives(&self) -> impl ExactSizeIterator<Item = GeometryRealization> + '_ {
        self.rows
            .iter()
            .map(|&row| GeometryRealization::from_catalog(self.realizations[row as usize]))
    }

    pub fn realizations(
        &self,
        placement_index: usize,
    ) -> Option<impl ExactSizeIterator<Item = GeometryRealization> + '_> {
        let index = *self.rows.get(placement_index)? as usize;
        let first = self.realizations[index];
        let suffix = &self.realizations[index..];
        let count = suffix.partition_point(|row| same_owner(first, *row));
        Some(
            suffix[..count]
                .iter()
                .copied()
                .map(GeometryRealization::from_catalog),
        )
    }
}

/// Streams exact covers for the declared frame and inventory. Stopping after
/// a prefix does not give the consumer complete-family authority. A native
/// host or an externally cancellable runtime can signal the token.
pub fn stream_geometry(
    request: GeometryRequest<'_>,
    limits: GeometryLimits,
    cancellation: &ExecutionCancellationToken,
    mut consume: impl FnMut(GeometryCover<'_>) -> ControlFlow<()>,
) -> Result<GeometryReceipt, GeometryError> {
    validate(request)?;
    let mut receipt = GeometryReceipt::new();
    if cancellation.is_cancelled() {
        return Ok(receipt.finish(GeometryEnd::Cancelled));
    }
    let required = request.required & !request.initial;
    let required_count = required.count_ones();
    if required_count == 0 || !required_count.is_multiple_of(FILL_PER_PLACEMENT) {
        // As in the C compact contract, do not emit a zero-placement row.
        return Ok(receipt);
    }
    let depth = (required_count / FILL_PER_PLACEMENT) as u8;
    if usize::from(depth) > MAX_PIECES {
        return Err(GeometryError::PieceCountOutsideCompactDomain);
    }
    if depth > request.max_pieces
        || depth > request.inventory.total_count()
        || request.exact_pieces.is_some_and(|exact| exact != depth)
        || request
            .exact_family
            .is_some_and(|family| !family.iter().any(|member| member.total_count() == depth))
    {
        return Ok(receipt);
    }
    let width = request.layout.width() as u8;
    let height = request.layout.height() as u8;
    let (projected_board, projected_deleted_rows) =
        project_clear(width, height, request.initial | required);
    if matches!(request.terminal, ProjectedTerminal::BoardAfterLineClears(expected) if expected != projected_board)
    {
        return Ok(receipt);
    }

    let Some((capacity, bound)) = checked_realization_capacity(width, height).and_then(|count| {
        count
            .checked_mul(size_of::<Realization>() + 4 * size_of::<u32>())?
            .checked_add(fixed_payload_bytes())
            .map(|bytes| (count, bytes))
    }) else {
        return Ok(receipt.finish(GeometryEnd::Resource(
            ResourceTruncationReason::MemoryExceeded,
        )));
    };
    receipt.engine_bytes_upper_bound = Some(bound);
    if limits.engine_bytes.is_some_and(|limit| bound > limit) {
        return Ok(receipt.finish(GeometryEnd::Resource(
            ResourceTruncationReason::MemoryExceeded,
        )));
    }
    let realizations = match compile_inverse_clear_realizations(
        width,
        height,
        request.initial,
        required,
        ClearRowDomain::unrestricted(),
        capacity,
    ) {
        Ok(realizations) => realizations,
        Err(_) => {
            return Ok(receipt.finish(GeometryEnd::Resource(
                ResourceTruncationReason::MemoryExceeded,
            )))
        }
    };
    let Some(support) = Support::new(&realizations) else {
        return Ok(receipt.finish(GeometryEnd::Resource(
            ResourceTruncationReason::MemoryExceeded,
        )));
    };
    receipt.resource.observe_cpu_bytes(
        fixed_payload_bytes()
            + realizations.capacity() * size_of::<Realization>()
            + support.rows.capacity() * size_of::<u32>(),
    );

    let family = Family::new(request.exact_family, depth);
    let mut search = Search {
        receipt,
        limits,
        cancellation,
        selection: request.selection,
        counts: [0; 7],
        inventory: request.inventory.counts(),
        family: &family,
        realizations: &realizations,
        support: &support,
        projected_board,
        projected_deleted_rows,
        path: [0; MAX_PIECES],
        consume: &mut consume,
    };
    let end = match search.visit(required, family.initial, 0) {
        ControlFlow::Continue(()) if cancellation.is_cancelled() => GeometryEnd::Cancelled,
        ControlFlow::Continue(()) => GeometryEnd::Complete,
        ControlFlow::Break(end) => end,
    };
    Ok(search.receipt.finish(end))
}

fn validate(request: GeometryRequest<'_>) -> Result<(), GeometryError> {
    if request.layout.height() > 16 {
        return Err(GeometryError::HeightOutsideCompactDomain);
    }
    let frame = request.layout.all_cells_mask();
    if request.region == 0
        || (request.initial | request.region | request.forbidden) & !frame != 0
        || request.required & !request.region != 0
        || request.forbidden & (request.initial | request.region) != 0
        || matches!(request.terminal, ProjectedTerminal::BoardAfterLineClears(board) if board & !frame != 0)
    {
        return Err(GeometryError::InvalidMasks);
    }
    let valid_depth = |depth| (1..=MAX_PIECES as u8).contains(&depth);
    let envelope = request.inventory.counts();
    if !valid_depth(request.max_pieces)
        || request
            .exact_pieces
            .is_some_and(|exact| !valid_depth(exact) || exact > request.max_pieces)
        || envelope
            .iter()
            .any(|&count| usize::from(count) > MAX_PIECES)
    {
        return Err(GeometryError::InvalidPieceWindow);
    }
    if request.exact_family.is_some_and(|family| {
        family.len() > FAMILY_WORDS * u64::BITS as usize
            || family.iter().any(|member| {
                !valid_depth(member.total_count())
                    || member
                        .counts()
                        .iter()
                        .zip(envelope)
                        .any(|(&count, limit)| count > limit)
            })
    }) {
        return Err(GeometryError::InvalidExactFamily);
    }
    Ok(())
}

fn project_clear(width: u8, height: u8, board: u64) -> (u64, u16) {
    let row_mask = u64::MAX >> (u64::BITS - u32::from(width));
    let mut projected = 0;
    let mut deleted = 0;
    let mut destination = 0;
    for source in 0..height {
        let row = (board >> (u32::from(source) * u32::from(width))) & row_mask;
        if row == row_mask {
            deleted |= 1 << source;
        } else {
            projected |= row << destination;
            destination += u32::from(width);
        }
    }
    (projected, deleted)
}

fn fixed_payload_bytes() -> usize {
    size_of::<Family>()
        + size_of::<Support>()
        + size_of::<Vec<Realization>>()
        + size_of::<Search<'_, fn(GeometryCover<'_>) -> ControlFlow<()>>>()
}

fn same_owner(left: Realization, right: Realization) -> bool {
    left.piece == right.piece && left.cells == right.cells
}

struct Support {
    offsets: [u32; 65],
    rows: Vec<u32>,
}

impl Support {
    fn new(realizations: &[Realization]) -> Option<Self> {
        let mut offsets = [0_u32; 65];
        let representative_rows = || {
            realizations
                .iter()
                .copied()
                .enumerate()
                .filter(|(index, row)| *index == 0 || !same_owner(realizations[*index - 1], *row))
        };
        for (_, row) in representative_rows() {
            for cell in cells(row.cells) {
                offsets[cell + 1] = offsets[cell + 1].checked_add(1)?;
            }
        }
        for cell in 0..64 {
            offsets[cell + 1] = offsets[cell + 1].checked_add(offsets[cell])?;
        }
        let row_count = offsets[64] as usize;
        let mut rows = Vec::new();
        rows.try_reserve_exact(row_count).ok()?;
        rows.resize(row_count, 0);
        let mut cursor = offsets;
        for (id, row) in representative_rows() {
            for cell in cells(row.cells) {
                rows[cursor[cell] as usize] = u32::try_from(id).ok()?;
                cursor[cell] += 1;
            }
        }
        Some(Self { offsets, rows })
    }

    fn at(&self, cell: usize) -> &[u32] {
        &self.rows[self.offsets[cell] as usize..self.offsets[cell + 1] as usize]
    }
}

fn cells(mut mask: u64) -> impl Iterator<Item = usize> {
    core::iter::from_fn(move || {
        if mask == 0 {
            return None;
        }
        let cell = mask.trailing_zeros() as usize;
        mask &= mask - 1;
        Some(cell)
    })
}

type FamilyMask = [u64; FAMILY_WORDS];

struct Family {
    constrained: bool,
    initial: FamilyMask,
    at_least: [[FamilyMask; MAX_PIECES + 1]; 7],
}

impl Family {
    fn new(members: Option<&[PieceMultisetKey]>, depth: u8) -> Self {
        let mut family = Self {
            constrained: members.is_some(),
            initial: [0; FAMILY_WORDS],
            at_least: [[[0; FAMILY_WORDS]; MAX_PIECES + 1]; 7],
        };
        for (index, member) in members
            .unwrap_or_default()
            .iter()
            .filter(|member| member.total_count() == depth)
            .enumerate()
        {
            let word = index / u64::BITS as usize;
            let bit = 1_u64 << (index % u64::BITS as usize);
            family.initial[word] |= bit;
            for (piece, maximum) in member.counts().into_iter().enumerate() {
                for count in 1..=usize::from(maximum) {
                    family.at_least[piece][count][word] |= bit;
                }
            }
        }
        family
    }

    fn restrict(&self, active: FamilyMask, piece: usize, count: u8) -> Option<FamilyMask> {
        if !self.constrained {
            return Some(active);
        }
        let bins = &self.at_least[piece][usize::from(count)];
        let next: FamilyMask = core::array::from_fn(|index| active[index] & bins[index]);
        // Members were admitted at the exact area-derived depth. For a leaf,
        // componentwise <= and equal totals imply equality (not merely a
        // match to a prefix of a larger family member).
        next.iter().any(|&word| word != 0).then_some(next)
    }
}

struct Search<'a, Sink> {
    receipt: GeometryReceipt,
    limits: GeometryLimits,
    cancellation: &'a ExecutionCancellationToken,
    selection: CoverSelection,
    counts: [u8; 7],
    inventory: [u8; 7],
    family: &'a Family,
    realizations: &'a [Realization],
    support: &'a Support,
    projected_board: u64,
    projected_deleted_rows: u16,
    path: [u32; MAX_PIECES],
    consume: &'a mut Sink,
}

impl<Sink: FnMut(GeometryCover<'_>) -> ControlFlow<()>> Search<'_, Sink> {
    fn visit(
        &mut self,
        remaining: u64,
        active: FamilyMask,
        depth: usize,
    ) -> ControlFlow<GeometryEnd> {
        if self.cancellation.is_cancelled() {
            return ControlFlow::Break(GeometryEnd::Cancelled);
        }
        if self.receipt.expanded_nodes >= self.limits.nodes.unwrap_or(usize::MAX) {
            return ControlFlow::Break(GeometryEnd::Resource(
                ResourceTruncationReason::FrontierBudgetExceeded,
            ));
        }
        self.receipt.expanded_nodes += 1;
        self.receipt.resource.observe_frontier_states(depth + 1);
        if remaining == 0 {
            return self.emit(depth);
        }
        // Every residual cover owns this pivot in exactly one disjoint row.
        // Branch only on those rows; rotations never partition its ownership
        // (the support table keeps one representative per piece + mask).
        let pivot = cells(remaining)
            .min_by_key(|&cell| self.support.at(cell).len())
            .unwrap_or(0);
        let support = self.support;
        for &id in support.at(pivot) {
            let row = self.realizations[id as usize];
            let piece = piece_index(row.piece);
            let count = self.counts[piece] + 1;
            if row.cells & !remaining != 0 || count > self.inventory[piece] {
                continue;
            }
            let Some(next) = self.family.restrict(active, piece, count) else {
                continue;
            };
            self.path[depth] = id;
            self.counts[piece] = count;
            self.visit(remaining & !row.cells, next, depth + 1)?;
            self.counts[piece] -= 1;
        }
        ControlFlow::Continue(())
    }

    fn emit(&mut self, depth: usize) -> ControlFlow<GeometryEnd> {
        if self.receipt.emitted >= self.limits.candidates.unwrap_or(usize::MAX) {
            return ControlFlow::Break(GeometryEnd::Resource(
                ResourceTruncationReason::CandidateBudgetExceeded,
            ));
        }
        let mut rows = self.path;
        rows[..depth].sort_unstable();
        let result = (self.consume)(GeometryCover {
            realizations: self.realizations,
            rows: &rows[..depth],
            projected_board: self.projected_board,
            projected_deleted_rows: self.projected_deleted_rows,
        });
        self.receipt.emitted += 1;
        self.receipt
            .resource
            .observe_candidate_rows(self.receipt.emitted);
        if self.selection == CoverSelection::First {
            ControlFlow::Break(GeometryEnd::FirstSolution)
        } else if result.is_break() {
            ControlFlow::Break(GeometryEnd::SinkStopped)
        } else {
            ControlFlow::Continue(())
        }
    }
}

#[cfg(test)]
#[path = "generic_geometry_tests.rs"]
mod tests;
