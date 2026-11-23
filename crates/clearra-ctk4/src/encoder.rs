use std::io::{Read, Seek, SeekFrom, Write};

use crate::format::{
    self, Block, BlockKind, Manifest, Root, DIGEST_BYTES, DIRECTORY_BYTES, HEADER_BYTES, NODE_BYTES,
};
use crate::reader::{SeekReadAt, UnverifiedArchive};
use crate::{limited, ArchiveError, ArchiveLimits, Node, OperationControl, ProducerSeal, Work};

/// A frozen family snapshot. Nonterminals occupy IDs `2..node_count + 2`;
/// terminals are implicit. The encoder never enumerates root members.
/// Mutable receivers allow a producer to reuse its existing count cache.
pub trait ArchiveSource {
    fn variable_count(&self) -> u32;
    fn node_count(&self) -> u64;
    /// The encoder clears `output` before each call. The source appends one
    /// opaque descriptor, without interpreting or expanding another family.
    fn copy_atom(&mut self, variable: u32, output: &mut Vec<u8>) -> Result<(), ArchiveError>;
    fn node(&mut self, id: u64) -> Result<Node, ArchiveError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamedRoot {
    pub name: String,
    pub node: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EncodeOptions {
    pub atom_block_items: u32,
    pub node_block_items: u32,
}

impl Default for EncodeOptions {
    fn default() -> Self {
        Self {
            atom_block_items: 1024,
            node_block_items: 4096,
        }
    }
}

/// Writes an experimental ordered-ZDD archive to an initially empty sink.
///
/// The sink is readable because the final seal validates the actual serialized
/// nodes with the same strict scanner used for untrusted archives. This is a
/// second O(nodes) pass, with a bounded block cache and no set enumeration or
/// per-node count array. A mutable or inconsistent source cannot forge a seal
/// by returning different child counts during encoding.
///
/// On error, bytes may have been written and are not publishable. The host owns
/// a temporary file, durability, and atomic publication after success. The
/// codec never renames, deletes, publishes, or opens a file itself.
pub fn write_archive<S: ArchiveSource, W: Read + Write + Seek>(
    source: &mut S,
    roots: &[NamedRoot],
    metadata: &[u8],
    destination: &mut W,
    options: EncodeOptions,
    limits: ArchiveLimits,
    control: OperationControl<'_>,
) -> Result<ProducerSeal, ArchiveError> {
    let mut work = Work::new(control)?;
    if destination.seek(SeekFrom::End(0))? != 0 {
        return Err(ArchiveError::Invalid("destination must be empty"));
    }
    let variables = source.variable_count();
    let nodes = source.node_count();
    let last = nodes.checked_add(2).ok_or(ArchiveError::IntegerOverflow)?;
    if roots.is_empty() || options.atom_block_items == 0 || options.node_block_items == 0 {
        return Err(ArchiveError::Invalid("roots or block item count"));
    }
    limited(
        u64::from(variables),
        u64::from(limits.max_variables),
        "variables",
    )?;
    limited(nodes, limits.max_nodes, "nodes")?;
    limited(roots.len() as u64, limits.max_roots as u64, "roots")?;
    u32::try_from(roots.len()).map_err(|_| ArchiveError::IntegerOverflow)?;
    u32::try_from(metadata.len()).map_err(|_| ArchiveError::IntegerOverflow)?;
    let block_count = u64::from(variables)
        .div_ceil(u64::from(options.atom_block_items))
        .checked_add(nodes.div_ceil(u64::from(options.node_block_items)))
        .ok_or(ArchiveError::IntegerOverflow)?;
    let block_count = u32::try_from(block_count).map_err(|_| ArchiveError::IntegerOverflow)?;
    let mut manifest_bytes = (HEADER_BYTES as u64)
        .checked_add(metadata.len() as u64)
        .and_then(|size| size.checked_add(u64::from(block_count) * DIRECTORY_BYTES as u64))
        .ok_or(ArchiveError::IntegerOverflow)?;
    for (index, root) in roots.iter().enumerate() {
        work.step()?;
        if root.name.is_empty()
            || root.name.len() > u16::MAX as usize
            || root.node >= last
            || roots[..index]
                .iter()
                .any(|earlier| earlier.name == root.name)
        {
            return Err(ArchiveError::Invalid("root name or node"));
        }
        manifest_bytes = manifest_bytes
            .checked_add(26 + root.name.len() as u64)
            .ok_or(ArchiveError::IntegerOverflow)?;
    }
    limited(
        manifest_bytes,
        limits.max_manifest_bytes as u64,
        "manifest bytes",
    )?;
    usize::try_from(manifest_bytes).map_err(|_| ArchiveError::IntegerOverflow)?;
    let mut manifest_roots = Vec::new();
    manifest_roots
        .try_reserve_exact(roots.len())
        .map_err(|_| ArchiveError::Allocation)?;
    for root in roots {
        let count = if let Some(count) = format::terminal_count(root.node) {
            count
        } else {
            source.node(root.node)?.count
        };
        let mut name = String::new();
        name.try_reserve_exact(root.name.len())
            .map_err(|_| ArchiveError::Allocation)?;
        name.push_str(&root.name);
        manifest_roots.push(Root {
            name,
            node: root.node,
            declared_count: count,
        });
    }
    let mut blocks = Vec::new();
    blocks
        .try_reserve_exact(block_count as usize)
        .map_err(|_| ArchiveError::Allocation)?;
    let mut next_offset = manifest_bytes
        .checked_add(DIGEST_BYTES as u64)
        .ok_or(ArchiveError::IntegerOverflow)?;
    // Keep the reserved manifest invalid while data blocks are written.
    // Publication still requires the seal returned after the final scan.
    let zeros = [0_u8; 4096];
    let mut remaining = next_offset;
    while remaining != 0 {
        work.step()?;
        let take = remaining.min(zeros.len() as u64) as usize;
        destination.write_all(&zeros[..take])?;
        remaining -= take as u64;
    }
    let mut payload = Vec::new();
    let mut atom = Vec::new();
    let mut first = 0_u64;
    while first < u64::from(variables) {
        let items = (u64::from(variables) - first).min(u64::from(options.atom_block_items)) as u32;
        payload.clear();
        for variable in first..first + u64::from(items) {
            work.step()?;
            atom.clear();
            source.copy_atom(variable as u32, &mut atom)?;
            limited(
                atom.len() as u64,
                limits.max_atom_bytes as u64,
                "atom bytes",
            )?;
            let length = u32::try_from(atom.len()).map_err(|_| ArchiveError::IntegerOverflow)?;
            let new_length = payload
                .len()
                .checked_add(4)
                .and_then(|size| size.checked_add(atom.len()))
                .ok_or(ArchiveError::IntegerOverflow)?;
            limited(
                new_length as u64,
                limits.max_block_bytes as u64,
                "block bytes",
            )?;
            payload
                .try_reserve(4 + atom.len())
                .map_err(|_| ArchiveError::Allocation)?;
            payload.extend_from_slice(&length.to_le_bytes());
            payload.extend_from_slice(&atom);
        }
        blocks.push(write_block(
            destination,
            BlockKind::Atoms,
            first,
            items,
            &payload,
            &mut next_offset,
        )?);
        first += u64::from(items);
    }
    drop(atom);
    first = 2;
    while first < last {
        let items = (last - first).min(u64::from(options.node_block_items)) as u32;
        let size = (items as usize)
            .checked_mul(NODE_BYTES)
            .ok_or(ArchiveError::IntegerOverflow)?;
        limited(size as u64, limits.max_block_bytes as u64, "block bytes")?;
        payload.clear();
        payload
            .try_reserve_exact(size)
            .map_err(|_| ArchiveError::Allocation)?;
        for id in first..first + u64::from(items) {
            work.step()?;
            let node = source.node(id)?;
            format::validate_node_shape(node, id, variables)?;
            format::encode_node(node, &mut payload);
        }
        blocks.push(write_block(
            destination,
            BlockKind::Nodes,
            first,
            items,
            &payload,
            &mut next_offset,
        )?);
        first += u64::from(items);
    }
    drop(payload);
    let manifest = Manifest {
        variables,
        nodes,
        metadata: format::copy_bytes(metadata)?,
        roots: manifest_roots,
        blocks,
        manifest_bytes,
        archive_bytes: next_offset,
        digest: [0; 32],
    };
    let encoded = format::encode_manifest(&manifest)?;
    let digest = format::digest(&encoded);
    destination.seek(SeekFrom::Start(0))?;
    destination.write_all(&encoded)?;
    destination.write_all(&digest)?;
    destination.flush()?;
    drop(encoded);
    drop(manifest);
    work.check_cancelled()?;
    let verified =
        UnverifiedArchive::open_with_work(SeekReadAt::new(&mut *destination)?, limits, &mut work)?
            .verify_with_work(&mut work)?;
    let seal = verified.seal();
    drop(verified);
    destination.seek(SeekFrom::End(0))?;
    Ok(seal)
}

fn write_block(
    destination: &mut impl Write,
    kind: BlockKind,
    first: u64,
    items: u32,
    payload: &[u8],
    offset: &mut u64,
) -> Result<Block, ArchiveError> {
    let bytes = u32::try_from(payload.len()).map_err(|_| ArchiveError::IntegerOverflow)?;
    let descriptor = Block {
        kind,
        first,
        items,
        offset: *offset,
        bytes,
        digest: format::digest(payload),
    };
    destination.write_all(payload)?;
    *offset = offset
        .checked_add(u64::from(bytes))
        .ok_or(ArchiveError::IntegerOverflow)?;
    Ok(descriptor)
}
