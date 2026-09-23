#![cfg(all(feature = "parallel", feature = "local-search-ab"))]

//! Local-only, full-family differential for a generated legal-board candidate.
//! The generator's internal proof chain is not used as the result oracle here:
//! the ordinary exact PC search must return the same complete canonical set
//! with and without the candidate filter. This is one KAT, not profile-wide
//! asset qualification or a performance A/B.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::Instant,
};

use clearra_core_domain::{
    execution_cancellation::{ExecutionCancellationToken, ExecutionControl},
    pc::pc_target::PcTarget,
    piece::piece_kind::PieceKind,
};
use clearra_core_executor::{
    built_in_legal_board_binding, enumerate_pc4_ilc_geometric_predecessor_fields,
    enumerate_pc4_ilc_predecessor_fields, enumerate_pc4_ilc_target_fields,
    install_local_pc4_legal_board_index, materialize_pc4_ilc_transition,
    set_local_search_prune_policy, CompletionCapability, CoreExecutionResult, ExactLegalBoard,
    LegalBoardDecision, LegalBoardExpectation, LegalBoardQuery, LocalPc4LegalBoardIndex,
    LocalSearchPrunePolicy, WasmCpuSearchBackend,
};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcExecutionPolicy, PcHoldPolicy, PcQueueInput, RequestedSearchBackend,
};
use clearra_problem::ProblemCompiler;
use clearra_rules::{
    kicks::KickTableProfileId,
    profile::builtin_rules::{jstris_180, no_kick, srs, srs_plus, srs_x},
};

#[test]
#[ignore = "requires one local generated profile bundle and a full 4L P7P4 exact search"]
fn generated_bundle_preserves_complete_p7p4_solution_identity_set() {
    let profile_name = std::env::var("CLEARRA_LOCAL_PC4_LEGAL_BOARD_PROFILE")
        .expect("set CLEARRA_LOCAL_PC4_LEGAL_BOARD_PROFILE explicitly");
    let profile = KickTableProfileId::parse(&profile_name).expect("known kick-table profile");
    let workers = std::env::var("CLEARRA_LOCAL_PC4_KAT_WORKERS")
        .ok()
        .map(|raw| raw.parse::<usize>().expect("KAT worker count is numeric"))
        .unwrap_or(4);
    assert!((1..=64).contains(&workers), "KAT worker count is bounded");
    let baseline_only = std::env::var("CLEARRA_LOCAL_PC4_KAT_MODE")
        .ok()
        .is_some_and(|value| value == "baseline-only");
    let previous = set_local_search_prune_policy(LocalSearchPrunePolicy::product_default());
    let started = Instant::now();
    let baseline = execute_p7p4(profile, workers);
    report_result(
        "baseline",
        workers,
        started.elapsed().as_millis(),
        &baseline,
    );
    assert_complete_flags(&baseline);
    if baseline_only {
        set_local_search_prune_policy(previous);
        assert_known_count(&baseline, profile);
        return;
    }

    let bundle_path = PathBuf::from(
        std::env::var_os("CLEARRA_LOCAL_PC4_LEGAL_BOARD_BUNDLE")
            .expect("set CLEARRA_LOCAL_PC4_LEGAL_BOARD_BUNDLE to the generated bundle"),
    );

    let bytes = std::fs::read(&bundle_path).expect("read local candidate bundle");
    let binding = built_in_legal_board_binding(profile).expect("legal-board profile binding");
    let index = LocalPc4LegalBoardIndex::load_bundle(
        Arc::from(bytes),
        LegalBoardExpectation {
            binding,
            generation_identity: None,
        },
    )
    .expect("generated exact-intersection bundle parses for its own profile");
    eprintln!(
        "candidate legal-board resident index: bundle_bytes={} sparse_index_bytes={}",
        index.compressed_bytes(),
        index.sparse_index_bytes(),
    );
    assert!(index.compressed_bytes() + index.sparse_index_bytes() <= 128 * 1024 * 1024);
    install_local_pc4_legal_board_index(index).expect("isolated test has one installed bundle");
    set_local_search_prune_policy(LocalSearchPrunePolicy::product_default().with_legal_board(true));
    let started = Instant::now();
    let filtered = execute_p7p4(profile, workers);
    set_local_search_prune_policy(previous);

    report_result(
        "filtered",
        workers,
        started.elapsed().as_millis(),
        &filtered,
    );
    assert_complete_flags(&filtered);
    if baseline.normalized_solution_identities() != filtered.normalized_solution_identities() {
        let baseline_set: HashSet<_> = baseline
            .normalized_solution_identities()
            .iter()
            .copied()
            .collect();
        let filtered_set: HashSet<_> = filtered
            .normalized_solution_identities()
            .iter()
            .copied()
            .collect();
        let missing: Vec<_> = baseline_set.difference(&filtered_set).take(3).collect();
        let extra: Vec<_> = filtered_set.difference(&baseline_set).take(3).collect();
        eprintln!("candidate legal-board identity delta: missing={missing:?} extra={extra:?}");
    }
    assert_eq!(
        filtered.usize_field("normalized_unique_solution_count"),
        baseline.usize_field("normalized_unique_solution_count")
    );
    assert_eq!(
        filtered.field("normalized_solution_set_hash"),
        baseline.field("normalized_solution_set_hash")
    );
    assert!(
        filtered.normalized_solution_identities() == baseline.normalized_solution_identities(),
        "candidate legal-board changed the complete canonical solution identity set"
    );
    assert!(
        filtered.normalized_solution_coverages() == baseline.normalized_solution_coverages(),
        "candidate legal-board changed complete solution coverage"
    );
    assert_known_count(&baseline, profile);
}

fn execute_p7p4(profile: KickTableProfileId, workers: usize) -> CoreExecutionResult {
    let rule = match profile {
        KickTableProfileId::Srs90 => srs(),
        KickTableProfileId::SrsPlus => srs_plus(),
        KickTableProfileId::SrsX => srs_x(),
        KickTableProfileId::Jstris180 => jstris_180(),
        KickTableProfileId::NoKick => no_kick(),
        _ => panic!("unsupported legal-board profile"),
    };
    let policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_workers(workers)
        .with_worker_hardware_limit(workers)
        .with_use_all_logical_processors(true)
        .with_cpu_warmup(true);
    let query = OpeningPcSearchQuery::new(PcTarget::four_lines())
        .with_rule(rule)
        .with_queue(PcQueueInput::standard_7_bag())
        .with_hold_policy(PcHoldPolicy::EnabledEmpty)
        .with_objective(ObjectivePolicy::unique())
        .with_execution_policy(policy);
    let problem = ProblemCompiler::compile_opening_pc(&query).expect("exact P7P4 problem");
    WasmCpuSearchBackend::execute_with_control(
        &problem,
        &ExecutionControl::new(ExecutionCancellationToken::new()),
    )
    .expect("exact P7P4 CPU search")
}

fn report_result(
    stage: &str,
    requested_workers: usize,
    elapsed_ms: u128,
    result: &CoreExecutionResult,
) {
    eprintln!(
        "candidate legal-board {stage}: elapsed_ms={elapsed_ms} requested_workers={requested_workers} workers_used={:?} active_workers={:?} cpu_parallel={:?} decision={:?} warmup={:?} count={:?} hash={:?} geometry_nodes={:?} build_order_nodes={:?} worker_min_candidates={:?} worker_max_candidates={:?}",
        result.usize_field("workers_used"),
        result.usize_field("parallel_active_workers"),
        result.bool_field("cpu_parallel_execution"),
        result.field("cpu_parallel_decision_reason"),
        result.bool_field("cpu_warmup_performed"),
        result.usize_field("normalized_unique_solution_count"),
        result.field("normalized_solution_set_hash"),
        result.usize_field("searched_nodes"),
        result.usize_field("total_build_order_nodes"),
        result.usize_field("parallel_minimum_worker_candidates"),
        result.usize_field("parallel_maximum_worker_candidates"),
    );
}

fn assert_complete_flags(result: &CoreExecutionResult) {
    assert_eq!(result.bool_field("count_complete"), Some(true));
    assert_eq!(result.bool_field("probability_complete"), Some(true));
}

fn assert_known_count(result: &CoreExecutionResult, profile: KickTableProfileId) {
    let known_count = match profile {
        KickTableProfileId::SrsPlus => Some(456_923),
        KickTableProfileId::Jstris180 => Some(456_459),
        _ => None,
    };
    if let Some(known_count) = known_count {
        assert_eq!(
            result.usize_field("normalized_unique_solution_count"),
            Some(known_count)
        );
    }
}

/// Bounded local diagnosis of the one SRS+ identity lost by the first generated
/// F∩R bundle. This is not an asset qualification test or a product fixture.
#[test]
#[ignore = "requires the local SRS+ candidate bundle; inspects one missing identity only"]
fn diagnose_one_missing_solution_projection_against_generated_graph() {
    const MASKS: [u64; 10] = [
        515_396_075_520,
        3_148_800,
        29_360_136,
        550_561_644_544,
        12_294,
        393_984,
        25_769_803_824,
        114_752,
        234_881_152,
        7_516_192_769,
    ];
    const PIECES: [PieceKind; 10] = [
        PieceKind::I,
        PieceKind::O,
        PieceKind::T,
        PieceKind::T,
        PieceKind::S,
        PieceKind::Z,
        PieceKind::Z,
        PieceKind::J,
        PieceKind::J,
        PieceKind::L,
    ];
    let bundle_path = PathBuf::from(
        std::env::var_os("CLEARRA_LOCAL_PC4_LEGAL_BOARD_BUNDLE")
            .expect("set CLEARRA_LOCAL_PC4_LEGAL_BOARD_BUNDLE to the SRS+ candidate"),
    );
    let binding = built_in_legal_board_binding(KickTableProfileId::SrsPlus).unwrap();
    let board = ExactLegalBoard::load(
        Arc::from(std::fs::read(bundle_path).unwrap()),
        LegalBoardExpectation {
            binding,
            generation_identity: None,
        },
    )
    .unwrap();
    let states: Vec<_> = (0..1_usize << 10)
        .map(|subset| projected_missing_solution_state(subset, &MASKS))
        .collect();
    // The user's concrete sequence places the second J after I. Report this
    // path separately from the graph-reconstructed path; one canonical
    // placement family can have more than one exact BuildUp ordering.
    const REPORTED_ORDER: [usize; 10] = [4, 7, 5, 1, 3, 0, 8, 9, 2, 6];
    let mut reported_subset = 0_usize;
    for (depth, operation) in REPORTED_ORDER.into_iter().enumerate() {
        let source = states[reported_subset].0;
        reported_subset |= 1 << operation;
        let target = states[reported_subset].0;
        let outgoing =
            enumerate_pc4_ilc_target_fields(source, PIECES[operation], KickTableProfileId::SrsPlus)
                .expect("exact reported-order edge");
        let decision = board.decide(LegalBoardQuery {
            width: 10,
            height: 4,
            initial_board: 0,
            kick_profile: KickTableProfileId::SrsPlus,
            physical_board: physical_from_product(target, states[reported_subset].1),
            deleted_original_rows: states[reported_subset].1,
            placed_piece_count: depth + 1,
            completion: CompletionCapability::ClearToEmpty,
        });
        let placements = if operation == 8 {
            Some(
                materialize_pc4_ilc_transition(
                    source,
                    target,
                    PieceKind::J,
                    KickTableProfileId::SrsPlus,
                )
                .expect("reported J2 materialization"),
            )
        } else {
            None
        };
        eprintln!(
            "missing identity reported order: depth={} operation={operation} edge={} product={target:010x} deleted={:04b} decision={decision:?} J2_placements={placements:?}",
            depth + 1,
            outgoing.binary_search(&target).is_ok(),
            states[reported_subset].1,
        );
    }
    let mut reached = [false; 1 << 10];
    let mut parent = [None; 1 << 10];
    let mut targets_cache: HashMap<(u64, PieceKind), Vec<u64>> = HashMap::new();
    reached[0] = true;
    for subset in 0..(1 << 10) {
        if !reached[subset] {
            continue;
        }
        let (source, deleted_rows) = states[subset];
        for operation in 0..10 {
            let bit = 1 << operation;
            if subset & bit != 0 {
                continue;
            }
            let deleted_mask = (0..4)
                .filter(|row| deleted_rows & (1 << row) != 0)
                .fold(0_u64, |mask, row| mask | (1023_u64 << (row * 10)));
            if MASKS[operation] & deleted_mask != 0 {
                continue;
            }
            let child = subset | bit;
            let target = states[child].0;
            let targets = targets_cache
                .entry((source, PIECES[operation]))
                .or_insert_with(|| {
                    enumerate_pc4_ilc_target_fields(
                        source,
                        PIECES[operation],
                        KickTableProfileId::SrsPlus,
                    )
                    .expect("exact local graph transition")
                });
            if targets.binary_search(&target).is_ok() && !reached[child] {
                reached[child] = true;
                parent[child] = Some((subset, operation));
            }
        }
    }
    let reached_count = reached.iter().filter(|value| **value).count();
    eprintln!(
        "missing identity graph diagnosis: reached_subsets={reached_count} edge_queries={} complete={}",
        targets_cache.len(),
        reached[(1 << 10) - 1],
    );
    assert!(
        reached[(1 << 10) - 1],
        "generator graph cannot reconstruct the solver's one missing identity"
    );
    let mut path = vec![(1 << 10) - 1];
    while let Some((source, _operation)) = parent[*path.last().unwrap()] {
        path.push(source);
    }
    path.reverse();
    let mut verified_absent = Vec::new();
    for (depth, &subset) in path.iter().enumerate() {
        let (physical_product_board, deleted_rows) = states[subset];
        let decision = board.decide(LegalBoardQuery {
            width: 10,
            height: 4,
            initial_board: 0,
            kick_profile: KickTableProfileId::SrsPlus,
            physical_board: physical_from_product(physical_product_board, deleted_rows),
            deleted_original_rows: deleted_rows,
            placed_piece_count: subset.count_ones() as usize,
            completion: CompletionCapability::ClearToEmpty,
        });
        let next = path.get(depth + 1).map(|&child| {
            let operation = parent[child].expect("reachable child has a parent").1;
            let target = states[child].0;
            let geometric = enumerate_pc4_ilc_geometric_predecessor_fields(
                target,
                PIECES[operation],
                KickTableProfileId::SrsPlus,
            )
            .expect("geometric predecessor enumeration")
            .binary_search(&physical_product_board)
            .is_ok();
            let exact = enumerate_pc4_ilc_predecessor_fields(
                target,
                PIECES[operation],
                KickTableProfileId::SrsPlus,
            )
            .expect("exact predecessor enumeration")
            .binary_search(&physical_product_board)
            .is_ok();
            (operation, target, geometric, exact)
        });
        eprintln!(
            "missing identity path: depth={depth} subset={subset:010b} product={physical_product_board:010x} deleted={deleted_rows:04b} decision={decision:?} next={next:?}",
        );
        if decision == LegalBoardDecision::VerifiedAbsent {
            verified_absent.push((depth, subset));
        }
    }
    assert!(
        verified_absent.is_empty(),
        "generated legal-board falsely rejects a concrete graph path: {verified_absent:?}"
    );
}

fn projected_missing_solution_state(subset: usize, masks: &[u64; 10]) -> (u64, u16) {
    let occupied = masks
        .iter()
        .enumerate()
        .filter(|(operation, _)| subset & (1 << operation) != 0)
        .fold(0_u64, |board, (_, mask)| board | mask);
    let mut deleted_rows = 0_u16;
    let mut physical = 0_u64;
    let mut physical_row = 0_u32;
    for original_row in 0..4_u32 {
        let row = (occupied >> (original_row * 10)) & 1023;
        if row == 1023 {
            deleted_rows |= 1 << original_row;
        } else {
            physical |= row << (physical_row * 10);
            physical_row += 1;
        }
    }
    let prefix_bits = deleted_rows.count_ones() * 10;
    let prefix = if prefix_bits == 0 {
        0
    } else {
        (1_u64 << prefix_bits) - 1
    };
    ((physical << prefix_bits) | prefix, deleted_rows)
}

fn physical_from_product(product: u64, deleted_rows: u16) -> u64 {
    product >> (deleted_rows.count_ones() * 10)
}
