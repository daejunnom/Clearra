//! Independent request, result, and error contracts for future PC probability.
//!
//! These modules do not reuse the current PC chance, coverage, tablebase, or
//! product payload types. Nothing in this namespace performs I/O or probability
//! arithmetic.

pub mod error;
pub mod pc_krylov_snapshot_adapter;
pub mod pc_next_probability_port;
pub mod request;
pub mod result;

#[cfg(test)]
mod contract_tests;
