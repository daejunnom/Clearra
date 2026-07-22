use std::{cell::RefCell, rc::Rc};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReleaseSignalSnapshot {
    pub context_releases: usize,
    pub search_scope_releases: usize,
    pub batch_scope_releases: usize,
}

#[derive(Debug, Default)]
struct ReleaseSignalState {
    snapshot: ReleaseSignalSnapshot,
}

#[derive(Debug, Clone, Default)]
pub struct ReleaseSignal {
    state: Rc<RefCell<ReleaseSignalState>>,
}

impl ReleaseSignal {
    pub fn snapshot(&self) -> ReleaseSignalSnapshot {
        self.state.borrow().snapshot
    }
}
impl ReleaseSignal {
    pub fn context_releases(&self) -> usize {
        self.snapshot().context_releases
    }
}
impl ReleaseSignal {
    pub fn search_scope_releases(&self) -> usize {
        self.snapshot().search_scope_releases
    }
}
impl ReleaseSignal {
    pub fn batch_scope_releases(&self) -> usize {
        self.snapshot().batch_scope_releases
    }
}
impl ReleaseSignal {
    pub(crate) fn record_context_release(&self) {
        self.state.borrow_mut().snapshot.context_releases += 1;
    }
}
impl ReleaseSignal {
    pub(crate) fn record_search_scope_release(&self) {
        self.state.borrow_mut().snapshot.search_scope_releases += 1;
    }
}
impl ReleaseSignal {
    pub(crate) fn record_batch_scope_release(&self) {
        self.state.borrow_mut().snapshot.batch_scope_releases += 1;
    }
}
