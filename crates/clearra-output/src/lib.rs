//! Text, JSON, CSV, explanation, and output dispatch contracts.

pub mod csv;
pub mod explanation;
pub mod fumen_like;
pub mod json;
pub mod model;
#[cfg(feature = "bitmap-render")]
pub mod render;
pub mod scoring;
pub mod spin;
pub mod text;

#[cfg(feature = "bitmap-render")]
pub use render::{RenderFormat, RenderFormatDispatcher};
