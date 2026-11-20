//! Exact finite domain for the test-only symbolic placement prefix relation.
//!
//! The bits of a concrete executed subset are its numeric identity. The low
//! six bits locate a lane, the remaining bits select a word. Dependency words
//! are labelled by the actual operation, not its (possibly repeated) piece.

pub(super) const MAX_OPERATIONS: usize = 10;
const LANE_OPERATIONS: usize = 6;
const LANES: usize = u64::BITS as usize;
pub(super) const MAX_PREFIX_WORDS: usize = 1 << (MAX_OPERATIONS - LANE_OPERATIONS);

const IN_WORD_OWNER_MASKS: [u64; LANE_OPERATIONS] = [
    0xaaaa_aaaa_aaaa_aaaa,
    0xcccc_cccc_cccc_cccc,
    0xf0f0_f0f0_f0f0_f0f0,
    0xff00_ff00_ff00_ff00,
    0xffff_0000_ffff_0000,
    0xffff_ffff_0000_0000,
];

type PrefixWords = [u64; MAX_PREFIX_WORDS];
type OperationParents = [PrefixWords; MAX_OPERATIONS];

pub(super) struct PrefixDomain {
    operations: usize,
    word_count: usize,
    lanes_per_word: u64,
}

impl PrefixDomain {
    pub(super) fn new(operations: usize) -> Option<Self> {
        if !(1..=MAX_OPERATIONS).contains(&operations) {
            return None;
        }
        let prefix_count = 1_usize << operations;
        let word_count = prefix_count.div_ceil(LANES);
        let lanes_per_word = u64::MAX >> (LANES - prefix_count.min(LANES));
        Some(Self {
            operations,
            word_count,
            lanes_per_word,
        })
    }

    pub(super) const fn word_count(&self) -> usize {
        self.word_count
    }

    pub(super) const fn terminal(&self) -> usize {
        (1 << self.operations) - 1
    }

    pub(super) fn word_owners(&self, word: usize) -> [u64; MAX_OPERATIONS] {
        let mut owners = [0; MAX_OPERATIONS];
        if word >= self.word_count {
            return owners;
        }
        for (operation, mask) in owners[..self.operations].iter_mut().enumerate() {
            *mask = if operation < LANE_OPERATIONS {
                self.lanes_per_word & IN_WORD_OWNER_MASKS[operation]
            } else if word & (1 << (operation - LANE_OPERATIONS)) != 0 {
                self.lanes_per_word
            } else {
                0
            };
        }
        owners
    }

    pub(super) fn nonterminal_lanes(&self, word: usize) -> u64 {
        if word >= self.word_count {
            return 0;
        }
        let terminal = self.terminal();
        if word == terminal / LANES {
            self.lanes_per_word & !(1_u64 << (terminal % LANES))
        } else {
            self.lanes_per_word
        }
    }

    /// Intersect the terminal's exact predecessors with the start's
    /// successors. A zero result means no complete operation word.
    ///
    /// Cross-word edges always increase the word index; sweep those once
    /// in topological order. Within a word, at most six operations remain.
    pub(super) fn live_from_start(
        &self,
        permitted: &OperationParents,
        composition_words: &mut usize,
    ) -> PrefixWords {
        let parents = self.sanitized_parents(permitted);
        self.live_from_legal_parents(&parents, composition_words)
    }

    /// Return exactly the source lanes of upper edges lying on a complete
    /// upper path. The edge itself must be present as well as its endpoints:
    /// a reachable source can connect to a dead child and a live child can
    /// have an incoming edge from an unreachable source.
    ///
    /// If every exact edge is contained in the upper relation, every edge of
    /// every complete exact path survives this restriction. The result does
    /// not certify that any remaining edge is exact.
    pub(super) fn potentially_live_edges(
        &self,
        upper: &OperationParents,
        composition_words: &mut usize,
    ) -> OperationParents {
        let parents = self.sanitized_parents(upper);
        let live = self.live_from_legal_parents(&parents, composition_words);
        let mut result = [[0; MAX_PREFIX_WORDS]; MAX_OPERATIONS];
        if live[0] & 1 != 0 {
            self.for_each_transfer(
                &parents,
                composition_words,
                |operation, parent, child, shift, allowed| {
                    result[operation][parent] = allowed & live[parent] & (live[child] >> shift);
                },
            );
        }
        result
    }

    fn sanitized_parents(&self, permitted: &OperationParents) -> OperationParents {
        let mut parents = [[0; MAX_PREFIX_WORDS]; MAX_OPERATIONS];
        for word in 0..self.word_count {
            let owners = self.word_owners(word);
            let lanes = self.nonterminal_lanes(word);
            for (operation, owner) in owners[..self.operations].iter().enumerate() {
                parents[operation][word] = permitted[operation][word] & lanes & !owner;
            }
        }
        parents
    }

    fn live_from_legal_parents(
        &self,
        parents: &OperationParents,
        composition_words: &mut usize,
    ) -> PrefixWords {
        let (within_word, between_words) =
            parents[..self.operations].split_at(self.operations.min(LANE_OPERATIONS));
        let mut completable = [0; MAX_PREFIX_WORDS];
        let terminal = self.terminal();
        completable[terminal / LANES] = 1_u64 << (terminal % LANES);
        for word in (0..self.word_count).rev() {
            let mut known = completable[word];
            // Higher words already include their full local closure.
            for (bit, allowed) in between_words.iter().enumerate() {
                let sources = allowed[word];
                if sources != 0 {
                    *composition_words += 1;
                    known |= completable[word + (1 << bit)] & sources;
                }
            }
            completable[word] = close_word(
                known,
                within_word,
                word,
                composition_words,
                |bits, shift, sources| (bits >> shift) & sources,
            );
        }

        let mut reachable = [0; MAX_PREFIX_WORDS];
        if completable[0] & 1 == 0 {
            return reachable;
        }
        reachable[0] = 1;
        for word in 0..self.word_count {
            let known = reachable[word];
            if known == 0 {
                continue;
            }
            let live = close_word(
                known,
                within_word,
                word,
                composition_words,
                |bits, shift, sources| ((bits & sources) << shift) & completable[word],
            );
            reachable[word] = live;
            for (bit, allowed) in between_words.iter().enumerate() {
                let sources = allowed[word];
                if sources != 0 {
                    *composition_words += 1;
                    let child = word + (1 << bit);
                    reachable[child] |= live & sources & completable[child];
                }
            }
        }
        reachable
    }

    fn for_each_transfer(
        &self,
        parents: &OperationParents,
        composition_words: &mut usize,
        mut transfer: impl FnMut(usize, usize, usize, usize, u64),
    ) {
        for (operation, words) in parents[..self.operations].iter().enumerate() {
            // Setting an unexecuted low bit moves lanes in the same
            // word; setting an unexecuted high bit moves words and keeps
            // precisely the same lane. The parent filter already excludes
            // sources with bit q set, so neither addition can carry.
            let (word_delta, lane_delta) = if operation < LANE_OPERATIONS {
                (0, 1 << operation)
            } else {
                (1 << (operation - LANE_OPERATIONS), 0)
            };
            for (parent, &allowed) in words[..self.word_count].iter().enumerate() {
                if allowed != 0 {
                    *composition_words += 1;
                    transfer(operation, parent, parent + word_delta, lane_delta, allowed);
                }
            }
        }
    }
}

/// Closing one word never changes a high operation bit. In-place updates
/// can propagate farther in a round, but no path has more than six steps.
fn close_word(
    mut reached: u64,
    operations: &[PrefixWords],
    word: usize,
    composition_words: &mut usize,
    transfer: impl Fn(u64, usize, u64) -> u64,
) -> u64 {
    if reached == 0 {
        return 0;
    }
    for _ in operations {
        let previous = reached;
        for (bit, allowed) in operations.iter().enumerate() {
            let sources = allowed[word];
            if sources != 0 {
                *composition_words += 1;
                reached |= transfer(reached, 1 << bit, sources);
            }
        }
        if reached == previous {
            break;
        }
    }
    reached
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_bit(words: &mut PrefixWords, prefix: usize) {
        words[prefix / LANES] |= 1_u64 << (prefix % LANES);
    }

    fn has_bit(words: &PrefixWords, prefix: usize) -> bool {
        words[prefix / LANES] & (1_u64 << (prefix % LANES)) != 0
    }

    fn scalar_live(operations: usize, permitted: &OperationParents) -> PrefixWords {
        let terminal = (1_usize << operations) - 1;
        let mut completable = vec![false; terminal + 1];
        completable[terminal] = true;
        let children = |prefix: usize| {
            permitted[..operations]
                .iter()
                .enumerate()
                .filter(move |(q, word)| prefix & (1 << q) == 0 && has_bit(word, prefix))
                .map(move |(q, _)| prefix | (1 << q))
        };
        for prefix in (0..terminal).rev() {
            completable[prefix] = children(prefix).any(|child| completable[child]);
        }
        let mut live = [0; MAX_PREFIX_WORDS];
        if !completable[0] {
            return live;
        }
        with_bit(&mut live, 0);
        for prefix in 0..terminal {
            if has_bit(&live, prefix) {
                for child in children(prefix).filter(|&child| completable[child]) {
                    with_bit(&mut live, child);
                }
            }
        }
        live
    }

    fn scalar_live_edges(operations: usize, permitted: &OperationParents) -> OperationParents {
        let live = scalar_live(operations, permitted);
        let mut result = [[0; MAX_PREFIX_WORDS]; MAX_OPERATIONS];
        for prefix in 0..(1_usize << operations) {
            if !has_bit(&live, prefix) {
                continue;
            }
            for (q, result) in result[..operations].iter_mut().enumerate() {
                let bit = 1 << q;
                if prefix & bit == 0
                    && has_bit(&permitted[q], prefix)
                    && has_bit(&live, prefix | bit)
                {
                    with_bit(result, prefix);
                }
            }
        }
        result
    }

    #[test]
    fn symbolic_prefix_word_owners_match_every_concrete_subset() {
        assert!(PrefixDomain::new(0).is_none());
        assert!(PrefixDomain::new(11).is_none());
        for operations in 1..=MAX_OPERATIONS {
            let domain = PrefixDomain::new(operations).unwrap();
            let terminal = (1_usize << operations) - 1;
            assert_eq!(domain.terminal(), terminal);
            assert_eq!(domain.word_count(), (terminal + 1).div_ceil(LANES));
            for word in 0..=MAX_PREFIX_WORDS {
                let owners = domain.word_owners(word);
                for lane in 0..LANES {
                    let prefix = word * LANES + lane;
                    let flag = 1_u64 << lane;
                    let parent = domain.nonterminal_lanes(word) & flag != 0;
                    assert_eq!(
                        parent,
                        prefix < terminal,
                        "operations={operations} prefix={prefix}"
                    );
                    for (q, &owner) in owners.iter().enumerate() {
                        assert_eq!(
                            owner & flag != 0,
                            prefix <= terminal && q < operations && prefix & (1 << q) != 0,
                            "operations={operations} prefix={prefix} q={q}",
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn symbolic_prefix_word_closure_matches_scalar_across_boundaries() {
        let mut random = 0xa8b0_d5e4_af8d_b263_u64;
        let mut next = || {
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            random
        };
        for operations in 1..=MAX_OPERATIONS {
            let domain = PrefixDomain::new(operations).unwrap();
            for trial in 0..96 {
                let mut permitted = [[0; MAX_PREFIX_WORDS]; MAX_OPERATIONS];
                for words in &mut permitted {
                    for word in words {
                        // Deliberately include unused lanes, executed bits
                        // and surplus words; only legal sources may transfer.
                        *word = match trial {
                            0 => u64::MAX,
                            1 => 0,
                            _ => match trial % 3 {
                                0 => next() & next(),
                                1 => next(),
                                _ => next() | next(),
                            },
                        };
                    }
                }
                let mut work = 0;
                let actual = domain.live_from_start(&permitted, &mut work);
                assert_eq!(
                    actual,
                    scalar_live(operations, &permitted),
                    "operations={operations} trial={trial}",
                );
                assert!(work <= 2 * operations * operations * domain.word_count());
                work = 0;
                assert_eq!(
                    domain.potentially_live_edges(&permitted, &mut work),
                    scalar_live_edges(operations, &permitted),
                    "edge restriction: operations={operations} trial={trial}",
                );
                assert!(work <= (2 * operations + 1) * operations * domain.word_count());
            }
        }
    }

    #[test]
    fn symbolic_prefix_word_closure_keeps_distinct_operations_and_drops_dead_branches() {
        // Concrete operations 0 and 6 can even have the same mino kind.
        // They are still independent choices on opposite sides of a word
        // boundary. Both orders can complete; a separate root edge is dead.
        let domain = PrefixDomain::new(7).unwrap();
        let mut permitted = [[0; MAX_PREFIX_WORDS]; MAX_OPERATIONS];
        for (prefix, operation) in [(0, 0), (0, 6), (1, 6), (64, 0), (0, 1)] {
            with_bit(&mut permitted[operation], prefix);
        }
        let mut prefix = 65;
        for (q, parents) in permitted.iter_mut().enumerate().take(6).skip(1) {
            with_bit(parents, prefix);
            prefix |= 1 << q;
        }
        assert_eq!(prefix, domain.terminal());
        let live = |parents: &OperationParents| domain.live_from_start(parents, &mut 0);
        let actual = live(&permitted);
        assert_eq!(actual, scalar_live(7, &permitted));
        assert!(has_bit(&actual, 1) && has_bit(&actual, 64));
        assert!(!has_bit(&actual, 2));
        permitted[6][0] &= !(1 << 1);
        let actual = live(&permitted);
        assert_eq!(actual, scalar_live(7, &permitted));
        assert!(!has_bit(&actual, 1) && has_bit(&actual, 64));
        permitted[0][1] = 0;
        assert_eq!(live(&permitted), [0; MAX_PREFIX_WORDS]);
    }

    #[test]
    fn symbolic_prefix_word_closure_crosses_each_word_boundary_once_per_direction() {
        let domain = PrefixDomain::new(10).unwrap();
        let mut allowed = [[0; MAX_PREFIX_WORDS]; MAX_OPERATIONS];
        let mut prefix = 0;
        // Force interleaving of local and cross-word operations; choose the
        // local order adversarially for the forward closure's scan order.
        for operation in [9, 5, 8, 4, 7, 3, 6, 2, 1, 0] {
            with_bit(&mut allowed[operation], prefix);
            prefix |= 1 << operation;
        }
        let mut work = 0;
        let live = domain.live_from_start(&allowed, &mut work);
        assert_eq!(live, scalar_live(10, &allowed));
        assert_eq!(live.into_iter().map(u64::count_ones).sum::<u32>(), 11);
        // Even including local closure, none of the ten edges needs to be
        // rescanned across all ten global ranks in this graph.
        assert!(work < 2 * 10 * 10, "{work}");
    }

    #[test]
    fn symbolic_upper_edges_require_both_endpoints_across_words() {
        let domain = PrefixDomain::new(7).unwrap();
        let mut upper = [[0; MAX_PREFIX_WORDS]; MAX_OPERATIONS];
        // Root -> 1 -> 65 -> 67, then complete with operations 2..=5.
        with_bit(&mut upper[0], 0);
        with_bit(&mut upper[6], 1);
        with_bit(&mut upper[1], 65);
        let mut prefix = 67;
        for (q, parents) in upper.iter_mut().enumerate().take(6).skip(2) {
            with_bit(parents, prefix);
            prefix |= 1 << q;
        }
        assert_eq!(prefix, domain.terminal());
        let exact = upper;
        // Same high operation: an edge out of the live root into a dead
        // prefix, and an edge into live 67 from an unreachable prefix.
        with_bit(&mut upper[6], 0);
        with_bit(&mut upper[6], 3);
        assert_eq!(domain.potentially_live_edges(&upper, &mut 0), exact);
        assert_eq!(scalar_live_edges(7, &upper), exact);

        upper[6][0] &= !(1 << 1);
        assert_eq!(
            domain.potentially_live_edges(&upper, &mut 0),
            [[0; MAX_PREFIX_WORDS]; MAX_OPERATIONS],
        );
    }
}
