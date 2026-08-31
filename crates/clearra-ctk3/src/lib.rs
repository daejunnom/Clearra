//! Native CTK3 codec for Clearra hosts.
//!
//! The browser package in `packages/ctk3` and this crate implement the same
//! language-neutral bitstream. This crate has no JavaScript runtime,
//! subprocess, network, or third-party-codec dependency.

mod big_nat;
mod bitstream;
mod cell;
mod codec;
mod decoder;
mod geometry;
mod transform;
mod transport;

use core::fmt;

pub use codec::{
    encode_ctk3, encode_ctk3_bundle, encode_ctk3_bundle_into, encode_ctk3_compact,
    encode_ctk3_compact_into, encode_ctk3_into, encode_ctk3_segmented_documents_into,
    encode_ctk3_segmented_documents_iter_into,
};
pub use decoder::{
    decode_ctk3, decode_ctk3_exact, decode_ctk3_segment, inspect_ctk3_exact, split_ctk3_segments,
};
pub use geometry::operation_cells;
pub use transform::{TypedCtk3DocumentTransform, TypedCtk3TransformError};

pub const CTK3_PREFIX: &str = "ctk3_";
pub const CTK3_LEGACY_PREFIX: &str = "ctk3@";
pub const CTK3_BUNDLE_PREFIX: &str = "ctk3b_";
pub const CTK3_MAX_SEGMENT_PAGES: usize = 4_096;
pub const CTK3_MAX_BUNDLE_PAGES: usize = 1_048_576;

pub(crate) const MAGIC: u32 = 0xc3;
pub(crate) const LEGACY_SCHEMA_REVISION: u32 = 0;
pub(crate) const COMPACT_SCHEMA_REVISION: u32 = 1;
pub(crate) const TEMPORAL_SCHEMA_REVISION: u32 = 2;
pub(crate) const SHARED_FIELD_SCHEMA_REVISION: u32 = 3;
pub(crate) const MAX_WIDTH: usize = 31;
pub(crate) const MAX_HEIGHT: usize = 31;
pub(crate) const MAX_COMMENT_BYTES: usize = 1 << 20;
pub(crate) const MAX_PAYLOAD_BYTES: usize = 16 << 20;
pub(crate) const MAX_OPERATION_COORDINATE: i32 = 0x3fff_ffff;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Ctk3Piece {
    I,
    O,
    T,
    S,
    Z,
    J,
    L,
}

impl Ctk3Piece {
    pub(crate) const fn wire_index(self) -> u32 {
        match self {
            Self::I => 0,
            Self::O => 1,
            Self::T => 2,
            Self::S => 3,
            Self::Z => 4,
            Self::J => 5,
            Self::L => 6,
        }
    }

    pub(crate) const fn from_wire_index(index: u32) -> Option<Self> {
        match index {
            0 => Some(Self::I),
            1 => Some(Self::O),
            2 => Some(Self::T),
            3 => Some(Self::S),
            4 => Some(Self::Z),
            5 => Some(Self::J),
            6 => Some(Self::L),
            _ => None,
        }
    }

    const fn color_code(self) -> u8 {
        self.wire_index() as u8 + 2
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Ctk3Color {
    Empty,
    Gray,
    Piece(Ctk3Piece),
}

impl Ctk3Color {
    pub(crate) const fn code(self) -> u8 {
        match self {
            Self::Empty => 0,
            Self::Gray => 1,
            Self::Piece(piece) => piece.color_code(),
        }
    }

    pub(crate) const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Empty),
            1 => Some(Self::Gray),
            2 => Some(Self::Piece(Ctk3Piece::I)),
            3 => Some(Self::Piece(Ctk3Piece::O)),
            4 => Some(Self::Piece(Ctk3Piece::T)),
            5 => Some(Self::Piece(Ctk3Piece::S)),
            6 => Some(Self::Piece(Ctk3Piece::Z)),
            7 => Some(Self::Piece(Ctk3Piece::J)),
            8 => Some(Self::Piece(Ctk3Piece::L)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Ctk3Rotation {
    Spawn,
    Right,
    Reverse,
    Left,
}

impl Ctk3Rotation {
    pub(crate) const fn from_quarter_turns(index: u32) -> Option<Self> {
        match index {
            0 => Some(Self::Spawn),
            1 => Some(Self::Right),
            2 => Some(Self::Reverse),
            3 => Some(Self::Left),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Ctk3Operation {
    pub piece: Ctk3Piece,
    pub rotation: Ctk3Rotation,
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Ctk3PageFlags {
    pub lock: bool,
    pub mirror: bool,
    pub colorize: bool,
    pub rise: bool,
    pub quiz: bool,
}

impl Default for Ctk3PageFlags {
    fn default() -> Self {
        Self {
            lock: true,
            mirror: false,
            colorize: true,
            rise: false,
            quiz: false,
        }
    }
}

impl Ctk3PageFlags {
    pub(crate) const fn bits(self) -> u32 {
        self.lock as u32
            | ((self.mirror as u32) << 1)
            | ((self.colorize as u32) << 2)
            | ((self.rise as u32) << 3)
            | ((self.quiz as u32) << 4)
    }

    pub(crate) const fn from_bits(bits: u32) -> Self {
        Self {
            lock: bits & 1 != 0,
            mirror: bits & 2 != 0,
            colorize: bits & 4 != 0,
            rise: bits & 8 != 0,
            quiz: bits & 16 != 0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ctk3Page {
    pub height: usize,
    pub cells: Vec<Ctk3Color>,
    pub comment: String,
    pub operation: Option<Ctk3Operation>,
    pub flags: Ctk3PageFlags,
    pub garbage: Option<Vec<Ctk3Color>>,
}

impl Ctk3Page {
    pub fn new(height: usize, cells: Vec<Ctk3Color>) -> Self {
        Self {
            height,
            cells,
            comment: String::new(),
            operation: None,
            flags: Ctk3PageFlags::default(),
            garbage: None,
        }
    }

    pub fn with_comment(mut self, comment: impl Into<String>) -> Self {
        self.comment = comment.into();
        self
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ctk3Document {
    pub width: usize,
    pub pages: Vec<Ctk3Page>,
}

impl Ctk3Document {
    pub fn new(width: usize, pages: Vec<Ctk3Page>) -> Self {
        Self { width, pages }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ctk3DocumentInfo {
    pub width: usize,
    pub page_count: usize,
    pub segment_count: usize,
    pub segment_page_counts: Vec<usize>,
    pub bundled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Ctk3CodecError {
    InvalidWidth {
        width: usize,
    },
    InvalidPageCount {
        count: usize,
    },
    InvalidHeight {
        page: usize,
        height: usize,
    },
    InvalidCellCount {
        page: usize,
        expected: usize,
        actual: usize,
    },
    CommentTooLong {
        page: usize,
        bytes: usize,
    },
    InvalidGarbageWidth {
        page: usize,
        expected: usize,
        actual: usize,
    },
    InvalidOperationCoordinate {
        page: usize,
    },
    InvalidBundleSegmentCount {
        count: usize,
    },
    BundleSegmentCountMismatch {
        expected: usize,
        actual: usize,
    },
    InvalidBundleSegment {
        index: usize,
    },
    BundleWidthMismatch {
        index: usize,
    },
    BundlePageLimitExceeded,
    PayloadTooLarge {
        bytes: usize,
    },
    InvalidPayload(&'static str),
    InvalidUtf8,
    IntegerOverflow,
}

/// Backwards-compatible name retained for native output callers.
pub type Ctk3EncodeError = Ctk3CodecError;

impl Ctk3CodecError {
    pub(crate) const fn invalid(reason: &'static str) -> Self {
        Self::InvalidPayload(reason)
    }
}

impl fmt::Display for Ctk3CodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidWidth { width } => write!(formatter, "invalid CTK3 width: {width}"),
            Self::InvalidPageCount { count } => {
                write!(formatter, "invalid CTK3 page count: {count}")
            }
            Self::InvalidHeight { page, height } => {
                write!(formatter, "invalid CTK3 height on page {page}: {height}")
            }
            Self::InvalidCellCount {
                page,
                expected,
                actual,
            } => write!(
                formatter,
                "invalid CTK3 cell count on page {page}: expected {expected}, got {actual}"
            ),
            Self::CommentTooLong { page, bytes } => {
                write!(
                    formatter,
                    "CTK3 comment on page {page} is too long: {bytes} bytes"
                )
            }
            Self::InvalidGarbageWidth {
                page,
                expected,
                actual,
            } => write!(
                formatter,
                "invalid CTK3 garbage width on page {page}: expected {expected}, got {actual}"
            ),
            Self::InvalidOperationCoordinate { page } => {
                write!(
                    formatter,
                    "invalid CTK3 operation coordinate on page {page}"
                )
            }
            Self::InvalidBundleSegmentCount { count } => {
                write!(formatter, "invalid CTK3 bundle segment count: {count}")
            }
            Self::BundleSegmentCountMismatch { expected, actual } => write!(
                formatter,
                "CTK3 bundle segment count mismatch: expected {expected}, got {actual}"
            ),
            Self::InvalidBundleSegment { index } => {
                write!(formatter, "invalid CTK3 bundle segment at index {index}")
            }
            Self::BundleWidthMismatch { index } => {
                write!(formatter, "CTK3 bundle width mismatch at index {index}")
            }
            Self::BundlePageLimitExceeded => formatter.write_str("CTK3 bundle page limit exceeded"),
            Self::PayloadTooLarge { bytes } => {
                write!(formatter, "CTK3 payload is too large: {bytes} bytes")
            }
            Self::InvalidPayload(reason) => write!(formatter, "invalid CTK3 payload: {reason}"),
            Self::InvalidUtf8 => formatter.write_str("CTK3 comment is not valid UTF-8"),
            Self::IntegerOverflow => formatter.write_str("CTK3 integer capacity exceeded"),
        }
    }
}

impl std::error::Error for Ctk3CodecError {}

#[derive(Debug)]
pub enum Ctk3WriteError {
    Codec(Ctk3CodecError),
    Io(std::io::Error),
}

impl fmt::Display for Ctk3WriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Codec(error) => error.fmt(formatter),
            Self::Io(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for Ctk3WriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Codec(error) => Some(error),
            Self::Io(error) => Some(error),
        }
    }
}

impl From<Ctk3CodecError> for Ctk3WriteError {
    fn from(error: Ctk3CodecError) -> Self {
        Self::Codec(error)
    }
}

impl From<std::io::Error> for Ctk3WriteError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct NormalizedPage {
    pub(crate) height: usize,
    pub(crate) codes: Vec<u8>,
    pub(crate) comment: String,
    pub(crate) operation: Option<Ctk3Operation>,
    pub(crate) flags: Ctk3PageFlags,
    pub(crate) garbage_codes: Option<Vec<u8>>,
}

pub(crate) fn normalize_document(
    document: &Ctk3Document,
) -> Result<Vec<NormalizedPage>, Ctk3CodecError> {
    if document.width == 0 || document.width > MAX_WIDTH {
        return Err(Ctk3CodecError::InvalidWidth {
            width: document.width,
        });
    }
    if document.pages.is_empty() || document.pages.len() > CTK3_MAX_SEGMENT_PAGES {
        return Err(Ctk3CodecError::InvalidPageCount {
            count: document.pages.len(),
        });
    }
    document
        .pages
        .iter()
        .enumerate()
        .map(|(index, page)| normalize_page(document.width, index, page))
        .collect()
}

fn normalize_page(
    width: usize,
    index: usize,
    page: &Ctk3Page,
) -> Result<NormalizedPage, Ctk3CodecError> {
    if page.height > MAX_HEIGHT {
        return Err(Ctk3CodecError::InvalidHeight {
            page: index,
            height: page.height,
        });
    }
    let expected = page
        .height
        .checked_mul(width)
        .ok_or(Ctk3CodecError::IntegerOverflow)?;
    if page.cells.len() != expected {
        return Err(Ctk3CodecError::InvalidCellCount {
            page: index,
            expected,
            actual: page.cells.len(),
        });
    }
    if page.comment.len() > MAX_COMMENT_BYTES {
        return Err(Ctk3CodecError::CommentTooLong {
            page: index,
            bytes: page.comment.len(),
        });
    }
    let mut height = page.height;
    let mut codes = page
        .cells
        .iter()
        .copied()
        .map(Ctk3Color::code)
        .collect::<Vec<_>>();
    while height > 0
        && codes[(height - 1) * width..height * width]
            .iter()
            .all(|code| *code == 0)
    {
        height -= 1;
    }
    codes.truncate(height * width);
    let operation = match page.operation {
        Some(operation) => {
            if operation.x.unsigned_abs() > MAX_OPERATION_COORDINATE as u32
                || operation.y.unsigned_abs() > MAX_OPERATION_COORDINATE as u32
            {
                return Err(Ctk3CodecError::InvalidOperationCoordinate { page: index });
            }
            let canonical = geometry::canonicalize_operation(operation)
                .ok_or(Ctk3CodecError::InvalidOperationCoordinate { page: index })?;
            if canonical.x.unsigned_abs() > MAX_OPERATION_COORDINATE as u32
                || canonical.y.unsigned_abs() > MAX_OPERATION_COORDINATE as u32
            {
                return Err(Ctk3CodecError::InvalidOperationCoordinate { page: index });
            }
            Some(canonical)
        }
        None => None,
    };
    let garbage_codes = match &page.garbage {
        Some(garbage) if garbage.len() != width => {
            return Err(Ctk3CodecError::InvalidGarbageWidth {
                page: index,
                expected: width,
                actual: garbage.len(),
            });
        }
        Some(garbage) => {
            let codes = garbage
                .iter()
                .copied()
                .map(Ctk3Color::code)
                .collect::<Vec<_>>();
            codes.iter().any(|code| *code != 0).then_some(codes)
        }
        None => None,
    };
    let mut flags = page.flags;
    if page.comment.starts_with("#Q=") {
        flags.quiz = true;
    }
    Ok(NormalizedPage {
        height,
        codes,
        comment: page.comment.clone(),
        operation,
        flags,
        garbage_codes,
    })
}

pub(crate) fn normalized_to_page(page: &NormalizedPage) -> Result<Ctk3Page, Ctk3CodecError> {
    Ok(Ctk3Page {
        height: page.height,
        cells: page
            .codes
            .iter()
            .map(|code| {
                Ctk3Color::from_code(*code)
                    .ok_or_else(|| Ctk3CodecError::invalid("field color is invalid"))
            })
            .collect::<Result<_, _>>()?,
        comment: page.comment.clone(),
        operation: page.operation,
        flags: page.flags,
        garbage: page
            .garbage_codes
            .as_ref()
            .map(|codes| {
                codes
                    .iter()
                    .map(|code| {
                        Ctk3Color::from_code(*code)
                            .ok_or_else(|| Ctk3CodecError::invalid("garbage color is invalid"))
                    })
                    .collect()
            })
            .transpose()?,
    })
}
