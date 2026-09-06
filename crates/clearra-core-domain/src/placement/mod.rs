pub mod placed_piece;
// Preserve the public `placement::placement` path used by the cross-crate domain API.
#[allow(clippy::module_inception)]
pub mod placement;
pub mod placement_error;
