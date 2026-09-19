//! Synthetic I/O scheduling contracts, not upstream or end-to-end speed proof.
use super::*;
use crate::{
    AppCommand, AppContext, AppCoreExecutorService, AppRenderModel, AppRequest, AppServices,
    AppStatus, CooperativeAppAdvance, Pc4OnlineHostExecution, ScenarioAppCommand,
};
use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_pc_graph::request::{PcExecutionPolicy, PcScenarioBoard, PcScenarioQuery, PieceWindow};

fn start(fixture: &ClearPath) -> Pc4OnlineHostExecution {
    let request = AppRequest::new(AppCommand::Scenario(ScenarioAppCommand::new(
        PcScenarioQuery::new(
            PcScenarioBoard::standard_10(2, fixture.initial_board),
            compact_queue("OOO"),
            PieceWindow::new(3),
        )
        .with_exact_pieces(Some(3))
        .with_allow_hold(false)
        .with_rule(clearra_rules::profile::builtin_rules::srs())
        .with_execution_policy(PcExecutionPolicy::mvp_default().with_workers(1)),
    )));
    let snapshot = activated_snapshot_for_dataset(
        "compact-host-fixture",
        Some(Pc4RuleProfile::Srs),
        Pc4TargetLines::new(2).unwrap(),
        8,
        *fixture.ids_by_step.last().unwrap(),
        &fixture.dataset,
    );
    AppContext::new(AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()))
        .start_online_pc4_execution(request, snapshot)
        .unwrap()
}

fn bytes(fixture: &ClearPath, range: &clearra_pc4_tablebase::RangeRequest) -> Vec<u8> {
    let data = match range.artifact() {
        Pc4ArtifactRole::FieldHashIndex => &fixture.dataset.field_index,
        Pc4ArtifactRole::GraphOffsets => &fixture.dataset.graph_offsets,
        Pc4ArtifactRole::Graph => &fixture.dataset.graph,
    };
    data[range.offset() as usize..range.end_exclusive() as usize].to_vec()
}

fn supply(
    execution: &mut Pc4OnlineHostExecution,
    fixture: &ClearPath,
    range: &clearra_pc4_tablebase::RangeRequest,
    control: &ExecutionControl,
    local: bool,
) {
    let bytes = bytes(fixture, range);
    if local {
        execution
            .admit_local_slice(
                range.lookup_session().get(),
                range.request_id(),
                bytes,
                control,
            )
            .unwrap();
    } else {
        let header = format!(
            "bytes {}-{}/{}",
            range.offset(),
            range.end_exclusive() - 1,
            range.artifact_descriptor().byte_len()
        );
        execution
            .admit_range(
                range.lookup_session().get(),
                range.request_id(),
                206,
                Some(header),
                bytes,
                control,
            )
            .unwrap();
    }
}

#[test]
fn pc4_compact_graph_union_host_out_of_order_and_scalar_local_http_parity() {
    let _resource = crate::execution_resource_test_support::execution_resource_test_guard();
    let fixture = three_o_fixture();
    let control = ExecutionControl::default();
    let mut reference = None;
    for (local, reverse, scalar) in [
        (true, false, true),
        (false, false, false),
        (false, true, false),
        (true, true, false),
    ] {
        let mut execution = start(&fixture);
        let mut peak = 0;
        let mut complete = false;
        for _ in 0..10_000 {
            match execution.advance(8, &control).unwrap() {
                CooperativeAppAdvance::Completed(response) => {
                    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
                    let core = response
                        .render_model()
                        .and_then(AppRenderModel::core_result)
                        .unwrap();
                    let actual = core.normalized_solution_identities().to_vec();
                    assert_eq!(actual.len(), 1, "six histories must be one layout");
                    if let Some(reference) = &reference {
                        assert_eq!(&actual, reference);
                    }
                    reference = Some(actual);
                    complete = true;
                    break;
                }
                CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
                other => panic!("unexpected compact host state: {other:?}"),
            }
            let mut ranges: Vec<_> = execution.pending_ranges().into_iter().cloned().collect();
            peak = peak.max(ranges.len());
            assert!(ranges.len() <= 8);
            // Do not force an I/O wait while there is independent CPU work.
            if !scalar && execution.has_ready_work() {
                continue;
            }
            if scalar {
                ranges = execution.pending_range().into_iter().cloned().collect();
            }
            if reverse {
                ranges.reverse();
            }
            for range in ranges {
                supply(&mut execution, &fixture, &range, &control, local);
            }
        }
        assert!(complete, "bounded host fixture stalled");
        if !scalar {
            assert!(peak >= 2, "independent lookups must overlap");
        }
    }
}

fn reach_parallel(fixture: &ClearPath, control: &ExecutionControl) -> Pc4OnlineHostExecution {
    let mut execution = start(fixture);
    for _ in 0..10_000 {
        assert!(matches!(
            execution.advance(8, control).unwrap(),
            CooperativeAppAdvance::Pending
        ));
        if execution.pending_ranges().len() >= 2 {
            return execution;
        }
        if !execution.has_ready_work() {
            let range = execution.pending_range().unwrap().clone();
            supply(&mut execution, fixture, &range, control, true);
        }
    }
    panic!("fixture never registered independent lookups")
}

#[test]
fn pc4_compact_graph_union_host_rejects_duplicate_and_wrong_response_without_losing_live_demand() {
    let _resource = crate::execution_resource_test_support::execution_resource_test_guard();
    let fixture = three_o_fixture();
    let control = ExecutionControl::default();
    let mut execution = reach_parallel(&fixture, &control);
    let range = (*execution.pending_ranges()[1]).clone();
    let count = execution.pending_ranges().len();
    assert!(execution
        .admit_local_slice(
            range.lookup_session().get(),
            u64::MAX,
            bytes(&fixture, &range),
            &control
        )
        .is_err());
    assert_eq!(execution.pending_ranges().len(), count);
    supply(&mut execution, &fixture, &range, &control, true);
    assert_eq!(execution.pending_ranges().len(), count - 1);
    assert!(execution.has_ready_work());
    assert!(execution
        .admit_local_slice(
            range.lookup_session().get(),
            range.request_id(),
            bytes(&fixture, &range),
            &control
        )
        .is_err());
    assert_eq!(execution.pending_ranges().len(), count - 1);
}

#[test]
fn pc4_compact_graph_union_host_failure_cancellation_and_late_packets_cannot_complete() {
    let _resource = crate::execution_resource_test_support::execution_resource_test_guard();
    let fixture = three_o_fixture();
    for cancelled in [false, true] {
        let control = ExecutionControl::default();
        let mut execution = reach_parallel(&fixture, &control);
        let range = execution.pending_range().unwrap().clone();
        if cancelled {
            control.cancellation.handle().cancel();
            assert!(execution
                .admit_local_slice(
                    range.lookup_session().get(),
                    range.request_id(),
                    bytes(&fixture, &range),
                    &control
                )
                .is_err());
            assert!(matches!(
                execution.advance(64, &control),
                Ok(CooperativeAppAdvance::Cancelled)
            ));
        } else {
            assert!(execution
                .admit_range(
                    range.lookup_session().get(),
                    range.request_id(),
                    200,
                    None,
                    bytes(&fixture, &range),
                    &control
                )
                .is_err());
            assert!(execution.advance(64, &control).is_err());
        }
        assert!(execution.pending_ranges().is_empty());
        assert!(execution
            .admit_local_slice(
                range.lookup_session().get(),
                range.request_id(),
                bytes(&fixture, &range),
                &control
            )
            .is_err());
    }
}
