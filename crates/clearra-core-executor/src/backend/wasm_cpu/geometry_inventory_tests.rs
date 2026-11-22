use std::collections::{BTreeMap, BTreeSet};

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_pc_graph::request::{
    PcCountPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
};
use clearra_problem::ProblemCompiler;
use clearra_rules::profile::builtin_rules::srs_plus;
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::*;

const PIECES: [PieceKind; 7] = [
    PieceKind::I,
    PieceKind::O,
    PieceKind::T,
    PieceKind::S,
    PieceKind::Z,
    PieceKind::J,
    PieceKind::L,
];

fn full_targets() -> Arc<[TargetGroup]> {
    let membership = Arc::new(PatternBitSet::all(1));
    let mut targets = (0..7)
        .map(|extra| {
            let mut counts = [2; 7];
            counts[extra] += 1;
            TargetGroup {
                key: PieceMultisetKey::from_counts(counts),
                pattern_index_id: extra as u32,
                possible_patterns: Arc::clone(&membership),
                pattern_index: None,
            }
        })
        .collect::<Vec<_>>();
    targets.sort_unstable_by_key(|target| target.key);
    targets.into()
}

fn remaining_languages(targets: &[TargetGroup], used: [u8; 7]) -> Vec<[u8; 7]> {
    let mut languages = targets
        .iter()
        .filter_map(|target| {
            let total = target.key.counts();
            total
                .iter()
                .zip(used)
                .all(|(a, b)| *a >= b)
                .then(|| core::array::from_fn(|piece| total[piece] - used[piece]))
        })
        .collect::<Vec<_>>();
    languages.sort_unstable();
    languages.dedup();
    languages
}

#[test]
fn inventory_reuse_descriptor_matches_all_original_prefixes_and_transitions() {
    let targets = full_targets();
    let prefixes = compile_admissible_prefixes(&targets);
    let descriptor = TwoBagPlusOneInventory::compile(&targets, &prefixes).unwrap();
    assert_eq!(prefixes.len(), 7_290);
    let mut code_languages = BTreeMap::new();
    let mut language_codes = BTreeMap::new();
    for packed in prefixes.iter().copied() {
        let used = unpack_piece_counts(packed);
        let actual = descriptor.analyze_used(used, packed).unwrap();
        let language = remaining_languages(&targets, used);
        assert!(!language.is_empty());
        if let Some(previous) = code_languages.insert(actual.memo_code(), language.clone()) {
            assert_eq!(
                previous, language,
                "different suffix languages cannot alias"
            );
        }
        if let Some(previous) = language_codes.insert(language.clone(), actual.memo_code()) {
            assert_eq!(
                previous,
                actual.memo_code(),
                "equal languages must share one key"
            );
        }
        let expected_mask = (0..7).fold(0_u8, |mask, piece| {
            mask | (u8::from(language.iter().any(|counts| counts[piece] > 0)) << piece)
        });
        assert_eq!(actual.feasible_piece_mask(), expected_mask);
        assert_eq!(
            actual.feasible_piece_mask(),
            feasible_piece_mask_for(&prefixes, used)
        );
        for piece in 0..7 {
            let mut next_used = used;
            next_used[piece] += 1;
            let next = descriptor.analyze_used(next_used, pack_piece_counts(next_used));
            if expected_mask & (1 << piece) == 0 {
                assert!(next.is_none());
                continue;
            }
            let mut expected = language
                .iter()
                .copied()
                .filter(|counts| counts[piece] > 0)
                .map(|mut counts| {
                    counts[piece] -= 1;
                    counts
                })
                .collect::<Vec<_>>();
            expected.sort_unstable();
            expected.dedup();
            assert_eq!(remaining_languages(&targets, next_used), expected);
            assert!(next.is_some());
        }
    }
    assert_eq!(code_languages.len(), 4_246);
    assert_eq!(language_codes.len(), 4_246);
    assert_eq!(
        code_languages
            .values()
            .filter(|language| language.len() == 1)
            .count(),
        2_059
    );
    assert_eq!(
        code_languages
            .values()
            .filter(|language| language.len() == 7)
            .count(),
        2_187
    );
}

#[test]
fn inventory_reuse_requires_actual_targets_and_complete_prefix_authority() {
    let targets = full_targets();
    let prefixes = compile_admissible_prefixes(&targets);
    assert!(TwoBagPlusOneInventory::compile(&targets[..6], &prefixes).is_none());
    let mut duplicate = targets.to_vec();
    duplicate[0] = duplicate[1].clone();
    assert!(TwoBagPlusOneInventory::compile(&duplicate, &prefixes).is_none());
    let mut changed = targets.to_vec();
    changed[0].key = PieceMultisetKey::from_counts([2; 7]);
    assert!(TwoBagPlusOneInventory::compile(&changed, &prefixes).is_none());
    assert!(TwoBagPlusOneInventory::compile(&targets, &prefixes[1..]).is_none());
    let mut duplicate_prefix = prefixes.clone();
    duplicate_prefix[1] = duplicate_prefix[0];
    assert!(TwoBagPlusOneInventory::compile(&targets, &duplicate_prefix).is_none());
    let mut invalid_prefix = prefixes.clone();
    *invalid_prefix.last_mut().unwrap() = u32::MAX;
    assert!(TwoBagPlusOneInventory::compile(&targets, &invalid_prefix).is_none());

    let descriptor = TwoBagPlusOneInventory::compile(&targets, &prefixes).unwrap();
    for used in [[4, 0, 0, 0, 0, 0, 0], [3, 3, 0, 0, 0, 0, 0]] {
        assert!(descriptor
            .analyze_used(used, pack_piece_counts(used))
            .is_none());
    }
    let bounded = FamilyCompiler::try_new((1_u64 << 60) - 1, Arc::clone(&targets)).unwrap();
    assert!(bounded.inventory.is_none());
    assert!(!bounded.residual_memo.inventory_alias_space);
    let post_clear = GeometryCompilerState::empty_post_clear_permutation_closed(15, prefixes);
    assert!(post_clear.inventory.is_none());
    assert!(!post_clear.residual_memo.inventory_alias_space);
}

fn catalog(initial: u64) -> GeometryCatalog {
    let locks = (60 - initial.count_ones()) as usize / 4;
    let mut sequence = PIECES
        .into_iter()
        .flat_map(|piece| [piece; 2])
        .collect::<Vec<_>>();
    if locks == 15 {
        sequence.push(PieceKind::I);
    }
    assert_eq!(sequence.len(), locks);
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(6, initial),
        PcQueueInput::fixed_sequence(FixedSequence::new(sequence)),
        PieceWindow::new(locks),
    )
    .with_rule(srs_plus())
    .with_allow_hold(false)
    .with_exact_pieces(Some(locks))
    .with_count_policy(PcCountPolicy::CountUnique);
    let problem = ProblemCompiler::compile_scenario_pc(&query).expect("six-line catalog fixture");
    GeometryCatalog::compile(&problem).expect("actual six-line catalog")
}

fn residual(catalog: &GeometryCatalog, pieces: &[PieceKind]) -> (u64, [u8; 7]) {
    let mut remaining = 0;
    let mut counts = [0; 7];
    for piece in pieces.iter().copied() {
        let row = (0..catalog.skeleton_count() as u32)
            .map(|id| catalog.skeleton(id))
            .find(|row| row.piece == piece && row.cells & remaining == 0)
            .expect("small catalog residual has disjoint rows");
        remaining |= row.cells;
        counts[piece_index(piece)] += 1;
    }
    assert_eq!(remaining.count_ones() as usize, pieces.len() * 4);
    (remaining, counts)
}

fn contexts(remaining_counts: [u8; 7]) -> Vec<[u8; 7]> {
    (0..7)
        .filter(|extra| remaining_counts[*extra] == 0)
        .map(|extra| {
            let mut used = core::array::from_fn(|piece| 2 - remaining_counts[piece]);
            used[extra] += 1;
            used
        })
        .collect()
}

fn state(targets: &Arc<[TargetGroup]>, optimize: bool) -> GeometryCompilerState {
    let mut compiler = FamilyCompiler::new((1_u64 << 60) - 1, Arc::clone(targets));
    if !optimize {
        compiler.inventory = None;
        compiler.residual_memo.inventory_alias_space = false;
    }
    compiler.into_completion_state()
}

#[derive(Clone, Copy, Debug, Default)]
struct ExactWork {
    expanded: usize,
    scanned_rows: u64,
    memo_entries: u32,
    family_nodes: u32,
}

fn finish(
    mut compiler: FamilyCompiler,
    catalog: &GeometryCatalog,
) -> (GeometryCompilerState, u32, ExactWork) {
    for _ in 0..2_000_000 {
        match compiler.advance(catalog) {
            CompileAdvance::Pending => {}
            CompileAdvance::Complete => {
                let root = compiler.root_family;
                let work = ExactWork {
                    expanded: compiler.expanded_nodes,
                    scanned_rows: compiler.profile.work[FamilyCompileStage::SupportRows as usize],
                    memo_entries: compiler.residual_memo.entry_count,
                    family_nodes: compiler.family.node_count(),
                };
                return (compiler.into_completion_state(), root, work);
            }
            CompileAdvance::ResourceIncomplete => panic!("small residual must be complete"),
        }
    }
    panic!("small residual must exhaust within finite test work");
}

#[derive(Debug, Eq, PartialEq)]
struct SuffixSnapshot {
    nodes: Vec<super::super::geometry_family::FamilyNode>,
    root: u32,
    count: Option<u128>,
    ordered: Vec<(StandardBoard64TilingIdentity, Vec<u32>)>,
    ordered_hash: u64,
    unordered_hash: u64,
}

fn snapshot(
    state: &mut GeometryCompilerState,
    root: u32,
    remaining_counts: [u8; 7],
    catalog: &GeometryCatalog,
) -> SuffixSnapshot {
    let nodes = (2..state.family.node_count() + 2)
        .map(|reference| state.family.node(reference).unwrap())
        .collect();
    let targets = Arc::from([TargetGroup {
        key: PieceMultisetKey::from_counts(remaining_counts),
        pattern_index_id: 0,
        possible_patterns: Arc::new(PatternBitSet::all(1)),
        pattern_index: None,
    }]);
    // Reuse the real enumerator, then return the uniquely owned family to the
    // same memo owner so later raw histories can reuse its exact node IDs.
    let family = core::mem::replace(&mut state.family, GeometrySolutionFamily::new());
    let (mut enumerator, count) = FamilyEnumerator::from_completed_family(
        targets,
        family,
        root,
        remaining_counts.into_iter().sum(),
        None,
    );
    let mut ordered = Vec::new();
    let mut ordered_hash = 0;
    let mut unordered_hash = 0;
    while let Some(candidate) = enumerator
        .next_candidate(catalog)
        .expect("suffix traversal")
    {
        let hash = candidate.identity.bucket_hash();
        ordered_hash = mix_digest(ordered_hash, hash);
        unordered_hash = super::super::mix_order_independent_candidate_digest(unordered_hash, hash);
        ordered.push((candidate.identity, candidate.row_ids().to_vec()));
    }
    state.family = Arc::try_unwrap(enumerator.family).expect("single family owner");
    assert!(!ordered.is_empty());
    assert_eq!(count, Some(ordered.len() as u128));
    assert_eq!(
        ordered
            .iter()
            .map(|(identity, _)| *identity)
            .collect::<BTreeSet<_>>()
            .len(),
        ordered.len()
    );
    SuffixSnapshot {
        nodes,
        root,
        count,
        ordered,
        ordered_hash,
        unordered_hash,
    }
}

#[test]
fn inventory_reuse_actual_full6_residuals_preserve_nodes_order_identities_and_hashes() {
    let catalog = catalog(0);
    let targets = full_targets();
    let mut reference = state(&targets, false);
    let mut optimized = state(&targets, true);
    let mut total_reference_expanded = 0;
    let mut total_optimized_expanded = 0;
    let mut total_reference_rows = 0;
    let mut total_optimized_rows = 0;
    for pieces in [
        vec![PieceKind::I],
        vec![PieceKind::I, PieceKind::I],
        vec![PieceKind::I, PieceKind::O, PieceKind::T],
        vec![PieceKind::I, PieceKind::I, PieceKind::O, PieceKind::O],
        vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
            PieceKind::Z,
        ],
    ] {
        let (remaining, counts) = residual(&catalog, &pieces);
        let histories = contexts(counts);
        assert!(histories.len() >= 2);
        let mut first_root = None;
        let mut first_node_count = None;
        for (index, used) in histories.iter().copied().enumerate() {
            let old =
                FamilyCompiler::from_residual(remaining, Arc::clone(&targets), used, reference);
            let new =
                FamilyCompiler::from_residual(remaining, Arc::clone(&targets), used, optimized);
            let (old_state, old_root, old_work) = finish(old, &catalog);
            let (new_state, new_root, new_work) = finish(new, &catalog);
            reference = old_state;
            optimized = new_state;
            assert_eq!(
                snapshot(&mut optimized, new_root, counts, &catalog),
                snapshot(&mut reference, old_root, counts, &catalog)
            );
            assert_eq!(old_work.family_nodes, new_work.family_nodes);
            if index == 0 {
                first_root = Some(old_root);
                first_node_count = Some(old_work.family_nodes);
            } else {
                assert_eq!(Some(old_root), first_root);
                assert_eq!(
                    Some(old_work.family_nodes),
                    first_node_count,
                    "active interning already shared the duplicate suffix DAG"
                );
                assert!(
                    old_work.expanded > 0,
                    "the raw history still recompiles states"
                );
                assert_eq!(new_work.expanded, 0, "canonical root is an exact memo hit");
                assert_eq!(new_work.scanned_rows, 0);
            }
            total_reference_expanded += old_work.expanded;
            total_optimized_expanded += new_work.expanded;
            total_reference_rows += old_work.scanned_rows;
            total_optimized_rows += new_work.scanned_rows;
            eprintln!("inventory suffix {:07x} history={index} expanded {}->{} rows {}->{} memo {}->{} nodes={}", pack_piece_counts(counts), old_work.expanded, new_work.expanded, old_work.scanned_rows, new_work.scanned_rows, old_work.memo_entries, new_work.memo_entries, new_work.family_nodes);
        }
    }
    assert!(total_optimized_expanded < total_reference_expanded);
    assert!(total_optimized_rows < total_reference_rows);
    assert!(optimized.residual_memo.entry_count < reference.residual_memo.entry_count);
}

#[test]
fn inventory_reuse_open_frame_keeps_lookup_key_when_interner_or_bound_changes() {
    let catalog = catalog(0);
    let targets = full_targets();
    let (remaining, counts) = residual(&catalog, &[PieceKind::I, PieceKind::O]);
    let used = contexts(counts)[0];
    for disable_interner in [false, true] {
        let mut reference = FamilyCompiler::from_residual(
            remaining,
            Arc::clone(&targets),
            used,
            state(&targets, false),
        );
        let mut optimized = FamilyCompiler::from_residual(
            remaining,
            Arc::clone(&targets),
            used,
            state(&targets, true),
        );
        assert!(matches!(
            reference.advance(&catalog),
            CompileAdvance::Pending
        ));
        assert!(matches!(
            optimized.advance(&catalog),
            CompileAdvance::Pending
        ));
        let queried = optimized.stack[0].memo_key;
        assert!(queried.is_inventory_alias());
        assert!(optimized.stack[0].entered);
        let raw = ResidualKey {
            remaining,
            packed_counts: pack_piece_counts(used),
        };
        for compiler in [&mut reference, &mut optimized] {
            if disable_interner {
                compiler.family.set_retained_limit_bytes(Some(0));
                compiler.family.set_retained_limit_bytes(None);
            } else {
                compiler.family.set_retained_limit_bytes(Some(u128::MAX));
            }
            assert!(!compiler.family.unbounded_interning_active());
        }
        assert_eq!(optimized.stack[0].memo_key, queried);
        let (mut old, old_root, _) = finish(reference, &catalog);
        let (mut new, new_root, _) = finish(optimized, &catalog);
        assert_eq!(
            snapshot(&mut new, new_root, counts, &catalog),
            snapshot(&mut old, old_root, counts, &catalog)
        );
        assert_eq!(
            new.residual_memo.lookup_memo(queried),
            None,
            "do not commit the invalidated alias"
        );
        assert_eq!(
            new.residual_memo.lookup(raw),
            None,
            "do not insert under a different key than the one queried"
        );
        assert!(old.residual_memo.lookup(raw).is_some());
        let mut next = FamilyCompiler::from_residual(remaining, Arc::clone(&targets), used, new);
        assert!(matches!(next.advance(&catalog), CompileAdvance::Pending));
        assert!(!next.stack[0].memo_key.is_inventory_alias());
    }
}

#[test]
fn inventory_reuse_other_domains_and_resource_owners_use_raw_memo() {
    let blank = catalog(0);
    let nonblank = catalog(0xf);
    let targets = full_targets();
    for mode in 0..4 {
        let active_catalog = if mode == 0 { &nonblank } else { &blank };
        let (remaining, counts) = residual(active_catalog, &[PieceKind::I, PieceKind::O]);
        let used = contexts(counts)[0];
        let mut compiler = FamilyCompiler::from_residual(
            remaining,
            Arc::clone(&targets),
            used,
            state(&targets, true),
        );
        match mode {
            0 => {}
            1 => compiler.compile_domain = FamilyCompileDomain::PermutationClosedPostClearResidual,
            2 => compiler.resource_authoritative = true,
            3 => {
                compiler.family.set_retained_limit_bytes(Some(0));
                compiler.family.set_retained_limit_bytes(None);
            }
            _ => unreachable!(),
        }
        assert!(matches!(
            compiler.advance(active_catalog),
            CompileAdvance::Pending
        ));
        assert_eq!(
            compiler.stack[0].memo_key,
            ResidualMemoKey::raw(ResidualKey {
                remaining,
                packed_counts: pack_piece_counts(used)
            })
        );
    }
}

#[test]
fn inventory_reuse_oracle_raw_queries_cannot_address_reserved_aliases() {
    let catalog = catalog(0);
    let targets = full_targets();
    let (remaining, counts) = residual(&catalog, &[PieceKind::I]);
    let used = contexts(counts)[0];
    let compiler =
        FamilyCompiler::from_residual(remaining, Arc::clone(&targets), used, state(&targets, true));
    let descriptor = compiler.inventory.unwrap();
    let alias = ResidualMemoKey::inventory(
        remaining,
        descriptor
            .analyze_used(used, pack_piece_counts(used))
            .unwrap(),
    );
    let (mut state, expected_root, _) = finish(compiler, &catalog);
    assert_eq!(state.residual_memo.lookup_memo(alias), Some(expected_root));
    let forged_raw = ResidualKey {
        remaining,
        packed_counts: alias.inventory_code,
    };
    let entries = state.residual_memo.entry_count;
    assert_eq!(state.residual_memo.lookup(forged_raw), None);
    state
        .residual_memo
        .insert_after_miss(forged_raw, FAMILY_INVALID);
    assert_eq!(state.residual_memo.entry_count, entries);
    let prefixes = state.geometry_prefixes.clone();
    let mut oracle =
        GeometryCompletionOracle::new(state, targets, prefixes, 15, catalog.skeleton_count())
            .unwrap();
    assert_eq!(
        oracle
            .ensure_family_root(
                forged_raw,
                FamilyCompileDomain::PermutationClosedGeometry,
                &catalog,
                &ExecutionControl::default()
            )
            .unwrap(),
        (FAMILY_INVALID, false)
    );
    assert_eq!(
        oracle
            .compiler_state(FamilyCompileDomain::PermutationClosedGeometry)
            .residual_memo
            .lookup_memo(alias),
        Some(expected_root)
    );

    // Reserving the alias space is local to the authenticated owner; existing
    // generic memo callers keep their original arbitrary-u32 key behavior.
    let mut generic = ResidualMemo::new(5);
    generic.insert_after_miss(forged_raw, 17);
    assert_eq!(generic.lookup(forged_raw), Some(17));
}
