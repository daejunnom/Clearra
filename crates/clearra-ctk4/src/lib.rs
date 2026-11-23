//! Experimental CTK4 dialect for finite families represented by ordered ZDDs.
//!
//! This is a shared-node archive, not a new CTK3 page revision. Atoms and
//! application metadata are opaque. The codec knows no boards, queues, kicks,
//! movement rules, probabilities, or geometry-product transformations.
//!
//! A writer seals the actual encoded graph with an O(nodes) structural/count
//! scan. An unknown archive must pass that scan before exact paging is exposed.
//! A live, non-deserializable producer seal can instead authorize lazy opening
//! of the identical manifest; every fetched block still verifies its checksum.
//! SHA-256 binds bytes, while the scan establishes the count recurrence.

mod encoder;
mod format;
mod reader;

#[cfg(test)]
mod tests;

use std::fmt;
use std::io;

pub use encoder::{write_archive, ArchiveSource, EncodeOptions, NamedRoot};
pub use reader::{
    ArchiveInfo, ArchiveIoMetrics, DeclaredRoot, ExactPage, ReadAt, SeekReadAt, UnverifiedArchive,
    VerifiedArchive,
};

/// Terminal 0 denotes no sets; terminal 1 denotes exactly the empty set.
pub const EMPTY: u64 = 0;
pub const UNIT: u64 = 1;

/// A nonterminal denotes `low union {set union {variable}: set in high}`.
/// Children have smaller IDs and strictly larger variables. Unlike a BDD,
/// equal low/high children remain meaningful and must not be eliminated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Node {
    pub variable: u32,
    pub low: u64,
    pub high: u64,
    pub count: u128,
}

/// Representation bounds, never elapsed-time deadlines. Zero means unlimited
/// for `max_*` fields. A zero cache budget disables retention of fetched blocks.
/// One transient fetched block and the requested output still need storage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ArchiveLimits {
    pub max_manifest_bytes: usize,
    pub max_block_bytes: usize,
    pub max_atom_bytes: usize,
    pub max_variables: u32,
    pub max_nodes: u64,
    pub max_roots: usize,
    pub max_page_items: usize,
    pub max_page_variables: usize,
    pub max_cached_blocks: usize,
    pub cache_bytes: usize,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_manifest_bytes: 16 * 1024 * 1024,
            max_block_bytes: 4 * 1024 * 1024,
            max_atom_bytes: 1024 * 1024,
            max_variables: 0,
            max_nodes: 0,
            max_roots: 1024,
            max_page_items: 4096,
            max_page_variables: 1024 * 1024,
            max_cached_blocks: 64,
            cache_bytes: 8 * 1024 * 1024,
        }
    }
}

pub trait CancelCheck {
    fn is_cancelled(&self) -> bool;
}

impl<F: Fn() -> bool> CancelCheck for F {
    fn is_cancelled(&self) -> bool {
        self()
    }
}

/// Each public operation gets its own work counter. The writer includes its
/// final validation scan in the same counter. Zero permits unlimited work.
#[derive(Clone, Copy, Default)]
pub struct OperationControl<'a> {
    pub max_work: u64,
    pub cancellation: Option<&'a dyn CancelCheck>,
}

/// An in-process capability created only after actual graph validation.
/// It has no decoder, serde implementation, or public constructor: a digest
/// read from an untrusted file is not sufficient to create this authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProducerSeal {
    manifest_digest: [u8; 32],
    archive_bytes: u64,
}

impl ProducerSeal {
    pub const fn manifest_digest(&self) -> &[u8; 32] {
        &self.manifest_digest
    }

    pub const fn archive_bytes(&self) -> u64 {
        self.archive_bytes
    }
}

#[derive(Debug)]
pub enum ArchiveError {
    Io(io::Error),
    Invalid(&'static str),
    Limit(&'static str),
    Allocation,
    CountOverflow,
    IntegerOverflow,
    ChecksumMismatch,
    SealMismatch,
    Cancelled,
    WorkLimit,
}

impl fmt::Display for ArchiveError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(output, "CTK4 I/O: {error}"),
            Self::Invalid(reason) => write!(output, "invalid CTK4: {reason}"),
            Self::Limit(reason) => write!(output, "CTK4 representation limit: {reason}"),
            Self::Allocation => output.write_str("CTK4 allocation failed"),
            Self::CountOverflow => output.write_str("CTK4 exact count exceeds u128"),
            Self::IntegerOverflow => output.write_str("CTK4 address or size overflow"),
            Self::ChecksumMismatch => output.write_str("CTK4 block or manifest checksum mismatch"),
            Self::SealMismatch => output.write_str("CTK4 producer seal belongs to another archive"),
            Self::Cancelled => output.write_str("CTK4 operation cancelled"),
            Self::WorkLimit => output.write_str("CTK4 work limit exceeded"),
        }
    }
}

impl std::error::Error for ArchiveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for ArchiveError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub(crate) struct Work<'a> {
    control: OperationControl<'a>,
    steps: u64,
}

impl<'a> Work<'a> {
    fn new(control: OperationControl<'a>) -> Result<Self, ArchiveError> {
        let work = Self { control, steps: 0 };
        work.check_cancelled()?;
        Ok(work)
    }

    fn check_cancelled(&self) -> Result<(), ArchiveError> {
        if self
            .control
            .cancellation
            .is_some_and(CancelCheck::is_cancelled)
        {
            Err(ArchiveError::Cancelled)
        } else {
            Ok(())
        }
    }

    fn step(&mut self) -> Result<(), ArchiveError> {
        if self.steps & 255 == 0 {
            self.check_cancelled()?;
        }
        if self.control.max_work != 0 && self.steps >= self.control.max_work {
            return Err(ArchiveError::WorkLimit);
        }
        self.steps = self.steps.checked_add(1).ok_or(ArchiveError::WorkLimit)?;
        Ok(())
    }
}

pub(crate) fn limited(value: u64, maximum: u64, name: &'static str) -> Result<(), ArchiveError> {
    if maximum != 0 && value > maximum {
        Err(ArchiveError::Limit(name))
    } else {
        Ok(())
    }
}

pub(crate) fn zeroed(length: usize) -> Result<Vec<u8>, ArchiveError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| ArchiveError::Allocation)?;
    bytes.resize(length, 0);
    Ok(bytes)
}
