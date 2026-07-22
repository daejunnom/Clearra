use super::{
    contract_core_context::{ContractCoreContext, CoreMemoryError, CoreScopeHandle, CoreScopeKind},
    release_signal::ReleaseSignal,
};

#[derive(Debug)]
pub struct ContractSearchScope {
    handle: Option<CoreScopeHandle>,
    release_signal: ReleaseSignal,
}

impl ContractSearchScope {
    pub fn create(context: &ContractCoreContext) -> Result<Self, CoreMemoryError> {
        let handle = context.create_scope(CoreScopeKind::Search)?;
        Ok(Self {
            handle: Some(handle),
            release_signal: context.release_signal(),
        })
    }
}
impl ContractSearchScope {
    pub fn release(&mut self) -> Result<(), CoreMemoryError> {
        let handle = self.handle.take().ok_or(CoreMemoryError::DoubleRelease)?;
        handle.release()
    }
}
impl ContractSearchScope {
    pub fn release_signal(&self) -> ReleaseSignal {
        self.release_signal.clone()
    }
}

impl Drop for ContractSearchScope {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            let _ = handle.release();
        }
    }
}
