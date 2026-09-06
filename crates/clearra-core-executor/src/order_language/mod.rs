pub mod build_order_language;
// Explicit-order hold/intersection adapters are parity helpers, not product
// coverage authority. Keep them out of non-test builds instead of suppressing
// dead-code diagnostics for each whole module.
#[cfg(test)]
pub(crate) mod hold_reachable_language;
#[cfg(test)]
pub(crate) mod language_intersection;
pub mod operation_sequence;
pub mod sequence_dependencies;
