use std::sync::mpsc::{self, Receiver};

use clearra_core_domain::execution_cancellation::{ExecutionProgress, ProgressSink};

use crate::{GuiJobEvent, GuiJobId};

use super::GuiProgressSink;

#[test]
fn desktop_relay_keeps_an_unknown_total_indeterminate() {
    let (sender, receiver) = mpsc::channel();
    let job_id = GuiJobId::new(1);
    let sink = GuiProgressSink::new(job_id, sender);
    sink.report(ExecutionProgress::new("geometry", 1_000, None));
    sink.report(ExecutionProgress::new("buildup", 4, Some(12)));

    assert_progress(&receiver, job_id, "geometry", 1_000, 0);
    assert_progress(&receiver, job_id, "buildup", 4, 12);
}

#[test]
fn desktop_relay_deduplicates_only_matching_solver_samples_within_a_job() {
    let (sender, receiver) = mpsc::channel();
    let job_id = GuiJobId::new(1);
    let second_job_id = GuiJobId::new(2);
    let sink = GuiProgressSink::new(job_id, sender.clone());
    let other_sink = GuiProgressSink::new(second_job_id, sender);
    let geometry = ExecutionProgress::new("build-geometry", 9, None);
    let verification = ExecutionProgress::new("build-verification", 9, None);
    let boundary = ExecutionProgress::new("postprocess", 1, Some(1));

    sink.report(geometry);
    sink.report(geometry);
    sink.report(verification);
    sink.report(geometry);
    sink.report(ExecutionProgress::new("build-geometry", 10, None));
    sink.report(boundary);
    sink.report(boundary);
    sink.report(geometry);
    other_sink.report(geometry);

    assert_progress(&receiver, job_id, "build-geometry", 9, 0);
    assert_progress(&receiver, job_id, "build-verification", 9, 0);
    assert_progress(&receiver, job_id, "build-geometry", 9, 0);
    assert_progress(&receiver, job_id, "build-geometry", 10, 0);
    assert_progress(&receiver, job_id, "postprocess", 1, 1);
    assert_progress(&receiver, job_id, "postprocess", 1, 1);
    assert_progress(&receiver, job_id, "build-geometry", 9, 0);
    assert_progress(&receiver, second_job_id, "build-geometry", 9, 0);
    assert!(
        receiver.try_recv().is_err(),
        "identical consecutive counters should send only one update"
    );
}

fn assert_progress(
    receiver: &Receiver<GuiJobEvent>,
    expected_job_id: GuiJobId,
    expected_stage: &str,
    done: u32,
    total: u32,
) {
    let event = receiver.try_recv().expect("queued Desktop progress sample");
    let GuiJobEvent::Progress { job_id, progress } = event else {
        panic!("expected a Desktop progress event");
    };
    assert_eq!(job_id, expected_job_id);
    assert_eq!(progress.label(), expected_stage);
    assert_eq!(progress.done(), done);
    assert_eq!(progress.total(), total);
}
