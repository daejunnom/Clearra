//! Fixed, single-worker work comparison; never an adoption/timing benchmark.

#[cfg(not(feature = "local-search-ab"))]
fn main() {
    eprintln!("conditional_closed_within_work requires --features local-search-ab");
    std::process::exit(2);
}

#[cfg(feature = "local-search-ab")]
fn main() -> Result<(), String> {
    use clearra_core_domain::{
        piece::piece_kind::PieceKind::{I, J, L, T},
        solution::normalized_tiling_solution::{
            NormalizedTilingSolutionKey, PiecePlacementMask, StandardBoard64ColoredTilingIdentity,
            StandardBoard64TilingIdentity,
        },
    };
    use clearra_core_executor::backend::{
        run_conditional_ordered_work,
        ConditionalOrderedWorkMode::{ClosedWithin, Off, Point},
    };
    use clearra_coverage::pattern::pattern_id::PatternId;
    use clearra_pc_graph::request::{
        PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery,
        PcSolutionProbabilityPolicy, PieceWindow, RequestedSearchBackend, SupplyWindowSize,
    };
    use clearra_problem::{ProblemCompiler, SearchOutputPolicy};
    use clearra_rules::profile::builtin_rules::srs;
    use serde_json::json;

    // Other benchmark-only shortcuts cannot silently change the measured work.
    // No switch is set here; the ordered modes are strictly run-scoped.
    for key in [
        "CLEARRA_NO_ENTRY_COMPONENTS",
        "CLEARRA_GEOMETRIC_EDGE_REUSE",
        "CLEARRA_ILC_LAZY_FRAME",
        "CLEARRA_INCREMENTAL_DOMAIN",
        "CLEARRA_MINIMUM_DOMAIN_CUTOFF",
        "CLEARRA_MINIMUM_DOMAIN_WORK",
        "CLEARRA_SAME_TILE_PARENT_REUSE",
        "CLEARRA_TERMINAL_IDENTITY_MOVE",
    ] {
        if std::env::var_os(key).is_some_and(|value| value != "0") {
            return Err(format!(
                "{key} must be unset or disabled for this diagnostic"
            ));
        }
    }
    // Existing primitive-reference fixture from structural_graph_probability_tests:
    // these words are expected results, not a second solver.
    const BOARD: u64 = 0x203a_07dd;
    let placements = [
        (I, 0x03c0_0000),
        (T, 0x1c020),
        (J, 0x1c04_0000),
        (L, 0x3802),
    ];
    let expected_rows = [
        (255, [T, I, J, L]),
        (323, [T, J, I, L]),
        (643, [J, T, I, L]),
    ];
    let identity = StandardBoard64TilingIdentity::from_placements(
        BOARD,
        placements.map(|(kind, cells)| PiecePlacementMask::new(kind, cells)),
    )
    .map_err(|error| format!("fixture identity: {error:?}"))?;
    let colored = StandardBoard64ColoredTilingIdentity::from_standard_board64_identity(identity);
    let policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_workers(1)
        .with_exact_legal_board_enabled(false)
        .with_conditioned_reachability_enabled(false)
        .with_tablebase_requested(false)
        .with_max_memory_mib(Some(256));
    let problem = ProblemCompiler::compile_scenario_pc(
        &PcScenarioQuery::new(
            PcScenarioBoard::standard_10(3, BOARD),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(4),
        )
        .with_rule(srs())
        .with_allow_hold(false)
        .with_exact_pieces(Some(4))
        .with_supply_window_size(SupplyWindowSize::new(4))
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include)
        .with_allowed_colored_solution_identities([colored])
        .with_retained_trace_limit(0)
        .with_execution_policy(policy),
    )
    .map_err(|error| format!("fixture compile: {error:?}"))?
    .with_output_policy(SearchOutputPolicy::CoverageSummary);
    let universe = problem
        .piece_source()
        .materialized_universe()
        .ok_or("no fixture universe")?;
    if universe.pattern_count() != 840 {
        return Err("fixture must expose all 840 original queue ordinals".into());
    }
    let mut rows = Vec::new();
    for (ordinal, word) in expected_rows {
        let queue = universe.sequence_at(ordinal);
        if queue.as_ref() != word.as_slice() {
            return Err(format!(
                "fixture queue at original ordinal {ordinal} changed"
            ));
        }
        rows.push((
            ordinal,
            queue
                .iter()
                .map(|piece| piece.as_ascii())
                .collect::<String>(),
        ));
    }

    let mut reports = Vec::new();
    let mut baseline = None::<clearra_core_executor::CoreExecutionResult>;
    let mut point_work = None;
    for mode in [Off, Point, ClosedWithin] {
        let (result, work) = run_conditional_ordered_work(&problem, mode)?;
        let [row] = result.solution_coverages() else {
            return Err("expected one solution row".into());
        };
        if row.identity() != identity
            || row.covered_patterns().count_ones() != 3
            || [
                "count_complete",
                "probability_complete",
                "solution_probability_complete",
            ]
            .iter()
            .any(|key| result.bool_field(key) != Some(true))
            || result.bool_field("resource_truncated") != Some(false)
        {
            return Err(format!("{mode:?}: incomplete result or wrong solution row"));
        }
        for ordinal in 0..universe.pattern_count() {
            if row.covered_patterns().contains(PatternId::new(ordinal))
                != rows.iter().any(|(n, _)| *n == ordinal)
            {
                return Err(format!(
                    "{mode:?}: canonical original ordinal {ordinal} differs"
                ));
            }
        }
        if let Some(reference) = &baseline {
            if result.coverage_pattern_words() != reference.coverage_pattern_words()
                || result.normalized_solution_identities()
                    != reference.normalized_solution_identities()
                || result.solution_coverages() != reference.solution_coverages()
                || result.normalized_solution_coverages()
                    != reference.normalized_solution_coverages()
                || result.solution_probabilities() != reference.solution_probabilities()
            {
                return Err(format!(
                    "{mode:?}: solution, probability or coverage differs from Off"
                ));
            }
        }
        let order = work.conditional_order;
        let ordered = work.conditional_ordered_negative;
        let primitive = result
            .usize_field("total_reachability_states")
            .ok_or("missing primitive states")?;
        let graph = result
            .usize_field("total_build_order_nodes")
            .ok_or("missing graph nodes")?;
        let matches = match mode {
            Off => {
                ordered == Default::default()
                    && order.unknown_candidates == 1
                    && order.graph_bypasses == 0
                    && graph > 0
            }
            Point => {
                point_work = Some((order, primitive));
                ordered.proofs == 4
                    && ordered.checked_poses == 4
                    && ordered.attempts >= 4
                    && ordered.reused == 0
                    && order.graph_bypasses == 1
                    && graph == 0
                    && baseline
                        .as_ref()
                        .and_then(|r| r.usize_field("total_reachability_states"))
                        .is_some_and(|off| primitive + 4 < off)
            }
            ClosedWithin => {
                point_work.is_some_and(|(point, before)| point == order && primitive <= before)
                    && ordered.attempts == 1
                    && ordered.proofs == 1
                    && ordered.checked_poses == 1
                    && ordered.reused == 3
                    && order.graph_bypasses == 1
                    && graph == 0
            }
        };
        if !matches
            || order.attempted_candidates != 1
            || order.checked_transitions > 256
            || order.residual_states > 64
        {
            return Err(format!("{mode:?}: unexpected solver work: {order:?}, {ordered:?}, primitive={primitive}, graph={graph}"));
        }
        reports.push(json!({
            "mode": format!("{mode:?}"), "workers_used": result.usize_field("workers_used"), "ordinary_primitive_states": primitive,
            "ordinary_graph_nodes": graph, "graph_bypasses": order.graph_bypasses,
            "conditional_transitions": order.checked_transitions, "residual_states": order.residual_states,
            "ordered_attempts": ordered.attempts, "ordered_proofs": ordered.proofs,
            "ordered_checked_poses": ordered.checked_poses, "bounded_reuses": ordered.reused,
        }));
        if baseline.is_none() {
            baseline = Some(result);
        }
    }
    println!(
        "{}",
        json!({
            "schema":"clearra.conditional-closed-within-work.v1", "adoption_authority":false,
            "fixture":"srs90-3L-203a07dd-no-hold", "original_queues":universe.pattern_count(),
            "solution_identity":NormalizedTilingSolutionKey::from_standard_board64_identity(identity).as_str(),
            "colored_placements":placements.map(|(kind,cells)| (kind.as_ascii(),format!("{cells:#x}"))),
            "covered_ordinal_queue_rows":rows,"arms":reports,
        })
    );
    Ok(())
}

#[cfg(all(test, feature = "local-search-ab"))]
#[test]
fn canonical_840_queues_check_three_compiled_arms() {
    main().unwrap();
}
