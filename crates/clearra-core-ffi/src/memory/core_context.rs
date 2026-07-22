pub use super::contract_core_context::{
    ContractCoreContext, CoreLeakReport, CoreMemoryError, CoreScopeKind,
};

/// Compatibility facade for the current Rust-side memory lifetime contract.
///
/// New memory work should prefer `ContractCoreContext` until the native C
/// memory API is connected through `native_core_context`.
pub type CoreContext = ContractCoreContext;
