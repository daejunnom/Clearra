//! Compact B2B Path deferral retains the eager path's exact filtered authority.
use std::{collections::BTreeMap, collections::BTreeSet, sync::Arc};

use clearra_core_domain::execution_cancellation::{ExecutionCancellationToken, ExecutionControl};
use clearra_core_executor::{CoreExecutionError, CoreExecutionResult, WasmCpuSearchBackend};
use clearra_host_contract::PcPathWitnessPayload;
use clearra_objectives::policy::{
    objective_policy::ObjectivePolicy, score_objective_policy::SpinProfileSelection,
};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
};
use clearra_problem::SearchProblem;
use clearra_replay::ExactScoringExecutionBatch;
use clearra_rules::profile::builtin_rules::srs_plus;
use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

use super::AppCoreExecutorService;
use crate::{
    pc_path_result::{
        pc_path_witness_payload, project_canonical_execution_with_context,
        validate_pc_path_family_v2_result, PcPathProjectionContext, PcPathQueryBinding,
    },
    AppCommand, AppContext, AppRequest, AppServices, AppStatus, PcPathIngressOrigin,
    PcReplayPageSource, PcReplayPageStore, PcResultProjection, ProductCapabilityContract,
    ScenarioAppCommand,
};

fn service() -> AppCoreExecutorService {
    AppCoreExecutorService::wasm_cpu()
        .with_product_retention_budget(crate::ProductRetentionBudget::new(64 * 1024 * 1024))
}

fn eager_reference_service() -> AppCoreExecutorService {
    // This oracle deliberately keeps every replay alive. The IIOOO fixture
    // retains about 44 MiB before the eager product's existing 16x projection
    // reserve, so it cannot share the lazy product's 64 MiB host contract.
    // Keep a finite oracle budget; the deferred source and pages use service().
    AppCoreExecutorService::wasm_cpu().with_product_retention_budget(
        crate::ProductRetentionBudget::new(crate::ProductRetentionBudget::MAXIMUM_BYTES),
    )
}

fn source(
    height: u16,
    board: u64,
    pattern: &str,
    pieces: usize,
    hold: bool,
    spin: SpinProfileSelection,
) -> (Arc<PcScenarioQuery>, SearchProblem, CoreExecutionResult) {
    let query = Arc::new(
        PcScenarioQuery::new(
            PcScenarioBoard::standard_10(height, board),
            PcQueueInput::pattern_expression(QueuePatternExpression::parse(pattern, 16).unwrap()),
            PieceWindow::new(pieces),
        )
        .with_exact_pieces(Some(pieces))
        .with_allow_hold(hold)
        .with_rule(srs_plus())
        .with_execution_policy(PcExecutionPolicy::mvp_default().with_workers(1))
        .with_count_policy(PcCountPolicy::CountAll)
        .with_objective(ObjectivePolicy::all().with_back_to_back_preservation(spin)),
    );
    let problem = PcPathQueryBinding::Scenario(&query)
        .compile_expected()
        .unwrap();
    let result =
        WasmCpuSearchBackend::execute_with_control(&problem, &ExecutionControl::default()).unwrap();
    assert_eq!(
        result.bool_field("postprocess_pc_path_requested"),
        Some(true)
    );
    assert_ne!(
        result.bool_field("execution_constraint_materialized"),
        Some(true)
    );
    assert!(!result.exact_scoring_execution_batches().is_empty());
    assert!(result
        .exact_scoring_execution_batches()
        .iter()
        .all(|batch| batch.complete()));
    assert!(result.postprocess_executions().is_empty());
    (query, problem, result)
}

fn page_source(problem: &SearchProblem, result: &CoreExecutionResult) -> Arc<PcReplayPageSource> {
    page_source_with_service(&service(), problem, result)
}

fn page_source_with_service(
    executor: &AppCoreExecutorService,
    problem: &SearchProblem,
    result: &CoreExecutionResult,
) -> Arc<PcReplayPageSource> {
    let mut session = executor
        .prepare_pc_replay_page_source(problem, result, 0)
        .unwrap();
    for _ in 0..10_000 {
        if session.advance(8192, &ExecutionControl::default()).unwrap() {
            return session.complete().unwrap();
        }
    }
    panic!("small B2B replay fixture did not complete its manifest")
}

fn all_pages(source: &Arc<PcReplayPageSource>) -> Vec<PcPathWitnessPayload> {
    let mut store = PcReplayPageStore::new(Arc::clone(source));
    let mut witnesses = Vec::new();
    for geometry in 1..=source.geometry_count() {
        let first = store
            .page(geometry, 1, &ExecutionControl::default())
            .unwrap();
        let page_count: usize = first.metadata.member_page_count.parse().unwrap();
        for page in 1..=page_count {
            witnesses.extend(
                store
                    .page(geometry, page, &ExecutionControl::default())
                    .unwrap()
                    .witnesses,
            );
        }
    }
    witnesses
}

#[test]
fn b2b_path_deferred_replays_match_eager_authority_and_every_ordered_witness() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let control = ExecutionControl::default();
    for spin in [
        SpinProfileSelection::TSpins,
        SpinProfileSelection::AllMiniPlus,
    ] {
        for (height, board, pattern, pieces, hold) in [
            (1, 0x3f0, "I", 1, false),
            (2, 0xfc3f0, "OO", 2, false),
            (2, 0xfc3f0, "II", 2, false),
            (2, 0xfc3f0, "[OI]O", 2, true),
            (2, 0, "IIOOO", 5, false),
        ] {
            let (query, problem, raw) = source(height, board, pattern, pieces, hold, spin);
            let raw_complete = raw.postprocess_execution_complete();
            // Recreate the old eager-before-filter boundary without a product
            // toggle. Its pre-filter traces remain live through postprocess.
            let eager_input = eager_reference_service()
                .materialize_terminal_replay_partition(raw.clone(), &control)
                .unwrap();
            let eager_output_minimum = super::checked_materialized_replay_bytes(
                eager_input.postprocess_executions(),
                eager_input.postprocess_executions().len(),
            )
            .unwrap();
            let eager_reference_allowance =
                super::CompleteReplayWholeLiveBudget::from_result_with_host(
                    &raw,
                    eager_reference_service().product_retention_budget,
                )
                .unwrap()
                .checked_output_allowance()
                .unwrap();
            // Full Core retention is an upper bound on its replay output;
            // len-based slots below are a lower bound even after deduplication.
            assert!(
                eager_input.checked_resource_retained_bytes().unwrap() <= eager_reference_allowance
            );
            if pattern == "IIOOO" {
                let product_allowance =
                    super::CompleteReplayWholeLiveBudget::from_result_with_host(
                        &raw,
                        service().product_retention_budget,
                    )
                    .unwrap()
                    .checked_output_allowance()
                    .unwrap();
                assert!(eager_output_minimum > product_allowance);
                assert!(matches!(
                    service().materialize_terminal_replay_partition(raw.clone(), &control),
                    Err(CoreExecutionError::Pc(message))
                        if message.starts_with("complete_replay_memory_limit_exceeded:")
                ));
            }
            let eager = eager_reference_service()
                .postprocess_search_result(eager_input, &control)
                .unwrap();
            let deferred = service().postprocess_search_result(raw, &control).unwrap();
            assert!(deferred.postprocess_executions().is_empty());
            assert_eq!(deferred.postprocess_execution_complete(), raw_complete);
            assert_eq!(
                deferred.bool_field("execution_constraint_materialized"),
                Some(true)
            );

            // Every raw audit, coverage/count field, weight, and filtered
            // physical/scoring graph must match. Only the redundant replay
            // owner and its completeness bit may differ at this seam.
            let comparable = eager.clone().with_postprocess_execution_batch(
                Vec::new(),
                raw_complete,
                eager.postprocess_pattern_weights().to_vec(),
            );
            assert_eq!(comparable, deferred, "{pattern} {spin:?}");
            if !eager.postprocess_executions().is_empty() {
                assert!(
                    deferred.checked_resource_retained_bytes().unwrap()
                        < eager.checked_resource_retained_bytes().unwrap()
                );
            }

            let old_source = page_source_with_service(&eager_reference_service(), &problem, &eager);
            let new_source = page_source(&problem, &deferred);
            assert_eq!(new_source.identity_sha256(), old_source.identity_sha256());
            assert_eq!(new_source.geometry_count(), old_source.geometry_count());
            assert_eq!(new_source.witness_count(), old_source.witness_count());
            assert_eq!(new_source.first_members(), old_source.first_members());
            let actual = all_pages(&new_source);
            assert_eq!(actual, all_pages(&old_source));
            assert_eq!(actual.len() as u128, new_source.witness_count());

            // An independent complete expansion AFTER filtering is the exact
            // witness/order oracle, including every step and supply state.
            let expanded = eager_reference_service()
                .materialize_terminal_replay_partition(deferred.clone(), &control)
                .unwrap();
            assert!(expanded.postprocess_execution_complete());
            // Complete replay expansion supplies this oracle's count. The
            // filtered Core's raw build count remains intentionally inexact,
            // so it cannot enter the scalar-authorized eager product route.
            let canonical_ids = expanded
                .postprocess_executions()
                .iter()
                .map(|execution| execution.candidate_id())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .enumerate()
                .map(|(index, producer)| (producer, index as u64 + 1))
                .collect::<BTreeMap<_, _>>();
            let pattern_count = problem
                .piece_source()
                .materialized_universe()
                .unwrap()
                .pattern_count();
            let projection = PcPathProjectionContext::from_problem(&problem);
            let expected_witnesses = expanded
                .postprocess_executions()
                .iter()
                .map(|execution| {
                    project_canonical_execution_with_context(
                        projection,
                        execution,
                        pattern_count,
                        canonical_ids[&execution.candidate_id()],
                    )
                    .unwrap()
                })
                .collect::<Vec<_>>();
            let expected = expected_witnesses
                .iter()
                .map(pc_path_witness_payload)
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "{pattern} {spin:?}");
            let lazy = validate_pc_path_family_v2_result(
                PcPathQueryBinding::Scenario(&query),
                PcPathIngressOrigin::CanonicalPcPath,
                &deferred,
                Some(new_source),
            )
            .unwrap();
            assert_eq!(lazy.witness_count(), expected_witnesses.len() as u128);
            assert_eq!(lazy.witnesses().first(), expected_witnesses.first());
            assert!(lazy.completeness().complete());
            if pattern == "OO" {
                assert!(
                    lazy.witness_count() > 1,
                    "repeated pieces must retain distinct operation orders"
                );
            } else if pattern == "II" {
                assert_eq!(
                    lazy.witness_count(),
                    0,
                    "a plain non-PC first clear breaks B2B"
                );
            } else if pattern == "[OI]O" {
                assert_eq!(lazy.materialized_pattern_count(), 2);
            }
        }
    }
}

#[test]
fn b2b_path_deferred_direct_finalizer_preserves_page_family_and_releases_producers() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    for pattern in ["OO", "II"] {
        let (query, problem, raw) =
            source(2, 0xfc3f0, pattern, 2, false, SpinProfileSelection::TSpins);
        let control = ExecutionControl::default();
        let eager_input = service()
            .materialize_terminal_replay_partition(raw, &control)
            .unwrap();
        let eager = service()
            .postprocess_search_result(eager_input, &control)
            .unwrap();
        let expected = page_source(&problem, &eager);
        let request = AppRequest::new(AppCommand::Scenario(
            ScenarioAppCommand::new(query.as_ref().clone()).with_result_projection(
                PcResultProjection::PathFamilyV2(PcPathIngressOrigin::CanonicalPcPath),
            ),
        ))
        .with_product_capability_contract(ProductCapabilityContract::PcPath)
        .unwrap();
        let response =
            AppContext::new(AppServices::default().with_core_executor(service())).run(request);
        assert_eq!(
            response.status(),
            AppStatus::Success,
            "{:?}",
            response.error()
        );
        let family = response
            .product_capability_result()
            .unwrap()
            .pc_path_family_v2()
            .unwrap();
        let actual = family
            .page_source()
            .expect("direct B2B Path uses the checked lazy source");
        assert_eq!(actual.identity_sha256(), expected.identity_sha256());
        assert_eq!(family.witness_count(), expected.witness_count());
        assert_eq!(actual.first_members(), expected.first_members());
        assert_eq!(all_pages(actual), all_pages(&expected));
        let core = response.render_model().unwrap().core_result().unwrap();
        assert!(core.postprocess_executions().is_empty());
        assert!(core.exact_scoring_execution_batches().is_empty());
        assert_eq!(core.field("build_variant_count"), Some("0"));
        assert_eq!(core.bool_field("build_variant_count_exact"), Some(false));
    }
}

#[test]
fn b2b_path_exact_manifest_count_requires_complete_bound_authority() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let (query, problem, raw) = source(1, 0x3f0, "I", 1, false, SpinProfileSelection::TSpins);
    let control = ExecutionControl::default();
    let filtered = service()
        .postprocess_search_result(raw.clone(), &control)
        .unwrap();
    assert_eq!(filtered.field("build_variant_count"), Some("0"));
    assert_eq!(
        filtered.bool_field("build_variant_count_exact"),
        Some(false)
    );
    let source = page_source(&problem, &filtered);
    let validate = |result: &CoreExecutionResult, source: Option<Arc<PcReplayPageSource>>| {
        validate_pc_path_family_v2_result(
            PcPathQueryBinding::Scenario(&query),
            PcPathIngressOrigin::CanonicalPcPath,
            result,
            source,
        )
    };
    let family = validate(&filtered, Some(Arc::clone(&source))).unwrap();
    assert!(family.completeness().complete());
    assert!(family.witness_count() > 0);
    assert_eq!(family.witness_count(), source.witness_count());
    assert_eq!(filtered.field("build_variant_count"), Some("0"));
    assert_eq!(
        filtered.bool_field("build_variant_count_exact"),
        Some(false)
    );

    let eager = service()
        .materialize_terminal_replay_partition(filtered.clone(), &control)
        .unwrap();
    assert!(eager.postprocess_execution_complete());
    assert_eq!(
        validate(&eager, None),
        Err("pc path build-variant count is inexact")
    );
    for (field, value, expected) in [
        (
            "count_complete",
            "false",
            "pc path count evidence is incomplete",
        ),
        (
            "objective_complete",
            "false",
            "pc path objective evidence is incomplete",
        ),
        (
            "objective_search_complete",
            "false",
            "pc path objective evidence is incomplete",
        ),
        (
            "resource_truncated",
            "true",
            "pc path execution was resource truncated",
        ),
        (
            "materialized_pattern_count",
            "2",
            "pc path materialized pattern count mismatch",
        ),
    ] {
        let invalid = filtered
            .clone()
            .with_replaced_fields(vec![(field.to_owned(), value.to_owned())]);
        assert_eq!(
            validate(&invalid, Some(Arc::clone(&source))),
            Err(expected),
            "{field} cannot be waived by a complete manifest"
        );
    }
    let different_graph = filtered
        .clone()
        .with_exact_scoring_execution_batches(Vec::new());
    assert_eq!(
        validate(&different_graph, Some(source)),
        Err("pc replay page source is not bound to the typed result")
    );

    let pending = service()
        .prepare_pc_replay_page_source(&problem, &filtered, 0)
        .unwrap();
    assert!(pending.complete().is_err());
    let batch = &raw.exact_scoring_execution_batches()[0];
    let incomplete = ExactScoringExecutionBatch::new(
        batch.layout(),
        batch.initial_occupied(),
        batch.patterns().to_vec(),
        batch.initial_cursor(),
        batch.initial_hold(),
        batch.hold_enabled(),
        batch.projects_unplaced_lookahead(),
        batch.projects_standard_bag_lookahead(),
        batch.kick_table_id(),
        batch.rule_profile_id(),
        batch.graphs().to_vec(),
        false,
    );
    let incomplete = raw.with_exact_scoring_execution_batches(vec![incomplete]);
    assert!(service()
        .prepare_pc_replay_page_source(&problem, &incomplete, 0)
        .is_err());
}

#[test]
fn b2b_path_deferred_replays_keep_fail_closed_and_eager_route_boundaries() {
    let _resource_guard = crate::execution_resource_test_support::execution_resource_test_guard();
    let (_, _, raw) = source(1, 0x3f0, "I", 1, false, SpinProfileSelection::TSpins);
    let control = ExecutionControl::default();
    let native = AppCoreExecutorService::native_c_reference()
        .postprocess_search_result(raw.clone(), &control)
        .unwrap();
    assert!(native.postprocess_execution_complete());
    assert!(!native.postprocess_executions().is_empty());
    let save = raw.clone().with_replaced_fields(vec![
        (
            "postprocess_pc_path_requested".to_owned(),
            "false".to_owned(),
        ),
        (
            "postprocess_pc_save_requested".to_owned(),
            "true".to_owned(),
        ),
    ]);
    let save = service().postprocess_search_result(save, &control).unwrap();
    assert!(save.postprocess_execution_complete());
    assert!(!save.postprocess_executions().is_empty());
    let ordinary = raw.clone().with_replaced_fields(vec![(
        "execution_constraint_preserve_b2b".to_owned(),
        "false".to_owned(),
    )]);
    assert_eq!(
        service()
            .postprocess_search_result(ordinary.clone(), &control)
            .unwrap(),
        ordinary
    );

    let missing = raw.clone().with_exact_scoring_execution_batches(Vec::new());
    assert!(matches!(
        service().postprocess_search_result(missing, &control),
        Err(CoreExecutionError::RuntimeUnavailable {
            component: "b2b_preservation_execution_evidence_missing"
        })
    ));
    let batch = &raw.exact_scoring_execution_batches()[0];
    let incomplete = ExactScoringExecutionBatch::new(
        batch.layout(),
        batch.initial_occupied(),
        batch.patterns().to_vec(),
        batch.initial_cursor(),
        batch.initial_hold(),
        batch.hold_enabled(),
        batch.projects_unplaced_lookahead(),
        batch.projects_standard_bag_lookahead(),
        batch.kick_table_id(),
        batch.rule_profile_id(),
        batch.graphs().to_vec(),
        false,
    );
    let incomplete = raw
        .clone()
        .with_exact_scoring_execution_batches(vec![incomplete]);
    assert!(service()
        .postprocess_search_result(incomplete, &control)
        .is_err());
    let cancellation = ExecutionCancellationToken::new();
    cancellation.handle().cancel();
    assert_eq!(
        service().postprocess_search_result(raw.clone(), &ExecutionControl::new(cancellation)),
        Err(CoreExecutionError::Cancelled)
    );
    let oversized = raw.with_replaced_fields(vec![
        ("execution_max_memory_mib".to_owned(), "1".to_owned()),
        ("retained-source".to_owned(), "x".repeat(600 * 1024)),
    ]);
    assert!(matches!(
        service().postprocess_search_result(oversized, &control),
        Err(CoreExecutionError::RuntimeUnavailable {
            component: "complete_replay_whole_live_limit_exceeded"
        })
    ));
}
