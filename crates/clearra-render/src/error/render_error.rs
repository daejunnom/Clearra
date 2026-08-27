use crate::{RenderFrameFormat, RenderUnsupportedReason};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RenderError {
    MissingSkinManifest,
    MissingSkinAtlas,
    InvalidSkinManifest {
        reason: String,
    },
    InvalidSkinAtlas {
        reason: String,
    },
    InvalidBoardRows,
    ReplayLayoutMismatch,
    UnknownCell {
        value: char,
    },
    EmptyTimeline,
    ExportLimitExceeded {
        limit: &'static str,
        actual: u64,
        max: u64,
    },
    AllocationFailed {
        allocation: &'static str,
        requested_bytes: u64,
    },
    UnsupportedFrameFormat {
        frame_format: RenderFrameFormat,
        reason: RenderUnsupportedReason,
    },
    PngEncodingFailed {
        reason: String,
    },
    GifEncodingFailed {
        reason: String,
    },
}
