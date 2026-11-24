//! Allocation-free heap sorting with one sift edge per cooperative step.
//! Catalogs and candidate sets may grow with the request; no complete sort
//! is hidden inside a browser-worker quantum.
use std::cmp::Ordering;

pub(crate) struct CooperativeSort {
    heap_next: usize,
    end: usize,
    sift: Option<usize>,
}

impl CooperativeSort {
    pub(crate) fn new(len: usize) -> Self {
        Self {
            heap_next: len / 2,
            end: len,
            sift: None,
        }
    }

    /// Returns true only when every element is in canonical ascending order.
    pub(crate) fn step<T>(
        &mut self,
        values: &mut [T],
        mut compare: impl FnMut(&T, &T) -> Ordering,
    ) -> bool {
        if let Some(root) = self.sift {
            let left = root * 2 + 1;
            if left >= self.end {
                self.sift = None;
                return false;
            }
            let right = left + 1;
            let child = if right < self.end && compare(&values[left], &values[right]).is_lt() {
                right
            } else {
                left
            };
            if compare(&values[root], &values[child]).is_lt() {
                values.swap(root, child);
                self.sift = Some(child);
            } else {
                self.sift = None;
            }
            return false;
        }
        if self.heap_next > 0 {
            self.heap_next -= 1;
            self.sift = Some(self.heap_next);
            return false;
        }
        if self.end <= 1 {
            return true;
        }
        self.end -= 1;
        values.swap(0, self.end);
        self.sift = Some(0);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arbitrary_quantums_preserve_the_total_order() {
        for len in [0, 1, 2, 19, 256, 4097] {
            let mut values = (0..len).map(|i| (i * 47 + 19) % 137).collect::<Vec<_>>();
            let mut expected = values.clone();
            expected.sort_unstable();
            let mut sort = CooperativeSort::new(values.len());
            let mut work = 0;
            while !sort.step(&mut values, Ord::cmp) {
                work += 1;
                assert!(work < 1_000_000);
            }
            assert_eq!(values, expected);
        }
    }
}
