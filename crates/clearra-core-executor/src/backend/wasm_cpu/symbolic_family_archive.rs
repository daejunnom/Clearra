//! Experimental CTK4 view of the already-computed symbolic result relation.
//!
//! This adapter serializes shared ZDD nodes, never geometry products or all
//! identities. The generic codec validates finite atom-set semantics. Board,
//! source and placement descriptors remain an application schema: a foreign
//! archive is not a certificate of gameplay legality or query completeness.

use std::fmt;
use std::io::{Read, Seek, Write};

use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_domain::solution::normalized_tiling_solution::{
    PiecePlacementMask, StandardBoard64TilingIdentity,
};
use clearra_ctk4::{
    write_archive, ArchiveError, ArchiveInfo, ArchiveIoMetrics, ArchiveLimits, ArchiveSource,
    EncodeOptions, NamedRoot, Node, OperationControl, ProducerSeal, ReadAt, VerifiedArchive,
};

use super::{SymbolicFamilyError, SymbolicFamilyResult};

const SCOPE_MAGIC: &[u8; 8] = b"CLRSF001";
const SCOPE_BYTES: usize = 40;
const ROW_ATOM: u8 = 0;
const SOURCE_ATOM: u8 = 1;
const ROOT_NAMES: [&str; 3] = ["accepted", "identities", "covered_sources"];
const PIECE_CODES: &[u8; 7] = b"IOTSZJL";

/// The coordinate frame and variable dictionary schema, not original-request
/// authority. Source length includes any suffix hidden from execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SymbolicFamilyArchiveScope {
    pub width: u8,
    pub height: u8,
    pub initial_board: u64,
    pub required_cells: u64,
    pub row_count: u32,
    pub source_length: u32,
}

#[derive(Debug)]
pub enum SymbolicFamilyArchiveError {
    Archive(ArchiveError),
    Family(SymbolicFamilyError),
}

impl fmt::Display for SymbolicFamilyArchiveError {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Archive(error) => error.fmt(output),
            Self::Family(error) => error.fmt(output),
        }
    }
}

impl std::error::Error for SymbolicFamilyArchiveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Archive(error) => Some(error),
            Self::Family(error) => Some(error),
        }
    }
}

impl From<ArchiveError> for SymbolicFamilyArchiveError {
    fn from(error: ArchiveError) -> Self {
        Self::Archive(error)
    }
}

impl From<SymbolicFamilyError> for SymbolicFamilyArchiveError {
    fn from(error: SymbolicFamilyError) -> Self {
        Self::Family(error)
    }
}

impl SymbolicFamilyResult {
    /// Save the three roots in one shared-node archive. Compaction changes
    /// internal addresses but preserves their sets. Dense temporary counts use
    /// 16 bytes per retained node, then are dropped when this call returns.
    ///
    /// `annotation` may carry a request fingerprint for diagnostics. It is
    /// explicitly unverified context; attaching it certifies no foreign query.
    /// The caller owns the empty temporary sink and publication after success.
    /// Arena preparation uses its existing work/cancellation control; `control`
    /// governs the codec's writing and serialized validation pass.
    pub fn write_ctk4<W: Read + Write + Seek>(
        &mut self,
        destination: &mut W,
        annotation: &[u8],
        options: EncodeOptions,
        limits: ArchiveLimits,
        control: OperationControl<'_>,
    ) -> Result<ProducerSeal, SymbolicFamilyArchiveError> {
        if control
            .cancellation
            .is_some_and(|cancellation| cancellation.is_cancelled())
        {
            return Err(ArchiveError::Cancelled.into());
        }
        let scope = SymbolicFamilyArchiveScope {
            width: self.catalog.width(),
            height: self.catalog.height(),
            initial_board: self.catalog.initial_board(),
            required_cells: self.catalog.required_cells(),
            row_count: u32::try_from(self.catalog.skeleton_count())
                .map_err(|_| ArchiveError::IntegerOverflow)?,
            source_length: u32::try_from(self.source_length)
                .map_err(|_| ArchiveError::IntegerOverflow)?,
        };
        let variable_count = validate_scope(scope)?;
        if limits.max_variables != 0 && variable_count > limits.max_variables {
            return Err(ArchiveError::Limit("variables").into());
        }
        let metadata_bytes = SCOPE_BYTES
            .checked_add(annotation.len())
            .ok_or(ArchiveError::IntegerOverflow)?;
        if limits.max_manifest_bytes != 0 && metadata_bytes > limits.max_manifest_bytes {
            return Err(ArchiveError::Limit("manifest bytes").into());
        }
        let metadata = encode_scope(scope, annotation)?;
        self.compact()?;
        if limits.max_nodes != 0 && self.sets.node_count() as u64 > limits.max_nodes {
            return Err(ArchiveError::Limit("nodes").into());
        }
        let counts = self
            .sets
            .prepare_archive_counts()
            .map_err(SymbolicFamilyError::from)?;
        let root_ids = [self.accepted, self.identities, self.covered_sources];
        let roots: [NamedRoot; 3] = std::array::from_fn(|index| NamedRoot {
            name: ROOT_NAMES[index].to_owned(),
            node: root_ids[index].archive_id(),
        });
        let mut source = ResultSource {
            result: self,
            counts: &counts,
            variable_count,
            row_count: scope.row_count,
        };
        Ok(write_archive(
            &mut source,
            &roots,
            &metadata,
            destination,
            options,
            limits,
            control,
        )?)
    }
}

struct ResultSource<'a> {
    result: &'a SymbolicFamilyResult,
    counts: &'a [u128],
    variable_count: u32,
    row_count: u32,
}

impl ArchiveSource for ResultSource<'_> {
    fn variable_count(&self) -> u32 {
        self.variable_count
    }

    fn node_count(&self) -> u64 {
        self.result.sets.node_count() as u64
    }

    fn copy_atom(&mut self, variable: u32, output: &mut Vec<u8>) -> Result<(), ArchiveError> {
        if variable >= self.variable_count {
            return Err(ArchiveError::Invalid("symbolic atom variable"));
        }
        output
            .try_reserve(10)
            .map_err(|_| ArchiveError::Allocation)?;
        if variable < self.row_count {
            let row = self.result.catalog.skeleton(variable);
            output.extend_from_slice(&[ROW_ATOM, row.piece.as_ascii() as u8]);
            output.extend_from_slice(&row.cells.to_le_bytes());
        } else {
            let relative = variable - self.row_count;
            output.push(SOURCE_ATOM);
            output.extend_from_slice(&(relative / 7).to_le_bytes());
            output.push(PIECE_CODES[(relative % 7) as usize]);
        }
        Ok(())
    }

    fn node(&mut self, id: u64) -> Result<Node, ArchiveError> {
        let node = self
            .result
            .sets
            .archive_node(id)
            .map_err(|_| ArchiveError::Invalid("symbolic archive node"))?;
        let index = usize::try_from(id).map_err(|_| ArchiveError::IntegerOverflow)?;
        let count = *self
            .counts
            .get(index)
            .ok_or(ArchiveError::Invalid("symbolic archive count"))?;
        Ok(Node {
            variable: node.variable,
            low: node.low.archive_id(),
            high: node.high.archive_id(),
            count,
        })
    }
}

/// A lazy application view over a structurally verified CTK4 archive.
///
/// Opening checks only the manifest's schema. Requested atoms and identities
/// are checked as they are loaded. Counts describe archived atom-ID sets;
/// this view does not claim that a foreign dictionary is injective or that its
/// three roots are the exact projections of a complete solver result. Archives
/// emitted by `SymbolicFamilyResult::write_ctk4` preserve those producer facts.
pub struct SymbolicFamilyArchive<R> {
    archive: VerifiedArchive<R>,
    scope: SymbolicFamilyArchiveScope,
    roots: [usize; 3],
}

impl<R: ReadAt> SymbolicFamilyArchive<R> {
    pub fn from_verified(archive: VerifiedArchive<R>) -> Result<Self, ArchiveError> {
        let info = archive.info();
        let scope = decode_scope(info.metadata)?;
        if validate_scope(scope)? != info.variable_count || info.roots.len() != ROOT_NAMES.len() {
            return Err(ArchiveError::Invalid(
                "symbolic archive variable or root count",
            ));
        }
        let mut roots = [0; 3];
        for (index, name) in ROOT_NAMES.iter().enumerate() {
            roots[index] = info
                .roots
                .iter()
                .position(|root| root.name == *name)
                .ok_or(ArchiveError::Invalid("symbolic archive root name"))?;
        }
        Ok(Self {
            archive,
            scope,
            roots,
        })
    }

    pub const fn scope(&self) -> SymbolicFamilyArchiveScope {
        self.scope
    }

    /// Diagnostic bytes, not certified original-query or rule authority.
    pub fn unverified_annotation(&self) -> &[u8] {
        &self.archive.info().metadata[SCOPE_BYTES..]
    }

    pub fn info(&self) -> ArchiveInfo<'_> {
        self.archive.info()
    }

    pub fn metrics(&self) -> ArchiveIoMetrics {
        self.archive.metrics()
    }

    pub fn identity_row_set_count(&self) -> Result<u128, ArchiveError> {
        self.archive.count(self.roots[1])
    }

    pub fn covered_source_set_count(&self) -> Result<u128, ArchiveError> {
        self.archive.count(self.roots[2])
    }

    pub fn joint_set_count(&self) -> Result<u128, ArchiveError> {
        self.archive.count(self.roots[0])
    }

    /// Decode only a bounded requested page. Its order matches the producer's
    /// ZDD page order. A caller may pass these identities to an existing CTK3
    /// encoder for a requested display projection; the archive is not expanded.
    /// `control` applies independently to paging and each atom lookup.
    pub fn identity_page(
        &mut self,
        offset: u128,
        limit: usize,
        control: OperationControl<'_>,
    ) -> Result<Vec<StandardBoard64TilingIdentity>, ArchiveError> {
        let page = self.archive.page(self.roots[1], offset, limit, control)?;
        let mut output = Vec::new();
        output
            .try_reserve_exact(page.sets.len())
            .map_err(|_| ArchiveError::Allocation)?;
        for variables in page.sets {
            let expected = self.scope.required_cells.count_ones() as usize / 4;
            if variables.len() != expected {
                return Err(ArchiveError::Invalid("symbolic identity placement count"));
            }
            let mut placements = Vec::new();
            placements
                .try_reserve_exact(expected)
                .map_err(|_| ArchiveError::Allocation)?;
            let mut occupied = self.scope.initial_board;
            for variable in variables {
                if variable >= self.scope.row_count {
                    return Err(ArchiveError::Invalid("symbolic identity source variable"));
                }
                let atom = self.archive.atom(variable, control)?;
                let placement = decode_row_atom(&atom, self.scope)?;
                if occupied & placement.cells_mask() != 0 {
                    return Err(ArchiveError::Invalid("symbolic identity overlapping cells"));
                }
                occupied |= placement.cells_mask();
                placements.push(placement);
            }
            if occupied != self.scope.initial_board | self.scope.required_cells {
                return Err(ArchiveError::Invalid("symbolic identity incomplete cover"));
            }
            output.push(
                StandardBoard64TilingIdentity::from_placements(
                    self.scope.initial_board,
                    placements,
                )
                .map_err(|_| ArchiveError::Invalid("symbolic identity placements"))?,
            );
        }
        Ok(output)
    }

    /// Global membership in the archived covered-source root, including any
    /// hidden suffix. This neither enumerates queues nor verifies per-identity
    /// correlation; that relation remains in the shared `accepted` root.
    /// `control` applies independently to each atom lookup and membership.
    pub fn covers_source(
        &mut self,
        source: &[PieceKind],
        control: OperationControl<'_>,
    ) -> Result<bool, ArchiveError> {
        if source.len() != self.scope.source_length as usize {
            return Err(ArchiveError::Invalid("symbolic source length"));
        }
        let mut variables = Vec::new();
        variables
            .try_reserve_exact(source.len())
            .map_err(|_| ArchiveError::Allocation)?;
        for (position, &piece) in source.iter().enumerate() {
            let code = piece.as_ascii() as u8;
            let piece_index = PIECE_CODES
                .iter()
                .position(|&candidate| candidate == code)
                .ok_or(ArchiveError::Invalid("symbolic source piece"))?
                as u32;
            let position = position as u32;
            let variable = self.scope.row_count + position * 7 + piece_index;
            let atom = self.archive.atom(variable, control)?;
            if atom.len() != 6
                || atom[0] != SOURCE_ATOM
                || atom[1..5] != position.to_le_bytes()
                || atom[5] != code
            {
                return Err(ArchiveError::Invalid("symbolic source atom"));
            }
            variables.push(variable);
        }
        self.archive.contains(self.roots[2], &variables, control)
    }

    pub fn clear_cache(&mut self) {
        self.archive.clear_cache();
    }

    pub fn into_archive(self) -> VerifiedArchive<R> {
        self.archive
    }
}

fn validate_scope(scope: SymbolicFamilyArchiveScope) -> Result<u32, ArchiveError> {
    if scope.width != 10 || !(1..=6).contains(&scope.height) {
        return Err(ArchiveError::Invalid("symbolic archive board frame"));
    }
    let cells = u32::from(scope.width) * u32::from(scope.height);
    let all_cells = (1_u64 << cells) - 1;
    if scope.required_cells == 0
        || !scope.required_cells.count_ones().is_multiple_of(4)
        || scope.initial_board & scope.required_cells != 0
        || scope.initial_board | scope.required_cells != all_cells
    {
        return Err(ArchiveError::Invalid("symbolic archive board masks"));
    }
    scope
        .source_length
        .checked_mul(7)
        .and_then(|sources| scope.row_count.checked_add(sources))
        .ok_or(ArchiveError::IntegerOverflow)
}

fn encode_scope(
    scope: SymbolicFamilyArchiveScope,
    annotation: &[u8],
) -> Result<Vec<u8>, ArchiveError> {
    let length = u32::try_from(annotation.len()).map_err(|_| ArchiveError::IntegerOverflow)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(
            SCOPE_BYTES
                .checked_add(annotation.len())
                .ok_or(ArchiveError::IntegerOverflow)?,
        )
        .map_err(|_| ArchiveError::Allocation)?;
    output.extend_from_slice(SCOPE_MAGIC);
    output.extend_from_slice(&[scope.width, scope.height, 0, 0]);
    output.extend_from_slice(&scope.row_count.to_le_bytes());
    output.extend_from_slice(&scope.source_length.to_le_bytes());
    output.extend_from_slice(&scope.initial_board.to_le_bytes());
    output.extend_from_slice(&scope.required_cells.to_le_bytes());
    output.extend_from_slice(&length.to_le_bytes());
    output.extend_from_slice(annotation);
    Ok(output)
}

fn decode_scope(input: &[u8]) -> Result<SymbolicFamilyArchiveScope, ArchiveError> {
    if input.len() < SCOPE_BYTES || &input[..8] != SCOPE_MAGIC || input[10..12] != [0, 0] {
        return Err(ArchiveError::Invalid("symbolic archive scope schema"));
    }
    let length = u32::from_le_bytes(input[36..40].try_into().expect("scope length field")) as usize;
    if input.len().checked_sub(SCOPE_BYTES) != Some(length) {
        return Err(ArchiveError::Invalid("symbolic archive annotation length"));
    }
    Ok(SymbolicFamilyArchiveScope {
        width: input[8],
        height: input[9],
        row_count: u32::from_le_bytes(input[12..16].try_into().expect("scope row count field")),
        source_length: u32::from_le_bytes(
            input[16..20].try_into().expect("scope source length field"),
        ),
        initial_board: u64::from_le_bytes(
            input[20..28].try_into().expect("scope initial board field"),
        ),
        required_cells: u64::from_le_bytes(
            input[28..36]
                .try_into()
                .expect("scope required cells field"),
        ),
    })
}

fn decode_row_atom(
    input: &[u8],
    scope: SymbolicFamilyArchiveScope,
) -> Result<PiecePlacementMask, ArchiveError> {
    if input.len() != 10 || input[0] != ROW_ATOM || !PIECE_CODES.contains(&input[1]) {
        return Err(ArchiveError::Invalid("symbolic placement atom"));
    }
    let piece = PieceKind::from_ascii(char::from(input[1]))
        .map_err(|_| ArchiveError::Invalid("symbolic placement piece"))?;
    let cells = u64::from_le_bytes(input[2..10].try_into().expect("placement cells field"));
    if cells.count_ones() != 4 || cells & !scope.required_cells != 0 {
        return Err(ArchiveError::Invalid("symbolic placement cells"));
    }
    Ok(PiecePlacementMask::new(piece, cells))
}

#[cfg(test)]
#[path = "symbolic_family_archive_tests.rs"]
mod tests;
