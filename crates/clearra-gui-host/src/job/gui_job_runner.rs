use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::{GuiJob, GuiJobCancelHandle, GuiJobEvent, GuiJobId, GuiJobProgress, GuiJobResult};
use clearra_app::{AppContext, AppResponse, AppStatus};
use clearra_core_domain::execution_cancellation::{
    ExecutionControl, ExecutionProgress, ProgressSink,
};

const GUI_JOB_STACK_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug)]
pub struct GuiJobRunner;

impl GuiJobRunner {
    pub fn spawn(job: GuiJob, app_context: AppContext) -> GuiJobHandle {
        let (sender, receiver) = gui_event_channel();
        let job_id = job.job_id();
        let cancel_token = job.cancel_token();
        let cancel_handle = job.cancel_handle();
        let request = job.into_request();

        let join_handle = thread::Builder::new()
            .name(format!("clearra-gui-job-{}", job_id.get()))
            .stack_size(GUI_JOB_STACK_BYTES)
            .spawn(move || {
                if cancel_token.is_cancelled() {
                    let _ = sender.send(GuiJobEvent::Cancelled { job_id });
                    return GuiJobResult::cancelled(job_id);
                }

                let _ = sender.send(GuiJobEvent::Started { job_id });
                let _ = sender.send(GuiJobEvent::Progress {
                    job_id,
                    progress: GuiJobProgress::new(0, 1, "clearra-app"),
                });
                let progress_sink = Arc::new(GuiProgressSink::new(job_id, sender.clone()));
                let control = ExecutionControl::new(cancel_token.execution_token().clone())
                    .with_progress_sink(progress_sink);
                let response = app_context.run_with_execution_control(request, &control);
                if cancel_token.is_cancelled() {
                    let _ = sender.send(GuiJobEvent::Cancelled { job_id });
                    return GuiJobResult::cancelled(job_id);
                }

                emit_diagnostics(job_id, &response, |event| {
                    let _ = sender.send(event);
                });

                let _ = sender.send(GuiJobEvent::Progress {
                    job_id,
                    progress: GuiJobProgress::new(1, 1, "clearra-app"),
                });
                #[cfg(feature = "wasm-cpu-runtime")]
                let search_report_json =
                    clearra_wasm::serialize_search_report_from_app_response(&response);
                #[cfg(not(feature = "wasm-cpu-runtime"))]
                let search_report_json = None;
                let product_page_source_owner = response.public_page_source_owner();
                let _ = sender.send(GuiJobEvent::Completed {
                    job_id,
                    // Keep completion independent of CTK3/Fumen encoding. The GUI obtains
                    // normalized members from its bounded page source only when requested.
                    response: response.to_host_response(),
                    search_report_json,
                    product_page_source_owner,
                });
                if response.status() == AppStatus::Success {
                    GuiJobResult::completed(job_id, response)
                } else {
                    GuiJobResult::failed(job_id, response)
                }
            })
            .expect("spawn Clearra GUI job runner thread");

        GuiJobHandle {
            receiver,
            join_handle,
            cancel_handle,
        }
    }
}

// Progress is a sampled display value. Keeping every sample in an unbounded
// channel lets a slow or suspended UI retain an arbitrarily large history.
// One shared slot owns the latest undelivered sample between control events.
// Started/diagnostic/terminal events seal that slot and retain their ordering.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum QueuedGuiEvent {
    Direct(GuiJobEvent),
    Progress {
        job_id: GuiJobId,
        value: Arc<Mutex<Option<GuiJobProgress>>>,
    },
}

#[derive(Debug)]
struct PendingGuiProgress {
    job_id: GuiJobId,
    value: Arc<Mutex<Option<GuiJobProgress>>>,
}

#[derive(Debug, Default)]
struct GuiEventSendState {
    pending_progress: Option<PendingGuiProgress>,
    terminal_sent: bool,
}

#[derive(Clone, Debug)]
struct GuiEventSender {
    sender: Sender<QueuedGuiEvent>,
    state: Arc<Mutex<GuiEventSendState>>,
}

#[derive(Debug)]
struct GuiEventReceiver {
    receiver: Receiver<QueuedGuiEvent>,
}

fn gui_event_channel() -> (GuiEventSender, GuiEventReceiver) {
    let (sender, receiver) = mpsc::channel();
    (
        GuiEventSender {
            sender,
            state: Arc::new(Mutex::new(GuiEventSendState::default())),
        },
        GuiEventReceiver { receiver },
    )
}

impl GuiEventSender {
    fn send(&self, event: GuiJobEvent) -> Result<(), ()> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.terminal_sent {
            // A late worker sample must not reopen a completed/cancelled job.
            return Err(());
        }
        if let GuiJobEvent::Progress { job_id, progress } = event {
            if let Some(pending) = state
                .pending_progress
                .as_ref()
                .filter(|pending| pending.job_id == job_id)
            {
                let mut latest = pending
                    .value
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                let needs_notification = latest.is_none();
                *latest = Some(progress);
                if needs_notification {
                    self.sender
                        .send(QueuedGuiEvent::Progress {
                            job_id,
                            value: Arc::clone(&pending.value),
                        })
                        .map_err(|_| ())?;
                }
                return Ok(());
            }
            let value = Arc::new(Mutex::new(Some(progress)));
            self.sender
                .send(QueuedGuiEvent::Progress {
                    job_id,
                    value: Arc::clone(&value),
                })
                .map_err(|_| ())?;
            state.pending_progress = Some(PendingGuiProgress { job_id, value });
        } else {
            // No progress slot remains writable across this ordering barrier.
            // This queue never blocks completion on the UI consuming samples,
            // so join_with_events can still join before draining the channel.
            state.pending_progress = None;
            state.terminal_sent = event.is_terminal();
            self.sender
                .send(QueuedGuiEvent::Direct(event))
                .map_err(|_| ())?;
        }
        Ok(())
    }
}

impl GuiEventReceiver {
    fn try_recv(&self) -> Result<GuiJobEvent, TryRecvError> {
        loop {
            match self.receiver.try_recv()? {
                QueuedGuiEvent::Direct(event) => return Ok(event),
                QueuedGuiEvent::Progress { job_id, value } => {
                    let progress = value
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .take();
                    if let Some(progress) = progress {
                        return Ok(GuiJobEvent::Progress { job_id, progress });
                    }
                }
            }
        }
    }

    fn drain(&self) -> Vec<GuiJobEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.try_recv() {
            events.push(event);
        }
        events
    }
}

struct GuiProgressSink {
    job_id: GuiJobId,
    sender: GuiEventSender,
    // This hint neither orders concurrent samples nor signals a phase boundary.
    // Racing reporters may emit a duplicate rather than blocking a search worker.
    previous_counter: AtomicU64,
}

impl GuiProgressSink {
    fn new(job_id: GuiJobId, sender: GuiEventSender) -> Self {
        Self {
            job_id,
            sender,
            previous_counter: AtomicU64::new(0),
        }
    }
}

impl ProgressSink for GuiProgressSink {
    fn report(&self, progress: ExecutionProgress) {
        let done = saturating_u32(progress.completed);
        let previous = self.previous_counter.load(Ordering::Relaxed);
        let counter = if progress.total.is_none() {
            SolverCounter::from_stage(progress.stage).map(|counter| counter.key(done))
        } else {
            None
        };
        if let Some(counter) = counter {
            if counter == previous {
                return;
            }
            self.previous_counter.store(counter, Ordering::Relaxed);
        } else if previous != 0 {
            // A different kind of event starts a new sequence of samples.
            self.previous_counter.store(0, Ordering::Relaxed);
        }
        let _ = self.sender.send(GuiJobEvent::Progress {
            job_id: self.job_id,
            progress: GuiJobProgress::new(
                done,
                // Keep an indeterminate total in the numeric Desktop wire schema.
                progress.total.map_or(0, saturating_u32),
                progress.stage,
            ),
        });
    }
}

#[repr(u8)]
enum SolverCounter {
    Geometry = 1,
    BuildUp,
    BuildGeometry,
    BuildCandidates,
    BuildVerification,
    BuildProbabilityCandidates,
    WasmExactCover,
}

impl SolverCounter {
    fn from_stage(stage: &str) -> Option<Self> {
        match stage {
            "geometry" => Some(Self::Geometry),
            "buildup" => Some(Self::BuildUp),
            "build-geometry" => Some(Self::BuildGeometry),
            "build-candidates" => Some(Self::BuildCandidates),
            "build-verification" => Some(Self::BuildVerification),
            "build-probability-candidates" => Some(Self::BuildProbabilityCandidates),
            "wasm-exact-cover" => Some(Self::WasmExactCover),
            _ => None,
        }
    }

    fn key(self, done: u32) -> u64 {
        (u64::from(self as u8) << u32::BITS) | u64::from(done)
    }
}

fn saturating_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[derive(Debug)]
pub struct GuiJobHandle {
    receiver: GuiEventReceiver,
    join_handle: JoinHandle<GuiJobResult>,
    cancel_handle: GuiJobCancelHandle,
}

impl GuiJobHandle {
    pub fn cancel(&self) {
        self.cancel_handle.cancel();
    }
}
impl GuiJobHandle {
    pub fn try_recv_event(&self) -> Result<Option<GuiJobEvent>, TryRecvError> {
        match self.receiver.try_recv() {
            Ok(event) => Ok(Some(event)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(error @ TryRecvError::Disconnected) => Err(error),
        }
    }
}
impl GuiJobHandle {
    pub fn drain_events(&self) -> Vec<GuiJobEvent> {
        self.receiver.drain()
    }
}
impl GuiJobHandle {
    pub fn join(self) -> thread::Result<GuiJobResult> {
        self.join_handle.join()
    }
}
impl GuiJobHandle {
    pub fn join_with_events(self) -> thread::Result<(GuiJobResult, Vec<GuiJobEvent>)> {
        let GuiJobHandle {
            receiver,
            join_handle,
            cancel_handle: _,
        } = self;
        let result = join_handle.join()?;
        let events = receiver.drain();
        Ok((result, events))
    }
}
impl GuiJobHandle {
    pub fn is_finished(&self) -> bool {
        self.join_handle.is_finished()
    }
}
impl GuiJobHandle {
    pub fn cancel_handle(&self) -> &GuiJobCancelHandle {
        &self.cancel_handle
    }
}

fn emit_diagnostics(
    job_id: crate::GuiJobId,
    response: &AppResponse,
    mut emit: impl FnMut(GuiJobEvent),
) {
    for diagnostic in response.diagnostics().validation().diagnostics().iter() {
        emit(GuiJobEvent::Diagnostic {
            job_id,
            code: diagnostic.code().as_str().to_owned(),
            severity: format!("{:?}", diagnostic.severity()).to_ascii_lowercase(),
        });
    }
}

#[cfg(test)]
#[path = "gui_job_runner_progress_tests.rs"]
mod progress_tests;
