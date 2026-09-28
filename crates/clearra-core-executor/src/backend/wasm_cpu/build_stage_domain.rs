//! Queue-independent Build domains for stage composition. This is the SAME
//! inverse lock-clear catalog and APDP propagator as ordinary Build coverage.
//! A positive result is only a geometry possibility; actual reachability, hold,
//! scoring and inter-stage timing still belong to the caller's Build verifier.
use super::{
    catalog::GeometryCatalog,
    extended_board::ExtendedBoard,
    extended_geometry_domain::{ExtendedDomainResult, ExtendedDomainWorkspace},
    extended_inverse_catalog::ExtendedInverseCatalog,
    geometry_domain::{DomainPropagation, DomainStatus},
    piece_index,
};
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask, execution_cancellation::ExecutionControl,
};
use clearra_problem::BuildProbabilityField;
use std::{collections::HashMap, sync::Arc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildStageDomainError {
    InvalidField,
    Allocation,
    Cancelled,
}
#[allow(clippy::large_enum_variant)]
enum Catalog {
    Compact(GeometryCatalog),
    Extended(ExtendedInverseCatalog),
}

pub struct BuildStageDomain {
    catalog: Catalog,
    extended: ExtendedDomainWorkspace,
    memo: HashMap<([u64; 4], [u8; 7]), bool>,
    inventories: HashMap<([u64; 4], [u8; 7]), Arc<[[u8; 7]]>>,
}
impl BuildStageDomain {
    /// `base` may include other-stage FINAL cells as a relaxed geometry context.
    /// This admits every row that the combined target can complete. It must not
    /// be interpreted as proof that those context cells already exist in play.
    pub fn compile(
        field: BuildProbabilityField,
        control: &ExecutionControl,
    ) -> Result<Self, BuildStageDomainError> {
        if control.is_cancelled() {
            return Err(BuildStageDomainError::Cancelled);
        }
        let catalog = if field.height() <= 6 {
            Catalog::Compact(
                GeometryCatalog::compile_build_stage(
                    field.height(),
                    field.base().words()[0],
                    field.target().words()[0],
                )
                .map_err(|_| BuildStageDomainError::InvalidField)?,
            )
        } else {
            Catalog::Extended(
                ExtendedInverseCatalog::compile(field)
                    .map_err(|_| BuildStageDomainError::InvalidField)?,
            )
        };
        Ok(Self {
            catalog,
            extended: ExtendedDomainWorkspace::new(),
            memo: HashMap::new(),
            inventories: HashMap::new(),
        })
    }
    /// Upper bounds in IJLOSTZ order; a stage need not consume all of them.
    /// Only a negative answer may prune. No isolated-stage movement/B2B test is
    /// performed here, so an early block can still enable a combined-path spin.
    pub fn can_complete(
        &mut self,
        remaining: Board256Mask,
        upper: [u8; 7],
        control: &ExecutionControl,
    ) -> Result<bool, BuildStageDomainError> {
        self.complete(
            remaining.words(),
            [
                upper[0], upper[3], upper[5], upper[4], upper[6], upper[1], upper[2],
            ],
            control,
        )
    }
    pub fn skeleton_count(&self) -> usize {
        match &self.catalog {
            Catalog::Compact(c) => c.skeleton_count(),
            Catalog::Extended(c) => c.skeletons().len(),
        }
    }
    /// Every feasible residual piece-count vector, in public IJLOSTZ order.
    /// This is a geometric necessary condition only; no queue or reachability
    /// assumption is made. Alternative temporal skeletons remain admissible.
    pub fn completion_inventories(
        &mut self,
        remaining: Board256Mask,
        upper: [u8; 7],
        control: &ExecutionControl,
    ) -> Result<Arc<[[u8; 7]]>, BuildStageDomainError> {
        let caps = [
            upper[0], upper[3], upper[5], upper[4], upper[6], upper[1], upper[2],
        ];
        self.inventory(remaining.words(), caps, control)
    }
    fn inventory(
        &mut self,
        remaining: [u64; 4],
        mut caps: [u8; 7],
        control: &ExecutionControl,
    ) -> Result<Arc<[[u8; 7]]>, BuildStageDomainError> {
        if control.is_cancelled() {
            return Err(BuildStageDomainError::Cancelled);
        }
        let area = remaining
            .iter()
            .map(|w| w.count_ones() as usize)
            .sum::<usize>();
        for cap in &mut caps {
            *cap = (*cap).min((area / 4) as u8);
        }
        if let Some(found) = self.inventories.get(&(remaining, caps)) {
            return Ok(Arc::clone(found));
        }
        let mut values = Vec::new();
        if area == 0 {
            values
                .try_reserve(1)
                .map_err(|_| BuildStageDomainError::Allocation)?;
            values.push([0; 7]);
        } else if area % 4 == 0 && caps.iter().map(|&n| usize::from(n)).sum::<usize>() >= area / 4 {
            for (piece, cells) in self.choices(remaining, caps)? {
                let next = core::array::from_fn(|w| remaining[w] & !cells[w]);
                let mut child_caps = caps;
                child_caps[piece] -= 1;
                // Catalog order IOTSZJL -> public IJLOSTZ.
                let public_piece = [0, 3, 5, 4, 6, 1, 2][piece];
                for value in self.inventory(next, child_caps, control)?.iter() {
                    let mut value = *value;
                    value[public_piece] += 1;
                    values
                        .try_reserve(1)
                        .map_err(|_| BuildStageDomainError::Allocation)?;
                    values.push(value);
                }
            }
        }
        values.sort_unstable();
        values.dedup();
        let packed: Arc<[[u8; 7]]> = values.into();
        self.inventories
            .try_reserve(1)
            .map_err(|_| BuildStageDomainError::Allocation)?;
        self.inventories
            .insert((remaining, caps), Arc::clone(&packed));
        Ok(packed)
    }
    fn choices(
        &mut self,
        remaining: [u64; 4],
        caps: [u8; 7],
    ) -> Result<Vec<(usize, [u64; 4])>, BuildStageDomainError> {
        let area = remaining
            .iter()
            .map(|w| w.count_ones() as usize)
            .sum::<usize>();
        let mask = caps
            .iter()
            .enumerate()
            .fold(0_u8, |m, (p, &n)| m | if n > 0 { 1 << p } else { 0 });
        let mut choices = Vec::<(usize, [u64; 4])>::new();
        match &self.catalog {
            Catalog::Compact(c) => {
                if remaining[1..].iter().any(|&w| w != 0) {
                    return Err(BuildStageDomainError::InvalidField);
                }
                let domain = DomainPropagation::compile(c, remaining[0], mask);
                if domain.status != DomainStatus::Empty {
                    let d = domain.propagation;
                    for &row_id in c.support(d.pivot_cell) {
                        if d.row_allowed(c, row_id, remaining[0], mask) {
                            let row = c.skeleton(row_id);
                            choices
                                .try_reserve(1)
                                .map_err(|_| BuildStageDomainError::Allocation)?;
                            choices.push((piece_index(row.piece), [row.cells, 0, 0, 0]));
                        }
                    }
                }
            }
            Catalog::Extended(c) => {
                let rem = ExtendedBoard::from_mask(Board256Mask::from_words(remaining));
                let exact_inventory =
                    caps.iter().map(|&n| usize::from(n)).sum::<usize>() == area / 4;
                if exact_inventory {
                    if let ExtendedDomainResult::Supported(d) =
                        self.extended.compile(c, rem, [0; 7], &[caps], 0)
                    {
                        for &row_id in c.support(d.pivot_cell) {
                            if d.row_allowed(c, row_id, rem, mask) {
                                let row = c.skeleton(row_id);
                                choices
                                    .try_reserve(1)
                                    .map_err(|_| BuildStageDomainError::Allocation)?;
                                choices.push((piece_index(row.piece), row.cells.words()));
                            }
                        }
                    }
                } else {
                    // Exact-inventory projection constraints cannot be applied
                    // to a mere upper bound. Use the catalog's complete parents.
                    let mut pivot = None;
                    for cell in rem.cells() {
                        let count = c
                            .support(cell)
                            .iter()
                            .filter(|&&id| {
                                let row = c.skeleton(id);
                                caps[piece_index(row.piece)] > 0 && row.cells.is_subset_of(rem)
                            })
                            .count();
                        if pivot.is_none_or(|(_, old)| count < old) {
                            pivot = Some((cell, count));
                        }
                        if count == 0 {
                            break;
                        }
                    }
                    if let Some((cell, _)) = pivot {
                        for &id in c.support(cell) {
                            let row = c.skeleton(id);
                            if caps[piece_index(row.piece)] > 0 && row.cells.is_subset_of(rem) {
                                choices
                                    .try_reserve(1)
                                    .map_err(|_| BuildStageDomainError::Allocation)?;
                                choices.push((piece_index(row.piece), row.cells.words()));
                            }
                        }
                    }
                }
            }
        }
        Ok(choices)
    }
    fn complete(
        &mut self,
        remaining: [u64; 4],
        caps: [u8; 7],
        control: &ExecutionControl,
    ) -> Result<bool, BuildStageDomainError> {
        if control.is_cancelled() {
            return Err(BuildStageDomainError::Cancelled);
        }
        let area = remaining
            .iter()
            .map(|w| w.count_ones() as usize)
            .sum::<usize>();
        if area == 0 {
            return Ok(true);
        }
        if caps.iter().map(|&v| usize::from(v)).sum::<usize>() < area / 4 {
            return Ok(false);
        }
        if let Some(&value) = self.memo.get(&(remaining, caps)) {
            return Ok(value);
        }
        let choices = self.choices(remaining, caps)?;
        let mut value = false;
        for (piece, cells) in choices {
            let next = core::array::from_fn(|w| remaining[w] & !cells[w]);
            let mut cap = caps;
            cap[piece] -= 1;
            if self.complete(next, cap, control)? {
                value = true;
                break;
            }
        }
        self.memo
            .try_reserve(1)
            .map_err(|_| BuildStageDomainError::Allocation)?;
        self.memo.insert((remaining, caps), value);
        Ok(value)
    }
}
