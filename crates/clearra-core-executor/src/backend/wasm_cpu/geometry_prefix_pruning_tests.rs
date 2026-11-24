use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_pc_graph::request::{
    PcCountPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow, SupplyWindowSize,
};
use clearra_problem::ProblemCompiler;

use super::super::realization_feasibility::RealizationFeasibilityWorkspace;
use super::*;

type Record = (usize, u32, StandardBoard64TilingIdentity, Vec<u32>);

fn mixed_fixture(initial: u64, mode: PhysicalPrefixMode) -> (GeometryCatalog, GeometrySearch) {
    let locks = (40 - initial.count_ones()) as usize / 4;
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, initial),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(locks),
    )
    .with_rule(clearra_rules::profile::builtin_rules::srs_plus())
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
    let mut search =
        GeometrySearch::new(universe, &family, catalog.required_cells(), false).unwrap();
    search.enable_physical_prefix_pruning(mode);
    (catalog, search)
}

fn record(ordinal: usize, candidate: &GeometryCandidate) -> Record {
    (
        ordinal,
        candidate.target_index,
        candidate.identity,
        candidate.row_ids().to_vec(),
    )
}

fn collect(search: &mut GeometrySearch, catalog: &GeometryCatalog) -> Vec<(Record, bool)> {
    let mut output = Vec::new();
    for _ in 0..10_000_000 {
        match search.advance(catalog) {
            GeometryAdvance::Pending => {}
            GeometryAdvance::Candidate(candidate) => {
                let ordinal = search
                    .last_original_candidate_ordinal()
                    .unwrap_or(output.len());
                output.push((
                    record(ordinal, &candidate),
                    candidate.physical_prefix_impossible(),
                ));
            }
            GeometryAdvance::Complete => return output,
            GeometryAdvance::ResourceIncomplete(reason) => {
                panic!("finite geometry fixture: {reason}")
            }
        }
    }
    panic!("finite geometry fixture must complete");
}

fn full_negative(catalog: &GeometryCatalog, record: &Record) -> bool {
    let candidate = GeometryCandidate::from_rows(catalog, record.1, &record.3).unwrap();
    RealizationFeasibilityWorkspace::relaxed_ilc_order_is_impossible(catalog, &candidate)
        || prefix_pruning::selected_first_clear_stalls(catalog, candidate.row_ids())
}

#[test]
fn physical_prefix_modes_preserve_raw_order_digest_and_original_ordinals() {
    let mut total_negatives = 0;
    for initial in [0x3c0f_03c0f, 0x90_747d_83e5] {
        let (catalog, mut raw) = mixed_fixture(initial, PhysicalPrefixMode::Disabled);
        let (_, mut marked) = mixed_fixture(initial, PhysicalPrefixMode::MarkCandidates);
        let (_, mut skipped) = mixed_fixture(initial, PhysicalPrefixMode::SkipSubtrees);
        let raw_rows = collect(&mut raw, &catalog);
        let marked_rows = collect(&mut marked, &catalog);
        let skipped_rows = collect(&mut skipped, &catalog);
        assert!(!raw_rows.is_empty());
        assert_eq!(
            raw_rows.iter().map(|(row, _)| row).collect::<Vec<_>>(),
            marked_rows.iter().map(|(row, _)| row).collect::<Vec<_>>()
        );
        let digest = |rows: &[(Record, bool)]| {
            rows.iter().fold(0, |digest, (row, _)| {
                mix_digest(digest, row.2.bucket_hash())
            })
        };
        assert_eq!(digest(&raw_rows), digest(&marked_rows));
        let expected = raw_rows
            .iter()
            .filter(|(row, _)| !full_negative(&catalog, row))
            .cloned()
            .collect::<Vec<_>>();
        assert!(!expected.is_empty());
        assert_eq!(skipped_rows, expected);
        for (row, impossible) in &marked_rows {
            assert_eq!(*impossible, full_negative(&catalog, row));
        }
        let negatives = raw_rows.len() - expected.len();
        total_negatives += negatives;
        assert_eq!(
            marked.physical_prefix_stats().certified_candidates,
            negatives
        );
        assert_eq!(
            marked.physical_prefix_stats().emitted_candidates,
            raw_rows.len()
        );
        assert_eq!(
            skipped.physical_prefix_stats().skipped_candidates,
            negatives
        );
        assert_eq!(
            skipped.physical_prefix_stats().emitted_candidates,
            expected.len()
        );
        assert_eq!(marked.raw_candidate_progress(), Some(raw_rows.len()));
        assert_eq!(skipped.raw_candidate_progress(), Some(raw_rows.len()));
        assert_eq!(raw.candidate_family_count(), Some(raw_rows.len() as u128));
        assert_eq!(
            skipped.candidate_family_count(),
            raw.candidate_family_count()
        );
        assert!(marked.enumerator.is_none() && skipped.enumerator.is_none());
        // Compare against the preexisting complete-candidate closure. This
        // checks the subset relaxation mathematically, not a successful
        // physical witness (covered by separate product-result tests).
        for (row, _) in &raw_rows {
            let candidate = GeometryCandidate::from_rows(&catalog, row.1, &row.3).unwrap();
            let full_stalls = RealizationFeasibilityWorkspace::relaxed_ilc_order_is_impossible(
                &catalog, &candidate,
            );
            for subset in 1..1_usize << row.3.len() {
                let selected = row
                    .3
                    .iter()
                    .copied()
                    .enumerate()
                    .filter(|(index, _)| subset & (1 << index) != 0)
                    .map(|(_, row)| row)
                    .collect::<Vec<_>>();
                if prefix_pruning::selected_prefix_stalls(&catalog, &selected) {
                    assert!(
                        full_stalls,
                        "selected dependency reject must survive full completion"
                    );
                }
            }
        }
    }
    assert!(
        total_negatives > 0,
        "fixtures must exercise actual rejection"
    );
}

#[test]
fn physical_prefix_first_clear_keeps_grounded_o_witness_and_unknown_inputs() {
    let catalog =
        GeometryCatalog::compile_for_required_cells_on_dimensions(4, 4, 0, 0xffff).unwrap();
    let rows =
        [0x33, 0xcc, 0x3300, 0xcc00].map(|cells| catalog.skeleton_id(PieceKind::O, cells).unwrap());
    for subset in 0..1_usize << rows.len() {
        let selected = rows
            .iter()
            .copied()
            .enumerate()
            .filter(|(index, _)| subset & (1 << index) != 0)
            .map(|(_, row)| row)
            .collect::<Vec<_>>();
        assert!(!prefix_pruning::selected_prefix_stalls(&catalog, &selected));
        assert!(!prefix_pruning::selected_first_clear_stalls(
            &catalog, &selected
        ));
    }
    assert!(!prefix_pruning::selected_prefix_stalls(
        &catalog,
        &[u32::MAX]
    ));
    for selected in [
        vec![],
        vec![u32::MAX],
        vec![rows[0], rows[0]],
        vec![rows[0]; MAX_BOARD64_PIECES + 1],
    ] {
        assert!(!prefix_pruning::selected_first_clear_stalls(
            &catalog, &selected
        ));
    }
}

#[test]
fn physical_prefix_exact_weight_counts_all_product_continuations_and_overflow() {
    let mut family = GeometrySolutionFamily::new();
    let a = family.append(1, FAMILY_EMPTY).unwrap();
    let b = family.append(2, FAMILY_EMPTY).unwrap();
    let two = family.union(a, b).unwrap();
    let four = family.product(two, two).unwrap();
    let counts = family.path_count_table().unwrap();
    let mut task = TraversalTask {
        family: two,
        continuations: [FAMILY_INVALID; MAX_BOARD64_PIECES],
        depth: 0,
        continuation_count: 2,
        physical_prefix_state: PREFIX_IMPOSSIBLE,
    };
    task.continuations[..2].copy_from_slice(&[four, two]);
    assert_eq!(exact_traversal_weight(task, &counts), Some(16));
    task.family = FAMILY_EMPTY;
    assert_eq!(exact_traversal_weight(task, &counts), Some(8));
    let mut overflow_counts = counts;
    overflow_counts[two as usize] = u128::MAX;
    assert_eq!(exact_traversal_weight(task, &overflow_counts), None);
    let prior_task_bytes = (core::mem::size_of::<u32>() * (MAX_BOARD64_PIECES + 1) + 2 + 3) & !3;
    assert_eq!(
        core::mem::size_of::<TraversalTask>(),
        prior_task_bytes,
        "certificate reuses padding"
    );
}

#[test]
fn physical_prefix_single_node_yields_preserve_siblings_and_exact_counts() {
    let (catalog, mut search) = mixed_fixture(0x3c0f_03c0f, PhysicalPrefixMode::SkipSubtrees);
    let (_, mut reference) = mixed_fixture(0x3c0f_03c0f, PhysicalPrefixMode::SkipSubtrees);
    let expected = collect(&mut reference, &catalog);
    while search.enumerator.is_none() {
        assert!(matches!(search.advance(&catalog), GeometryAdvance::Pending));
    }
    let mut enumerator = search.enumerator.take().unwrap();
    assert!(
        enumerator.prefix_counts.is_some(),
        "reuse compiler count table"
    );
    let mut actual = Vec::new();
    let mut pending = 0;
    loop {
        match enumerator
            .next_candidate_impl::<true, true, true>(&catalog, 1)
            .unwrap()
        {
            FamilyEnumerationAdvance::Pending => pending += 1,
            FamilyEnumerationAdvance::Candidate(candidate) => actual.push((
                record(enumerator.prefix.last_original_ordinal.unwrap(), &candidate),
                candidate.physical_prefix_impossible(),
            )),
            FamilyEnumerationAdvance::Complete => break,
        }
    }
    assert!(pending > 0);
    assert_eq!(actual, expected);
    assert_eq!(
        enumerator.prefix.raw_candidates,
        reference.raw_candidate_progress().unwrap()
    );
    assert_eq!(
        enumerator.prefix.stats.skipped_candidates,
        reference.physical_prefix_stats().skipped_candidates
    );
}

#[cfg(feature = "parallel")]
#[test]
fn physical_prefix_parallel_seeds_preserve_ordinals_and_release_shared_counts() {
    let (catalog, raw) = mixed_fixture(0x3c0f_03c0f, PhysicalPrefixMode::Disabled);
    let (_, marked) = mixed_fixture(0x3c0f_03c0f, PhysicalPrefixMode::MarkCandidates);
    let (_, skipped) = mixed_fixture(0x3c0f_03c0f, PhysicalPrefixMode::SkipSubtrees);
    let prepare = |search: GeometrySearch| match search
        .prepare_parallel(&catalog, &ExecutionControl::default(), 2, |_, _| 2)
        .unwrap()
    {
        ParallelGeometryPreparation::Ready { plan, .. } => plan,
        ParallelGeometryPreparation::Serial { .. } => panic!("fixture must split"),
    };
    let mut raw = prepare(raw);
    let mut marked = prepare(marked);
    let mut skipped = prepare(skipped);
    assert_eq!(raw.searches.len(), marked.searches.len());
    assert_eq!(raw.searches.len(), skipped.searches.len());
    let counts = skipped.searches[0]
        .enumerator
        .as_ref()
        .unwrap()
        .prefix_counts
        .as_ref()
        .unwrap();
    let weak_counts = Arc::downgrade(counts);
    assert_eq!(
        skipped.shared_family_bytes - marked.shared_family_bytes,
        prefix_count_owner_bytes(counts)
    );
    for search in &skipped.searches {
        assert!(Arc::ptr_eq(
            counts,
            search
                .enumerator
                .as_ref()
                .unwrap()
                .prefix_counts
                .as_ref()
                .unwrap()
        ));
    }
    for index in 0..raw.searches.len() {
        let all = collect(&mut raw.searches[index], &catalog);
        let marked_rows = collect(&mut marked.searches[index], &catalog);
        let retained = collect(&mut skipped.searches[index], &catalog);
        assert_eq!(
            all.iter().map(|(row, _)| row).collect::<Vec<_>>(),
            marked_rows.iter().map(|(row, _)| row).collect::<Vec<_>>()
        );
        assert_eq!(
            retained,
            all.iter()
                .filter(|(row, _)| !full_negative(&catalog, row))
                .cloned()
                .collect::<Vec<_>>()
        );
        assert_eq!(
            skipped.searches[index].raw_candidate_progress(),
            Some(all.len())
        );
        assert_eq!(
            raw.searches[index].candidate_family_count(),
            Some(all.len() as u128)
        );
    }
    assert!(
        weak_counts.upgrade().is_none(),
        "last completed branch releases exact-count owner"
    );
    let merged = GeometrySearch::from_parallel_searches(
        &skipped.searches,
        skipped.group_pattern_index_bytes,
        skipped.shared_family_bytes,
    );
    assert_eq!(
        merged.retained_bytes(),
        0,
        "completed merge owns no family/count-table payload"
    );
    assert_eq!(
        merged.parallel_family_dispatch_bytes(),
        skipped.shared_family_bytes
    );
    assert!(
        merged.parallel_family_dispatch_bytes() > 0,
        "historical dispatch estimate remains available separately"
    );
    assert_eq!(
        merged.candidate_family_count(),
        raw.searches.iter().try_fold(0_u128, |total, search| total
            .checked_add(search.candidate_family_count()?))
    );
}
