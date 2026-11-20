//! Local diagnosis of exact keys from the full-p7p4 C/Rust family diff.
//! The singleton and graph-only probes are independent opt-in tests.

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
    CBuildUpProblemTemplate, CPackingProblem, CPackingProblemBuilder, CoreCNative,
    NativeBuildUpWorkspace, NativeBuildableGeometryTaskOutcome, NativeCandidateReducer,
    NativeCoreError, NativeGeometryCatalog, NativeGeometryPathConsumer,
    NativeGeometryPathSinkError, NativeGeometrySolutionGraph, C_BUILDUP_STATUS_LOGICAL_REJECT_MAX,
    C_BUILDUP_STATUS_LOGICAL_REJECT_MIN, C_BUILDUP_STATUS_OK,
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

fn original_bag_packing(
    problem: &SearchProblem,
    count: usize,
    required_multiset: Option<PieceMultisetKey>,
) -> Result<CPackingProblem, String> {
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
    if required_multiset
        .is_some_and(|multiset| !family.groups().iter().any(|group| group.key() == multiset))
    {
        return Err("candidate multiset is outside the original request".into());
    }
    CPackingProblemBuilder::from_search_problem_with_piece_multiset_family(problem, &family)
        .map_err(|error| format!("packing descriptor: {error:?}"))
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
                .map_err(|error| format!("packing descriptor: {error:?}"))?
        }
        Source::StandardBag => original_bag_packing(problem, count, Some(multiset))?,
    };
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
    compile_case_with_workers(source, 1)
}

fn compile_case_with_workers(source: Source, workers: usize) -> SearchProblem {
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
            .with_workers(workers)
            .with_worker_hardware_limit(workers)
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

const P7P4_DEPTH: usize = 10;
const ABBA_WORKERS: usize = 8;
// Mirrors the tested CPU production graph's worker-to-task ratio.
const TASKS_PER_WORKER: usize = 4;
const MAX_DAG_PROBE_KEYS: usize = 16;
const DEFAULT_DAG_PATH_LIMIT: u64 = 100_000_000;

fn exact_catalog_rows(
    catalog: &NativeGeometryCatalog,
    identity: StandardBoard64TilingIdentity,
) -> Result<Vec<u32>, String> {
    if identity.initial_board_mask() != 0 || identity.placement_count() != P7P4_DEPTH {
        return Err("candidate is not an empty-field 4L PC identity".into());
    }
    let view = catalog.view();
    (0..P7P4_DEPTH)
        .map(|index| {
            let placement = identity
                .placement(index)
                .ok_or("canonical placement missing")?;
            let row = view
                .skeleton_cell_masks()
                .iter()
                .zip(view.skeleton_piece_kinds())
                .position(|(&mask, &piece)| {
                    mask == placement.cells_mask() && piece == piece_code(placement.piece())
                })
                .ok_or_else(|| format!("catalog row not present: {placement:?}"))?;
            u32::try_from(row).map_err(|_| "catalog row does not fit u32".into())
        })
        .collect()
}

struct ExactPathMatches<'a> {
    expected: &'a [Vec<u32>],
    // A bit set for a query key means this exact catalog row is in that key.
    // Intersection is only a prefilter; exact unordered row equality is
    // confirmed before recording a hit and the original order is preserved.
    by_row: &'a [u32],
    hits: Vec<Option<Vec<u32>>>,
    unmatched: u32,
    visited: u64,
    path_limit: u64,
    path_limit_reached: bool,
}

impl NativeGeometryPathConsumer for ExactPathMatches<'_> {
    fn consume(&mut self, rows: &[u32]) -> Result<(), NativeGeometryPathSinkError> {
        if self.visited >= self.path_limit {
            self.path_limit_reached = true;
            return Err(NativeGeometryPathSinkError::Cancelled);
        }
        self.visited += 1;
        if rows.len() != P7P4_DEPTH {
            return Err(NativeGeometryPathSinkError::Invalid);
        }
        let mut candidates = self.unmatched;
        for &row in rows {
            let membership = self
                .by_row
                .get(row as usize)
                .ok_or(NativeGeometryPathSinkError::Invalid)?;
            candidates &= membership;
            if candidates == 0 {
                return Ok(());
            }
        }
        while candidates != 0 {
            let index = candidates.trailing_zeros() as usize;
            let bit = 1_u32 << index;
            candidates &= !bit;
            if self.expected[index].iter().all(|row| rows.contains(row)) {
                self.hits[index] = Some(rows.to_vec());
                self.unmatched &= !bit;
            }
        }
        if self.unmatched == 0 {
            // The C callback has no dedicated successful-stop status. This
            // local sentinel is accepted only after every exact hit is latched.
            return Err(NativeGeometryPathSinkError::Cancelled);
        }
        Ok(())
    }
}

#[derive(Debug)]
enum DagScanEnd {
    AllFound,
    Exhausted,
    Unknown(String),
}

struct DagScan {
    end: DagScanEnd,
    visited: u64,
    tasks: usize,
    hits: Vec<Option<Vec<u32>>>,
}

fn scan_graph(
    graph: &NativeGeometrySolutionGraph,
    task_count: usize,
    expected: &[Vec<u32>],
    by_row: &[u32],
    path_limit: u64,
) -> DagScan {
    let mut consumer = ExactPathMatches {
        expected,
        by_row,
        hits: vec![None; expected.len()],
        unmatched: (1_u32 << expected.len()) - 1,
        visited: 0,
        path_limit,
        path_limit_reached: false,
    };
    let (tasks, _) = match graph.split_tasks(task_count) {
        Ok(tasks) => tasks,
        Err(error) => {
            return DagScan {
                end: DagScanEnd::Unknown(format!("graph split: {error:?}")),
                visited: 0,
                tasks: 0,
                hits: consumer.hits,
            };
        }
    };
    let cancellation = ExecutionCancellationToken::default();
    let mut end = DagScanEnd::Exhausted;
    for task in &tasks {
        match graph.stream_task_paths(task, &cancellation, &mut consumer) {
            Ok(_) => {}
            Err(NativeCoreError::ExecutionCancelled) if consumer.unmatched == 0 => {
                end = DagScanEnd::AllFound;
                break;
            }
            Err(NativeCoreError::ExecutionCancelled) if consumer.path_limit_reached => {
                end = DagScanEnd::Unknown("path visitation limit reached".into());
                break;
            }
            Err(error) => {
                end = DagScanEnd::Unknown(format!("graph path traversal: {error:?}"));
                break;
            }
        }
    }
    DagScan {
        end,
        visited: consumer.visited,
        tasks: tasks.len(),
        hits: consumer.hits,
    }
}

fn print_graph_scan(label: &str, scan: &DagScan, elapsed: std::time::Duration) {
    let end = match &scan.end {
        DagScanEnd::AllFound => "stopped after all exact hits",
        DagScanEnd::Exhausted => "exhausted",
        DagScanEnd::Unknown(reason) => reason,
    };
    println!(
        "C Geometry DAG {label}: tasks={} raw paths={} elapsed={elapsed:?} end={end}",
        scan.tasks, scan.visited,
    );
    for (index, hit) in scan.hits.iter().enumerate() {
        if let Some(path) = hit {
            println!("  key {index}: Present, native row order={path:?}");
        } else if matches!(&scan.end, DagScanEnd::Exhausted) {
            println!("  key {index}: Absent (raw traversal complete)");
        } else {
            println!("  key {index}: Unknown (raw traversal incomplete)");
        }
    }
}

fn recheck_graph_rows(
    identity: StandardBoard64TilingIdentity,
    rows: &[u32],
    compact: &CPackingProblem,
    catalog: &NativeGeometryCatalog,
    template: &CBuildUpProblemTemplate,
) -> Result<(), String> {
    let mut scratch = template
        .new_standard_bag_automaton_scratch()
        .map_err(|error| format!("standard-bag scratch: {error:?}"))?;
    template.attach_geometry_catalog(&mut scratch, catalog);
    let mut workspace = NativeBuildUpWorkspace::new();
    let mut reducer =
        NativeCandidateReducer::new(compact).map_err(|error| format!("row reducer: {error:?}"))?;
    let report = catalog
        .stream_buildable_rows(
            rows,
            compact,
            &mut scratch,
            &mut workspace,
            &ExecutionCancellationToken::default(),
            PruningEvidencePolicy::BestEffort,
            &mut reducer,
        )
        .map_err(|error| format!("ordered row sink: {error:?}"))?;
    let retained = reducer.into_candidates();
    let exact = retained.iter().any(|candidate| {
        candidate.standard_board64_tiling_identity(identity.initial_board_mask()) == Ok(identity)
    });
    println!(
        "    C row-sink recheck: status={} complete={} buildable={} buildup_status={} retained={} exact={exact} prune_counts={:?}",
        report.status,
        report.report.complete,
        report.report.candidate_buildable,
        report.report.buildup_status,
        retained.len(),
        report.pruning_ledger.prune_reason_counts,
    );
    Ok(())
}

#[test]
#[ignore = "local-only: rebuilds the complete C Geometry DAG, not the full solver"]
fn full_p7p4_exact_c_dag_membership_probe() {
    let keys = std::env::var("CLEARRA_FAMILY_DAG_KEYS")
        .or_else(|_| std::env::var("CLEARRA_FAMILY_BOUNDARY_KEY"))
        .expect("set CLEARRA_FAMILY_DAG_KEYS to newline-separated exact canonical keys");
    let identities: Vec<_> = keys
        .lines()
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .map(|key| {
            NormalizedTilingSolutionKey::parse_canonical(key)
                .and_then(|key| key.standard_board64_identity())
                .expect("valid canonical Board64 family key")
        })
        .collect();
    assert!((1..=MAX_DAG_PROBE_KEYS).contains(&identities.len()));
    let path_limit = std::env::var("CLEARRA_FAMILY_DAG_PATH_LIMIT")
        .map_or(DEFAULT_DAG_PATH_LIMIT, |limit| {
            limit.parse().expect("positive raw path limit")
        });
    assert!(path_limit != 0);

    let problem = compile_case_with_workers(Source::StandardBag, ABBA_WORKERS);
    let compact =
        original_bag_packing(&problem, P7P4_DEPTH, None).expect("original P7P4 packing family");
    let cancellation = ExecutionCancellationToken::default();
    let catalog = CoreCNative::compile_geometry_catalog_with_cancellation(&compact, &cancellation)
        .expect("original native ILC catalog; failure is not DAG absence");
    let mut by_row = vec![0_u32; catalog.view().skeleton_cell_masks().len()];
    let expected: Vec<_> = identities
        .iter()
        .copied()
        .enumerate()
        .map(|(index, identity)| {
            let rows = exact_catalog_rows(&catalog, identity)
                .expect("query key's candidate rows are outside this catalog");
            catalog
                .materialize_row_ids(&compact, &rows)
                .expect("query key's rows do not form an exact original-family cover");
            for &row in &rows {
                by_row[row as usize] |= 1_u32 << index;
            }
            rows
        })
        .collect();

    let began = std::time::Instant::now();
    let outcome = match CoreCNative::search_geometry_solution_graph(
        &catalog,
        &compact,
        &cancellation,
        PruningEvidencePolicy::BestEffort,
    ) {
        Ok(outcome) if !outcome.resource_report.truncated => outcome,
        Ok(outcome) => {
            println!(
                "C Geometry DAG Unknown: truncated {:?}",
                outcome.resource_report
            );
            return;
        }
        Err(error) => {
            println!("C Geometry DAG Unknown: construction failed: {error:?}");
            return;
        }
    };
    println!(
        "C Geometry DAG built: nodes={} elapsed={:?}; raw path cap={path_limit}; candidate keys={}",
        outcome.graph.node_count(),
        began.elapsed(),
        identities.len(),
    );
    let began = std::time::Instant::now();
    let root = scan_graph(&outcome.graph, 1, &expected, &by_row, path_limit);
    print_graph_scan("one task", &root, began.elapsed());
    let began = std::time::Instant::now();
    let split = scan_graph(
        &outcome.graph,
        ABBA_WORKERS * TASKS_PER_WORKER,
        &expected,
        &by_row,
        path_limit,
    );
    print_graph_scan("production split", &split, began.elapsed());

    let template = CBuildUpProblemTemplate::compile_for_standard_bag_automaton(&problem)
        .expect("original standard-bag BuildUp template");
    for (index, &identity) in identities.iter().enumerate() {
        let mut previous_order = None;
        for (label, hit) in [
            ("one task", &root.hits[index]),
            ("production split", &split.hits[index]),
        ] {
            if let Some(rows) = hit.as_deref() {
                if previous_order == Some(rows) {
                    println!("  key {index}, {label}: same row order as previous recheck");
                    continue;
                }
                println!("  key {index}, {label}: rechecking exact emitted row order");
                if let Err(error) =
                    recheck_graph_rows(identity, rows, &compact, &catalog, &template)
                {
                    println!("    row-sink recheck Unknown: {error}");
                }
                previous_order = Some(rows);
            }
        }
    }
}
