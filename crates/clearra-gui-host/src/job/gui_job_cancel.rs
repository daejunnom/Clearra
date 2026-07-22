use clearra_core_domain::execution_cancellation::{
    ExecutionCancellationHandle, ExecutionCancellationToken,
};

#[derive(Clone, Debug, Default)]
pub struct GuiJobCancelToken {
    token: ExecutionCancellationToken,
}

impl GuiJobCancelToken {
    pub fn new() -> Self {
        Self::default()
    }
}
impl GuiJobCancelToken {
    pub fn handle(&self) -> GuiJobCancelHandle {
        GuiJobCancelHandle {
            handle: self.token.handle(),
        }
    }
}
impl GuiJobCancelToken {
    pub fn is_cancelled(&self) -> bool {
        self.token.is_cancelled()
    }

    pub(crate) fn execution_token(&self) -> &ExecutionCancellationToken {
        &self.token
    }
}

#[derive(Clone, Debug)]
pub struct GuiJobCancelHandle {
    handle: ExecutionCancellationHandle,
}

impl GuiJobCancelHandle {
    pub fn cancel(&self) {
        self.handle.cancel();
    }
}
impl GuiJobCancelHandle {
    pub fn is_cancelled(&self) -> bool {
        self.handle.is_cancelled()
    }
}
