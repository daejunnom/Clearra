//! A bounded resumed PC request through the native CLI's public App driver.
#![cfg(all(feature = "parallel", not(target_family = "wasm")))]

use std::{
    env,
    process::Command,
    sync::{Arc, Mutex},
};

use clearra_app::{
    AppCommand, AppContext, AppRenderModel, AppRequest, AppResponse, AppStatus, ContinueAppCommand,
};
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask,
    execution_cancellation::{ComputeBudget, ExecutionControl, ExecutionProgress, ProgressSink},
    piece::piece_kind::PieceKind,
    runtime_cpu_capacity::CpuCapacity,
};
use clearra_pc_graph::request::{
    PcContinuationTokenCodec, PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard,
    PcScenarioQuery, PieceWindow,
};
use clearra_supply::{queue::fixed_sequence::FixedSequence, QueueObservationPolicy};

#[derive(Default)]
struct ComputeEvents {
    budget: Option<Arc<ComputeBudget>>,
    stages: Mutex<Vec<(&'static str, std::thread::ThreadId)>>,
}

impl ProgressSink for ComputeEvents {
    fn report(&self, event: ExecutionProgress) {
        self.stages
            .lock()
            .unwrap()
            .push((event.stage, std::thread::current().id()));
        if matches!(event.stage, "validation" | "postprocess") {
            if let Some(budget) = &self.budget {
                // These serial stages must retain the parent's compute slot.
                // A detached source or a replacement budget would leave every
                // permit free here, even if its search happened on a thread.
                let mut free = Vec::new();
                while let Some(permit) = budget.try_acquire_worker() {
                    free.push(permit);
                }
                assert!(free.len() < budget.parallelism());
            }
        }
    }
}

fn request(visible: bool) -> AppRequest {
    // Seven I pieces fill this small 28-cell residual field. Its height uses
    // the admitted native Continue waves, without a full-board enumeration.
    let mut holes = Board256Mask::EMPTY;
    for row in 0..7_u16 {
        let start = if row < 3 { 0 } else { 6 };
        for col in start..start + 4 {
            holes = holes.union(Board256Mask::singleton(row * 10 + col).unwrap());
        }
    }
    let initial = Board256Mask::all_cells(70).unwrap().without(holes);
    let mut query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10_from_words(7, initial.words()).unwrap(),
        PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I; 7])),
        PieceWindow::new(7),
    )
    .with_exact_pieces(Some(7))
    .with_min_remaining_queue(0)
    .with_allow_hold(false)
    .with_count_policy(PcCountPolicy::CountAll);
    if visible {
        query = query.with_queue_observation_policy(QueueObservationPolicy::VisibleSeven);
    }
    let token = PcContinuationTokenCodec::encode_scenario_replay(&query).unwrap();
    AppRequest::new(AppCommand::Continue(ContinueAppCommand::new(Some(token))))
}

fn field(response: &AppResponse, key: &str) -> String {
    let Some(AppRenderModel::Continue(message)) = response.render_model() else {
        panic!("expected Continue response: {response:?}");
    };
    message
        .fields()
        .iter()
        .find(|field| field.key() == key)
        .unwrap_or_else(|| panic!("missing Continue field {key}"))
        .value()
        .as_text()
}

fn family(response: &AppResponse) -> Vec<String> {
    assert_eq!(response.status(), AppStatus::Success, "{response:?}");
    assert_eq!(field(response, "count_complete"), "true");
    [
        "normalized_solution_set_hash",
        "coverage_pattern_count",
        "covered_pattern_count",
        "build_variant_count",
    ]
    .into_iter()
    .map(|key| field(response, key))
    .collect()
}

#[test]
fn native_product_parallel_continue_keeps_admission_and_completion_in_the_parent_budget() {
    const TEST: &str =
        "native_product_parallel_continue_keeps_admission_and_completion_in_the_parent_budget";
    const CHILD: &str = "CLEARRA_TEST_CONTINUE_COMPUTE_OWNER_CHILD";
    if env::var_os(CHILD).is_none() {
        if CpuCapacity::current().hard_limit() < 3 {
            eprintln!("three logical processors are required for two default Continue workers");
            return;
        }
        let output = Command::new(env::current_exe().unwrap())
            .args(["--exact", TEST, "--nocapture"])
            .env(CHILD, "1")
            .env("CLEARRA_EXPECTED_VCPUS", "3")
            .output()
            .expect("bounded Continue worker-ownership regression");
        assert!(
            output.status.success()
                && String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .any(|line| line == "clearra_continue_compute_owner=complete"),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    assert_eq!(PcExecutionPolicy::mvp_default().workers(), 2);
    let context = AppContext::rust_cpu_product();
    let caller = std::thread::current().id();
    for visible in [false, true] {
        let source = request(visible);
        let expected = family(&context.run(source.clone()));
        // Visible-seven uses the existing synchronous fallback. Both routes
        // must keep the same explicit parent owner, including a one-slot one.
        for bound in [None, Some(1), Some(2)] {
            for prefer_cooperative in [false, true] {
                let control = bound.map_or_else(ExecutionControl::default, |workers| {
                    ExecutionControl::default().with_parallelism(workers)
                });
                let events = Arc::new(ComputeEvents {
                    budget: bound.map(|_| Arc::clone(control.compute_budget())),
                    ..ComputeEvents::default()
                });
                let control = control.with_progress_sink(events.clone());
                let response = if prefer_cooperative {
                    context.run_preferring_native_cooperative_continue(source.clone(), &control)
                } else {
                    context.run_with_execution_control(source.clone(), &control)
                };
                assert_eq!(family(&response), expected);
                if bound.is_none() {
                    assert_eq!(field(&response, "workers_used"), "2");
                }
                let observed = events.stages.lock().unwrap();
                assert!(observed.iter().any(|(stage, _)| *stage == "validation"));
                assert!(observed.iter().all(|(_, thread)| *thread != caller));
                drop(observed);
                if let Some(workers) = bound {
                    let permits = (0..workers)
                        .map(|_| control.compute_budget().try_acquire_worker().unwrap())
                        .collect::<Vec<_>>();
                    assert!(control.compute_budget().try_acquire_worker().is_none());
                    drop(permits);
                }
            }
        }
    }
    println!("clearra_continue_compute_owner=complete");
}
