use std::collections::BTreeSet;

use clearra_core_domain::{
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    pc::pc_target::PcTarget,
    solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{OpeningPcSearchQuery, PcHoldPolicy, PcQueueInput};
use clearra_problem::{ProblemCompiler, SearchProblem};

use super::{
    catalog::GeometryCatalog,
    geometry::{GeometryAdvance, GeometrySearch},
};

#[test]
fn split_family_traversal_matches_serial_geometry_on_small_search() {
    assert_serial_and_partitioned_geometry_match(
        problem_for(
            PcTarget::two_lines(),
            PcQueueInput::default(),
            PcHoldPolicy::Disabled,
        ),
        4,
    );
}

#[test]
#[ignore = "full P7P4 geometry traversal equivalence"]
fn split_family_traversal_matches_serial_p7p4_geometry_exactly() {
    let problem = empty_four_line_p7p4_problem();
    assert_serial_and_partitioned_geometry_match(problem, 4);
}

fn assert_serial_and_partitioned_geometry_match(problem: SearchProblem, workers: usize) {
    let catalog = GeometryCatalog::compile(&problem).expect("geometry catalog");
    let control = ExecutionControl::new(ExecutionCancellationToken::new());

    let mut serial = geometry_for(&problem, &catalog);
    serial
        .compile_for_parallel(&catalog, &control)
        .expect("serial family compile");
    assert_eq!(serial.parallel_target_count(), 0);
    let serial_count = serial
        .candidate_family_count()
        .expect("P7P3 family path count");
    let mut remaining_serial_identities = collect_identities(&mut serial, &catalog);

    let mut partition_source = geometry_for(&problem, &catalog);
    partition_source
        .compile_for_parallel(&catalog, &control)
        .expect("partitioned family compile");
    let plan = match partition_source.into_parallel_plan(workers) {
        Ok(plan) => plan,
        Err(_) => panic!("family has independent traversal partitions"),
    };
    let partition_count = plan
        .searches
        .iter()
        .map(|search| {
            search
                .candidate_family_count()
                .expect("partition path count")
        })
        .sum::<u128>();
    for mut search in plan.searches {
        visit_identities(&mut search, &catalog, |identity| {
            assert!(
                remaining_serial_identities.remove(&identity),
                "partition traversal emitted an extra or duplicate identity"
            );
        });
    }

    assert_eq!(partition_count, serial_count);
    assert!(
        remaining_serial_identities.is_empty(),
        "partition traversal omitted serial identities"
    );
}

fn empty_four_line_p7p4_problem() -> SearchProblem {
    problem_for(
        PcTarget::four_lines(),
        PcQueueInput::standard_7_bag(),
        PcHoldPolicy::EnabledEmpty,
    )
}

fn problem_for(target: PcTarget, queue: PcQueueInput, hold_policy: PcHoldPolicy) -> SearchProblem {
    let query = OpeningPcSearchQuery::new(target)
        .with_queue(queue)
        .with_hold_policy(hold_policy)
        .with_objective(ObjectivePolicy::unique());
    ProblemCompiler::compile_opening_pc(&query).expect("four-line problem")
}

fn geometry_for(problem: &SearchProblem, catalog: &GeometryCatalog) -> GeometrySearch {
    let universe = problem
        .piece_source()
        .materialized_universe()
        .expect("materialized standard-bag universe");
    let target_piece_count = catalog.required_cells().count_ones() as usize / 4;
    let hold_enabled = problem.supply().hold_enabled();
    let family = universe.packing_multiset_family_for_execution(
        target_piece_count,
        problem.initial_hold(),
        hold_enabled,
        super::packing_hold_projection(problem),
    );
    if target_piece_count == 10 {
        assert_eq!(family.groups().len(), if hold_enabled { 140 } else { 35 });
        assert!(family.groups().iter().all(|group| {
            let counts = group.key().counts();
            group.key().total_count() == 10 && counts.iter().all(|count| *count <= 2)
        }));
    }
    GeometrySearch::new(universe, &family, catalog.required_cells(), false)
        .expect("geometry search")
}

fn collect_identities(
    search: &mut GeometrySearch,
    catalog: &GeometryCatalog,
) -> BTreeSet<StandardBoard64TilingIdentity> {
    let mut identities = BTreeSet::new();
    visit_identities(search, catalog, |identity| {
        identities.insert(identity);
    });
    identities
}

fn visit_identities(
    search: &mut GeometrySearch,
    catalog: &GeometryCatalog,
    mut visit: impl FnMut(StandardBoard64TilingIdentity),
) {
    loop {
        match search.advance(catalog) {
            GeometryAdvance::Pending => {}
            GeometryAdvance::Candidate(candidate) => {
                visit(candidate.identity);
            }
            GeometryAdvance::Complete => return,
            GeometryAdvance::ResourceIncomplete(reason) => {
                panic!("geometry traversal incomplete: {reason}")
            }
        }
    }
}
