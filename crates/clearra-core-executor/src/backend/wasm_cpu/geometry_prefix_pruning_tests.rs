use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_pc_graph::request::{
    PcCountPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow, SupplyWindowSize,
};
use clearra_problem::ProblemCompiler;
use clearra_rules::profile::builtin_rules::srs_plus;
use clearra_supply::queue::fixed_sequence::FixedSequence;

use super::super::realization_feasibility::RealizationFeasibilityWorkspace;
use super::*;

type Sequence = Vec<(u32, StandardBoard64TilingIdentity, Vec<u32>)>;

fn candidate_record(
    candidate: GeometryCandidate,
) -> (u32, StandardBoard64TilingIdentity, Vec<u32>) {
    (
        candidate.target_index,
        candidate.identity,
        candidate.row_ids().to_vec(),
    )
}

fn mixed_fixture(initial: u64) -> (GeometryCatalog, GeometrySearch) {
    let locks = (40 - initial.count_ones()) as usize / 4;
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, initial),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(locks),
    )
    .with_rule(srs_plus())
    .with_allow_hold(true)
    .with_exact_pieces(Some(locks))
    .with_supply_window_size(SupplyWindowSize::new(7))
    .with_count_policy(PcCountPolicy::CountUnique);
    let problem = ProblemCompiler::compile_scenario_pc(&query).unwrap();
    let catalog = GeometryCatalog::compile(&problem).unwrap();
    let universe = problem.piece_source().materialized_universe().unwrap();
    let family = universe.packing_multiset_family_for_execution(
        locks,
        problem.initial_hold(),
        true,
        super::super::packing_hold_projection(&problem),
    );
    let search = GeometrySearch::new(universe, &family, catalog.required_cells(), false).unwrap();
    (catalog, search)
}

fn collect(search: &mut GeometrySearch, catalog: &GeometryCatalog) -> Vec<GeometryCandidate> {
    let mut output = Vec::new();
    for _ in 0..10_000_000 {
        match search.advance(catalog) {
            GeometryAdvance::Pending => {}
            GeometryAdvance::Candidate(candidate) => output.push(candidate),
            GeometryAdvance::Complete => return output,
            GeometryAdvance::ResourceIncomplete(reason) => panic!("{reason}"),
        }
    }
    panic!("finite geometry fixture must complete");
}

#[test]
fn physical_prefix_small_ilc_enumeration_matches_full_candidate_negative_filter() {
    for initial in [0x3c0f_03c0f, 0x90_747d_83e5] {
        let (catalog, mut raw) = mixed_fixture(initial);
        let (_, mut filtered) = mixed_fixture(initial);
        assert!(!raw.physical_prefix_pruning_enabled());
        filtered.enable_physical_prefix_pruning();
        let all = collect(&mut raw, &catalog);
        let expected = all
            .iter()
            .copied()
            .filter(|candidate| {
                !RealizationFeasibilityWorkspace::relaxed_ilc_order_is_impossible(
                    &catalog, candidate,
                )
            })
            .map(candidate_record)
            .collect::<Sequence>();
        let actual = collect(&mut filtered, &catalog)
            .into_iter()
            .map(candidate_record)
            .collect::<Sequence>();
        assert!(!expected.is_empty());
        assert_eq!(actual, expected);
        assert_eq!(filtered.candidate_family_count(), Some(all.len() as u128));
        assert_eq!(
            raw.candidate_family_count(),
            filtered.candidate_family_count()
        );
        // Every subset gives the future-completed relaxation. A stalled
        // subset must also fail the existing full-candidate necessary test.
        for candidate in all {
            let full_stalls = RealizationFeasibilityWorkspace::relaxed_ilc_order_is_impossible(
                &catalog, &candidate,
            );
            let rows = candidate.row_ids();
            for subset in 1..1_usize << rows.len() {
                let selected = rows
                    .iter()
                    .copied()
                    .enumerate()
                    .filter(|(index, _)| subset & (1 << index) != 0)
                    .map(|(_, row)| row)
                    .collect::<Vec<_>>();
                if prefix_pruning::selected_prefix_stalls(&catalog, &selected) {
                    assert!(
                        full_stalls,
                        "selected stall cannot reject an admitted full closure"
                    );
                }
            }
        }
    }
}

fn full_catalog() -> GeometryCatalog {
    let pieces = [
        PieceKind::I,
        PieceKind::O,
        PieceKind::T,
        PieceKind::S,
        PieceKind::Z,
        PieceKind::J,
        PieceKind::L,
    ];
    let mut queue = pieces
        .into_iter()
        .flat_map(|piece| [piece; 2])
        .collect::<Vec<_>>();
    queue.push(PieceKind::I);
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(6, 0),
        PcQueueInput::fixed_sequence(FixedSequence::new(queue)),
        PieceWindow::new(15),
    )
    .with_rule(srs_plus())
    .with_allow_hold(false)
    .with_exact_pieces(Some(15))
    .with_count_policy(PcCountPolicy::CountUnique);
    GeometryCatalog::compile(&ProblemCompiler::compile_scenario_pc(&query).unwrap()).unwrap()
}

fn vertical_cells(x: u32, rows: &[u32]) -> u64 {
    rows.iter()
        .fold(0, |cells, y| cells | 1_u64 << (y * 10 + x))
}

fn cycle_and_ordinary_rows(catalog: &GeometryCatalog) -> ([u32; 2], [u32; 4]) {
    let cycle = [
        catalog
            .skeleton_id(PieceKind::I, vertical_cells(0, &[0, 2, 3, 4]))
            .unwrap(),
        catalog
            .skeleton_id(PieceKind::I, vertical_cells(1, &[0, 1, 3, 4]))
            .unwrap(),
    ];
    let ordinary = core::array::from_fn(|x| {
        catalog
            .skeleton_id(PieceKind::I, vertical_cells(x as u32 + 2, &[0, 1, 2, 3]))
            .unwrap()
    });
    assert!(!prefix_pruning::selected_prefix_stalls(
        catalog,
        &cycle[..1]
    ));
    assert!(prefix_pruning::selected_prefix_stalls(catalog, &cycle));
    for row in ordinary {
        assert!(catalog.realization_requirement_is_satisfied(row, 0));
    }
    (cycle, ordinary)
}

fn i_targets(count: u8) -> Arc<[TargetGroup]> {
    let mut counts = [0; 7];
    counts[piece_index(PieceKind::I)] = count;
    Arc::from([TargetGroup {
        key: PieceMultisetKey::from_counts(counts),
        pattern_index_id: 0,
        possible_patterns: Arc::new(PatternBitSet::all(1)),
        pattern_index: None,
    }])
}

fn product_fixture(
    catalog: &GeometryCatalog,
    repetitions: usize,
    good_sibling: bool,
) -> (GeometrySolutionFamily, u32) {
    let (cycle, ordinary) = cycle_and_ordinary_rows(catalog);
    let mut family = GeometrySolutionFamily::new();
    let b = family.append(cycle[1], FAMILY_EMPTY).unwrap();
    let a_b = family.append(cycle[0], b).unwrap();
    let c = family.append(ordinary[0], FAMILY_EMPTY).unwrap();
    let d = family.append(ordinary[1], FAMILY_EMPTY).unwrap();
    let continuation = family.union(c, d).unwrap();
    let dead = family.product(a_b, continuation).unwrap();
    let mut root = dead;
    // Repeated negative paths form a large finite DAG traversal without any
    // leaf output. This isolates the host-yield contract from source size.
    for repetition in 1..repetitions {
        // Seed a distinct negative branch before repeating `dead`: union
        // simplifies identical immediate operands, so union(dead, dead)
        // alone would leave this intended long traversal at two paths.
        let branch = if repetition == 1 {
            family.product(a_b, c).unwrap()
        } else {
            dead
        };
        root = family.union(root, branch).unwrap();
    }
    if good_sibling {
        let tail = family.append(ordinary[2], FAMILY_EMPTY).unwrap();
        let middle = family.append(ordinary[1], tail).unwrap();
        let good = family.append(ordinary[0], middle).unwrap();
        root = family.union(root, good).unwrap();
    }
    (family, root)
}

fn enumerator(catalog: &GeometryCatalog, filtered: bool) -> FamilyEnumerator {
    let (family, root) = product_fixture(catalog, 1, true);
    let (mut enumerator, count) =
        FamilyEnumerator::from_completed_family(i_targets(3), family, root, 3, None);
    assert_eq!(count, Some(3));
    if filtered {
        enumerator.enable_physical_prefix_pruning();
    }
    enumerator
}

fn collect_steps(
    enumerator: &mut FamilyEnumerator,
    catalog: &GeometryCatalog,
    quantum: usize,
    grow: bool,
) -> (Sequence, usize) {
    let mut sequence = Vec::new();
    let mut pending = 0;
    for _ in 0..100_000 {
        match enumerator
            .next_candidate_step(catalog, grow, quantum)
            .unwrap()
        {
            FamilyEnumerationAdvance::Pending => pending += 1,
            FamilyEnumerationAdvance::Candidate(candidate) => {
                sequence.push(candidate_record(candidate))
            }
            FamilyEnumerationAdvance::Complete => return (sequence, pending),
        }
    }
    panic!("finite traversal must exhaust");
}

#[test]
fn physical_prefix_product_prune_preserves_union_siblings_and_resumable_order() {
    let catalog = full_catalog();
    let mut pure = enumerator(&catalog, false);
    let (all, _) = collect_steps(&mut pure, &catalog, usize::MAX, true);
    assert_eq!(all.len(), 3);
    let mut unbounded = enumerator(&catalog, true);
    let (expected, _) = collect_steps(&mut unbounded, &catalog, usize::MAX, true);
    assert_eq!(expected.len(), 1);
    assert_eq!(expected[0], all[2]);
    assert_eq!(unbounded.prefix_profile.pruned_subtrees, 1);
    for quantum in [1, 2, 3, 8] {
        let mut resumed = enumerator(&catalog, true);
        let (actual, pending) = collect_steps(&mut resumed, &catalog, quantum, true);
        assert_eq!(actual, expected);
        if quantum == 1 {
            assert!(pending > 0);
        }
        assert_eq!(
            resumed.prefix_profile.checks,
            unbounded.prefix_profile.checks
        );
        assert_eq!(resumed.prefix_profile.pruned_subtrees, 1);
    }
}

#[cfg(feature = "parallel")]
#[test]
fn physical_prefix_canonical_parallel_seeds_keep_domain_and_validate_frozen_prefixes() {
    let catalog = full_catalog();
    let raw = enumerator(&catalog, false);
    let raw_counts = raw.family.path_count_table().unwrap();
    let filtered = enumerator(&catalog, true);
    let filtered_counts = filtered.family.path_count_table().unwrap();
    let (raw_shards, _) = raw.split_parallel_enumerators(8, raw_counts, true).unwrap();
    let (filtered_shards, _) = filtered
        .split_parallel_enumerators(8, filtered_counts, true)
        .unwrap();
    assert_eq!(raw_shards.len(), filtered_shards.len());
    let mut total = 0;
    let mut frozen_prefix_pruned = false;
    for (mut pure, mut pruned) in raw_shards.into_iter().zip(filtered_shards) {
        assert!(pruned.physical_prefix_pruning);
        assert_eq!(pure.candidate_count, pruned.candidate_count);
        let frozen_depth = pruned.tasks[0].depth;
        let expected = collect_steps(&mut pure, &catalog, usize::MAX, true)
            .0
            .into_iter()
            .filter(|(_, _, rows)| !prefix_pruning::selected_prefix_stalls(&catalog, rows))
            .collect::<Sequence>();
        let actual = collect_steps(&mut pruned, &catalog, 1, true).0;
        assert_eq!(actual, expected);
        frozen_prefix_pruned |= frozen_depth > 0 && pruned.prefix_profile.pruned_subtrees > 0;
        total += actual.len();
    }
    assert_eq!(total, 1);
    assert!(frozen_prefix_pruned);

    let (cycle, _) = cycle_and_ordinary_rows(&catalog);
    let (mut leaf, _) = FamilyEnumerator::from_completed_family(
        i_targets(2),
        GeometrySolutionFamily::new(),
        FAMILY_EMPTY,
        2,
        None,
    );
    leaf.rows[..2].copy_from_slice(&cycle);
    leaf.tasks[0].depth = 2;
    leaf.enable_physical_prefix_pruning();
    assert!(matches!(
        leaf.next_candidate_step(&catalog, true, 1).unwrap(),
        FamilyEnumerationAdvance::Complete
    ));
    assert_eq!(leaf.prefix_profile.pruned_subtrees, 1);
}

#[test]
fn physical_prefix_negative_traversal_yields_before_any_leaf_without_generation_cap() {
    let catalog = full_catalog();
    let (family, root) = product_fixture(
        &catalog,
        PHYSICAL_PREFIX_ENUMERATION_WORK_QUANTUM * 2,
        false,
    );
    let (enumerator, raw_count) =
        FamilyEnumerator::from_completed_family(i_targets(3), family, root, 3, None);
    let mut search = GeometrySearch::placeholder();
    search.enumerator = Some(enumerator);
    search.candidate_family_count = raw_count;
    search.enable_physical_prefix_pruning();
    assert!(matches!(search.advance(&catalog), GeometryAdvance::Pending));
    assert!(search.enumerator.as_ref().unwrap().active_task.is_some());
    // The outer owner can observe cancellation/resource pressure here even
    // though no candidate has been emitted. Resuming still exhausts normally.
    let original_count = search.candidate_family_count();
    assert!(collect(&mut search, &catalog).is_empty());
    assert_eq!(search.candidate_family_count(), original_count);
    assert!(original_count.unwrap() > 1_024);
}

#[test]
fn physical_prefix_inline_resume_preserves_exact_memory_admission() {
    let catalog = full_catalog();
    for one_under in [false, true] {
        let targets = i_targets(3);
        let (family, root) = product_fixture(&catalog, 1, true);
        let exact = (target_bytes(&targets)
            + core::mem::size_of::<GeometrySolutionFamily>()
            + family.retained_bytes()
            + core::mem::size_of::<TraversalTask>()) as u128;
        let bounded = FamilyEnumerator::try_new_bounded(
            targets,
            family,
            root,
            3,
            exact - u128::from(one_under),
        );
        if one_under {
            assert!(bounded.is_none());
            continue;
        }
        let mut bounded = bounded.unwrap();
        bounded.enable_physical_prefix_pruning();
        let retained = bounded.retained_bytes();
        assert_eq!(retained as u128, exact);
        let (sequence, pending) = collect_steps(&mut bounded, &catalog, 1, false);
        assert_eq!(sequence.len(), 1);
        assert!(pending > 0);
        assert_eq!(bounded.retained_bytes(), retained);
    }
}
