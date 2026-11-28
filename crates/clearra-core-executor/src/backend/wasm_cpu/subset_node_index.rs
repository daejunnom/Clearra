use super::compact_exact_u64_map::{ExactU64MemoMap, ExactU64MemoStorage};

const PAGE_BITS: u32 = 6;
const PAGE_SLOTS: usize = 1 << PAGE_BITS;
const DIRECT_PAGES: usize = 1024;
const MISSING: u32 = u32::MAX;

struct SubsetPage {
    key: u64,
    nodes: [u32; PAGE_SLOTS],
}

#[derive(Clone, Copy, Default)]
pub(super) struct SubsetIndexGrowth {
    pages: usize,
    sparse_pages: usize,
}

/// A single reached-subset index for every operation count. Nearby subset
/// addresses share flat slots; only allocated high pages need an exact-key
/// directory entry. Neither construction nor clearing enumerates 2^k states.
pub(super) struct SubsetNodeIndex {
    direct: [u32; DIRECT_PAGES],
    sparse: ExactU64MemoMap,
    pages: Vec<SubsetPage>,
}

impl SubsetNodeIndex {
    pub fn new() -> Self {
        Self {
            direct: [MISSING; DIRECT_PAGES],
            sparse: ExactU64MemoMap::new(ExactU64MemoStorage::Compact),
            pages: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        // Clear just the directory cells owned by this candidate. A tiny
        // candidate must not pay to reset the entire direct address window.
        for page in &self.pages {
            if page.key < DIRECT_PAGES as u64 {
                self.direct[page.key as usize] = MISSING;
            }
        }
        self.sparse.clear();
        self.pages.clear();
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    #[inline]
    fn page_index(&self, page: u64) -> Option<u32> {
        if page < DIRECT_PAGES as u64 {
            let index = self.direct[page as usize];
            (index != MISSING).then_some(index)
        } else {
            self.sparse.get(&page).copied()
        }
    }

    #[inline]
    pub fn get(&self, subset: u64) -> Option<u32> {
        let page = self.page_index(subset >> PAGE_BITS)?;
        let node = self.pages[page as usize].nodes[subset as usize & (PAGE_SLOTS - 1)];
        (node != MISSING).then_some(node)
    }

    pub fn growth_for_key(&self, subset: u64) -> SubsetIndexGrowth {
        let page = subset >> PAGE_BITS;
        if self.page_index(page).is_some() {
            SubsetIndexGrowth::default()
        } else {
            SubsetIndexGrowth {
                pages: 1,
                sparse_pages: usize::from(page >= DIRECT_PAGES as u64),
            }
        }
    }

    /// Every low-bit child shares its already-retained parent's page. Each
    /// higher operation bit names a distinct child page, so this exact reserve
    /// plan needs no temporary key set or heap allocation.
    pub fn growth_for_children(&self, subset: u64, remaining: u64) -> SubsetIndexGrowth {
        debug_assert!(self.page_index(subset >> PAGE_BITS).is_some());
        let mut growth = SubsetIndexGrowth::default();
        let base_page = subset >> PAGE_BITS;
        let mut remaining_pages = remaining >> PAGE_BITS;
        while remaining_pages != 0 {
            let operation = remaining_pages.trailing_zeros();
            remaining_pages &= remaining_pages - 1;
            let page = base_page | (1_u64 << operation);
            if self.page_index(page).is_none() {
                growth.pages += 1;
                growth.sparse_pages += usize::from(page >= DIRECT_PAGES as u64);
            }
        }
        growth
    }

    pub fn checked_reserve_future_bytes(&self, growth: SubsetIndexGrowth) -> Option<u128> {
        let needed = self.pages.len().checked_add(growth.pages)?;
        let page_bytes = if needed > self.pages.capacity() {
            (needed.checked_next_power_of_two()? as u128)
                .checked_mul(core::mem::size_of::<SubsetPage>() as u128)?
        } else {
            0
        };
        page_bytes.checked_add(
            self.sparse
                .checked_reserve_future_bytes(growth.sparse_pages)?,
        )
    }

    pub fn try_reserve(&mut self, growth: SubsetIndexGrowth) -> Result<(), ()> {
        let needed = self.pages.len().checked_add(growth.pages).ok_or(())?;
        if needed > self.pages.capacity() {
            let capacity = needed.checked_next_power_of_two().ok_or(())?;
            self.pages
                .try_reserve_exact(capacity - self.pages.len())
                .map_err(|_| ())?;
        }
        self.sparse.try_reserve(growth.sparse_pages).map_err(|_| ())
    }

    /// The caller admits and reserves growth before inserting any child. This
    /// method only initializes reached pages and never grows either owner.
    pub fn insert_reserved(&mut self, subset: u64, node: u32) -> Option<u32> {
        assert_ne!(node, MISSING, "the absent-node marker is not a node index");
        let page_key = subset >> PAGE_BITS;
        let page = match self.page_index(page_key) {
            Some(page) => page,
            None => {
                assert!(self.pages.len() < self.pages.capacity());
                let page = u32::try_from(self.pages.len()).expect("subset page index fits u32");
                self.pages.push(SubsetPage {
                    key: page_key,
                    nodes: [MISSING; PAGE_SLOTS],
                });
                if page_key < DIRECT_PAGES as u64 {
                    self.direct[page_key as usize] = page;
                } else {
                    assert!(self.sparse.len() < self.sparse.capacity());
                    self.sparse.insert(page_key, page);
                }
                page
            }
        };
        let slot = &mut self.pages[page as usize].nodes[subset as usize & (PAGE_SLOTS - 1)];
        let previous = core::mem::replace(slot, node);
        if previous == MISSING {
            None
        } else {
            Some(previous)
        }
    }

    pub fn retained_bytes(&self) -> usize {
        let sparse_bytes = if self.sparse.capacity() == 0 {
            0
        } else {
            self.sparse.capacity().saturating_mul(8).div_ceil(7)
                * (self.sparse.storage().slot_payload_bytes() + 1)
                + 16
        };
        core::mem::size_of_val(&self.direct)
            + self.pages.capacity() * core::mem::size_of::<SubsetPage>()
            + sparse_bytes
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn insert(index: &mut SubsetNodeIndex, subset: u64, node: u32) -> Option<u32> {
        let growth = index.growth_for_key(subset);
        let before = index.retained_bytes() as u128;
        let future = index.checked_reserve_future_bytes(growth).unwrap();
        index.try_reserve(growth).unwrap();
        let previous = index.insert_reserved(subset, node);
        assert!(index.retained_bytes() as u128 <= before + future);
        previous
    }

    #[test]
    fn all_fifteen_and_sixteen_operation_subsets_use_one_index() {
        let mut index = SubsetNodeIndex::new();
        for operations in [15, 16] {
            index.clear();
            for subset in 0..1_u64 << operations {
                assert_eq!(insert(&mut index, subset, subset as u32), None);
            }
            for subset in 0..1_u64 << operations {
                assert_eq!(index.get(subset), Some(subset as u32));
            }
            assert_eq!(index.pages.len(), (1_usize << operations) / PAGE_SLOTS);
            assert_eq!(index.sparse.len(), 0);
        }
    }

    #[test]
    fn sparse_high_keys_and_reuse_match_an_exact_reference() {
        let mut index = SubsetNodeIndex::new();
        let mut reference = HashMap::new();
        for pass in 0..3 {
            index.clear();
            reference.clear();
            assert!(index.is_empty());
            let mut key = 0x3141_5926_5358_9793_u64;
            for node in 0..4096_u32 {
                key = key.wrapping_mul(6364136223846793005).wrapping_add(1);
                let subset = if node % 3 == 0 {
                    key & 0xffff
                } else {
                    key & ((1 << 60) - 1)
                };
                assert_eq!(
                    insert(&mut index, subset, node + pass),
                    reference.insert(subset, node + pass)
                );
            }
            for (&subset, &node) in &reference {
                assert_eq!(index.get(subset), Some(node));
            }
            assert_eq!(
                index
                    .pages
                    .iter()
                    .flat_map(|page| page.nodes)
                    .filter(|&node| node != MISSING)
                    .count(),
                reference.len(),
            );
        }
    }

    #[test]
    fn child_reserve_covers_sixty_operations_without_lattice_allocation() {
        let mut index = SubsetNodeIndex::new();
        insert(&mut index, 0, 0);
        let growth = index.growth_for_children(0, (1 << 60) - 1);
        assert_eq!(growth.pages, 54);
        assert_eq!(growth.sparse_pages, 44);
        index.try_reserve(growth).unwrap();
        let reserved = index.retained_bytes();
        for operation in 0..60 {
            assert_eq!(index.insert_reserved(1 << operation, operation + 1), None);
            assert_eq!(index.get(1 << operation), Some(operation + 1));
        }
        assert_eq!(index.retained_bytes(), reserved);
        assert!(reserved < 32 * 1024);
    }
}
