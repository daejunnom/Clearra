use sha2::{Digest, Sha256};

use crate::{limited, ArchiveError, ArchiveLimits, Node, EMPTY, UNIT};

pub(crate) const MAGIC: &[u8; 8] = b"CTK4ZDD\0";
pub(crate) const HEADER_BYTES: usize = 64;
pub(crate) const DIGEST_BYTES: usize = 32;
pub(crate) const DIRECTORY_BYTES: usize = 60;
pub(crate) const NODE_BYTES: usize = 36;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum BlockKind {
    Atoms = 0,
    Nodes = 1,
}

#[derive(Clone, Debug)]
pub(crate) struct Block {
    pub kind: BlockKind,
    pub first: u64,
    pub items: u32,
    pub offset: u64,
    pub bytes: u32,
    pub digest: [u8; 32],
}

#[derive(Clone, Debug)]
pub struct Root {
    pub name: String,
    pub node: u64,
    pub declared_count: u128,
}

#[derive(Clone, Debug)]
pub(crate) struct Manifest {
    pub variables: u32,
    pub nodes: u64,
    pub metadata: Vec<u8>,
    pub roots: Vec<Root>,
    pub blocks: Vec<Block>,
    pub manifest_bytes: u64,
    pub archive_bytes: u64,
    pub digest: [u8; 32],
}

#[derive(Clone, Copy)]
pub(crate) struct Header {
    pub variables: u32,
    pub nodes: u64,
    pub roots: u32,
    pub blocks: u32,
    pub metadata_bytes: u32,
    pub manifest_bytes: u64,
    pub archive_bytes: u64,
}

pub(crate) fn header(bytes: &[u8], limits: ArchiveLimits) -> Result<Header, ArchiveError> {
    if bytes.len() != HEADER_BYTES {
        return Err(ArchiveError::Invalid("header length"));
    }
    let mut input = Cursor::new(bytes);
    if input.take(8)? != MAGIC || input.u16()? != 1 || input.u16()? != 0 {
        return Err(ArchiveError::Invalid("magic, dialect revision or flags"));
    }
    let variables = input.u32()?;
    let nodes = input.u64()?;
    let roots = input.u32()?;
    let blocks = input.u32()?;
    let metadata_bytes = input.u32()?;
    if input.u16()? as usize != NODE_BYTES || input.u16()? != 0 {
        return Err(ArchiveError::Invalid("node record width or reserved field"));
    }
    let manifest_bytes = input.u64()?;
    let archive_bytes = input.u64()?;
    if input.u64()? != 0 || roots == 0 || nodes.checked_add(2).is_none() {
        return Err(ArchiveError::Invalid(
            "header roots, node range or reserved field",
        ));
    }
    limited(
        u64::from(variables),
        u64::from(limits.max_variables),
        "variables",
    )?;
    limited(nodes, limits.max_nodes, "nodes")?;
    limited(u64::from(roots), limits.max_roots as u64, "roots")?;
    limited(
        manifest_bytes,
        limits.max_manifest_bytes as u64,
        "manifest bytes",
    )?;
    let minimum = (HEADER_BYTES as u64)
        .checked_add(u64::from(metadata_bytes))
        .and_then(|n| n.checked_add(u64::from(roots) * 27))
        .and_then(|n| n.checked_add(u64::from(blocks) * DIRECTORY_BYTES as u64))
        .ok_or(ArchiveError::IntegerOverflow)?;
    if manifest_bytes < minimum
        || manifest_bytes
            .checked_add(DIGEST_BYTES as u64)
            .is_none_or(|size| size > archive_bytes)
    {
        return Err(ArchiveError::Invalid("manifest or archive length"));
    }
    Ok(Header {
        variables,
        nodes,
        roots,
        blocks,
        metadata_bytes,
        manifest_bytes,
        archive_bytes,
    })
}

pub(crate) fn decode_manifest(
    bytes: &[u8],
    expected_digest: [u8; 32],
    limits: ArchiveLimits,
) -> Result<Manifest, ArchiveError> {
    let fixed = bytes
        .get(..HEADER_BYTES)
        .ok_or(ArchiveError::Invalid("short manifest"))?;
    let header = header(fixed, limits)?;
    if header.manifest_bytes != bytes.len() as u64 {
        return Err(ArchiveError::Invalid("manifest extent"));
    }
    if digest(bytes) != expected_digest {
        return Err(ArchiveError::ChecksumMismatch);
    }
    let mut input = Cursor::new(&bytes[HEADER_BYTES..]);
    let metadata = copy_bytes(input.take(header.metadata_bytes as usize)?)?;
    let mut roots: Vec<Root> = Vec::new();
    roots
        .try_reserve_exact(header.roots as usize)
        .map_err(|_| ArchiveError::Allocation)?;
    for _ in 0..header.roots {
        let length = usize::from(input.u16()?);
        if length == 0 {
            return Err(ArchiveError::Invalid("empty root name"));
        }
        let name = std::str::from_utf8(input.take(length)?)
            .map_err(|_| ArchiveError::Invalid("root name UTF-8"))?;
        if roots.iter().any(|root| root.name == name) {
            return Err(ArchiveError::Invalid("duplicate root name"));
        }
        let mut owned_name = String::new();
        owned_name
            .try_reserve_exact(name.len())
            .map_err(|_| ArchiveError::Allocation)?;
        owned_name.push_str(name);
        let node = input.u64()?;
        let declared_count = input.u128()?;
        if node >= header.nodes + 2 {
            return Err(ArchiveError::Invalid("root node reference"));
        }
        roots.push(Root {
            name: owned_name,
            node,
            declared_count,
        });
    }
    let mut blocks = Vec::new();
    blocks
        .try_reserve_exact(header.blocks as usize)
        .map_err(|_| ArchiveError::Allocation)?;
    let mut atom_end = 0_u64;
    let mut node_end = 2_u64;
    let mut next_offset = header.manifest_bytes + DIGEST_BYTES as u64;
    let mut nodes_started = false;
    for _ in 0..header.blocks {
        let kind = match input.take(1)?[0] {
            0 if !nodes_started => BlockKind::Atoms,
            1 => {
                nodes_started = true;
                BlockKind::Nodes
            }
            _ => return Err(ArchiveError::Invalid("block kind or order")),
        };
        if input.take(3)? != [0, 0, 0] {
            return Err(ArchiveError::Invalid("block reserved field"));
        }
        let first = input.u64()?;
        let items = input.u32()?;
        let offset = input.u64()?;
        let bytes = input.u32()?;
        let digest: [u8; 32] = input.take(32)?.try_into().expect("fixed digest width");
        limited(
            u64::from(bytes),
            limits.max_block_bytes as u64,
            "block bytes",
        )?;
        if items == 0 || offset != next_offset {
            return Err(ArchiveError::Invalid(
                "empty, overlapping or noncontiguous block",
            ));
        }
        let end = first
            .checked_add(u64::from(items))
            .ok_or(ArchiveError::IntegerOverflow)?;
        match kind {
            BlockKind::Atoms => {
                if first != atom_end
                    || end > u64::from(header.variables)
                    || u64::from(bytes) < u64::from(items) * 4
                {
                    return Err(ArchiveError::Invalid("atom block range"));
                }
                atom_end = end;
            }
            BlockKind::Nodes => {
                if first != node_end
                    || end > header.nodes + 2
                    || u64::from(bytes) != u64::from(items) * NODE_BYTES as u64
                {
                    return Err(ArchiveError::Invalid("node block range or width"));
                }
                node_end = end;
            }
        }
        next_offset = offset
            .checked_add(u64::from(bytes))
            .ok_or(ArchiveError::IntegerOverflow)?;
        if next_offset > header.archive_bytes {
            return Err(ArchiveError::Invalid("block outside archive"));
        }
        blocks.push(Block {
            kind,
            first,
            items,
            offset,
            bytes,
            digest,
        });
    }
    if !input.is_empty()
        || atom_end != u64::from(header.variables)
        || node_end != header.nodes + 2
        || next_offset != header.archive_bytes
    {
        return Err(ArchiveError::Invalid(
            "incomplete directory or trailing manifest bytes",
        ));
    }
    Ok(Manifest {
        variables: header.variables,
        nodes: header.nodes,
        metadata,
        roots,
        blocks,
        manifest_bytes: header.manifest_bytes,
        archive_bytes: header.archive_bytes,
        digest: expected_digest,
    })
}

pub(crate) fn encode_manifest(manifest: &Manifest) -> Result<Vec<u8>, ArchiveError> {
    let length =
        usize::try_from(manifest.manifest_bytes).map_err(|_| ArchiveError::IntegerOverflow)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(length)
        .map_err(|_| ArchiveError::Allocation)?;
    output.extend_from_slice(MAGIC);
    output.extend_from_slice(&1_u16.to_le_bytes());
    output.extend_from_slice(&0_u16.to_le_bytes());
    output.extend_from_slice(&manifest.variables.to_le_bytes());
    output.extend_from_slice(&manifest.nodes.to_le_bytes());
    output.extend_from_slice(
        &u32::try_from(manifest.roots.len())
            .map_err(|_| ArchiveError::IntegerOverflow)?
            .to_le_bytes(),
    );
    output.extend_from_slice(
        &u32::try_from(manifest.blocks.len())
            .map_err(|_| ArchiveError::IntegerOverflow)?
            .to_le_bytes(),
    );
    output.extend_from_slice(
        &u32::try_from(manifest.metadata.len())
            .map_err(|_| ArchiveError::IntegerOverflow)?
            .to_le_bytes(),
    );
    output.extend_from_slice(&(NODE_BYTES as u16).to_le_bytes());
    output.extend_from_slice(&0_u16.to_le_bytes());
    output.extend_from_slice(&manifest.manifest_bytes.to_le_bytes());
    output.extend_from_slice(&manifest.archive_bytes.to_le_bytes());
    output.extend_from_slice(&0_u64.to_le_bytes());
    output.extend_from_slice(&manifest.metadata);
    for root in &manifest.roots {
        output.extend_from_slice(
            &u16::try_from(root.name.len())
                .map_err(|_| ArchiveError::IntegerOverflow)?
                .to_le_bytes(),
        );
        output.extend_from_slice(root.name.as_bytes());
        output.extend_from_slice(&root.node.to_le_bytes());
        output.extend_from_slice(&root.declared_count.to_le_bytes());
    }
    for block in &manifest.blocks {
        output.push(block.kind as u8);
        output.extend_from_slice(&[0; 3]);
        output.extend_from_slice(&block.first.to_le_bytes());
        output.extend_from_slice(&block.items.to_le_bytes());
        output.extend_from_slice(&block.offset.to_le_bytes());
        output.extend_from_slice(&block.bytes.to_le_bytes());
        output.extend_from_slice(&block.digest);
    }
    if output.len() != length {
        return Err(ArchiveError::Invalid("encoded manifest length"));
    }
    Ok(output)
}

pub(crate) fn decode_node(bytes: &[u8], id: u64, variables: u32) -> Result<Node, ArchiveError> {
    if bytes.len() != NODE_BYTES {
        return Err(ArchiveError::Invalid("node width"));
    }
    let mut input = Cursor::new(bytes);
    let node = Node {
        variable: input.u32()?,
        low: input.u64()?,
        high: input.u64()?,
        count: input.u128()?,
    };
    validate_node_shape(node, id, variables)?;
    Ok(node)
}

pub(crate) fn validate_node_shape(node: Node, id: u64, variables: u32) -> Result<(), ArchiveError> {
    if id < 2
        || node.variable >= variables
        || node.low >= id
        || node.high >= id
        || node.high == EMPTY
        || node.count == 0
    {
        return Err(ArchiveError::Invalid(
            "node variable, reference, cycle or zero-suppression",
        ));
    }
    Ok(())
}

pub(crate) fn encode_node(node: Node, output: &mut Vec<u8>) {
    output.extend_from_slice(&node.variable.to_le_bytes());
    output.extend_from_slice(&node.low.to_le_bytes());
    output.extend_from_slice(&node.high.to_le_bytes());
    output.extend_from_slice(&node.count.to_le_bytes());
}

pub(crate) fn terminal_count(id: u64) -> Option<u128> {
    match id {
        EMPTY => Some(0),
        UNIT => Some(1),
        _ => None,
    }
}

pub(crate) fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub(crate) fn copy_bytes(bytes: &[u8]) -> Result<Vec<u8>, ArchiveError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(bytes.len())
        .map_err(|_| ArchiveError::Allocation)?;
    output.extend_from_slice(bytes);
    Ok(output)
}

pub(crate) struct Cursor<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Cursor<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
    pub fn is_empty(&self) -> bool {
        self.position == self.bytes.len()
    }
    pub fn take(&mut self, length: usize) -> Result<&'a [u8], ArchiveError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(ArchiveError::IntegerOverflow)?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(ArchiveError::Invalid("truncated record"))?;
        self.position = end;
        Ok(value)
    }
    pub fn u16(&mut self) -> Result<u16, ArchiveError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("u16 width"),
        ))
    }
    pub fn u32(&mut self) -> Result<u32, ArchiveError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("u32 width"),
        ))
    }
    pub fn u64(&mut self) -> Result<u64, ArchiveError> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("u64 width"),
        ))
    }
    pub fn u128(&mut self) -> Result<u128, ArchiveError> {
        Ok(u128::from_le_bytes(
            self.take(16)?.try_into().expect("u128 width"),
        ))
    }
}
