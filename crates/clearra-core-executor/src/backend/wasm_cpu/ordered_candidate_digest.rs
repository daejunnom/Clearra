//! Ordered candidate evidence with allocation independent of future work.
//!
//! FNV cannot combine independently folded branches without changing the
//! accepted digest. Retain the original values in pages and fold them in
//! ordinal order. The first page starts small; later pages have fixed capacity.
//! Storage is proportional to candidates actually observed; a large geometric
//! family never causes a whole-family reservation.

use core::mem::size_of;

const VALUES_PER_PAGE: usize = 1_024;
const INITIAL_PAGE_CAPACITY: usize = 4;

#[derive(Default)]
pub(super) struct OrderedCandidateDigest {
    pages: Vec<Vec<u64>>,
    len: usize,
    page_bytes: usize,
}

impl OrderedCandidateDigest {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn get(&self, index: usize) -> Option<&u64> {
        if index >= self.len {
            return None;
        }
        self.pages
            .get(index / VALUES_PER_PAGE)?
            .get(index % VALUES_PER_PAGE)
    }

    pub fn iter(&self) -> impl Iterator<Item = &u64> {
        self.pages.iter().flat_map(|page| page.iter())
    }

    pub fn checked_retained_bytes(&self) -> Option<usize> {
        self.pages
            .capacity()
            .checked_mul(size_of::<Vec<u64>>())?
            .checked_add(self.page_bytes)
    }

    pub fn retained_bytes(&self) -> usize {
        self.checked_retained_bytes().unwrap_or(usize::MAX)
    }

    /// Additional peak before the next append. First-page and directory growth
    /// include complete new backing because their old allocations may coexist.
    pub fn checked_next_growth_bytes(&self) -> Option<u128> {
        self.len.checked_add(1)?;
        if let Some(capacity) = self.first_page_growth_capacity() {
            return (capacity as u128).checked_mul(size_of::<u64>() as u128);
        }
        if !self.needs_page() {
            return Some(0);
        }
        let capacity = self.next_directory_capacity()?;
        let directory = if capacity == self.pages.capacity() {
            0
        } else {
            (capacity as u128).checked_mul(size_of::<Vec<u64>>() as u128)?
        };
        directory.checked_add((self.new_page_capacity() * size_of::<u64>()) as u128)
    }

    /// The resource owner admits checked_next_growth_bytes before this call.
    /// A reserved page remains empty until verification accepts its ordinal.
    pub fn try_reserve_next(&mut self) -> Result<(), ()> {
        self.len.checked_add(1).ok_or(())?;
        if let Some(capacity) = self.first_page_growth_capacity() {
            let page = &mut self.pages[0];
            let previous_capacity = page.capacity();
            page.try_reserve_exact(capacity - page.len())
                .map_err(|_| ())?;
            let added = page
                .capacity()
                .checked_sub(previous_capacity)
                .and_then(|count| count.checked_mul(size_of::<u64>()))
                .ok_or(())?;
            self.page_bytes = self.page_bytes.checked_add(added).ok_or(())?;
            return Ok(());
        }
        if !self.needs_page() {
            return Ok(());
        }
        let capacity = self.next_directory_capacity().ok_or(())?;
        if capacity > self.pages.capacity() {
            self.pages
                .try_reserve_exact(capacity - self.pages.len())
                .map_err(|_| ())?;
        }
        let mut page = Vec::new();
        page.try_reserve_exact(self.new_page_capacity())
            .map_err(|_| ())?;
        let page_bytes = page.capacity().checked_mul(size_of::<u64>()).ok_or(())?;
        self.page_bytes = self.page_bytes.checked_add(page_bytes).ok_or(())?;
        self.pages.push(page);
        Ok(())
    }

    pub fn push_reserved(&mut self, value: u64) -> Result<(), ()> {
        let len = self.len.checked_add(1).ok_or(())?;
        let page = self.pages.last_mut().ok_or(())?;
        if page.len() == VALUES_PER_PAGE || page.len() == page.capacity() {
            return Err(());
        }
        page.push(value);
        self.len = len;
        Ok(())
    }

    pub fn try_push(&mut self, value: u64) -> Result<(), ()> {
        self.try_reserve_next()?;
        self.push_reserved(value)
    }

    fn needs_page(&self) -> bool {
        self.pages
            .last()
            .is_none_or(|page| page.len() == VALUES_PER_PAGE)
    }

    fn first_page_growth_capacity(&self) -> Option<usize> {
        if self.pages.len() != 1 {
            return None;
        }
        let page = &self.pages[0];
        (page.len() == page.capacity() && page.capacity() < VALUES_PER_PAGE)
            .then(|| (page.capacity() * 2).clamp(INITIAL_PAGE_CAPACITY, VALUES_PER_PAGE))
    }

    fn new_page_capacity(&self) -> usize {
        if self.pages.is_empty() {
            INITIAL_PAGE_CAPACITY
        } else {
            VALUES_PER_PAGE
        }
    }

    fn next_directory_capacity(&self) -> Option<usize> {
        if self.pages.capacity() == 0 {
            Some(1)
        } else if self.pages.len() < self.pages.capacity() {
            Some(self.pages.capacity())
        } else {
            self.pages
                .len()
                .checked_add(1)?
                .checked_mul(2)
                .map(|n| n.max(4))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paged_ordered_digest_matches_vec_across_page_boundaries() {
        let mut ledger = OrderedCandidateDigest::default();
        let mut reference = Vec::new();
        for index in 0..VALUES_PER_PAGE * 5 + 17 {
            let value = match index % 7 {
                0 => 0,
                1 => u64::MAX,
                2 | 3 => 19,
                _ => (index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15),
            };
            let before = ledger.retained_bytes() as u128;
            let old_directory = ledger.pages.capacity();
            let old_first_page = ledger.pages.first().map_or(0, Vec::capacity);
            let future = ledger.checked_next_growth_bytes().unwrap();
            ledger.try_reserve_next().unwrap();
            let retired_directory = if ledger.pages.capacity() != old_directory {
                old_directory * size_of::<Vec<u64>>()
            } else {
                0
            };
            let retired_first_page = if ledger.pages[0].capacity() != old_first_page {
                old_first_page * size_of::<u64>()
            } else {
                0
            };
            assert_eq!(
                before + future,
                ledger.retained_bytes() as u128 + (retired_directory + retired_first_page) as u128,
                "every replacement includes old and new backing at index {index}",
            );
            assert_eq!(ledger.checked_next_growth_bytes(), Some(0));
            ledger.push_reserved(value).unwrap();
            reference.push(value);
            assert_eq!(ledger.len(), reference.len());
            assert_eq!(ledger.get(index), Some(&value));
        }
        assert_eq!(ledger.get(reference.len()), None);
        assert!(ledger.iter().copied().eq(reference.iter().copied()));
        for initial in [0, 1, u64::MAX] {
            let fold = |digest, &value| super::super::mix_digest(digest, value);
            assert_eq!(
                ledger.iter().fold(initial, fold),
                reference.iter().fold(initial, fold)
            );
        }
    }

    #[test]
    fn paged_ordered_digest_one_value_uses_small_first_page() {
        let mut ledger = OrderedCandidateDigest::default();
        ledger.try_push(19).unwrap();
        assert_eq!(ledger.pages.capacity(), 1);
        assert_eq!(ledger.pages[0].capacity(), INITIAL_PAGE_CAPACITY);
        assert_eq!(
            ledger.retained_bytes(),
            size_of::<Vec<u64>>() + INITIAL_PAGE_CAPACITY * size_of::<u64>(),
        );
        for value in 1..INITIAL_PAGE_CAPACITY {
            ledger.push_reserved(value as u64).unwrap();
        }
        assert!(ledger.push_reserved(23).is_err());
        let old_page_bytes = INITIAL_PAGE_CAPACITY * size_of::<u64>();
        assert_eq!(
            ledger.checked_next_growth_bytes(),
            Some((old_page_bytes * 2) as u128),
            "the first replacement is charged in full, not as a capacity delta",
        );
        ledger.try_push(23).unwrap();
        assert_eq!(ledger.get(INITIAL_PAGE_CAPACITY), Some(&23));
    }

    #[test]
    fn paged_ordered_digest_requires_reservation_before_append() {
        let mut ledger = OrderedCandidateDigest::default();
        assert!(ledger.push_reserved(1).is_err());
        assert_eq!(ledger.len(), 0);
        for value in 0..VALUES_PER_PAGE as u64 {
            ledger.try_push(value).unwrap();
        }
        let retained = ledger.retained_bytes();
        assert!(ledger.push_reserved(1).is_err());
        assert_eq!(ledger.retained_bytes(), retained);
        assert_eq!(ledger.len(), VALUES_PER_PAGE);
        assert_eq!(
            ledger.checked_next_growth_bytes(),
            Some((VALUES_PER_PAGE * size_of::<u64>() + 4 * size_of::<Vec<u64>>()) as u128)
        );
        ledger.try_push(u64::MAX).unwrap();
        assert_eq!(ledger.get(VALUES_PER_PAGE), Some(&u64::MAX));
    }
}
