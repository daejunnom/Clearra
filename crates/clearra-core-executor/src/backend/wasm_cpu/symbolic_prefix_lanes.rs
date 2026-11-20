//! Deterministic renumbering of original prefix lanes for a physical relation.
//!
//! A batch changes the lane coordinate only. Each packed lane has exactly
//! one source (original word, bit), independently of the piece or pose.

use super::symbolic_prefix_domain::{PrefixDomain, MAX_OPERATIONS, MAX_PREFIX_WORDS};

const LANES: u32 = u64::BITS;
type PrefixWords = [u64; MAX_PREFIX_WORDS];

pub(super) struct PrefixLaneBatch {
    segments: PrefixWords,
    lane_count: u32,
}

impl PrefixLaneBatch {
    /// Consume the lowest available original prefix indices, up to 64.
    /// Refusing an out-of-domain selection leaves the owner unchanged.
    pub(super) fn gather(selected: &mut PrefixWords, domain: &PrefixDomain) -> Option<Self> {
        if selected
            .iter()
            .enumerate()
            .any(|(word, &lanes)| lanes & !domain.nonterminal_lanes(word) != 0)
        {
            return None;
        }

        let mut segments = [0; MAX_PREFIX_WORDS];
        let mut lane_count = 0;
        for (selected, segment) in selected.iter_mut().zip(&mut segments) {
            let remaining = LANES - lane_count;
            if remaining == 0 {
                break;
            }
            if selected.count_ones() <= remaining {
                *segment = *selected;
            } else {
                let mut unused = *selected;
                for _ in 0..remaining {
                    let first = unused.isolate_lowest_one();
                    *segment |= first;
                    unused ^= first;
                }
            }
            *selected ^= *segment;
            lane_count += segment.count_ones();
        }
        (lane_count != 0).then_some(Self {
            segments,
            lane_count,
        })
    }

    pub(super) fn valid_lanes(&self) -> u64 {
        lowest_lanes(self.lane_count)
    }

    pub(super) fn segment_count(&self) -> usize {
        self.segments
            .iter()
            .filter(|&&segment| segment != 0)
            .count()
    }

    /// Restrict each original operation mask to the selected prefix lanes.
    /// Compress contiguous runs as words, without materializing per-lane
    /// owner vectors.
    pub(super) fn packed_owners(&self, domain: &PrefixDomain) -> [u64; MAX_OPERATIONS] {
        let mut owners = [0; MAX_OPERATIONS];
        let mut next = 0;
        for (word, &segment) in self.segments.iter().enumerate() {
            if segment == 0 {
                continue;
            }
            let original = domain.word_owners(word);
            for_each_run(segment, |source, packed, mask| {
                for (result, owner) in owners.iter_mut().zip(original) {
                    *result |= ((owner >> source) & mask) << (next + packed);
                }
            });
            next += segment.count_ones();
        }
        owners
    }

    /// Unpack only the current batch's reported positives and preserve every
    /// prior positive. A foreign result bit is refused before any writes.
    pub(super) fn scatter(&self, resolved: u64, accepted: &mut PrefixWords) -> bool {
        if resolved & !self.valid_lanes() != 0 {
            return false;
        }
        let mut next = 0;
        for (&segment, accepted) in self.segments.iter().zip(accepted) {
            for_each_run(segment, |source, packed, mask| {
                *accepted |= ((resolved >> (next + packed)) & mask) << source;
            });
            next += segment.count_ones();
        }
        true
    }
}

fn lowest_lanes(count: u32) -> u64 {
    u64::MAX.checked_shr(LANES - count).unwrap_or(0)
}

/// Visit nonempty contiguous runs in low-to-high order. Offsets point at
/// actual bits, so neither offset may be 64 when the visitor runs.
fn for_each_run(mut selected: u64, mut visit: impl FnMut(u32, u32, u64)) {
    let mut source = 0;
    let mut packed = 0;
    while selected != 0 {
        let skip = selected.trailing_zeros();
        selected >>= skip;
        source += skip;
        let len = selected.trailing_ones();
        visit(source, packed, lowest_lanes(len));
        if len == LANES {
            break;
        }
        selected >>= len;
        source += len;
        packed += len;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbolic_prefix_lane_batch_sparse_cross_word_owner_and_scatter() {
        let domain = PrefixDomain::new(10).unwrap();
        let mut selected = [0; MAX_PREFIX_WORDS];
        selected[0] = (1 << 1) | (1 << 63);
        selected[2] = (1 << 1) | (1 << 62);
        selected[13] = 1 << 28;

        let batch = PrefixLaneBatch::gather(&mut selected, &domain).unwrap();
        assert_eq!(selected, [0; MAX_PREFIX_WORDS]);
        assert!(PrefixLaneBatch::gather(&mut selected, &domain).is_none());
        assert_eq!(batch.valid_lanes(), 0b1_1111);
        assert_eq!(batch.segment_count(), 3);
        let prefixes = [1, 63, 129, 190, 860];
        for (operation, owner) in batch.packed_owners(&domain).into_iter().enumerate() {
            let expected = prefixes.iter().enumerate().fold(0, |bits, (lane, prefix)| {
                bits | (u64::from(prefix & (1 << operation) != 0) << lane)
            });
            assert_eq!(owner, expected, "operation {operation}");
        }

        let mut accepted = [0; MAX_PREFIX_WORDS];
        accepted[0] = 1 << 62;
        accepted[15] = 1;
        let original = accepted;
        assert!(!batch.scatter((1 << 5) | 1, &mut accepted));
        assert_eq!(accepted, original);
        assert!(batch.scatter(0b1_0101, &mut accepted));
        let mut expected = original;
        expected[0] |= 1 << 1;
        expected[2] |= 1 << 1;
        expected[13] |= 1 << 28;
        assert_eq!(accepted, expected);
    }

    #[test]
    fn symbolic_prefix_lane_batch_full_partial_word_and_prior_positives() {
        let domain = PrefixDomain::new(10).unwrap();
        let mut selected = [0; MAX_PREFIX_WORDS];
        selected[0] = !(1 << 4);
        selected[1] = (1 << 11) | (1 << 48);
        selected[15] = 1 << 62;
        let expected = selected;
        let mut accepted = [0; MAX_PREFIX_WORDS];

        let full = PrefixLaneBatch::gather(&mut selected, &domain).unwrap();
        assert_eq!(full.valid_lanes(), u64::MAX);
        assert_eq!(full.segment_count(), 2);
        assert_eq!(full.packed_owners(&domain)[6], 1 << 63);
        assert_eq!(selected[1], 1 << 48);
        assert!(full.scatter(u64::MAX, &mut accepted));
        assert_eq!(accepted[1], 1 << 11);

        let tail = PrefixLaneBatch::gather(&mut selected, &domain).unwrap();
        assert_eq!(tail.valid_lanes(), 0b11);
        assert_eq!(tail.segment_count(), 2);
        assert!(tail.scatter(0b11, &mut accepted));
        assert_eq!(accepted, expected);
        assert_eq!(selected, [0; MAX_PREFIX_WORDS]);
    }

    #[test]
    fn symbolic_prefix_lane_batch_invalid_request_is_not_consumed() {
        let domain = PrefixDomain::new(3).unwrap();
        let mut selected = [0; MAX_PREFIX_WORDS];
        selected[0] = 1 | (1 << 7); // Includes the terminal.
        let original = selected;
        assert!(PrefixLaneBatch::gather(&mut selected, &domain).is_none());
        assert_eq!(selected, original);

        selected[0] = 1;
        selected[1] = 1; // Includes a word outside this smaller domain.
        let original = selected;
        assert!(PrefixLaneBatch::gather(&mut selected, &domain).is_none());
        assert_eq!(selected, original);
    }
}
