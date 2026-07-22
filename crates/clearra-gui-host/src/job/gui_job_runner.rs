use std::{
    sync::{
        mpsc::{self, Receiver, Sender, TryRecvError},
        Arc,
    },
    thread::{self, JoinHandle},
};

use clearra_app::{AppContext, AppResponse, AppStatus};
use clearra_core_domain::execution_cancellation::{
    ExecutionControl, ExecutionProgress, ProgressSink,
};

use crate::{GuiJob, GuiJobCancelHandle, GuiJobEvent, GuiJobProgress, GuiJobResult};

#[derive(Debug)]
pub struct GuiJobRunner;

impl GuiJobRunner {
    pub fn spawn(job: GuiJob, app_context: AppContext) -> GuiJobHandle {
        let (sender, receiver) = mpsc::channel();
        let job_id = job.job_id();
        let cancel_token = job.cancel_token();
        let cancel_handle = job.cancel_handle();
        let request = job.into_request();

        let join_handle = thread::spawn(move || {
            if cancel_token.is_cancelled() {
                let _ = sender.send(GuiJobEvent::Cancelled { job_id });
                return GuiJobResult::cancelled(job_id);
            }

            let _ = sender.send(GuiJobEvent::Started { job_id });
            let _ = sender.send(GuiJobEvent::Progress {
                job_id,
                progress: GuiJobProgress::new(0, 1, "clearra-app"),
            });
            let progress_sink = Arc::new(GuiProgressSink {
                job_id,
                sender: sender.clone(),
            });
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
            let _ = sender.send(GuiJobEvent::Completed {
                job_id,
                response: response.to_host_response(),
            });
            if response.status() == AppStatus::Success {
                GuiJobResult::completed(job_id, response)
            } else {
                GuiJobResult::failed(job_id, response)
            }
        });

        GuiJobHandle {
            receiver,
            join_handle,
            cancel_handle,
        }
    }
}

struct GuiProgressSink {
    job_id: crate::GuiJobId,
    sender: Sender<GuiJobEvent>,
}

impl ProgressSink for GuiProgressSink {
    fn report(&self, progress: ExecutionProgress) {
        let total = progress.total.unwrap_or(progress.completed.max(1));
        let _ = self.sender.send(GuiJobEvent::Progress {
            job_id: self.job_id,
            progress: GuiJobProgress::new(
                saturating_u32(progress.completed),
                saturating_u32(total),
                progress.stage,
            ),
        });
    }
}

fn saturating_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[derive(Debug)]
pub struct GuiJobHandle {
    receiver: Receiver<GuiJobEvent>,
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
        let mut events = Vec::new();
        while let Ok(event) = self.receiver.try_recv() {
            events.push(event);
        }
        events
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
        let events = receiver.try_iter().collect();
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
