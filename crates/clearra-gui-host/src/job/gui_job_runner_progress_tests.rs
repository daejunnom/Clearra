use std::sync::Arc;

use clearra_core_domain::execution_cancellation::{ExecutionProgress, ProgressSink};

use crate::{GuiJobEvent, GuiJobId};

use super::{gui_event_channel, GuiEventReceiver, GuiProgressSink};

#[test]
fn desktop_relay_keeps_an_unknown_total_indeterminate() {
    let (sender, receiver) = gui_event_channel();
    let job_id = GuiJobId::new(1);
    let sink = GuiProgressSink::new(job_id, sender);
    sink.report(ExecutionProgress::new("geometry", 1_000, None));
    assert_progress(&receiver, job_id, "geometry", 1_000, 0);
    sink.report(ExecutionProgress::new("buildup", 4, Some(12)));
    assert_progress(&receiver, job_id, "buildup", 4, 12);
}

#[test]
fn desktop_relay_coalesces_undelivered_samples_without_crossing_jobs() {
    let (sender, receiver) = gui_event_channel();
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
    assert_progress(&receiver, second_job_id, "build-geometry", 9, 0);
    assert!(
        receiver.try_recv().is_err(),
        "a stopped consumer retains only the latest progress for each uninterrupted job"
    );
}

#[test]
fn a_stalled_desktop_does_not_retain_a_solver_counter_history() {
    let (sender, receiver) = gui_event_channel();
    let job_id = GuiJobId::new(5);
    let sink = GuiProgressSink::new(job_id, sender);
    for count in 0..100_000 {
        sink.report(ExecutionProgress::new("build-verification", count, None));
    }
    assert_progress(&receiver, job_id, "build-verification", 99_999, 0);
    assert!(receiver.try_recv().is_err());
    // Consuming a slot rearms its notification, without losing the next sample.
    sink.report(ExecutionProgress::new("build-verification", 100_000, None));
    assert_progress(&receiver, job_id, "build-verification", 100_000, 0);
    assert!(receiver.try_recv().is_err());
}

#[test]
fn desktop_progress_never_crosses_control_barriers_or_reopens_a_terminal_job() {
    let job_id = GuiJobId::new(6);
    for terminal in [
        GuiJobEvent::Completed {
            job_id,
            response: clearra_host_contract::AppResponse::new(
                None,
                clearra_host_contract::AppStatus::Success,
            ),
            search_report_json: None,
            product_page_source_owner: None,
        },
        GuiJobEvent::Cancelled { job_id },
        GuiJobEvent::Failed {
            job_id,
            code: "E_TEST_FAILURE".to_owned(),
        },
    ] {
        let (sender, receiver) = gui_event_channel();
        let sink = GuiProgressSink::new(job_id, sender.clone());
        sender.send(GuiJobEvent::Started { job_id }).unwrap();
        sink.report(ExecutionProgress::new("geometry", 1, None));
        sink.report(ExecutionProgress::new("geometry", 2, None));
        let diagnostic = GuiJobEvent::Diagnostic {
            job_id,
            code: "W_TEST_PRESSURE".to_owned(),
            severity: "warning".to_owned(),
        };
        sender.send(diagnostic.clone()).unwrap();
        sink.report(ExecutionProgress::new("buildup", 3, None));
        sink.report(ExecutionProgress::new("buildup", 4, None));
        sender.send(terminal.clone()).unwrap();
        // A concurrent reporter that retained its sink cannot enqueue stale
        // progress after the owner has published completion or cancellation.
        sink.report(ExecutionProgress::new("geometry", 5, None));

        assert_eq!(
            receiver.try_recv().unwrap(),
            GuiJobEvent::Started { job_id }
        );
        assert_progress(&receiver, job_id, "geometry", 2, 0);
        assert_eq!(receiver.try_recv().unwrap(), diagnostic);
        assert_progress(&receiver, job_id, "buildup", 4, 0);
        assert_eq!(receiver.try_recv().unwrap(), terminal);
        assert!(receiver.try_recv().is_err());
    }
}

#[test]
fn desktop_producer_finishes_before_the_ui_drains_progress() {
    let (sender, receiver) = gui_event_channel();
    let job_id = GuiJobId::new(7);
    let producer = std::thread::spawn(move || {
        let sink = Arc::new(GuiProgressSink::new(job_id, sender.clone()));
        std::thread::scope(|scope| {
            for worker in 0..4 {
                let sink = Arc::clone(&sink);
                scope.spawn(move || {
                    for count in 0..2_000 {
                        sink.report(ExecutionProgress::new(
                            "build-verification",
                            worker * 2_000 + count,
                            None,
                        ));
                    }
                });
            }
        });
        sender.send(GuiJobEvent::Cancelled { job_id }).unwrap();
    });
    // A bounded blocking channel would deadlock here when its progress queue
    // fills. This matches GuiJobHandle::join_with_events ordering.
    producer.join().unwrap();
    let events = receiver.drain();
    assert_eq!(events.len(), 2);
    assert!(matches!(&events[0], GuiJobEvent::Progress { .. }));
    assert_eq!(events[1], GuiJobEvent::Cancelled { job_id });
}

fn assert_progress(
    receiver: &GuiEventReceiver,
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
