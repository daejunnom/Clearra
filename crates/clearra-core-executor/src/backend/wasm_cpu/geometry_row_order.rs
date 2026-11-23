//! An optional, request-owned negative certificate for the complete family.
//!
//! For each of the 24 orders of original rows 1..=4, a bit records whether
//! some path has a compatible positive ILC realization for every operation.
//! The same order is used by both operands of a Product. A nonzero mask is
//! only optimistic feasibility; movement and the exact queue verifier retain
//! all positive authority.
//!
//! One unused bit also records whether any represented operation can be the
//! first grounded lock on an empty board. Append and Product require a common
//! row order, but either operand may supply that first lock. Its absence is a
//! negative certificate only for an empty initial board.

use std::sync::{Arc, Weak};

use super::{FamilyNodeKind, GeometryCatalog, GeometrySolutionFamily};

pub(super) const ROW_ORDER_ALL: u32 = (1 << 24) - 1;
const FIRST_LOCK_POSSIBLE: u32 = 1 << 24;
// Row-only metadata: the selected/family combiners do not propagate this bit.
const FRAME_ZERO_AVAILABLE: u32 = 1 << 25;
const INTERNAL_ROWS: u16 = 0b01_1110;

// Bits are the lexicographic permutations of [1, 2, 3, 4].
const ROW_ORDERS: [[u8; 4]; 24] = [
    [1, 2, 3, 4],
    [1, 2, 4, 3],
    [1, 3, 2, 4],
    [1, 3, 4, 2],
    [1, 4, 2, 3],
    [1, 4, 3, 2],
    [2, 1, 3, 4],
    [2, 1, 4, 3],
    [2, 3, 1, 4],
    [2, 3, 4, 1],
    [2, 4, 1, 3],
    [2, 4, 3, 1],
    [3, 1, 2, 4],
    [3, 1, 4, 2],
    [3, 2, 1, 4],
    [3, 2, 4, 1],
    [3, 4, 1, 2],
    [3, 4, 2, 1],
    [4, 1, 2, 3],
    [4, 1, 3, 2],
    [4, 2, 1, 3],
    [4, 2, 3, 1],
    [4, 3, 1, 2],
    [4, 3, 2, 1],
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CatalogBinding {
    identity: u64,
    initial: u64,
    required: u64,
    width: u8,
    height: u8,
}

impl CatalogBinding {
    fn of(catalog: &GeometryCatalog) -> Self {
        Self {
            identity: catalog.identity_digest(),
            initial: catalog.initial_board(),
            required: catalog.required_cells(),
            width: catalog.width(),
            height: catalog.height(),
        }
    }
}

#[derive(Debug)]
pub(super) struct RowOrderCapability {
    binding: CatalogBinding,
    row_states: Vec<u32>,
}

impl RowOrderCapability {
    pub(super) fn try_compile(catalog: &GeometryCatalog) -> Option<Self> {
        if catalog.height() != 6 || catalog.width() == 0 {
            return None;
        }
        let cells = u32::from(catalog.width()).checked_mul(6)?;
        if cells > 64 {
            return None;
        }
        let full = u64::MAX >> (64 - cells);
        if catalog.initial_board() | catalog.required_cells() != full {
            return None;
        }

        let mut row_states = Vec::new();
        row_states
            .try_reserve_exact(catalog.skeleton_count())
            .ok()?;
        for row in 0..catalog.skeleton_count() {
            let row = u32::try_from(row).ok()?;
            // This authenticates every concrete alternative. No selected
            // representative, forbidden-clear domain, or hash grants this
            // capability. Rows 0 and 5 can then be precedence sinks.
            if catalog
                .realizations(row)
                .iter()
                .any(|realization| realization.required_deleted_rows & !INTERNAL_ROWS != 0)
            {
                return None;
            }
            let occupied = catalog.skeleton_occupied_rows(row);
            let mask = mask_for_domain(occupied, |deleted| {
                // Reuse the catalog's already compiled OR of all positive
                // realization requirements, without instantiating a pose.
                catalog.realization_requirement_is_satisfied(row, deleted)
            })?;
            // On an empty board a valid pose at the floor has an unobstructed
            // vertical drop. Check the exact frame-zero projection without
            // warming the catalog's optional lazy instantiation cache. For a
            // nonempty board this certificate stays unknown; a failed drop
            // must not rule out a lock reached through a rotation or slide.
            let frame_zero = catalog.has_frame_zero_realization(row);
            let first_lock = catalog.initial_board() == 0 && occupied & 1 != 0 && frame_zero;
            row_states.push(
                mask | if first_lock { FIRST_LOCK_POSSIBLE } else { 0 }
                    | if frame_zero { FRAME_ZERO_AVAILABLE } else { 0 },
            );
        }
        Some(Self {
            binding: CatalogBinding::of(catalog),
            row_states,
        })
    }

    fn row_state(&self, row: u32) -> Option<u32> {
        self.row_states.get(row as usize).copied()
    }

    fn selected_state(&self, rows: &[u32]) -> Option<u32> {
        rows.iter().try_fold(ROW_ORDER_ALL, |state, &row| {
            Some(combine_selected(state, self.row_state(row)?))
        })
    }

    #[cfg(test)]
    pub(super) fn row_mask(&self, row: u32) -> Option<u32> {
        Some(self.row_state(row)? & ROW_ORDER_ALL)
    }

    #[cfg(test)]
    pub(super) fn selected_mask(&self, rows: &[u32]) -> Option<u32> {
        Some(self.selected_state(rows)? & ROW_ORDER_ALL)
    }

    fn retained_bytes(&self) -> usize {
        self.row_states.capacity() * core::mem::size_of::<u32>()
    }
}

fn combine_selected(left: u32, right: u32) -> u32 {
    (left & right & ROW_ORDER_ALL) | ((left | right) & FIRST_LOCK_POSSIBLE)
}

fn mask_for_domain(
    occupied: u16,
    mut requirement_is_satisfied: impl FnMut(u16) -> bool,
) -> Option<u32> {
    if occupied & !0b11_1111 != 0 {
        return None;
    }
    let mut mask = 0_u32;
    for (bit, order) in ROW_ORDERS.iter().enumerate() {
        let mut deleted = 0_u16;
        for &row in order {
            if occupied & (1 << row) != 0 {
                break;
            }
            deleted |= 1 << row;
        }
        if requirement_is_satisfied(deleted) {
            mask |= 1 << bit;
        }
    }
    Some(mask)
}

#[cfg(test)]
pub(super) fn mask_for_requirement_alternatives(
    occupied: u16,
    alternatives: &[u16],
) -> Option<u32> {
    if alternatives
        .iter()
        .any(|required| required & !INTERNAL_ROWS != 0)
    {
        return None;
    }
    mask_for_domain(occupied, |deleted| {
        alternatives.iter().any(|required| required & !deleted == 0)
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RowOrderBuildAdvance {
    Pending,
    Complete,
    Unavailable,
}

#[derive(Debug)]
pub(super) struct FamilyRowOrderBuilder {
    family: Weak<GeometrySolutionFamily>,
    node_count: u32,
    capability: RowOrderCapability,
    values: Vec<u32>,
}

impl FamilyRowOrderBuilder {
    pub(super) fn try_new(
        catalog: &GeometryCatalog,
        family: &Arc<GeometrySolutionFamily>,
        max_additional_bytes: Option<u128>,
    ) -> Option<Self> {
        let entries = (family.node_count() as usize).checked_add(2)?;
        let requested = entries
            .checked_add(catalog.skeleton_count())?
            .checked_mul(core::mem::size_of::<u32>())?
            .checked_add(FamilyRowOrderTable::owner_bytes())?;
        if max_additional_bytes.is_some_and(|limit| requested as u128 > limit) {
            return None;
        }
        let capability = RowOrderCapability::try_compile(catalog)?;
        let mut values = Vec::new();
        values.try_reserve_exact(entries).ok()?;
        values.push(0); // FAMILY_INVALID: no path.
        values.push(ROW_ORDER_ALL); // FAMILY_EMPTY: the empty path.
        let builder = Self {
            family: Arc::downgrade(family),
            node_count: family.node_count(),
            capability,
            values,
        };
        if max_additional_bytes.is_some_and(|limit| {
            builder.retained_bytes() as u128 + FamilyRowOrderTable::owner_bytes() as u128 > limit
        }) {
            return None;
        }
        Some(builder)
    }

    pub(super) fn advance(
        &mut self,
        family: &Arc<GeometrySolutionFamily>,
        work_quantum: usize,
    ) -> RowOrderBuildAdvance {
        if self.family.as_ptr() != Arc::as_ptr(family) || self.node_count != family.node_count() {
            return RowOrderBuildAdvance::Unavailable;
        }
        let entries = self.node_count as usize + 2;
        let end = self
            .values
            .len()
            .saturating_add(work_quantum.max(1))
            .min(entries);
        while self.values.len() < end {
            let reference = self.values.len() as u32;
            let Some(node) = family.node(reference) else {
                return RowOrderBuildAdvance::Unavailable;
            };
            let Some(left) = self.values.get(node.left as usize).copied() else {
                return RowOrderBuildAdvance::Unavailable;
            };
            let value = match node.kind {
                FamilyNodeKind::Append => {
                    let Some(row) = self.capability.row_state(node.row_id) else {
                        return RowOrderBuildAdvance::Unavailable;
                    };
                    combine_selected(left, row)
                }
                FamilyNodeKind::Union | FamilyNodeKind::Product => {
                    let Some(right) = self.values.get(node.right as usize).copied() else {
                        return RowOrderBuildAdvance::Unavailable;
                    };
                    if node.kind == FamilyNodeKind::Union {
                        left | right
                    } else {
                        combine_selected(left, right)
                    }
                }
            };
            self.values.push(value);
        }
        if self.values.len() == entries {
            RowOrderBuildAdvance::Complete
        } else {
            RowOrderBuildAdvance::Pending
        }
    }

    pub(super) fn retained_bytes(&self) -> usize {
        self.capability.retained_bytes() + self.values.capacity() * core::mem::size_of::<u32>()
    }

    pub(super) fn completed_nodes(&self) -> usize {
        self.values.len().saturating_sub(2)
    }

    pub(super) fn finish(self) -> Option<FamilyRowOrderTable> {
        if self.values.len() != self.node_count as usize + 2 {
            return None;
        }
        Some(FamilyRowOrderTable {
            family: self.family,
            capability: self.capability,
            values: self.values,
        })
    }
}

#[derive(Debug)]
pub(super) struct FamilyRowOrderTable {
    // A Weak binding preserves the serial family's strong-count accounting
    // and prevents this table from acquiring a second payload owner.
    // This private table is created from that frozen family's immutable
    // session catalog and travels only with the same family through splits.
    // CatalogBinding is a defensive mismatch check, not a hash-only grant
    // for reuse across catalogs or a process-global cache.
    family: Weak<GeometrySolutionFamily>,
    capability: RowOrderCapability,
    values: Vec<u32>,
}

impl FamilyRowOrderTable {
    pub(super) const fn owner_bytes() -> usize {
        // One Arc control block; the large Vec allocation is moved intact.
        core::mem::size_of::<Self>() + 2 * core::mem::size_of::<usize>()
    }

    pub(super) fn retained_bytes(&self) -> usize {
        Self::owner_bytes()
            + self.capability.retained_bytes()
            + self.values.capacity() * core::mem::size_of::<u32>()
    }

    pub(super) fn matches(
        &self,
        catalog: &GeometryCatalog,
        family: &Arc<GeometrySolutionFamily>,
    ) -> bool {
        self.family.as_ptr() == Arc::as_ptr(family)
            && self.capability.binding == CatalogBinding::of(catalog)
            && self.values.len() == family.node_count() as usize + 2
            && self.capability.row_states.len() == catalog.skeleton_count()
    }

    #[cfg(test)]
    pub(super) fn mask(&self, reference: u32) -> Option<u32> {
        Some(*self.values.get(reference as usize)? & ROW_ORDER_ALL)
    }

    #[cfg(test)]
    pub(super) fn row_mask(&self, row: u32) -> Option<u32> {
        self.capability.row_mask(row)
    }

    #[cfg(test)]
    pub(super) fn selected_mask(&self, rows: &[u32]) -> Option<u32> {
        self.capability.selected_mask(rows)
    }

    pub(super) fn prefix_state(&self, rows: &[u32]) -> Option<u32> {
        self.capability.selected_state(rows)
    }

    pub(super) fn frame_zero_is_available(&self, row: u32) -> Option<bool> {
        Some(self.capability.row_state(row)? & FRAME_ZERO_AVAILABLE != 0)
    }

    pub(super) fn extend_prefix(&self, prefix_state: u32, row: u32) -> Option<u32> {
        Some(combine_selected(
            prefix_state,
            self.capability.row_state(row)?,
        ))
    }

    pub(super) fn task_mask(
        &self,
        prefix_state: u32,
        family: u32,
        continuations: &[u32],
    ) -> Option<u32> {
        let mut state = combine_selected(prefix_state, *self.values.get(family as usize)?);
        for &continuation in continuations {
            state = combine_selected(state, *self.values.get(continuation as usize)?);
        }
        if self.capability.binding.initial == 0 && state & FIRST_LOCK_POSSIBLE == 0 {
            return Some(0);
        }
        // The first-lock bit never grants row-order or positive authority.
        Some(state & ROW_ORDER_ALL)
    }
}
