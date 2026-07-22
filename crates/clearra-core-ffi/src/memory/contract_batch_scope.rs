use super::{
    contract_core_context::{ContractCoreContext, CoreMemoryError, CoreScopeHandle, CoreScopeKind},
    release_signal::ReleaseSignal,
};

#[derive(Debug)]
pub struct ContractBatchScope {
    handle: Option<CoreScopeHandle>,
    release_signal: ReleaseSignal,
}

impl ContractBatchScope {
    pub fn create(context: &ContractCoreContext) -> Result<Self, CoreMemoryError> {
        let handle = context.create_scope(CoreScopeKind::Batch)?;
        Ok(Self {
            handle: Some(handle),
            release_signal: context.release_signal(),
        })
    }
}
impl ContractBatchScope {
    pub fn release(&mut self) -> Result<(), CoreMemoryError> {
        let handle = self.handle.take().ok_or(CoreMemoryError::DoubleRelease)?;
        handle.release()
    }
}
impl ContractBatchScope {
    pub fn release_signal(&self) -> ReleaseSignal {
        self.release_signal.clone()
    }
}

impl Drop for ContractBatchScope {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.release();
        }
    }
}
