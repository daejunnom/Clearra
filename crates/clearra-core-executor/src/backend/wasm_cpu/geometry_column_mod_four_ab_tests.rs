use std::{collections::BTreeSet, hint::black_box, time::Instant};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::{
        normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities,
        StandardBoard64TilingIdentity,
    },
};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
};
use clearra_problem::{ProblemCompiler, SearchProblem};
use clearra_supply::queue::fixed_sequence::FixedSequence;

use crate::WasmCpuSearchBackend;

use super::{
    catalog::GeometryCatalog,
    geometry::{GeometryAdvance, GeometrySearch},
    geometry_projection::with_column_mod_four_ab_mode,
};

const REPEATS: usize = 15;

#[derive(Clone)]
struct Fixture {
    id: &'static str,
    lines: u16,
    vacancies: &'static [(u8, u8)],
    pieces: &'static [PieceKind],
    expected: &'static str,
}

#[derive(Clone)]
struct Measurement {
    geometry_us: u128,
    total_us: u128,
    expanded_nodes: usize,
    projection_prunes: usize,
    geometry_count: usize,
    geometry_hash: String,
    solution_count: usize,
    solution_hash: String,
}

#[test]
#[ignore = "local-only bounded column-mod-four A/B; never run in CI"]
fn benchmark_column_mod_four_residual_filter() {
    let fixtures = fixtures();
    for (arm, enabled) in [
        ("A1-baseline", false),
        ("B1-candidate", true),
        ("B2-candidate", true),
        ("A2-baseline", false),
    ] {
        with_column_mod_four_ab_mode(enabled, || measure_arm(arm, &fixtures));
    }
}

fn measure_arm(arm: &str, fixtures: &[Fixture]) {
    // Warm the exact same binary and code path before recording every arm.
    for fixture in fixtures {
        let _ = measure(fixture);
    }

    println!("AB_ENV\tarm={arm}\trepeats={REPEATS}\tworkers=1");
    for fixture in fixtures {
        let mut measurements = Vec::with_capacity(REPEATS);
        for _ in 0..REPEATS {
            measurements.push(measure(fixture));
        }
        let first = &measurements[0];
        assert!(measurements.iter().all(|sample| {
            sample.expanded_nodes == first.expanded_nodes
                && sample.projection_prunes == first.projection_prunes
                && sample.geometry_count == first.geometry_count
                && sample.geometry_hash == first.geometry_hash
                && sample.solution_count == first.solution_count
                && sample.solution_hash == first.solution_hash
        }));
        let geometry = summarize(
            measurements
                .iter()
                .map(|sample| sample.geometry_us)
                .collect(),
        );
        let total = summarize(measurements.iter().map(|sample| sample.total_us).collect());
        println!(
            "AB_RESULT\t{}\t{}\t{}\t{}L\tnodes={}\tprojection_prunes={}\tgeometry_count={}\tgeometry_hash={}\tsolution_count={}\tsolution_hash={}\tgeometry_median_us={}\tgeometry_p95_us={}\ttotal_median_us={}\ttotal_p95_us={}",
            arm,
            fixture.id,
            fixture.expected,
            fixture.lines,
            first.expanded_nodes,
            first.projection_prunes,
            first.geometry_count,
            first.geometry_hash,
            first.solution_count,
            first.solution_hash,
            geometry.0,
            geometry.1,
            total.0,
            total.1,
        );
    }
}

fn measure(fixture: &Fixture) -> Measurement {
    let problem = problem(fixture);
    let catalog = GeometryCatalog::compile(&problem).expect("geometry catalog");
    let mut geometry = geometry_for(&problem, &catalog);
    let geometry_started = Instant::now();
    let mut identities = BTreeSet::new();
    loop {
        match geometry.advance(&catalog) {
            GeometryAdvance::Pending => {}
            GeometryAdvance::Candidate(candidate) => {
                identities.insert(candidate.identity);
            }
            GeometryAdvance::Complete => break,
            GeometryAdvance::ResourceIncomplete(reason) => {
                panic!("{} geometry incomplete: {reason}", fixture.id)
            }
        }
    }
    let geometry_us = geometry_started.elapsed().as_micros();
    let geometry_identities: Vec<StandardBoard64TilingIdentity> = identities.into_iter().collect();
    let geometry_hash = normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(
        &geometry_identities,
    );

    let total_started = Instant::now();
    let result = WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default())
        .unwrap_or_else(|error| panic!("{} execution failed: {error:?}", fixture.id));
    let total_us = total_started.elapsed().as_micros();
    let solution_hash = result
        .field("normalized_solution_set_hash")
        .unwrap_or("absent")
        .to_owned();

    black_box(&result);
    Measurement {
        geometry_us,
        total_us,
        expanded_nodes: geometry.expanded_nodes(),
        projection_prunes: geometry.column_pruned_states(),
        geometry_count: geometry_identities.len(),
        geometry_hash,
        solution_count: result.normalized_solution_identities().len(),
        solution_hash,
    }
}

fn problem(fixture: &Fixture) -> SearchProblem {
    let visible_bits = u32::from(fixture.lines) * 10;
    let full = if visible_bits == 64 {
        u64::MAX
    } else {
        (1_u64 << visible_bits) - 1
    };
    let required = fixture.vacancies.iter().fold(0_u64, |mask, &(x, y)| {
        mask | (1_u64 << (u64::from(y) * 10 + u64::from(x)))
    });
    assert_eq!(required.count_ones() as usize, fixture.pieces.len() * 4);
    for y in 0..fixture.lines {
        let row = (required >> (y * 10)) & 0x3ff;
        assert_ne!(
            row, 0,
            "{} would trigger initial line normalization",
            fixture.id
        );
    }
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(fixture.lines, full ^ required),
        PcQueueInput::fixed_sequence(FixedSequence::new(fixture.pieces.to_vec())),
        PieceWindow::new(fixture.pieces.len()),
    )
    .with_allow_hold(false)
    .with_exact_pieces(Some(fixture.pieces.len()))
    .with_objective(ObjectivePolicy::tiling())
    .with_execution_policy(
        PcExecutionPolicy::default()
            .with_workers(1)
            .with_worker_hardware_limit(1)
            .with_use_all_logical_processors(true)
            .with_deterministic(true),
    );
    ProblemCompiler::compile_scenario_pc(&query).expect("bounded scenario problem")
}

fn geometry_for(problem: &SearchProblem, catalog: &GeometryCatalog) -> GeometrySearch {
    let universe = problem
        .piece_source()
        .materialized_universe()
        .expect("fixed queue universe");
    let target_piece_count = catalog.required_cells().count_ones() as usize / 4;
    let family = universe.packing_multiset_family_for_execution(
        target_piece_count,
        problem.initial_hold(),
        problem.supply().hold_enabled(),
        super::packing_hold_projection(problem),
    );
    GeometrySearch::new(universe, &family, catalog.required_cells(), false)
        .expect("geometry search")
}

fn summarize(mut samples: Vec<u128>) -> (u128, u128) {
    samples.sort_unstable();
    let median = samples[samples.len() / 2];
    let p95_index = ((samples.len() * 95).div_ceil(100)).saturating_sub(1);
    (median, samples[p95_index])
}

fn fixtures() -> Vec<Fixture> {
    use PieceKind::{I, J, L, O, S, T, Z};
    vec![
        Fixture {
            id: "pass-1l-i",
            lines: 1,
            vacancies: &[(0, 0), (1, 0), (2, 0), (3, 0)],
            pieces: &[I],
            expected: "pass",
        },
        Fixture {
            id: "pass-2l-o",
            lines: 2,
            vacancies: &[(0, 0), (1, 0), (0, 1), (1, 1)],
            pieces: &[O],
            expected: "pass",
        },
        Fixture {
            id: "pass-3l-io",
            lines: 3,
            vacancies: &[
                (0, 0),
                (1, 0),
                (0, 1),
                (1, 1),
                (0, 2),
                (1, 2),
                (2, 2),
                (3, 2),
            ],
            pieces: &[O, I],
            expected: "pass",
        },
        Fixture {
            id: "pass-4l-oooo-well",
            lines: 4,
            vacancies: &[
                (0, 0),
                (1, 0),
                (2, 0),
                (3, 0),
                (0, 1),
                (1, 1),
                (2, 1),
                (3, 1),
                (0, 2),
                (1, 2),
                (2, 2),
                (3, 2),
                (0, 3),
                (1, 3),
                (2, 3),
                (3, 3),
            ],
            pieces: &[O, O, O, O],
            expected: "pass",
        },
        Fixture {
            id: "pass-5l-ioooo",
            lines: 5,
            vacancies: &[
                (0, 0),
                (1, 0),
                (2, 0),
                (3, 0),
                (0, 1),
                (1, 1),
                (2, 1),
                (3, 1),
                (0, 2),
                (1, 2),
                (2, 2),
                (3, 2),
                (0, 3),
                (1, 3),
                (2, 3),
                (3, 3),
                (0, 4),
                (1, 4),
                (2, 4),
                (3, 4),
            ],
            pieces: &[I, O, O, O, O],
            expected: "pass",
        },
        Fixture {
            id: "pass-6l-oooooo-well",
            lines: 6,
            vacancies: &[
                (0, 0),
                (1, 0),
                (2, 0),
                (3, 0),
                (0, 1),
                (1, 1),
                (2, 1),
                (3, 1),
                (0, 2),
                (1, 2),
                (2, 2),
                (3, 2),
                (0, 3),
                (1, 3),
                (2, 3),
                (3, 3),
                (0, 4),
                (1, 4),
                (2, 4),
                (3, 4),
                (0, 5),
                (1, 5),
                (2, 5),
                (3, 5),
            ],
            pieces: &[O, O, O, O, O, O],
            expected: "pass",
        },
        Fixture {
            id: "reject-2l-oo-mod4",
            lines: 2,
            vacancies: &[
                (0, 0),
                (1, 0),
                (0, 1),
                (1, 1),
                (3, 0),
                (4, 0),
                (3, 1),
                (5, 1),
            ],
            pieces: &[O, O],
            expected: "reject",
        },
        Fixture {
            id: "reject-3l-ots",
            lines: 3,
            vacancies: &[
                (0, 0),
                (1, 0),
                (2, 0),
                (3, 0),
                (0, 1),
                (2, 1),
                (3, 1),
                (5, 1),
                (0, 2),
                (1, 2),
                (4, 2),
                (5, 2),
            ],
            pieces: &[O, T, S],
            expected: "reject",
        },
        Fixture {
            id: "mixed-4l-iotjl-open5",
            lines: 4,
            vacancies: &[
                (0, 0),
                (1, 0),
                (2, 0),
                (3, 0),
                (4, 0),
                (0, 1),
                (1, 1),
                (2, 1),
                (3, 1),
                (4, 1),
                (0, 2),
                (1, 2),
                (2, 2),
                (3, 2),
                (4, 2),
                (0, 3),
                (1, 3),
                (2, 3),
                (3, 3),
                (4, 3),
            ],
            pieces: &[I, O, T, J, L],
            expected: "mixed",
        },
        Fixture {
            id: "mixed-6l-iotszj-open4",
            lines: 6,
            vacancies: &[
                (0, 0),
                (1, 0),
                (2, 0),
                (3, 0),
                (0, 1),
                (1, 1),
                (2, 1),
                (3, 1),
                (0, 2),
                (1, 2),
                (2, 2),
                (3, 2),
                (0, 3),
                (1, 3),
                (2, 3),
                (3, 3),
                (0, 4),
                (1, 4),
                (2, 4),
                (3, 4),
                (0, 5),
                (1, 5),
                (2, 5),
                (3, 5),
            ],
            pieces: &[I, O, T, S, Z, J],
            expected: "mixed",
        },
    ]
}
