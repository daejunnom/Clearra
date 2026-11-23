//! Exact cell images and inverse filters for joint placement/source families.
//!
//! Placement IDs remain the identity in the ZDD. A cell assignment is the OR
//! of their original footprints and the initial board, including overlaps.
//! Source suffixes are projected only for the movement domain; filtering keeps
//! the original suffix. Neither operation enumerates boards or complete sets.

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use clearra_core_domain::execution_cancellation::ExecutionControl;

use super::{
    geometry_partial_family::{ZddArena, ZddId},
    symbolic_boolean::{BooleanArena, BooleanId},
    symbolic_family::SymbolicFamilyError,
};

const WORK_PROGRESS_MASK: u64 = (1 << 18) - 1;

pub(super) struct CellFamilyBridge {
    row_cells: Vec<u64>,
    row_count: u32,
    initial: u64,
    area: u32,
    all_cells: u64,
    max_work: u64,
    max_memo: usize,
    work: u64,
    support_owner: Weak<()>,
    support_epoch: Option<u64>,
    support: HashMap<ZddId, u64>,
    support_work: u64,
}

enum RowTask {
    Visit(ZddId),
    Finish(ZddId),
}

enum FilterTask {
    Visit(ZddId, BooleanId),
    Finish((ZddId, BooleanId), u32),
}

impl CellFamilyBridge {
    pub fn new(
        row_cells: Vec<u64>,
        initial: u64,
        area: u32,
        max_work: u64,
        max_memo: usize,
    ) -> Result<Self, SymbolicFamilyError> {
        if area > 64 {
            return Err(SymbolicFamilyError::Unsupported("symbolic_cell_area"));
        }
        let all_cells = if area == 64 {
            u64::MAX
        } else {
            (1_u64 << area) - 1
        };
        if initial & !all_cells != 0 || row_cells.iter().any(|&cells| cells & !all_cells != 0) {
            return Err(SymbolicFamilyError::InvalidProblem("symbolic_cell_mask"));
        }
        let row_count = u32::try_from(row_cells.len())
            .map_err(|_| SymbolicFamilyError::InvalidProblem("symbolic_cell_row_count"))?;
        if row_count == u32::MAX {
            return Err(SymbolicFamilyError::InvalidProblem(
                "symbolic_cell_row_count",
            ));
        }
        Ok(Self {
            row_cells,
            row_count,
            initial,
            area,
            all_cells,
            max_work,
            max_memo,
            work: 0,
            support_owner: Weak::new(),
            support_epoch: None,
            support: HashMap::new(),
            support_work: 0,
        })
    }

    pub const fn work_steps(&self) -> u64 {
        self.work
    }

    pub const fn support_work_steps(&self) -> u64 {
        self.support_work
    }

    pub fn support_cache_entries(&self) -> usize {
        self.support.len()
    }

    /// Release suffix support before a rank boundary or a ZDD compaction.
    /// Work remains cumulative. No predicate/cofactor memo survives a call.
    pub fn clear_support_cache(&mut self) {
        self.support = HashMap::new();
        self.support_owner = Weak::new();
        self.support_epoch = None;
    }

    /// The characteristic function of all occupancy unions in this family.
    /// Multiple identities or sources with the same occupancy yield one board.
    /// Memo IDs belong only to this call, so a later owner/epoch cannot alias.
    pub fn domain(
        &mut self,
        sets: &ZddArena,
        predicates: &mut BooleanArena,
        root: ZddId,
        control: &ExecutionControl,
    ) -> Result<BooleanId, SymbolicFamilyError> {
        self.check_arena(predicates, control)?;
        self.bind_support_owner(sets);
        let mut memo = HashMap::new();
        let mut initial_cube = None;
        let mut tasks = Vec::new();
        tasks.try_reserve(1).map_err(allocation)?;
        tasks.push(RowTask::Visit(root));
        while let Some(task) = tasks.pop() {
            self.tick(control)?;
            match task {
                RowTask::Visit(id) => {
                    if memo.contains_key(&id) {
                        continue;
                    }
                    let node = sets.node(id)?;
                    let terminal = if id == ZddId::EMPTY {
                        Some(BooleanId::FALSE)
                    } else if node.is_none_or(|node| node.variable >= self.row_count) {
                        let cube = if let Some(cube) = initial_cube {
                            cube
                        } else {
                            let cube = self.initial_cube(predicates, control)?;
                            initial_cube = Some(cube);
                            cube
                        };
                        Some(cube)
                    } else {
                        None
                    };
                    if let Some(value) = terminal {
                        Self::insert(self.max_memo, &mut memo, id, value)?;
                        continue;
                    }
                    let node = node.expect("nonterminal placement node");
                    tasks.try_reserve(3).map_err(allocation)?;
                    tasks.push(RowTask::Finish(id));
                    tasks.push(RowTask::Visit(node.high));
                    tasks.push(RowTask::Visit(node.low));
                }
                RowTask::Finish(id) => {
                    let node = sets.node(id)?.expect("visited placement node");
                    let high = predicates.force_cells_true(
                        memo[&node.high],
                        self.row_cells[node.variable as usize],
                    )?;
                    let value = predicates.or(memo[&node.low], high)?;
                    Self::insert(self.max_memo, &mut memo, id, value)?;
                }
            }
        }
        Ok(memo[&root])
    }

    /// Keep exactly the original (placement set, source) pairs whose occupancy
    /// satisfies `condition`. Complete cofactors carry prior true cells in the
    /// residual function, so the product key needs no pending-cell mask.
    ///
    /// The Boolean owner must remain writable for new residual predicates.
    /// No compaction or rewind may occur inside either arena during this call.
    pub fn filter(
        &mut self,
        sets: &mut ZddArena,
        predicates: &mut BooleanArena,
        root: ZddId,
        condition: BooleanId,
        control: &ExecutionControl,
    ) -> Result<ZddId, SymbolicFamilyError> {
        self.check_arena(predicates, control)?;
        self.bind_support_owner(sets);
        sets.node(root)?;
        predicates.validate(condition)?;
        self.tick(control)?;
        if root == ZddId::EMPTY || condition == BooleanId::FALSE {
            return Ok(ZddId::EMPTY);
        }
        if condition == BooleanId::TRUE {
            return Ok(root);
        }
        let condition = predicates.cofactor_cells(condition, self.initial, 0)?;
        if condition == BooleanId::FALSE {
            return Ok(ZddId::EMPTY);
        }
        if condition == BooleanId::TRUE {
            return Ok(root);
        }
        self.ensure_support(sets, root, control)?;
        let condition = predicates.cofactor_cells(
            condition,
            0,
            self.all_cells & !(self.initial | self.support[&root]),
        )?;
        let mut memo = HashMap::new();
        let mut tasks = Vec::new();
        let mut values = Vec::new();
        tasks.try_reserve(1).map_err(allocation)?;
        tasks.push(FilterTask::Visit(root, condition));
        while let Some(task) = tasks.pop() {
            self.tick(control)?;
            match task {
                FilterTask::Visit(joint, predicate) => {
                    let key = (joint, predicate);
                    let terminal = if joint == ZddId::EMPTY || predicate == BooleanId::FALSE {
                        Some(ZddId::EMPTY)
                    } else if predicate == BooleanId::TRUE {
                        Some(joint)
                    } else {
                        memo.get(&key).copied()
                    };
                    if let Some(value) = terminal {
                        values.try_reserve(1).map_err(allocation)?;
                        values.push(value);
                        continue;
                    }
                    let node = sets.node(joint)?;
                    if node.is_none_or(|node| node.variable >= self.row_count) {
                        // Under Vars(predicate) <= support(joint), the source
                        // boundary is already constant. Flush all cells as a
                        // complete check and preserve the original source root.
                        let final_value =
                            predicates.cofactor_cells(predicate, 0, self.all_cells)?;
                        let value = match final_value {
                            BooleanId::FALSE => ZddId::EMPTY,
                            BooleanId::TRUE => joint,
                            _ => {
                                return Err(SymbolicFamilyError::InvalidProblem(
                                    "symbolic_cell_filter_residual",
                                ));
                            }
                        };
                        values.try_reserve(1).map_err(allocation)?;
                        values.push(value);
                        continue;
                    }
                    let node = node.expect("placement branch checked");
                    let cells = self.row_cells[node.variable as usize];
                    let possible = self.support[&joint];
                    let low = predicates.cofactor_cells(
                        predicate,
                        0,
                        possible & !self.support[&node.low],
                    )?;
                    // The true and false masks are disjoint. Previously true
                    // variables have already disappeared from this residual.
                    let high = predicates.cofactor_cells(
                        predicate,
                        cells,
                        possible & !(cells | self.support[&node.high]),
                    )?;
                    tasks.try_reserve(3).map_err(allocation)?;
                    tasks.push(FilterTask::Finish(key, node.variable));
                    tasks.push(FilterTask::Visit(node.high, high));
                    tasks.push(FilterTask::Visit(node.low, low));
                }
                FilterTask::Finish(key, variable) => {
                    let high = values.pop().expect("high child evaluated");
                    let low = values.pop().expect("low child evaluated");
                    let value = sets.mk(variable, low, high)?;
                    Self::insert(self.max_memo, &mut memo, key, value)?;
                    values.push(value);
                }
            }
        }
        values.pop().ok_or(SymbolicFamilyError::InvalidProblem(
            "symbolic_cell_filter_result",
        ))
    }

    /// Exact union of possible suffix footprints. This is a nodewise fold,
    /// never a walk over selected-set prefixes. Source-only support is empty.
    /// Existing nodes are immutable within one owner/epoch, so every legal-row
    /// filter can reuse their support even when the ZDD gains new result nodes.
    fn ensure_support(
        &mut self,
        sets: &ZddArena,
        root: ZddId,
        control: &ExecutionControl,
    ) -> Result<(), SymbolicFamilyError> {
        if self.support.contains_key(&root) {
            return Ok(());
        }
        let mut tasks = Vec::new();
        tasks.try_reserve(1).map_err(allocation)?;
        tasks.push(RowTask::Visit(root));
        while let Some(task) = tasks.pop() {
            self.tick_support(control)?;
            match task {
                RowTask::Visit(id) => {
                    if self.support.contains_key(&id) {
                        continue;
                    }
                    let node = sets.node(id)?;
                    if node.is_none_or(|node| node.variable >= self.row_count) {
                        Self::insert(self.max_memo, &mut self.support, id, 0)?;
                        continue;
                    }
                    let node = node.expect("placement branch checked");
                    tasks.try_reserve(3).map_err(allocation)?;
                    tasks.push(RowTask::Finish(id));
                    tasks.push(RowTask::Visit(node.high));
                    tasks.push(RowTask::Visit(node.low));
                }
                RowTask::Finish(id) => {
                    let node = sets.node(id)?.expect("visited placement node");
                    // Canonical ZDD nodes always have a nonempty high branch.
                    let cells = self.support[&node.low]
                        | self.support[&node.high]
                        | self.row_cells[node.variable as usize];
                    Self::insert(self.max_memo, &mut self.support, id, cells)?;
                }
            }
        }
        Ok(())
    }

    fn bind_support_owner(&mut self, sets: &ZddArena) {
        let owner = Arc::downgrade(sets.owner_token());
        if self.support_epoch != Some(sets.epoch()) || !self.support_owner.ptr_eq(&owner) {
            self.support = HashMap::new();
            self.support_owner = owner;
            self.support_epoch = Some(sets.epoch());
        }
    }

    fn initial_cube(
        &mut self,
        predicates: &mut BooleanArena,
        control: &ExecutionControl,
    ) -> Result<BooleanId, SymbolicFamilyError> {
        let mut cube = BooleanId::TRUE;
        for variable in (0..self.area).rev() {
            self.tick(control)?;
            cube = if self.initial & (1_u64 << variable) == 0 {
                predicates.mk(variable, cube, BooleanId::FALSE)?
            } else {
                predicates.mk(variable, BooleanId::FALSE, cube)?
            };
        }
        Ok(cube)
    }

    fn check_arena(
        &self,
        predicates: &BooleanArena,
        control: &ExecutionControl,
    ) -> Result<(), SymbolicFamilyError> {
        if control.is_cancelled() {
            return Err(SymbolicFamilyError::Cancelled);
        }
        if predicates.variable_count() != self.area {
            return Err(SymbolicFamilyError::InvalidProblem(
                "symbolic_cell_arena_dimensions",
            ));
        }
        Ok(())
    }

    fn insert<K: Eq + core::hash::Hash, V>(
        max_memo: usize,
        memo: &mut HashMap<K, V>,
        key: K,
        value: V,
    ) -> Result<(), SymbolicFamilyError> {
        if let Some(previous) = memo.get_mut(&key) {
            *previous = value;
            return Ok(());
        }
        if max_memo != 0 && memo.len() >= max_memo {
            return Err(SymbolicFamilyError::Resource(
                "symbolic_cell_bridge_memo_limit",
            ));
        }
        memo.try_reserve(1).map_err(allocation)?;
        memo.insert(key, value);
        Ok(())
    }

    fn tick_support(&mut self, control: &ExecutionControl) -> Result<(), SymbolicFamilyError> {
        self.tick_work::<true>(control)
    }

    fn tick(&mut self, control: &ExecutionControl) -> Result<(), SymbolicFamilyError> {
        self.tick_work::<false>(control)
    }

    fn tick_work<const SUPPORT: bool>(
        &mut self,
        control: &ExecutionControl,
    ) -> Result<(), SymbolicFamilyError> {
        if self.work & 255 == 0 && control.is_cancelled() {
            return Err(SymbolicFamilyError::Cancelled);
        }
        if self.max_work != 0 && self.work >= self.max_work {
            return Err(SymbolicFamilyError::Resource(
                "symbolic_cell_bridge_work_limit",
            ));
        }
        let work = self
            .work
            .checked_add(1)
            .ok_or(SymbolicFamilyError::Resource(
                "symbolic_cell_bridge_work_overflow",
            ))?;
        if SUPPORT {
            self.support_work =
                self.support_work
                    .checked_add(1)
                    .ok_or(SymbolicFamilyError::Resource(
                        "symbolic_cell_support_work_overflow",
                    ))?;
        }
        self.work = work;
        if self.work & WORK_PROGRESS_MASK == 0 {
            control.report_progress("symbolic_work_bridge", self.work, None);
            control.report_progress("symbolic_cell_support_work", self.support_work, None);
            control.report_progress(
                "symbolic_cell_support_entries",
                self.support.len() as u64,
                None,
            );
        }
        Ok(())
    }
}

fn allocation(_: std::collections::TryReserveError) -> SymbolicFamilyError {
    SymbolicFamilyError::Resource("symbolic_cell_bridge_allocation")
}

#[cfg(test)]
#[path = "symbolic_cell_bridge_tests.rs"]
mod tests;
