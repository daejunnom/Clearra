use super::WasmExactSearchError;

const PAGE_SHIFT: usize = 8;
const PAGE_SIZE: usize = 1 << PAGE_SHIFT;
const PAGE_MASK: usize = PAGE_SIZE - 1;
const PAGE_WORD_COUNT: usize = PAGE_SIZE / u64::BITS as usize;
const NO_PAGE: u32 = u32::MAX;

struct GenerationPage<T> {
    initialized: [u64; PAGE_WORD_COUNT],
    values: Box<[T]>,
}

impl<T> GenerationPage<T> {
    fn clear(&mut self) {
        self.initialized.fill(0);
    }

    fn contains(&self, slot: usize) -> bool {
        self.initialized[slot / u64::BITS as usize] & (1_u64 << (slot % u64::BITS as usize)) != 0
    }

    fn insert(&mut self, slot: usize) -> bool {
        let word = &mut self.initialized[slot / u64::BITS as usize];
        let bit = 1_u64 << (slot % u64::BITS as usize);
        let first = *word & bit == 0;
        *word |= bit;
        first
    }
}

pub(super) struct SegmentedGenerationArray<T> {
    capacity: usize,
    directory: Vec<u64>,
    pages: Vec<GenerationPage<T>>,
    generation: u32,
    next_page: usize,
}

impl<T: Clone + Default> SegmentedGenerationArray<T> {
    pub(super) fn new(capacity: usize) -> Result<Self, WasmExactSearchError> {
        let directory_len = capacity.div_ceil(PAGE_SIZE);
        let mut directory = Vec::new();
        directory.try_reserve_exact(directory_len).map_err(|_| {
            WasmExactSearchError::InvalidProblem(
                "setup_parallel_segment_directory_storage_unavailable",
            )
        })?;
        directory.resize(directory_len, 0);
        Ok(Self {
            capacity,
            directory,
            pages: Vec::new(),
            generation: 0,
            next_page: 0,
        })
    }

    pub(super) fn begin_generation(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        if self.generation == 0 {
            self.directory.fill(0);
            self.generation = 1;
        }
        self.next_page = 0;
    }

    pub(super) fn get(&self, index: usize) -> Option<&T> {
        if index >= self.capacity {
            return None;
        }
        let encoded = self.directory[index >> PAGE_SHIFT];
        if (encoded >> 32) as u32 != self.generation {
            return None;
        }
        let page_index = (encoded as u32).wrapping_sub(1) as usize;
        let slot = index & PAGE_MASK;
        let page = &self.pages[page_index];
        page.contains(slot).then_some(&page.values[slot])
    }

    pub(super) fn get_mut_or_default(
        &mut self,
        index: usize,
    ) -> Result<(&mut T, bool), WasmExactSearchError> {
        if index >= self.capacity {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_parallel_segment_index_out_of_range",
            ));
        }
        let logical_page = index >> PAGE_SHIFT;
        let encoded = self.directory[logical_page];
        let page_index = if (encoded >> 32) as u32 == self.generation {
            (encoded as u32).wrapping_sub(1) as usize
        } else {
            let page_index = self.next_page;
            self.next_page += 1;
            self.ensure_page(page_index)?;
            self.pages[page_index].clear();
            self.directory[logical_page] =
                (u64::from(self.generation) << 32) | (page_index as u64 + 1);
            page_index
        };
        let slot = index & PAGE_MASK;
        let page = &mut self.pages[page_index];
        let first = page.insert(slot);
        if first {
            page.values[slot] = T::default();
        }
        Ok((&mut page.values[slot], first))
    }

    pub(super) fn active_page_count(&self) -> usize {
        self.next_page
    }

    fn ensure_page(&mut self, page_index: usize) -> Result<(), WasmExactSearchError> {
        if page_index < self.pages.len() {
            return Ok(());
        }
        self.pages.try_reserve(1).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_parallel_segment_page_list_unavailable")
        })?;
        let mut page = Vec::new();
        page.try_reserve_exact(PAGE_SIZE).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_parallel_segment_page_unavailable")
        })?;
        page.resize_with(PAGE_SIZE, T::default);
        self.pages.push(GenerationPage {
            initialized: [0; PAGE_WORD_COUNT],
            values: page.into_boxed_slice(),
        });
        Ok(())
    }
}

pub(super) struct SegmentedArray<T> {
    capacity: usize,
    directory: Vec<u32>,
    pages: Vec<Box<[T]>>,
}

impl<T: Clone + Default> SegmentedArray<T> {
    pub(super) fn new(capacity: usize) -> Result<Self, WasmExactSearchError> {
        let directory_len = capacity.div_ceil(PAGE_SIZE);
        let mut directory = Vec::new();
        directory.try_reserve_exact(directory_len).map_err(|_| {
            WasmExactSearchError::InvalidProblem("setup_parallel_accumulator_directory_unavailable")
        })?;
        directory.resize(directory_len, NO_PAGE);
        Ok(Self {
            capacity,
            directory,
            pages: Vec::new(),
        })
    }

    pub(super) fn get(&self, index: usize) -> Option<&T> {
        if index >= self.capacity {
            return None;
        }
        let page_index = *self.directory.get(index >> PAGE_SHIFT)?;
        if page_index == NO_PAGE {
            return None;
        }
        self.pages
            .get(page_index as usize)
            .and_then(|page| page.get(index & PAGE_MASK))
    }

    pub(super) fn get_mut_or_default(
        &mut self,
        index: usize,
    ) -> Result<&mut T, WasmExactSearchError> {
        if index >= self.capacity {
            return Err(WasmExactSearchError::InvalidProblem(
                "setup_parallel_accumulator_index_out_of_range",
            ));
        }
        let logical_page = index >> PAGE_SHIFT;
        let page_index = if self.directory[logical_page] == NO_PAGE {
            let page_index = u32::try_from(self.pages.len()).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "setup_parallel_accumulator_page_index_overflow",
                )
            })?;
            self.pages.try_reserve(1).map_err(|_| {
                WasmExactSearchError::InvalidProblem(
                    "setup_parallel_accumulator_page_list_unavailable",
                )
            })?;
            let mut page = Vec::new();
            page.try_reserve_exact(PAGE_SIZE).map_err(|_| {
                WasmExactSearchError::InvalidProblem("setup_parallel_accumulator_page_unavailable")
            })?;
            page.resize(PAGE_SIZE, T::default());
            self.pages.push(page.into_boxed_slice());
            self.directory[logical_page] = page_index;
            page_index
        } else {
            self.directory[logical_page]
        };
        Ok(&mut self.pages[page_index as usize][index & PAGE_MASK])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_array_reuses_sparse_pages_without_exposing_old_values() {
        let mut values = SegmentedGenerationArray::<u32>::new(PAGE_SIZE * 5).expect("array");

        values.begin_generation();
        *values.get_mut_or_default(3).expect("first slot").0 = 11;
        *values
            .get_mut_or_default(PAGE_SIZE * 4 + 7)
            .expect("second page")
            .0 = 29;
        assert_eq!(values.active_page_count(), 2);
        assert_eq!(values.get(3), Some(&11));

        values.begin_generation();
        assert_eq!(values.get(3), None);
        *values
            .get_mut_or_default(PAGE_SIZE + 5)
            .expect("new generation")
            .0 = 41;
        assert_eq!(values.active_page_count(), 1);
        assert_eq!(values.get(PAGE_SIZE + 5), Some(&41));
        assert_eq!(values.pages.len(), 2);
    }

    #[test]
    fn segmented_array_allocates_only_touched_logical_pages() {
        let mut values = SegmentedArray::<u32>::new(PAGE_SIZE * 6).expect("array");

        *values.get_mut_or_default(1).expect("first") = 7;
        *values.get_mut_or_default(PAGE_SIZE * 5 + 1).expect("last") = 13;

        assert_eq!(values.pages.len(), 2);
        assert_eq!(values.get(1), Some(&7));
        assert_eq!(values.get(PAGE_SIZE * 5 + 1), Some(&13));
        assert_eq!(values.get(PAGE_SIZE * 2), None);
    }
}
