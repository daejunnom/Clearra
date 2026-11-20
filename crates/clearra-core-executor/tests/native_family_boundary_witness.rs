//! Local diagnosis of one exact key from the full-p7p4 C/Rust family diff.
//! This intentionally does not construct or search a Geometry solution DAG.

#![cfg(feature = "native-c-core")]

use clearra_core_domain::{
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    piece::piece_kind::PieceKind,
    pruning::PruningEvidencePolicy,
    solution::{NormalizedTilingSolutionKey, PiecePlacementMask, StandardBoard64TilingIdentity},
};
use clearra_core_executor::{CoreExecutionResult, WasmCpuSearchBackend};
use clearra_core_ffi::{
    problem::{C_PIECE_I, C_PIECE_J, C_PIECE_L, C_PIECE_O, C_PIECE_S, C_PIECE_T, C_PIECE_Z},
    CBuildUpProblemTemplate, CPackingProblemBuilder, CoreCNative, NativeBuildUpWorkspace,
    NativeBuildableGeometryTaskOutcome, NativeCandidateReducer,
    C_BUILDUP_STATUS_LOGICAL_REJECT_MAX, C_BUILDUP_STATUS_LOGICAL_REJECT_MIN, C_BUILDUP_STATUS_OK,
};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
    PcSolutionProbabilityPolicy, PieceWindow, RequestedSearchBackend, SupplyWindowSize,
};
use clearra_problem::{ProblemCompiler, SearchProblem};
use clearra_rules::profile::builtin_rules::srs_plus;
use clearra_supply::{
    queue::fixed_sequence::FixedSequence, PackingHoldProjection, PieceMultisetKey,
};

#[derive(Clone, Copy)]
enum Source {
    Fixed,
    StandardBag,
}

#[derive(Debug)]
struct BoundaryProbe {
    missing_catalog_rows: Vec<(PieceKind, u64)>,
    production: Option<ProductionProbe>,
}

#[derive(Debug)]
struct ProductionProbe {
    sink: NativeBuildableGeometryTaskOutcome,
    retained_count: usize,
    retained_exact_identity: bool,
    bare_buildup_status: i32,
    bare_without_catalog_status: Option<i32>,
}

fn rust_singleton(
    problem: &SearchProblem,
    identity: StandardBoard64TilingIdentity,
) -> Result<CoreExecutionResult, String> {
    // The diagnostic input domain is exactly this one identity. Keep the
    // original problem's catalog and supply language, but never export the
    // reducer's completion flags as claims about the original full family.
    WasmCpuSearchBackend::execute_complete_precomputed_geometry_candidates_with_control(
        problem,
        &[identity],
        &ExecutionControl::default(),
    )
    .map_err(|error| format!("Rust singleton verifier: {error:?}"))
}

fn piece_code(piece: PieceKind) -> u32 {
    u32::from(match piece {
        PieceKind::I => C_PIECE_I,
        PieceKind::O => C_PIECE_O,
        PieceKind::T => C_PIECE_T,
        PieceKind::S => C_PIECE_S,
        PieceKind::Z => C_PIECE_Z,
        PieceKind::J => C_PIECE_J,
        PieceKind::L => C_PIECE_L,
    })
}

fn probe(
    problem: &SearchProblem,
    identity: StandardBoard64TilingIdentity,
    source: Source,
) -> Result<BoundaryProbe, String> {
    let count = identity.placement_count();
    if identity.initial_board_mask() != problem.initial_board().occupied_mask()
        || problem.exact_pieces() != Some(count)
    {
        return Err("candidate initial field or placement count differs from the request".into());
    }

    let placements: Vec<_> = (0..count)
        .map(|index| {
            identity
                .placement(index)
                .ok_or("canonical placement missing")
        })
        .collect::<Result<_, _>>()?;
    let multiset = PieceMultisetKey::from_pieces(placements.iter().map(|p| p.piece()));
    let compact = match source {
        Source::Fixed => {
            CPackingProblemBuilder::from_search_problem_with_piece_multiset(problem, multiset)
        }
        Source::StandardBag => {
            let universe = problem
                .piece_source()
                .materialized_universe()
                .ok_or("request does not have its original materialized universe")?;
            // Match PackingRunner's hold projection and preserve the full family
            // descriptor, rather than accidentally fixing supply to one queue.
            let hold_projection = if problem.supply().projects_unplaced_lookahead()
                && !problem.supply().projects_standard_bag_lookahead()
            {
                PackingHoldProjection::ReleaseHeldAtTerminal
            } else {
                PackingHoldProjection::PreserveFinalHoldLanguage
            };
            let family = universe.packing_multiset_family_for_execution(
                count,
                problem.initial_hold(),
                problem.supply().hold_enabled(),
                hold_projection,
            );
            if !family.groups().iter().any(|group| group.key() == multiset) {
                return Err("candidate multiset is outside the original request".into());
            }
            CPackingProblemBuilder::from_search_problem_with_piece_multiset_family(problem, &family)
        }
    }
    .map_err(|error| format!("packing descriptor: {error:?}"))?;
    let cancellation = ExecutionCancellationToken::default();
    let catalog = CoreCNative::compile_geometry_catalog_with_cancellation(&compact, &cancellation)
        .map_err(|error| format!("catalog: {error:?}"))?;
    let view = catalog.view();
    let mut rows = Vec::with_capacity(count);
    let mut missing_catalog_rows = Vec::new();
    for placement in &placements {
        let piece = placement.piece();
        let mask = placement.cells_mask();
        let row = view
            .skeleton_cell_masks()
            .iter()
            .zip(view.skeleton_piece_kinds())
            .position(|(&cells, &kind)| cells == mask && kind == piece_code(piece));
        if let Some(row) = row {
            rows.push(u32::try_from(row).map_err(|_| "catalog row does not fit u32")?);
        } else {
            missing_catalog_rows.push((piece, mask));
        }
    }
    if !missing_catalog_rows.is_empty() {
        return Ok(BoundaryProbe {
            missing_catalog_rows,
            production: None,
        });
    }

    // C's raw exact materializer checks the target cover and piece multiset
    // independently of its DAG, then retains the full realization domains.
    let raw = catalog
        .materialize_row_ids(&compact, &rows)
        .map_err(|error| format!("exact rows do not materialize: {error:?}"))?;
    let template = match source {
        Source::Fixed => CBuildUpProblemTemplate::compile(problem),
        Source::StandardBag => CBuildUpProblemTemplate::compile_for_standard_bag_automaton(problem),
    }
    .map_err(|error| format!("BuildUp template: {error:?}"))?;

    let mut scratch = match source {
        Source::Fixed => {
            let mut scratch = template.new_scratch();
            template
                .configure_piece_source_pattern(&mut scratch, 0)
                .map_err(|error| format!("fixed supply: {error:?}"))?;
            scratch
        }
        Source::StandardBag => template
            .new_standard_bag_automaton_scratch()
            .map_err(|error| format!("standard-bag supply: {error:?}"))?,
    };
    template.attach_geometry_catalog(&mut scratch, &catalog);
    let mut workspace = NativeBuildUpWorkspace::new();
    let mut reducer = NativeCandidateReducer::new(&compact)
        .map_err(|error| format!("candidate reducer: {error:?}"))?;
    let sink = catalog
        .stream_buildable_rows(
            &rows,
            &compact,
            &mut scratch,
            &mut workspace,
            &cancellation,
            PruningEvidencePolicy::BestEffort,
            &mut reducer,
        )
        .map_err(|error| format!("production row sink: {error:?}"))?;
    let retained = reducer.into_candidates();
    let retained_count = retained.len();
    let retained_exact_identity = retained.iter().any(|candidate| {
        candidate.standard_board64_tiling_identity(identity.initial_board_mask()) == Ok(identity)
    });

    // An owned operation set bypasses the row sink's realization-feasibility
    // pass and its derived order/predecessors. The FFI sets the neutral
    // natural-order hint; it does not constrain the search to that order.
    let mut bare = template.new_scratch();
    match source {
        Source::Fixed => template.configure_packing_candidate(&mut bare, &raw, 0, 0),
        Source::StandardBag => {
            template.configure_packing_candidate_with_standard_bag_automaton(&mut bare, &raw)
        }
    }
    .map_err(|error| format!("bare BuildUp descriptor: {error:?}"))?;
    template.attach_geometry_catalog(&mut bare, &catalog);
    let bare_buildup_status = NativeBuildUpWorkspace::new()
        .buildup_exists_with_cancellation(&bare, &cancellation)
        .map_err(|error| format!("bare BuildUp: {error:?}"))?;
    let bare_without_catalog_status = if (C_BUILDUP_STATUS_LOGICAL_REJECT_MIN
        ..=C_BUILDUP_STATUS_LOGICAL_REJECT_MAX)
        .contains(&bare_buildup_status)
    {
        // Keep the variant-domain bits, but resolve them by direct
        // placement generation rather than the ILC catalog. Use a fresh
        // workspace so a previous negative cannot influence the result.
        bare.geometry_catalog = 0;
        Some(
            NativeBuildUpWorkspace::new()
                .buildup_exists_with_cancellation(&bare, &cancellation)
                .map_err(|error| format!("bare BuildUp without catalog: {error:?}"))?,
        )
    } else {
        None
    };

    Ok(BoundaryProbe {
        missing_catalog_rows,
        production: Some(ProductionProbe {
            sink,
            retained_count,
            retained_exact_identity,
            bare_buildup_status,
            bare_without_catalog_status,
        }),
    })
}

fn compile_case(source: Source) -> SearchProblem {
    let (height, count, queue, hold, supply_count) = match source {
        Source::Fixed => (
            2,
            5,
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O; 5])),
            false,
            5,
        ),
        Source::StandardBag => (4, 10, PcQueueInput::standard_7_bag(), true, 11),
    };
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(height, 0),
        queue,
        PieceWindow::new(count),
    )
    .with_exact_pieces(Some(count))
    .with_min_remaining_queue(0)
    .with_allow_hold(hold)
    .with_rule(srs_plus())
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
    .with_supply_window_size(SupplyWindowSize::new(supply_count))
    .with_retained_trace_limit(1)
    .with_execution_policy(
        PcExecutionPolicy::mvp_default()
            .with_requested_backend(RequestedSearchBackend::Cpu)
            .with_allow_backend_fallback(false)
            .with_workers(1)
            .with_worker_hardware_limit(1)
            .with_use_all_logical_processors(true)
            .with_cpu_warmup(false)
            .with_exact_legal_board_enabled(false)
            .with_conditioned_reachability_enabled(false)
            .with_tablebase_requested(false),
    );
    ProblemCompiler::compile_scenario_pc(&query).expect("compile candidate diagnosis request")
}

#[test]
fn fixed_candidate_passes_catalog_buildability_and_exact_sink() {
    let identity = StandardBoard64TilingIdentity::from_placements(
        0,
        (0..5).map(|index| PiecePlacementMask::new(PieceKind::O, 0xc03_u64 << (2 * index))),
    )
    .expect("canonical O-only two-line tiling");
    let problem = compile_case(Source::Fixed);
    let probe = probe(&problem, identity, Source::Fixed).expect("bounded native boundary probe");
    assert!(probe.missing_catalog_rows.is_empty(), "{probe:?}");
    let production = probe.production.expect("all canonical rows exist");
    assert_eq!(production.sink.status, 0, "{production:?}");
    assert_eq!(production.sink.report.complete, 1);
    assert_eq!(production.sink.report.generated_count, 1);
    assert_eq!(production.sink.report.buildable_count, 1);
    assert_eq!(production.retained_count, 1);
    assert!(production.retained_exact_identity);
    assert_eq!(production.bare_buildup_status, C_BUILDUP_STATUS_OK);
    assert_eq!(production.bare_without_catalog_status, None);
    let rust = rust_singleton(&problem, identity).expect("bounded Rust singleton");
    assert_eq!(rust.normalized_solution_identities(), &[identity]);
    assert_eq!(rust.path_steps().len(), identity.placement_count());
}

#[test]
#[ignore = "local-only: requires one exact full-p7p4 family-diff key"]
fn full_p7p4_one_exact_c_boundary_probe() {
    let key = std::env::var("CLEARRA_FAMILY_BOUNDARY_KEY")
        .expect("set CLEARRA_FAMILY_BOUNDARY_KEY to one exact key from the family diff");
    let identity = NormalizedTilingSolutionKey::parse_canonical(&key)
        .and_then(|key| key.standard_board64_identity())
        .expect("valid canonical Board64 family key");
    let problem = compile_case(Source::StandardBag);
    match rust_singleton(&problem, identity) {
        Ok(rust) => {
            let exact_match = rust.normalized_solution_identities() == [identity];
            println!(
                "Rust singleton: exact identity retained: {exact_match}; solution found: {:?}; resource truncated: {:?}",
                rust.bool_field("solution_found"),
                rust.bool_field("resource_truncated"),
            );
            if exact_match {
                // Core replay describes lock/hold/clear order, not the
                // movement inputs or the first-success kick sequence.
                println!("Rust singleton lock witness: {:#?}", rust.path_steps());
            }
        }
        Err(error) => println!("{error}"),
    }
    let report =
        probe(&problem, identity, Source::StandardBag).expect("one candidate boundary probe");
    println!("DAG membership: not tested");
    println!(
        "Missing (piece, original-cell mask): {:?}",
        report.missing_catalog_rows
    );
    if let Some(production) = report.production {
        println!("Production row sink: {:#?}", production.sink);
        println!(
            "Retained count: {}; exact identity retained: {}; bare BuildUp status: {}; bare BuildUp without catalog status: {:?}",
            production.retained_count,
            production.retained_exact_identity,
            production.bare_buildup_status,
            production.bare_without_catalog_status,
        );
    }
}
