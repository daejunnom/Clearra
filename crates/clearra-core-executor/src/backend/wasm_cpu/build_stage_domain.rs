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

type StageInventoryKey = ([u64; 4], [u8; 7]);
type StageInventory = Arc<[[u8; 7]]>;

pub struct BuildStageDomain {
    catalog: Catalog,
    extended: ExtendedDomainWorkspace,
    memo: HashMap<StageInventoryKey, bool>,
    inventories: HashMap<StageInventoryKey, StageInventory>,
    inventory_slots: usize,
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
            inventory_slots: 0,
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
    pub fn completion_choices(
        &mut self,
        remaining: Board256Mask,
        upper: [u8; 7],
        control: &ExecutionControl,
    ) -> Result<Vec<(u8, Board256Mask)>, BuildStageDomainError> {
        if control.is_cancelled() {
            return Err(BuildStageDomainError::Cancelled);
        }
        let caps = [
            upper[0], upper[3], upper[5], upper[4], upper[6], upper[1], upper[2],
        ];
        let to_public = [0_u8, 3, 5, 4, 6, 1, 2];
        let mut result = self.choices(remaining.words(), caps)?;
        result.sort_unstable();
        result.dedup();
        Ok(result
            .into_iter()
            .map(|(p, cells)| (to_public[p], Board256Mask::from_words(cells)))
            .collect())
    }
    pub fn skeleton_count(&self) -> usize {
        match &self.catalog {
            Catalog::Compact(c) => c.skeleton_count(),
            Catalog::Extended(c) => c.skeletons().len(),
        }
    }
    /// Complete type-count domains in IJLOSTZ order. This is still geometric
    /// possibility only: no independent-stage movement/clear schedule assumed.
    pub fn completion_inventories(
        &mut self,
        remaining: Board256Mask,
        upper: [u8; 7],
        control: &ExecutionControl,
    ) -> Result<Vec<[u8; 7]>, BuildStageDomainError> {
        let caps = [
            upper[0], upper[3], upper[5], upper[4], upper[6], upper[1], upper[2],
        ];
        let internal = self.inventory(remaining.words(), caps, control)?;
        Ok(internal
            .iter()
            .map(|s| [s[0], s[5], s[6], s[1], s[3], s[2], s[4]])
            .collect())
    }
    /// Both remaining stages must consume complementary inventories. Merely
    /// testing each against the same upper bound can spend a scarce I twice.
    pub fn can_complete_pair(
        first: &mut Self,
        first_remaining: Board256Mask,
        second: &mut Self,
        second_remaining: Board256Mask,
        total: [u8; 7],
        control: &ExecutionControl,
    ) -> Result<bool, BuildStageDomainError> {
        let area = first_remaining.count_ones() + second_remaining.count_ones();
        if !area.is_multiple_of(4)
            || u32::from(total.iter().map(|&n| u16::from(n)).sum::<u16>()) * 4 != area
        {
            return Ok(false);
        }
        if first_remaining.count_ones() < second_remaining.count_ones() {
            return Self::can_complete_pair(
                second,
                second_remaining,
                first,
                first_remaining,
                total,
                control,
            );
        }
        if second_remaining.is_empty() {
            return first.can_complete(first_remaining, total, control);
        }
        for suffix in second.completion_inventories(second_remaining, total, control)? {
            let mut prefix = [0_u8; 7];
            let mut valid = true;
            for p in 0..7 {
                if let Some(n) = total[p].checked_sub(suffix[p]) {
                    prefix[p] = n;
                } else {
                    valid = false;
                    break;
                }
            }
            if valid && first.can_complete(first_remaining, prefix, control)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn inventory(
        &mut self,
        remaining: [u64; 4],
        caps: [u8; 7],
        control: &ExecutionControl,
    ) -> Result<Arc<[[u8; 7]]>, BuildStageDomainError> {
        if control.is_cancelled() {
            return Err(BuildStageDomainError::Cancelled);
        }
        if remaining == [0; 4] {
            return Ok(Arc::from([[0; 7]]));
        }
        if let Some(value) = self.inventories.get(&(remaining, caps)) {
            return Ok(Arc::clone(value));
        }
        let mut found = Vec::<[u8; 7]>::new();
        for (piece, cells) in self.choices(remaining, caps)? {
            let next = core::array::from_fn(|w| remaining[w] & !cells[w]);
            // Keep the same cap on subdomains: prefix contributions are added
            // below. This shares suffix domains across different tile orders.
            let tails = self.inventory(next, caps, control)?;
            for tail in tails.iter() {
                if tail[piece] >= caps[piece] {
                    continue;
                }
                let mut counts = *tail;
                counts[piece] += 1;
                found
                    .try_reserve(1)
                    .map_err(|_| BuildStageDomainError::Allocation)?;
                found.push(counts);
            }
            found.sort_unstable();
            found.dedup();
        }
        let value: Arc<[[u8; 7]]> = found.into();
        // Cache eviction never truncates a domain or supplies negative evidence.
        // Bound retained count records rather than pretending every entry has
        // the same footprint; active Arc values survive a safe eviction.
        const MAX_RECORDS: usize = 65_536;
        if self.inventory_slots.saturating_add(value.len()) > MAX_RECORDS
            || self.inventories.len() >= 4096
        {
            self.inventories.clear();
            self.inventory_slots = 0;
        }
        if value.len() <= MAX_RECORDS {
            self.inventories
                .try_reserve(1)
                .map_err(|_| BuildStageDomainError::Allocation)?;
            self.inventory_slots += value.len();
            self.inventories
                .insert((remaining, caps), Arc::clone(&value));
        }
        Ok(value)
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
        if self.memo.len() >= 16_384 {
            self.memo.clear();
        }
        self.memo
            .try_reserve(1)
            .map_err(|_| BuildStageDomainError::Allocation)?;
        self.memo.insert((remaining, caps), value);
        Ok(value)
    }
}

#[cfg(test)]
mod inventory_tests {
    use super::*;
    fn domain(mask: u64) -> BuildStageDomain {
        BuildStageDomain::compile(
            BuildProbabilityField::from_words_preserving_height(4, [0; 4], [mask, 0, 0, 0])
                .unwrap(),
            &ExecutionControl::default(),
        )
        .unwrap()
    }
    #[test]
    fn recovery_inventory_complements_cannot_spend_one_piece_twice() {
        let control = ExecutionControl::default();
        let mask = Board256Mask::from_words([15, 0, 0, 0]);
        let mut a = domain(15);
        let mut b = domain(15);
        let io = [1, 0, 0, 1, 0, 0, 0];
        assert!(a.can_complete(mask, io, &control).unwrap());
        assert!(b.can_complete(mask, io, &control).unwrap());
        assert!(
            !BuildStageDomain::can_complete_pair(&mut a, mask, &mut b, mask, io, &control).unwrap()
        );
        assert!(BuildStageDomain::can_complete_pair(
            &mut a,
            mask,
            &mut b,
            mask,
            [2, 0, 0, 0, 0, 0, 0],
            &control
        )
        .unwrap());
    }
    #[test]
    fn recovery_inventory_domain_matches_separately_queried_exact_counts() {
        let control = ExecutionControl::default();
        // A 4x2 rectangle admits different complete tilings. Check the whole
        // count-vector domain, not only a chosen positive witness.
        let mask = Board256Mask::from_words([15 | (15 << 10), 0, 0, 0]);
        let mut d = domain(mask.words()[0]);
        let inventory = d.completion_inventories(mask, [2; 7], &control).unwrap();
        for x in 0..7 {
            for y in x..7 {
                let mut counts = [0; 7];
                counts[x] += 1;
                counts[y] += 1;
                let mut independent = domain(mask.words()[0]);
                assert_eq!(
                    inventory.contains(&counts),
                    independent.can_complete(mask, counts, &control).unwrap(),
                    "{counts:?}"
                );
            }
        }
    }
}
