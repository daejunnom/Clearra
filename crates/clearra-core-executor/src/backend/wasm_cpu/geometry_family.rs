pub(super) const FAMILY_INVALID: u32 = 0;
pub(super) const FAMILY_EMPTY: u32 = 1;

const FAMILY_NODE_CHUNK_CAPACITY: usize = 4096;
const FAMILY_INTERN_MIN_CAPACITY: usize = 4096;
const FAMILY_INTERN_LOAD_NUMERATOR: usize = 7;
const FAMILY_INTERN_LOAD_DENOMINATOR: usize = 10;

#[cfg(test)]
#[path = "geometry_family_intern_tests.rs"]
mod intern_tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(super) enum FamilyNodeKind {
    Append = 1,
    Union = 2,
    Product = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FamilyNode {
    pub left: u32,
    pub right: u32,
    pub row_id: u32,
    pub kind: FamilyNodeKind,
    reserved: [u8; 3],
}

const _: () = assert!(core::mem::size_of::<FamilyNode>() == 16);

#[derive(Debug)]
pub(super) struct GeometrySolutionFamily {
    chunks: Vec<Vec<FamilyNode>>,
    // Sum of actual backing capacities, including unused slots in the last
    // chunk. Directory and intern-table capacities are accounted separately.
    chunk_payload_capacity_bytes: usize,
    node_count: u32,
    intern_slots: Vec<u32>,
    intern_count: usize,
    // Changes whenever an index is replaced, rebuilt, or retired. A vacancy
    // is reusable only within the same synchronous intern call and generation.
    intern_generation: u64,
    interning_disabled: bool,
    retained_limit_bytes: Option<u128>,
    refused_retained_bytes: Option<u128>,
    #[cfg(test)]
    intern_work: InternWork,
}

#[derive(Clone, Copy, Debug)]
struct InternMiss {
    hash: u64,
    vacant_slot: Option<usize>,
    slot_count: usize,
    generation: u64,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct InternWork {
    hashes: usize,
    lookup_slots: usize,
    insertion_slots: usize,
    vacancy_validation_slots: usize,
    reused_vacancies: usize,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct FamilyCheckpoint {
    node_count: u32,
}

impl GeometrySolutionFamily {
    pub fn new() -> Self {
        let mut intern_slots = Vec::new();
        let interning_disabled = intern_slots
            .try_reserve_exact(FAMILY_INTERN_MIN_CAPACITY)
            .is_err();
        if !interning_disabled {
            intern_slots.resize(FAMILY_INTERN_MIN_CAPACITY, FAMILY_INVALID);
        }
        Self {
            chunks: Vec::new(),
            chunk_payload_capacity_bytes: 0,
            node_count: 0,
            intern_slots,
            intern_count: 0,
            intern_generation: 0,
            interning_disabled,
            retained_limit_bytes: None,
            refused_retained_bytes: None,
            #[cfg(test)]
            intern_work: InternWork::default(),
        }
    }

    pub fn set_retained_limit_bytes(&mut self, retained_limit_bytes: Option<u128>) {
        self.retained_limit_bytes = retained_limit_bytes;
        self.refused_retained_bytes = None;
        if retained_limit_bytes.is_some_and(|limit| self.retained_bytes() as u128 > limit) {
            self.disable_interning();
        }
    }

    pub fn refused_retained_bytes(&self) -> Option<u128> {
        self.refused_retained_bytes
    }

    /// Inventory aliases preserve interned node IDs and traversal order only
    /// while this unbounded owner retains its original active interner.
    pub(super) const fn unbounded_interning_active(&self) -> bool {
        !self.interning_disabled && self.retained_limit_bytes.is_none()
    }

    /// Release construction-only lookup storage at the consumed, unbounded
    /// compiler-to-enumerator boundary. This does not change any node or ID.
    /// Mutable completion-oracle owners and bounded handoffs retain it.
    pub(super) fn release_construction_index(&mut self) -> usize {
        let released = self.intern_slots.capacity() * core::mem::size_of::<u32>();
        self.disable_interning();
        released
    }

    pub fn append(&mut self, row_id: u32, suffix: u32) -> Option<u32> {
        if suffix == FAMILY_INVALID {
            return Some(FAMILY_INVALID);
        }
        self.intern(FamilyNode {
            left: suffix,
            right: FAMILY_INVALID,
            row_id,
            kind: FamilyNodeKind::Append,
            reserved: [0; 3],
        })
    }

    pub fn union(&mut self, mut left: u32, mut right: u32) -> Option<u32> {
        if left == FAMILY_INVALID {
            return Some(right);
        }
        if right == FAMILY_INVALID || left == right {
            return Some(left);
        }
        if right < left {
            core::mem::swap(&mut left, &mut right);
        }
        self.intern(FamilyNode {
            left,
            right,
            row_id: 0,
            kind: FamilyNodeKind::Union,
            reserved: [0; 3],
        })
    }

    pub fn product(&mut self, mut left: u32, mut right: u32) -> Option<u32> {
        if left == FAMILY_INVALID || right == FAMILY_INVALID {
            return Some(FAMILY_INVALID);
        }
        if left == FAMILY_EMPTY {
            return Some(right);
        }
        if right == FAMILY_EMPTY {
            return Some(left);
        }
        if right < left {
            core::mem::swap(&mut left, &mut right);
        }
        self.intern(FamilyNode {
            left,
            right,
            row_id: 0,
            kind: FamilyNodeKind::Product,
            reserved: [0; 3],
        })
    }

    pub fn node(&self, reference: u32) -> Option<FamilyNode> {
        let index = reference.checked_sub(2)? as usize;
        if index >= self.node_count as usize {
            return None;
        }
        let chunk = index / FAMILY_NODE_CHUNK_CAPACITY;
        let offset = index % FAMILY_NODE_CHUNK_CAPACITY;
        self.chunks
            .get(chunk)
            .and_then(|nodes| nodes.get(offset))
            .copied()
    }

    pub fn retained_bytes(&self) -> usize {
        self.chunks.capacity() * core::mem::size_of::<Vec<FamilyNode>>()
            + self.chunk_payload_capacity_bytes
            + self.intern_slots.capacity() * core::mem::size_of::<u32>()
    }

    pub const fn node_count(&self) -> u32 {
        self.node_count
    }

    pub fn path_count(&self, root: u32) -> Option<u128> {
        self.path_count_table()?.get(root as usize).copied()
    }

    pub fn path_count_table(&self) -> Option<Vec<u128>> {
        let value_count = self.node_count as usize + 2;
        let mut counts = Vec::new();
        counts.try_reserve_exact(value_count).ok()?;
        counts.resize(value_count, 0_u128);
        counts[FAMILY_EMPTY as usize] = 1;
        for reference in 2..self.node_count + 2 {
            counts[reference as usize] = self.path_count_for_reference(reference, &counts)?;
        }
        Some(counts)
    }

    /// Counts paths and bounds pending DFS tasks, including continuations
    /// from a Product.
    /// Append changes only the current task; Union leaves the right sibling
    /// on the stack while visiting the left; Product concatenates walks.
    #[cfg(all(feature = "parallel", not(target_family = "wasm")))]
    pub(super) fn parallel_traversal_tables(
        &self,
        root: u32,
        is_cancelled: impl Fn() -> bool,
    ) -> Option<(Vec<u128>, usize)> {
        let entries = (self.node_count as usize).checked_add(2)?;
        let mut counts = Vec::new();
        counts.try_reserve_exact(entries).ok()?;
        counts.resize(entries, 0_u128);
        counts[FAMILY_EMPTY as usize] = 1;
        let mut pending = Vec::new();
        pending.try_reserve_exact(entries).ok()?;
        pending.resize(entries, 0_usize);
        for reference in 2..self.node_count + 2 {
            if reference & 4095 == 0 && is_cancelled() {
                return None;
            }
            let node = self.node(reference)?;
            counts[reference as usize] = self.path_count_for_reference(reference, &counts)?;
            let left = *pending.get(node.left as usize)?;
            pending[reference as usize] = match node.kind {
                FamilyNodeKind::Append => left,
                FamilyNodeKind::Union => {
                    left.checked_add(1)?.max(*pending.get(node.right as usize)?)
                }
                FamilyNodeKind::Product => left.checked_add(*pending.get(node.right as usize)?)?,
            };
        }
        Some((counts, pending.get(root as usize)?.checked_add(1)?))
    }

    /// Direct and resumable owners use the same checked DAG recurrence.
    /// A resumable owner supplies only the already counted prefix.
    pub(super) fn path_count_for_reference(&self, reference: u32, counts: &[u128]) -> Option<u128> {
        let node = self.node(reference)?;
        let left = *counts.get(node.left as usize)?;
        match node.kind {
            FamilyNodeKind::Append => Some(left),
            FamilyNodeKind::Union => left.checked_add(*counts.get(node.right as usize)?),
            FamilyNodeKind::Product => left.checked_mul(*counts.get(node.right as usize)?),
        }
    }

    pub fn checkpoint(&self) -> FamilyCheckpoint {
        FamilyCheckpoint {
            node_count: self.node_count,
        }
    }

    pub fn rewind(&mut self, checkpoint: FamilyCheckpoint) {
        if checkpoint.node_count >= self.node_count {
            return;
        }
        self.node_count = checkpoint.node_count;
        let retained_chunks = (self.node_count as usize).div_ceil(FAMILY_NODE_CHUNK_CAPACITY);
        for removed in self.chunks.drain(retained_chunks..) {
            self.chunk_payload_capacity_bytes -=
                removed.capacity() * core::mem::size_of::<FamilyNode>();
        }
        if let Some(last) = self.chunks.last_mut() {
            let retained_in_last =
                self.node_count as usize - (retained_chunks - 1) * FAMILY_NODE_CHUNK_CAPACITY;
            last.truncate(retained_in_last);
        }
        if self.interning_disabled {
            return;
        }
        self.intern_generation = self.intern_generation.wrapping_add(1);
        self.intern_slots.fill(FAMILY_INVALID);
        self.intern_count = 0;
        for reference in 2..self.node_count + 2 {
            let Some(node) = self.node(reference) else {
                self.disable_interning();
                return;
            };
            let hash = self.hash_node(node);
            self.insert_interned(reference, hash);
        }
    }

    fn intern(&mut self, node: FamilyNode) -> Option<u32> {
        let miss = if self.interning_disabled {
            None
        } else {
            match self.probe_interned(node) {
                Ok(reference) => return Some(reference),
                Err(miss) => Some(miss),
            }
        };

        // Preserve the old admission/growth order: a lookup hit never grows
        // the table, and a refused node never requests an index replacement.
        let reference = self.push_node(node)?;
        if let Some(miss) = miss {
            self.ensure_intern_capacity();
            if !self.interning_disabled && !self.insert_at_probed_vacancy(reference, miss) {
                // A resize rebuilt the probe chain. Reuse only the hash and
                // perform the original insertion probe in the new table.
                self.insert_interned(reference, miss.hash);
            }
        }
        Some(reference)
    }

    fn push_node(&mut self, node: FamilyNode) -> Option<u32> {
        if self.node_count > u32::MAX - 2 {
            return None;
        }
        let needs_chunk = self
            .chunks
            .last()
            .is_none_or(|chunk| chunk.len() == FAMILY_NODE_CHUNK_CAPACITY);
        if needs_chunk {
            let requested_chunk_bytes = (FAMILY_NODE_CHUNK_CAPACITY as u128)
                .checked_mul(core::mem::size_of::<FamilyNode>() as u128)?;
            self.reserve_chunk_directory(requested_chunk_bytes)?;
            let projected = (self.retained_bytes() as u128).checked_add(requested_chunk_bytes)?;
            if self
                .retained_limit_bytes
                .is_some_and(|limit| projected > limit)
            {
                self.refused_retained_bytes = Some(projected);
                return None;
            }
            let mut chunk = Vec::new();
            chunk.try_reserve_exact(FAMILY_NODE_CHUNK_CAPACITY).ok()?;
            let chunk_bytes = chunk
                .capacity()
                .checked_mul(core::mem::size_of::<FamilyNode>())?;
            let projected = (self.retained_bytes() as u128).checked_add(chunk_bytes as u128)?;
            if self
                .retained_limit_bytes
                .is_some_and(|limit| projected > limit)
            {
                self.refused_retained_bytes = Some(projected);
                return None;
            }
            self.chunk_payload_capacity_bytes =
                self.chunk_payload_capacity_bytes.checked_add(chunk_bytes)?;
            self.chunks.push(chunk);
        }
        let reference = self.node_count + 2;
        self.chunks.last_mut()?.push(node);
        self.node_count += 1;
        Some(reference)
    }

    fn reserve_chunk_directory(&mut self, next_chunk_bytes: u128) -> Option<()> {
        if self.chunks.len() < self.chunks.capacity() {
            return Some(());
        }
        let minimum = self.chunks.len().checked_add(1)?;
        // A bounded owner keeps the previous minimal directory policy. Slack
        // admitted now could otherwise reject a later chunk near the same cap,
        // even when no directory growth is needed at that later boundary.
        let preferred = if self.retained_limit_bytes.is_some() {
            minimum
        } else {
            self.chunks
                .capacity()
                .checked_mul(2)
                .unwrap_or(minimum)
                .max(minimum)
        };
        let replacement = self.allocate_chunk_directory(preferred, next_chunk_bytes);
        let replacement = match replacement {
            Err(_) if preferred != minimum => {
                // If the optional slack allocation fails, the smaller
                // directory still preserves every existing node reference.
                self.allocate_chunk_directory(minimum, next_chunk_bytes)
            }
            result => result,
        };
        let replacement = match replacement {
            Ok(replacement) => replacement,
            Err(required) => {
                if let Some(required) = required {
                    self.refused_retained_bytes = Some(required);
                }
                return None;
            }
        };
        let old = core::mem::replace(&mut self.chunks, replacement);
        self.chunks.extend(old);
        Some(())
    }

    fn allocate_chunk_directory(
        &self,
        capacity: usize,
        next_chunk_bytes: u128,
    ) -> Result<Vec<Vec<FamilyNode>>, Option<u128>> {
        let check_capacity = |capacity: usize| {
            let replacement_bytes = (capacity as u128)
                .checked_mul(core::mem::size_of::<Vec<FamilyNode>>() as u128)
                .ok_or(None)?;
            let current = self.retained_bytes() as u128;
            let replacement_peak = current.checked_add(replacement_bytes).ok_or(None)?;
            let old_directory_bytes = (self.chunks.capacity() as u128)
                .checked_mul(core::mem::size_of::<Vec<FamilyNode>>() as u128)
                .ok_or(None)?;
            let with_next_chunk = current
                .checked_sub(old_directory_bytes)
                .and_then(|bytes| bytes.checked_add(replacement_bytes))
                .and_then(|bytes| bytes.checked_add(next_chunk_bytes))
                .ok_or(None)?;
            let required = replacement_peak.max(with_next_chunk);
            if self
                .retained_limit_bytes
                .is_some_and(|limit| required > limit)
            {
                Err(Some(required))
            } else {
                Ok(())
            }
        };
        check_capacity(capacity)?;
        let mut replacement = Vec::new();
        replacement.try_reserve_exact(capacity).map_err(|_| None)?;
        check_capacity(replacement.capacity())?;
        Ok(replacement)
    }

    #[inline]
    fn hash_node(&mut self, node: FamilyNode) -> u64 {
        #[cfg(test)]
        {
            self.intern_work.hashes += 1;
        }
        node_hash(node)
    }

    fn probe_interned(&mut self, node: FamilyNode) -> Result<u32, InternMiss> {
        let hash = self.hash_node(node);
        let mut miss = InternMiss {
            hash,
            vacant_slot: None,
            slot_count: self.intern_slots.len(),
            generation: self.intern_generation,
        };
        if self.intern_slots.is_empty() {
            return Err(miss);
        }
        let mask = self.intern_slots.len() - 1;
        let mut slot = hash as usize & mask;
        for _ in 0..self.intern_slots.len() {
            #[cfg(test)]
            {
                self.intern_work.lookup_slots += 1;
            }
            let reference = self.intern_slots[slot];
            if reference == FAMILY_INVALID {
                miss.vacant_slot = Some(slot);
                return Err(miss);
            }
            if self.node(reference) == Some(node) {
                return Ok(reference);
            }
            slot = (slot + 1) & mask;
        }
        Err(miss)
    }

    fn insert_at_probed_vacancy(&mut self, reference: u32, miss: InternMiss) -> bool {
        if self.interning_disabled
            || self.intern_generation != miss.generation
            || self.intern_slots.len() != miss.slot_count
        {
            return false;
        }
        let Some(slot) = miss
            .vacant_slot
            .and_then(|slot| self.intern_slots.get_mut(slot))
        else {
            return false;
        };
        #[cfg(test)]
        {
            self.intern_work.vacancy_validation_slots += 1;
        }
        if *slot != FAMILY_INVALID {
            return false;
        }
        *slot = reference;
        self.intern_count += 1;
        #[cfg(test)]
        {
            self.intern_work.reused_vacancies += 1;
        }
        true
    }

    fn ensure_intern_capacity(&mut self) {
        if (self.intern_count + 1) * FAMILY_INTERN_LOAD_DENOMINATOR
            < self.intern_slots.len() * FAMILY_INTERN_LOAD_NUMERATOR
        {
            return;
        }
        let Some(next_capacity) = self.intern_slots.len().checked_mul(2) else {
            self.disable_interning();
            return;
        };
        let replacement_bytes =
            (next_capacity as u128).checked_mul(core::mem::size_of::<u32>() as u128);
        let peak = replacement_bytes
            .and_then(|replacement| (self.retained_bytes() as u128).checked_add(replacement));
        if peak.is_none()
            || self
                .retained_limit_bytes
                .zip(peak)
                .is_some_and(|(limit, peak)| peak > limit)
        {
            self.disable_interning();
            return;
        }
        let mut replacement = Vec::new();
        if replacement.try_reserve_exact(next_capacity).is_err() {
            self.disable_interning();
            return;
        }
        let actual_peak = (replacement.capacity() as u128)
            .checked_mul(core::mem::size_of::<u32>() as u128)
            .and_then(|replacement| (self.retained_bytes() as u128).checked_add(replacement));
        if actual_peak.is_none()
            || self
                .retained_limit_bytes
                .zip(actual_peak)
                .is_some_and(|(limit, peak)| peak > limit)
        {
            self.disable_interning();
            return;
        }
        replacement.resize(next_capacity, FAMILY_INVALID);
        self.intern_generation = self.intern_generation.wrapping_add(1);
        let old = core::mem::replace(&mut self.intern_slots, replacement);
        self.intern_count = 0;
        for reference in old
            .into_iter()
            .filter(|reference| *reference != FAMILY_INVALID)
        {
            let Some(node) = self.node(reference) else {
                self.disable_interning();
                return;
            };
            let hash = self.hash_node(node);
            self.insert_interned(reference, hash);
        }
    }

    fn insert_interned(&mut self, reference: u32, hash: u64) {
        let mask = self.intern_slots.len() - 1;
        let mut slot = hash as usize & mask;
        loop {
            #[cfg(test)]
            {
                self.intern_work.insertion_slots += 1;
            }
            if self.intern_slots[slot] == FAMILY_INVALID {
                self.intern_slots[slot] = reference;
                self.intern_count += 1;
                return;
            }
            slot = (slot + 1) & mask;
        }
    }

    fn disable_interning(&mut self) {
        self.intern_generation = self.intern_generation.wrapping_add(1);
        self.intern_slots = Vec::new();
        self.intern_count = 0;
        self.interning_disabled = true;
    }
}

fn node_hash(node: FamilyNode) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for value in [
        node.kind as u64,
        u64::from(node.left),
        u64::from(node.right),
        u64::from(node.row_id),
    ] {
        hash ^= value.wrapping_add(0x9e37_79b9_7f4a_7c15);
        hash = hash.rotate_left(17).wrapping_mul(0x94d0_49bb_1331_11eb);
    }
    hash ^ (hash >> 31)
}

#[cfg(test)]
mod tests {
    use super::{
        FamilyNode, GeometrySolutionFamily, FAMILY_EMPTY, FAMILY_INVALID,
        FAMILY_NODE_CHUNK_CAPACITY,
    };

    fn assert_actual_retained_capacity(family: &GeometrySolutionFamily) {
        let chunk_bytes = family
            .chunks
            .iter()
            .map(|chunk| chunk.capacity() * core::mem::size_of::<FamilyNode>())
            .sum::<usize>();
        assert_eq!(family.chunk_payload_capacity_bytes, chunk_bytes);
        assert_eq!(
            family.retained_bytes(),
            family.chunks.capacity() * core::mem::size_of::<Vec<FamilyNode>>()
                + chunk_bytes
                + family.intern_slots.capacity() * core::mem::size_of::<u32>()
        );
    }

    #[test]
    fn construction_index_release_preserves_node_ids_counts_and_exact_retained_bytes() {
        let mut family = GeometrySolutionFamily::new();
        let mut first = FAMILY_INVALID;
        let mut last = FAMILY_INVALID;
        for row in 0..super::FAMILY_INTERN_MIN_CAPACITY as u32 {
            last = family.append(row, FAMILY_EMPTY).unwrap();
            if first == FAMILY_INVALID {
                first = last;
            }
        }
        let choices = family.union(first, last).unwrap();
        let root = family.product(choices, choices).unwrap();
        let nodes = (2..family.node_count() + 2)
            .map(|reference| family.node(reference).unwrap())
            .collect::<Vec<_>>();
        let counts = family.path_count_table().unwrap();
        let chunks = family
            .chunks
            .iter()
            .map(|chunk| (chunk.as_ptr(), chunk.len(), chunk.capacity()))
            .collect::<Vec<_>>();
        let retained = family.retained_bytes();
        let index_bytes = family.intern_slots.capacity() * core::mem::size_of::<u32>();
        assert!(index_bytes > 0 && family.unbounded_interning_active());
        assert_eq!(counts[root as usize], 4);

        assert_eq!(family.release_construction_index(), index_bytes);
        assert!(!family.unbounded_interning_active());
        assert_eq!(family.intern_slots.capacity(), 0);
        assert_eq!(family.intern_count, 0);
        assert_eq!(family.retained_bytes(), retained - index_bytes);
        assert_actual_retained_capacity(&family);
        assert_eq!(family.path_count_table(), Some(counts));
        assert_eq!(
            (2..family.node_count() + 2)
                .map(|reference| family.node(reference).unwrap())
                .collect::<Vec<_>>(),
            nodes,
        );
        assert_eq!(
            family
                .chunks
                .iter()
                .map(|chunk| (chunk.as_ptr(), chunk.len(), chunk.capacity()))
                .collect::<Vec<_>>(),
            chunks,
        );
        assert_eq!(family.release_construction_index(), 0);
        assert_eq!(family.retained_bytes(), retained - index_bytes);
    }

    #[test]
    fn family_root_count_preserves_append_union_and_product_multiplicity() {
        let mut family = GeometrySolutionFamily::new();
        let a = family.append(4, FAMILY_EMPTY).unwrap();
        let b = family.append(5, FAMILY_EMPTY).unwrap();
        let c = family.append(6, FAMILY_EMPTY).unwrap();
        let two = family.union(a, b).unwrap();
        let three = family.union(two, c).unwrap();
        let six = family.product(two, three).unwrap();
        let root = family.append(7, six).unwrap();

        assert_eq!(family.path_count(FAMILY_INVALID), Some(0));
        assert_eq!(family.path_count(FAMILY_EMPTY), Some(1));
        assert_eq!(family.path_count(two), Some(2));
        assert_eq!(family.path_count(three), Some(3));
        assert_eq!(family.path_count(root), Some(6));
    }

    #[test]
    fn capped_family_refuses_outer_chunk_backing_before_allocation() {
        let mut family = GeometrySolutionFamily::new();
        let before = family.retained_bytes() as u128;
        family.set_retained_limit_bytes(Some(before));
        assert!(family.append(7, FAMILY_EMPTY).is_none());
        assert_eq!(family.node_count(), 0);
        assert_eq!(family.chunks.capacity(), 0);
        assert_eq!(family.retained_bytes() as u128, before);
        assert!(family.refused_retained_bytes().unwrap() > before);
    }

    #[test]
    fn family_chunk_growth_accepts_exact_retained_cap_and_rejects_one_under() {
        let mut measured = GeometrySolutionFamily::new();
        measured
            .append(7, FAMILY_EMPTY)
            .expect("one family node is materialized");
        let exact_cap = measured.retained_bytes() as u128;

        let mut exact = GeometrySolutionFamily::new();
        exact.set_retained_limit_bytes(Some(exact_cap));
        assert!(exact.append(7, FAMILY_EMPTY).is_some());
        assert!(exact.retained_bytes() as u128 <= exact_cap);

        let mut one_under = GeometrySolutionFamily::new();
        one_under.set_retained_limit_bytes(Some(exact_cap - 1));
        assert!(one_under.append(7, FAMILY_EMPTY).is_none());
        assert!((one_under.retained_bytes() as u128) < exact_cap);
    }

    #[test]
    fn family_bounded_directory_keeps_minimum_capacity_at_exact_cap() {
        for one_under in [false, true] {
            let mut family = GeometrySolutionFamily::new();
            family.disable_interning();
            family.set_retained_limit_bytes(Some(u128::MAX));
            let existing_nodes = 5 * FAMILY_NODE_CHUNK_CAPACITY;
            for row in 0..existing_nodes {
                family.append(row as u32, FAMILY_EMPTY).unwrap();
            }
            assert_eq!(family.chunks.len(), 5);
            assert_eq!(family.chunks.capacity(), 5);
            assert_actual_retained_capacity(&family);
            let chunk_bytes = family.chunks[0].capacity() * core::mem::size_of::<FamilyNode>();
            let minimum_retained =
                family.retained_bytes() + chunk_bytes + core::mem::size_of::<Vec<FamilyNode>>();
            family.set_retained_limit_bytes(Some(minimum_retained as u128 - u128::from(one_under)));

            let appended = family.append(existing_nodes as u32, FAMILY_EMPTY);
            if one_under {
                assert_eq!(appended, None);
                assert_eq!(family.node_count() as usize, existing_nodes);
                assert_eq!(family.chunks.len(), 5);
                assert_eq!(
                    family.refused_retained_bytes(),
                    Some(minimum_retained as u128)
                );
            } else {
                assert_eq!(appended, Some(existing_nodes as u32 + 2));
                assert_eq!(family.chunks.len(), 6);
                assert_eq!(family.chunks.capacity(), 6);
                assert_eq!(family.retained_bytes(), minimum_retained);
                assert_eq!(family.refused_retained_bytes(), None);
            }
            assert_actual_retained_capacity(&family);
        }
    }

    #[test]
    fn family_retained_capacity_stays_exact_after_partial_rewind_and_refill() {
        let mut family = GeometrySolutionFamily::new();
        let empty = family.checkpoint();
        let prefix_count = FAMILY_NODE_CHUNK_CAPACITY + 17;
        let final_count = 4 * FAMILY_NODE_CHUNK_CAPACITY + 29;
        let mut root = FAMILY_EMPTY;
        for row in 0..prefix_count {
            root = family.append(row as u32, root).unwrap();
        }
        let prefix_root = root;
        let prefix = family.checkpoint();
        let prefix_chunk_bytes = family.chunk_payload_capacity_bytes;
        assert_actual_retained_capacity(&family);
        for row in prefix_count..final_count {
            root = family.append(row as u32, root).unwrap();
        }
        let expected_root = root;
        let expected_nodes = (2..family.node_count() + 2)
            .map(|reference| family.node(reference).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(family.chunks.len(), 5);
        assert_eq!(family.chunks.capacity(), 8);
        assert_actual_retained_capacity(&family);

        family.rewind(prefix);
        assert_eq!(family.node_count() as usize, prefix_count);
        assert_eq!(family.chunks.len(), 2);
        assert_eq!(family.chunks.last().unwrap().len(), 17);
        assert!(family.chunks.last().unwrap().capacity() > 17);
        assert_eq!(family.chunk_payload_capacity_bytes, prefix_chunk_bytes);
        assert_actual_retained_capacity(&family);
        root = prefix_root;
        for row in prefix_count..final_count {
            root = family.append(row as u32, root).unwrap();
        }
        assert_eq!(root, expected_root);
        for (index, expected) in expected_nodes.iter().copied().enumerate() {
            assert_eq!(family.node(index as u32 + 2), Some(expected));
        }
        assert_eq!(family.path_count(root), Some(1));
        assert_actual_retained_capacity(&family);

        family.rewind(empty);
        assert_eq!(family.node_count(), 0);
        assert!(family.chunks.is_empty());
        assert_eq!(family.chunk_payload_capacity_bytes, 0);
        assert_actual_retained_capacity(&family);
        assert_eq!(family.append(0, FAMILY_EMPTY), Some(2));
        assert_actual_retained_capacity(&family);
    }
}
