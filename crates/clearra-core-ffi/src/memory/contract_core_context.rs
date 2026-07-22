use std::{cell::RefCell, rc::Rc};

use super::{
    contract_batch_scope::ContractBatchScope, contract_search_scope::ContractSearchScope,
    memory_backend_kind::MemoryBackendKind, release_signal::ReleaseSignal,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreScopeKind {
    Search,
    Batch,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CoreLeakReport {
    pub live_search_scopes: usize,
    pub live_batch_scopes: usize,
}

impl CoreLeakReport {
    pub fn live_scopes(self) -> usize {
        self.live_search_scopes + self.live_batch_scopes
    }
}
impl CoreLeakReport {
    pub fn is_zero(self) -> bool {
        self.live_scopes() == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreMemoryError {
    ContextReleased,
    DoubleRelease,
}

#[derive(Debug)]
pub(crate) struct ContractCoreContextInner {
    released: bool,
    next_scope_id: u64,
    live_search_scopes: usize,
    live_batch_scopes: usize,
    release_signal: ReleaseSignal,
}

impl ContractCoreContextInner {
    fn release_scope(&mut self, kind: CoreScopeKind, _id: u64) -> Result<(), CoreMemoryError> {
        match kind {
            CoreScopeKind::Search => {
                if self.live_search_scopes == 0 {
                    return Err(CoreMemoryError::DoubleRelease);
                }
                self.live_search_scopes -= 1;
                self.release_signal.record_search_scope_release();
            }
            CoreScopeKind::Batch => {
                if self.live_batch_scopes == 0 {
                    return Err(CoreMemoryError::DoubleRelease);
                }
                self.live_batch_scopes -= 1;
                self.release_signal.record_batch_scope_release();
            }
        }
        Ok(())
    }
}

impl Drop for ContractCoreContextInner {
    fn drop(&mut self) {
        if !self.released {
            self.released = true;
            self.release_signal.record_context_release();
        }
    }
}

#[derive(Debug)]
pub(crate) struct CoreScopeHandle {
    context: Rc<RefCell<ContractCoreContextInner>>,
    kind: CoreScopeKind,
    id: u64,
}

impl CoreScopeHandle {
    pub(crate) fn release(self) -> Result<(), CoreMemoryError> {
        self.context.borrow_mut().release_scope(self.kind, self.id)
    }
}

/// Rust-side memory lifetime contract used before the native C memory API is
/// wired into the safe wrapper.
#[derive(Debug)]
pub struct ContractCoreContext {
    inner: Rc<RefCell<ContractCoreContextInner>>,
}

impl ContractCoreContext {
    pub fn backend_kind(&self) -> MemoryBackendKind {
        MemoryBackendKind::Contract
    }
}
impl ContractCoreContext {
    pub fn create() -> Result<Self, CoreMemoryError> {
        Ok(Self {
            inner: Rc::new(RefCell::new(ContractCoreContextInner {
                released: false,
                next_scope_id: 1,
                live_search_scopes: 0,
                live_batch_scopes: 0,
                release_signal: ReleaseSignal::default(),
            })),
        })
    }
}
impl ContractCoreContext {
    pub fn release_signal(&self) -> ReleaseSignal {
        self.inner.borrow().release_signal.clone()
    }
}
impl ContractCoreContext {
    pub fn leak_report(&self) -> CoreLeakReport {
        let inner = self.inner.borrow();
        CoreLeakReport {
            live_search_scopes: inner.live_search_scopes,
            live_batch_scopes: inner.live_batch_scopes,
        }
    }
}
impl ContractCoreContext {
    pub fn search_scope(&self) -> Result<ContractSearchScope, CoreMemoryError> {
        ContractSearchScope::create(self)
    }
}
impl ContractCoreContext {
    pub fn batch_scope(&self) -> Result<ContractBatchScope, CoreMemoryError> {
        ContractBatchScope::create(self)
    }
}
impl ContractCoreContext {
    pub(crate) fn create_scope(
        &self,
        kind: CoreScopeKind,
    ) -> Result<CoreScopeHandle, CoreMemoryError> {
        let mut inner = self.inner.borrow_mut();
        if inner.released {
            return Err(CoreMemoryError::ContextReleased);
        }

        let id = inner.next_scope_id;
        inner.next_scope_id += 1;
        match kind {
            CoreScopeKind::Search => inner.live_search_scopes += 1,
            CoreScopeKind::Batch => inner.live_batch_scopes += 1,
        }

        Ok(CoreScopeHandle {
            context: Rc::clone(&self.inner),
            kind,
            id,
        })
    }
}

#[cfg(test)]
#[path = "contract_core_context_tests.rs"]
mod tests;
