// Adapted from https://github.com/Alexhuszagh/rust-lexical.

//! Precalculated large powers for limbs.

#[cfg(all(
    not(target_pointer_width = "64"),
    not(target_arch = "wasm32")
))]
pub(crate) use super::large_powers32::*;

#[cfg(any(target_pointer_width = "64", target_arch = "wasm32"))]
pub(crate) use super::large_powers64::*;
