use clearra_core_ffi::{BatchScope, CoreContext, CoreMemoryError, SearchScope};

#[derive(Debug)]
pub enum ScopeGuard {
    Search(SearchScope),
    Batch(BatchScope),
}

impl ScopeGuard {
    pub fn search(context: &CoreContext) -> Result<Self, CoreMemoryError> {
        Ok(Self::Search(context.search_scope()?))
    }
}
impl ScopeGuard {
    pub fn batch(context: &CoreContext) -> Result<Self, CoreMemoryError> {
        Ok(Self::Batch(context.batch_scope()?))
    }
}

#[cfg(test)]
#[path = "scope_guard_tests.rs"]
mod tests;
