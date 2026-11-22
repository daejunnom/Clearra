//! Original-source-qualified active paths for the q14 fresh-bag domain.
//!
//! The caller must hold the q14 request certificate (complete original fresh
//! bag, n=7..=11, N=n+1, initial cursor zero/empty allowed hold, no terminal
//! target, both lookahead projection flags false). This type is an original-
//! source-qualified view, NEVER a claim that the raw physical word language
//! is unchanged. Both suffix compilation and board registration must consume
//! this same view; the original relation remains immutable.

use clearra_core_domain::{execution_cancellation::ExecutionControl, piece::piece_kind::PieceKind};

use clearra_supply::pattern_universe::PieceMultisetKey;

use super::ExactPrefixRelation;
use crate::backend::wasm_cpu::{
    piece_index,
    symbolic_prefix_domain::{PrefixDomain, MAX_OPERATIONS, MAX_PREFIX_WORDS},
    WasmExactSearchError,
};

type PrefixWords = [u64; MAX_PREFIX_WORDS];
type ParentWords = [PrefixWords; MAX_OPERATIONS];

const SIX_PREFIXES: PrefixWords = six_prefixes();

const fn six_prefixes() -> PrefixWords {
    let mut words = [0; MAX_PREFIX_WORDS];
    let mut prefix = 0_usize;
    while prefix < 1 << MAX_OPERATIONS {
        if prefix.count_ones() == 6 {
            words[prefix / 64] |= 1_u64 << (prefix % 64);
        }
        prefix += 1;
    }
    words
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::backend::wasm_cpu) struct SourceFilterMetrics {
    /// One per allowed-mask word, masked operation/parent word, and final
    /// containment-check word. Includes zero words actually visited.
    pub(in crate::backend::wasm_cpu) mask_words: usize,
    /// The existing PrefixDomain nonzero transfer-word accounting, unchanged.
    pub(in crate::backend::wasm_cpu) domain_words: usize,
    /// Active depth-six subsets whose kinds were actually examined.
    pub(in crate::backend::wasm_cpu) prefix_checks: usize,
}

/// The borrow pins the exact operation labels/parent relation which licensed
/// these active bits. No mask is accepted with an unrelated lookalike proof.
pub(in crate::backend::wasm_cpu) struct SourceQualifiedPrefixMask<'a> {
    original: &'a ExactPrefixRelation,
    active: PrefixWords,
}

impl<'a> SourceQualifiedPrefixMask<'a> {
    pub(in crate::backend::wasm_cpu) fn new(
        original: &'a ExactPrefixRelation,
        control: &ExecutionControl,
        metrics: &mut SourceFilterMetrics,
    ) -> Result<Self, WasmExactSearchError> {
        let active = project_source_mask(
            original.pieces(),
            original.active_words(),
            |operation| original.parent_words(operation),
            control,
            metrics,
        )?;
        Ok(Self { original, active })
    }

    #[cfg(test)]
    fn pieces(&self) -> &[PieceKind] {
        self.original.pieces()
    }

    pub(in crate::backend::wasm_cpu) fn active_words(&self) -> &[u64] {
        &self.active[..self.original.active_words().len()]
    }

    pub(in crate::backend::wasm_cpu) fn active_mask(&self) -> &PrefixWords {
        &self.active
    }

    pub(in crate::backend::wasm_cpu) fn matches_original(
        &self,
        original: &ExactPrefixRelation,
    ) -> bool {
        core::ptr::eq(self.original, original)
    }

    #[cfg(test)]
    pub(in crate::backend::wasm_cpu) fn contains_prefix(&self, prefix: u16) -> bool {
        let prefix = usize::from(prefix);
        prefix <= usize::from(self.original.terminal_prefix())
            && self.active[prefix / 64] & (1_u64 << (prefix % 64)) != 0
    }

    pub(in crate::backend::wasm_cpu) fn has_witness(&self) -> bool {
        self.active[0] & 1 != 0
    }

    #[cfg(test)]
    fn live_operations_after(&self, prefix: u16) -> impl Iterator<Item = (usize, PieceKind)> + '_ {
        let parent_live = self.contains_prefix(prefix);
        // The original derivative assumes an in-range parent. Even an
        // invalid caller input cannot index it outside the certified domain.
        let safe_prefix = if parent_live { prefix } else { 0 };
        self.original
            .live_operations_after(safe_prefix)
            .filter(move |&(operation, _)| {
                parent_live && self.contains_prefix(prefix | (1_u16 << operation))
            })
    }
}

fn count(counter: &mut usize, amount: usize) -> Result<(), WasmExactSearchError> {
    *counter = counter
        .checked_add(amount)
        .ok_or(WasmExactSearchError::InvalidProblem(
            "wasm_observation_source_filter_work_overflow",
        ))?;
    Ok(())
}

/// Production passes an immutable ExactPrefixRelation, whose original active
/// bits already certify reachability AND terminal productivity. The raw test
/// harness below obtains those bits from the same original domain closure;
/// this routine does not redundantly re-prove the original movement relation.
fn project_source_mask<'a>(
    pieces: &[PieceKind],
    original_active: &[u64],
    parent_words: impl Fn(usize) -> &'a [u64],
    control: &ExecutionControl,
    metrics: &mut SourceFilterMetrics,
) -> Result<PrefixWords, WasmExactSearchError> {
    let invalid = WasmExactSearchError::InvalidProblem;
    if control.is_cancelled() {
        return Err(WasmExactSearchError::Cancelled);
    }
    if !(7..=11).contains(&pieces.len())
        || PieceMultisetKey::from_pieces(pieces.iter().copied())
            .counts()
            .into_iter()
            .any(|count| count > 2)
    {
        return Err(invalid("wasm_observation_source_filter_scope_invalid"));
    }
    let domain = PrefixDomain::new(pieces.len())
        .ok_or_else(|| invalid("wasm_observation_source_filter_scope_invalid"))?;
    let words = domain.word_count();
    if original_active.len() != words
        || (0..pieces.len()).any(|operation| parent_words(operation).len() != words)
    {
        return Err(invalid("wasm_observation_source_filter_domain_mismatch"));
    }
    let terminal = domain.terminal();
    let root_present = original_active[0] & 1 != 0;
    let terminal_present = original_active[terminal / 64] & (1_u64 << (terminal % 64)) != 0;
    if root_present != terminal_present
        || (!root_present && original_active.iter().any(|&word| word != 0))
    {
        return Err(invalid(
            "wasm_observation_source_filter_active_inconsistent",
        ));
    }

    // Every complete path crosses depth six exactly once. Blocking only its
    // non-distinct subsets is equivalent to blocking repeated kinds at every
    // earlier depth: no duplicate prefix can grow into six distinct kinds.
    // The closure below removes both dead ancestors and orphan descendants.
    let mut allowed = [0; MAX_PREFIX_WORDS];
    for (word, &active) in original_active.iter().enumerate() {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        count(&mut metrics.mask_words, 1)?;
        allowed[word] = active;
        let mut boundary = active & SIX_PREFIXES[word];
        while boundary != 0 {
            let lane = boundary.trailing_zeros() as usize;
            boundary &= boundary - 1;
            count(&mut metrics.prefix_checks, 1)?;
            let mut prefix = word * 64 + lane;
            let mut kinds = 0_u8;
            while prefix != 0 {
                let operation = prefix.trailing_zeros() as usize;
                prefix &= prefix - 1;
                let kind = 1_u8 << piece_index(pieces[operation]);
                if kinds & kind != 0 {
                    allowed[word] &= !(1_u64 << lane);
                    break;
                }
                kinds |= kind;
            }
        }
    }

    let mut parents: ParentWords = [[0; MAX_PREFIX_WORDS]; MAX_OPERATIONS];
    for word in 0..words {
        if control.is_cancelled() {
            return Err(WasmExactSearchError::Cancelled);
        }
        let owners = domain.word_owners(word);
        let nonterminal = domain.nonterminal_lanes(word);
        for operation in 0..pieces.len() {
            count(&mut metrics.mask_words, 1)?;
            let child_live = if operation < 6 {
                allowed[word] >> (1 << operation)
            } else {
                allowed[word | (1 << (operation - 6))]
            };
            // Match PrefixDomain's sanitization, then restrict BOTH original
            // active endpoints. Cross-word transfers retain their lane.
            parents[operation][word] = parent_words(operation)[word]
                & nonterminal
                & !owners[operation]
                & allowed[word]
                & child_live;
        }
    }
    let mut domain_words = 0;
    let active = domain.live_from_start(&parents, &mut domain_words);
    count(&mut metrics.domain_words, domain_words)?;
    if control.is_cancelled() {
        return Err(WasmExactSearchError::Cancelled);
    }
    for word in 0..words {
        count(&mut metrics.mask_words, 1)?;
        if active[word] & !allowed[word] != 0 {
            return Err(invalid("wasm_observation_source_filter_escaped_domain"));
        }
    }
    Ok(active)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashSet};

    use clearra_rules::kicks::KickTableProfileId;

    use super::*;
    use crate::backend::wasm_cpu::{
        catalog::GeometryCatalog,
        geometry::GeometryCandidate,
        piece_order_language::{PieceOrderLanguageCache, PrefixSuffixCompilation},
        reachability::ReachabilityWorkspace,
        symbolic_prefix_orders::{
            SourceQualifiedPrefixOrders, SymbolicPrefixOrders, SymbolicPrefixWork,
        },
    };

    fn bit(words: &[u64], prefix: usize) -> bool {
        words[prefix / 64] & (1_u64 << (prefix % 64)) != 0
    }

    fn all_orders(n: usize) -> ParentWords {
        let mut parents = [[0; MAX_PREFIX_WORDS]; MAX_OPERATIONS];
        for prefix in 0..1 << n {
            for operation in 0..n {
                if prefix & (1 << operation) == 0 {
                    parents[operation][prefix / 64] |= 1_u64 << (prefix % 64);
                }
            }
        }
        parents
    }

    fn add_order(parents: &mut ParentWords, order: &[usize]) {
        let mut prefix = 0;
        for &operation in order {
            parents[operation][prefix / 64] |= 1_u64 << (prefix % 64);
            prefix |= 1 << operation;
        }
    }

    fn projected(
        pieces: &[PieceKind],
        parents: &ParentWords,
    ) -> (PrefixWords, SourceFilterMetrics) {
        let domain = PrefixDomain::new(pieces.len()).unwrap();
        let original = domain.live_from_start(parents, &mut 0);
        let mut metrics = SourceFilterMetrics::default();
        let filtered = project_source_mask(
            pieces,
            &original[..domain.word_count()],
            |operation| &parents[operation][..domain.word_count()],
            &ExecutionControl::default(),
            &mut metrics,
        )
        .unwrap();
        (filtered, metrics)
    }

    #[test]
    fn source_qualified_prefix_matches_complete_permutation_domain_at_word_boundaries() {
        use PieceKind::{I, J, O, S, T, Z};

        for n in 7..=11 {
            let mut pieces = vec![I, O, T, S, Z, J];
            pieces.extend([I, O, T, S, Z].into_iter().take(n - 6));
            let (active, metrics) = projected(&pieces, &all_orders(n));
            let mut six_count = 0;
            for prefix in 0_usize..1 << n {
                let selected: Vec<_> = pieces
                    .iter()
                    .enumerate()
                    .filter_map(|(index, &piece)| (prefix & (1 << index) != 0).then_some(piece))
                    .collect();
                let distinct = selected.iter().copied().collect::<HashSet<_>>().len();
                // In the complete permutation relation, a prefix before the
                // cut must be distinct; after the cut it must contain enough
                // distinct kinds to have passed a valid six-kind prefix.
                let expected = if selected.len() <= 6 {
                    distinct == selected.len()
                } else {
                    distinct >= 6
                };
                assert_eq!(bit(&active, prefix), expected, "n={n}, prefix={prefix}");
                six_count += usize::from(selected.len() == 6);
            }
            let words = (1_usize << n).div_ceil(64);
            assert_eq!(metrics.mask_words, words * (n + 2));
            assert_eq!(metrics.prefix_checks, six_count);
            assert!(metrics.domain_words > 0);
        }
    }

    #[test]
    fn source_qualified_prefix_removes_dead_early_branches_without_losing_the_other_path() {
        use PieceKind::{I, J, O, S, T, Z};

        let pieces = [I, I, O, T, S, Z, J];
        let bad = [0, 2, 3, 4, 5, 1, 6];
        let good = [1, 2, 3, 4, 6, 5, 0];
        let mut parents = [[0; MAX_PREFIX_WORDS]; MAX_OPERATIONS];
        add_order(&mut parents, &bad);
        add_order(&mut parents, &good);
        let domain = PrefixDomain::new(pieces.len()).unwrap();
        let original = domain.live_from_start(&parents, &mut 0);
        let (filtered, _) = projected(&pieces, &parents);
        let mut bad_prefix = 0;
        let mut good_prefix = 0;
        for depth in 1..=6 {
            bad_prefix |= 1 << bad[depth - 1];
            good_prefix |= 1 << good[depth - 1];
            assert!(bit(&original, bad_prefix));
            assert!(
                !bit(&filtered, bad_prefix),
                "dead ancestor at depth {depth}"
            );
            assert!(bit(&filtered, good_prefix));
        }
        assert!(bit(&filtered, 0));
        assert!(bit(&filtered, domain.terminal()));
    }

    #[test]
    fn source_qualified_prefix_is_not_a_permanent_one_per_kind_cap() {
        use PieceKind::{I, J, O, S, T, Z};

        let pieces = [I, I, O, T, S, Z, J];
        let (active, _) = projected(&pieces, &all_orders(pieces.len()));
        assert!(bit(&active, 1));
        assert!(bit(&active, 2));
        assert!(!bit(&active, 3));
        assert!(bit(&active, 127));
        // Both I-labelled root edges survive as distinct operation choices.
        // The final I repetition is legal after the six-kind boundary.
    }

    #[test]
    fn source_qualified_prefix_reports_empty_when_no_six_kind_path_exists() {
        use PieceKind::{I, O, S, T, Z};

        let pieces = [I, I, O, O, T, T, S, S, Z, Z];
        let (active, metrics) = projected(&pieces, &all_orders(pieces.len()));
        assert!(active.into_iter().all(|word| word == 0));
        assert_eq!(metrics.prefix_checks, 210);
        assert!(metrics.domain_words > 0);
    }

    fn real_ten_lock_orders(control: &ExecutionControl) -> SymbolicPrefixOrders {
        use PieceKind::{I, J, L, O, S, T, Z};
        let placements = [
            (I, 0xf),
            (I, 0xf0),
            (O, 0xc0300),
            (T, 0x201c00),
            (Z, 0xc06000),
            (Z, 0x1006008000),
            (S, 0x18030000),
            (L, 0x1c0100000),
            (T, 0xe01000000),
            (J, 0xe020000000),
        ];
        let catalog =
            GeometryCatalog::compile_for_required_cells_on_dimensions(10, 4, 0, (1_u64 << 40) - 1)
                .unwrap();
        let rows = placements.map(|(piece, cells)| catalog.skeleton_id(piece, cells).unwrap());
        let candidate = GeometryCandidate::from_rows(&catalog, 0, &rows).unwrap();
        SymbolicPrefixOrders::compile_product_with_workspace(
            &catalog,
            &candidate,
            KickTableProfileId::Jstris180,
            control,
            &mut ReachabilityWorkspace::default(),
            &mut SymbolicPrefixWork::default(),
            0,
        )
        .unwrap()
    }

    #[test]
    fn source_qualified_prefix_keeps_a_real_geometric_fresh_bag_word() {
        use PieceKind::{I, J, L, O, S, T, Z};

        fn accepts(view: &SourceQualifiedPrefixMask<'_>, prefix: u16, word: &[PieceKind]) -> bool {
            let Some((&piece, tail)) = word.split_first() else {
                return prefix.count_ones() as usize == view.pieces().len();
            };
            view.live_operations_after(prefix).any(|(operation, next)| {
                next == piece && accepts(view, prefix | (1_u16 << operation), tail)
            })
        }

        let control = ExecutionControl::default();
        let orders = real_ten_lock_orders(&control);
        let original = orders.observation_relation().unwrap();
        let snapshot = original.active_words().to_vec();
        let mut metrics = SourceFilterMetrics::default();
        let view = SourceQualifiedPrefixMask::new(original, &control, &mut metrics).unwrap();
        let lookalike = ExactPrefixRelation {
            permitted_parents: original.permitted_parents,
            pieces: original.pieces,
            operation_count: original.operation_count,
            active_prefixes: original.active_prefixes,
        };
        assert!(view.matches_original(original));
        assert!(!view.matches_original(&lookalike));
        assert!(view.has_witness());
        assert!(accepts(&view, 0, &[I, O, T, Z, S, L, J, Z, T, I]));
        assert_eq!(original.active_words(), snapshot);
        assert_eq!(view.active_words(), &view.active_mask()[..snapshot.len()]);
        for (filtered, original) in view.active_words().iter().zip(&snapshot) {
            assert_eq!(filtered & !original, 0);
        }
        assert_eq!(view.live_operations_after(u16::MAX).count(), 0);
    }

    #[test]
    fn source_qualified_prefix_full_and_boundary_suffixes_match_independent_word_filter() {
        fn words(cache: &PieceOrderLanguageCache, root: u32) -> BTreeSet<Vec<u8>> {
            fn visit(
                cache: &PieceOrderLanguageCache,
                node: u32,
                prefix: &mut Vec<u8>,
                output: &mut BTreeSet<Vec<u8>>,
            ) {
                if cache.node(node).unwrap().accepting {
                    output.insert(prefix.clone());
                }
                for edge in 0..cache.node_edge_count(node).unwrap() {
                    let edge = cache.edge_at(node, edge).unwrap();
                    prefix.push(edge.piece_code());
                    visit(cache, edge.child(), prefix, output);
                    prefix.pop();
                }
            }
            let mut output = BTreeSet::new();
            visit(cache, root, &mut Vec::new(), &mut output);
            output
        }

        let control = ExecutionControl::default();
        let original = real_ten_lock_orders(&control);
        let mut metrics = SourceFilterMetrics::default();
        let qualified =
            SourceQualifiedPrefixOrders::new(&original, &control, &mut metrics).unwrap();
        let mut full_cache = PieceOrderLanguageCache::default();
        let mut boundary_cache = PieceOrderLanguageCache::default();
        let mut raw_cache = PieceOrderLanguageCache::default();
        let mut full = Vec::new();
        let mut boundary = Vec::new();
        let mut raw = Vec::new();
        assert_eq!(
            qualified
                .compile_product_suffixes_with_residuals(
                    &mut full_cache,
                    0,
                    &mut SymbolicPrefixWork::default(),
                    &mut full,
                )
                .unwrap(),
            PrefixSuffixCompilation::Compiled,
        );
        assert_eq!(
            qualified
                .compile_product_suffixes_with_residuals(
                    &mut boundary_cache,
                    4,
                    &mut SymbolicPrefixWork::default(),
                    &mut boundary,
                )
                .unwrap(),
            PrefixSuffixCompilation::Compiled,
        );
        assert_eq!(
            original
                .compile_product_suffixes_with_residuals(
                    &mut raw_cache,
                    4,
                    &mut SymbolicPrefixWork::default(),
                    &mut raw,
                )
                .unwrap(),
            PrefixSuffixCompilation::Compiled,
        );
        assert_eq!(full.len(), boundary.len());
        assert_eq!(raw.len(), boundary.len());
        let original_relation = original.observation_relation().unwrap();
        let mut compared = 0;
        let mut cut_boundaries = 0;
        for prefix in 0..boundary.len() {
            let depth = prefix.count_ones() as usize;
            if !qualified.contains_prefix(prefix as u16) || depth < 4 {
                assert_eq!(boundary[prefix], u32::MAX);
                if !qualified.contains_prefix(prefix as u16) {
                    assert_eq!(full[prefix], u32::MAX);
                }
                continue;
            }
            let actual = words(&boundary_cache, boundary[prefix]);
            assert_eq!(actual, words(&full_cache, full[prefix]), "prefix={prefix}");
            let mut expected = words(&raw_cache, raw[prefix]);
            if depth < 6 {
                cut_boundaries += 1;
                let prefix_kinds: HashSet<_> = original_relation
                    .pieces()
                    .iter()
                    .enumerate()
                    .filter_map(|(operation, &piece)| {
                        (prefix & (1 << operation) != 0).then_some(piece_index(piece) as u8 + 1)
                    })
                    .collect();
                assert_eq!(prefix_kinds.len(), depth);
                // This reference enumerates the ORIGINAL suffix words, then
                // checks the next actual kinds with an independent set. It
                // does not consult the projected parent or active masks.
                expected.retain(|word| {
                    let mut seen = prefix_kinds.clone();
                    word[..6 - depth].iter().all(|&piece| seen.insert(piece))
                });
            }
            // At depth >=6 the qualified prefix already has a valid history;
            // the first-six condition imposes no further suffix restriction.
            assert_eq!(actual, expected, "prefix={prefix}, depth={depth}");
            compared += 1;
        }
        assert!(compared > 0);
        assert!(cut_boundaries > 0);
    }

    #[test]
    fn source_qualified_prefix_rejects_scope_and_domain_errors_and_observes_cancel() {
        use PieceKind::{I, J, O, S, T, Z};

        let pieces = [I, O, T, S, Z, J, I];
        let parents = all_orders(pieces.len());
        let domain = PrefixDomain::new(pieces.len()).unwrap();
        let original = domain.live_from_start(&parents, &mut 0);
        let control = ExecutionControl::default();
        let run = |pieces: &[PieceKind], active: &[u64], control: &ExecutionControl| {
            project_source_mask(
                pieces,
                active,
                |operation| &parents[operation][..domain.word_count()],
                control,
                &mut SourceFilterMetrics::default(),
            )
        };
        assert!(matches!(
            run(&pieces[..6], &original[..2], &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_source_filter_scope_invalid"
            ))
        ));
        assert!(matches!(
            run(&[I, I, I, O, T, S, Z], &original[..2], &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_source_filter_scope_invalid"
            ))
        ));
        assert!(matches!(
            run(&pieces, &original[..1], &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_source_filter_domain_mismatch"
            ))
        ));
        assert!(matches!(
            run(&pieces, &[0, 1_u64 << 63], &control),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_observation_source_filter_active_inconsistent"
            ))
        ));
        control.cancellation.handle().cancel();
        assert!(matches!(
            run(&pieces, &original[..2], &control),
            Err(WasmExactSearchError::Cancelled)
        ));
    }
}
