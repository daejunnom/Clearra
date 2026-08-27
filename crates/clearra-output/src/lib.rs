//! Text, JSON, CSV, explanation, and output dispatch contracts.

pub mod artifact;
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

pub use clearra_ctk3::{
    decode_ctk3_exact, encode_ctk3_compact, Ctk3CodecError, Ctk3Color, Ctk3Document, Ctk3Operation,
    Ctk3Page, Ctk3PageFlags, Ctk3Piece, Ctk3Rotation,
};

#[cfg(feature = "bitmap-render")]
pub use render::{
    ExactBitmapOutput, ExactBitmapOutputFormat, ExactFieldDocumentFormat, FieldDocumentRenderError,
    RenderExactOutputGate, RenderFormat, RenderFormatDispatcher, PUBLIC_BITMAP_ARTIFACT_MAX_BYTES,
};
