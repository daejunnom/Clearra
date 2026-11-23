use std::io::{self, Read, Seek, SeekFrom};

use crate::format::{self, BlockKind, Cursor, Manifest, DIGEST_BYTES, HEADER_BYTES, NODE_BYTES};
use crate::{
    limited, zeroed, ArchiveError, ArchiveLimits, Node, OperationControl, ProducerSeal, Work,
    EMPTY, UNIT,
};

pub use crate::format::Root as DeclaredRoot;

/// The host owns storage and transport. The decoder requests bounded ranges;
/// it never chooses a URL, opens a path, or loads the complete artifact itself.
/// An implementation must keep its length stable for this reader's lifetime.
pub trait ReadAt {
    fn len(&self) -> u64;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    fn read_exact_at(&mut self, offset: u64, output: &mut [u8]) -> io::Result<()>;
}

pub struct SeekReadAt<R> {
    source: R,
    length: u64,
}

impl<R: Read + Seek> SeekReadAt<R> {
    pub fn new(mut source: R) -> io::Result<Self> {
        let length = source.seek(SeekFrom::End(0))?;
        Ok(Self { source, length })
    }

    pub fn into_inner(self) -> R {
        self.source
    }
}

impl<R: Read + Seek> ReadAt for SeekReadAt<R> {
    fn len(&self) -> u64 {
        self.length
    }

    fn read_exact_at(&mut self, offset: u64, output: &mut [u8]) -> io::Result<()> {
        if offset
            .checked_add(output.len() as u64)
            .is_none_or(|end| end > self.length)
        {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "CTK4 range outside source",
            ));
        }
        self.source.seek(SeekFrom::Start(offset))?;
        self.source.read_exact(output)
    }
}

impl ReadAt for &[u8] {
    fn len(&self) -> u64 {
        (*self).len() as u64
    }

    fn read_exact_at(&mut self, offset: u64, output: &mut [u8]) -> io::Result<()> {
        let start = usize::try_from(offset)
            .map_err(|_| io::Error::new(io::ErrorKind::UnexpectedEof, "CTK4 range overflow"))?;
        let end = start
            .checked_add(output.len())
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "CTK4 range overflow"))?;
        let bytes = self.get(start..end).ok_or_else(|| {
            io::Error::new(io::ErrorKind::UnexpectedEof, "CTK4 range outside source")
        })?;
        output.copy_from_slice(bytes);
        Ok(())
    }
}

/// On an unverified reader every count is only a declaration. A valid manifest
/// checksum says nothing about the node recurrence or page offsets.
pub struct ArchiveInfo<'a> {
    pub variable_count: u32,
    pub node_count: u64,
    pub block_count: usize,
    pub archive_bytes: u64,
    pub manifest_digest: &'a [u8; 32],
    pub metadata: &'a [u8],
    pub roots: &'a [DeclaredRoot],
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ArchiveIoMetrics {
    pub bytes_read: u64,
    pub block_fetches: u64,
    pub cache_hits: u64,
    pub retained_cache_bytes: usize,
    pub uncached_block_bytes: usize,
    pub peak_cached_payload_bytes: usize,
    /// Live traversal frames, not bytes or allocator capacity. Counted page
    /// windows avoid retaining siblings outside the requested output range.
    pub peak_page_stack_entries: usize,
}

struct CachedBlock {
    index: usize,
    stamp: u64,
    bytes: Vec<u8>,
}

struct Reader<R> {
    source: R,
    manifest: Manifest,
    limits: ArchiveLimits,
    cache: Vec<CachedBlock>,
    cached_payload_bytes: usize,
    uncached: Vec<u8>,
    clock: u64,
    metrics: ArchiveIoMetrics,
}

/// Opening reads the manifest only. No exact count, contains or page API is
/// available until a full scan or a matching in-process producer seal succeeds.
pub struct UnverifiedArchive<R> {
    reader: Reader<R>,
}

pub struct VerifiedArchive<R> {
    reader: Reader<R>,
    full_scan_performed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExactPage {
    pub root_index: usize,
    pub offset: u128,
    pub total_count: u128,
    /// Each set is a strictly increasing list of atom/variable IDs.
    pub sets: Vec<Vec<u32>>,
}

impl<R: ReadAt> UnverifiedArchive<R> {
    pub fn open(
        source: R,
        limits: ArchiveLimits,
        control: OperationControl<'_>,
    ) -> Result<Self, ArchiveError> {
        let mut work = Work::new(control)?;
        Self::open_with_work(source, limits, &mut work)
    }

    pub(crate) fn open_with_work(
        mut source: R,
        limits: ArchiveLimits,
        work: &mut Work<'_>,
    ) -> Result<Self, ArchiveError> {
        work.step()?;
        let mut fixed = [0; HEADER_BYTES];
        source.read_exact_at(0, &mut fixed)?;
        let header = format::header(&fixed, limits)?;
        if header.archive_bytes != source.len() {
            return Err(ArchiveError::Invalid("source length differs from manifest"));
        }
        let manifest_bytes =
            usize::try_from(header.manifest_bytes).map_err(|_| ArchiveError::IntegerOverflow)?;
        let mut bytes = zeroed(manifest_bytes)?;
        bytes[..HEADER_BYTES].copy_from_slice(&fixed);
        source.read_exact_at(HEADER_BYTES as u64, &mut bytes[HEADER_BYTES..])?;
        let mut digest = [0; DIGEST_BYTES];
        source.read_exact_at(header.manifest_bytes, &mut digest)?;
        let manifest = format::decode_manifest(&bytes, digest, limits)?;
        let bytes_read = header
            .manifest_bytes
            .checked_add(DIGEST_BYTES as u64)
            .ok_or(ArchiveError::IntegerOverflow)?;
        work.check_cancelled()?;
        Ok(Self {
            reader: Reader {
                source,
                manifest,
                limits,
                cache: Vec::new(),
                cached_payload_bytes: 0,
                uncached: Vec::new(),
                clock: 0,
                metrics: ArchiveIoMetrics {
                    bytes_read,
                    ..ArchiveIoMetrics::default()
                },
            },
        })
    }

    pub fn info(&self) -> ArchiveInfo<'_> {
        self.reader.info()
    }
    pub fn metrics(&self) -> ArchiveIoMetrics {
        self.reader.metrics()
    }

    /// Validates all actual nodes and root counts without enumerating sets.
    /// The block cache is bounded; an archive wider than it may re-read blocks.
    pub fn verify_all(
        self,
        control: OperationControl<'_>,
    ) -> Result<VerifiedArchive<R>, ArchiveError> {
        let mut work = Work::new(control)?;
        self.verify_with_work(&mut work)
    }

    pub(crate) fn verify_with_work(
        mut self,
        work: &mut Work<'_>,
    ) -> Result<VerifiedArchive<R>, ArchiveError> {
        // Atom payloads are opaque, but every length and block extent is checked.
        for index in 0..self.reader.manifest.blocks.len() {
            if self.reader.manifest.blocks[index].kind == BlockKind::Atoms {
                self.reader.block(index, work)?;
            }
        }
        for id in 2..self.reader.manifest.nodes + 2 {
            work.step()?;
            let node = self
                .reader
                .node(id, work)?
                .ok_or(ArchiveError::Invalid("missing nonterminal"))?;
            let low = self.reader.node(node.low, work)?;
            let high = self.reader.node(node.high, work)?;
            if low.is_some_and(|child| child.variable <= node.variable)
                || high.is_some_and(|child| child.variable <= node.variable)
            {
                return Err(ArchiveError::Invalid("non-increasing variable order"));
            }
            let low_count = low
                .map(|child| child.count)
                .or_else(|| format::terminal_count(node.low))
                .ok_or(ArchiveError::Invalid("low count"))?;
            let high_count = high
                .map(|child| child.count)
                .or_else(|| format::terminal_count(node.high))
                .ok_or(ArchiveError::Invalid("high count"))?;
            if low_count
                .checked_add(high_count)
                .ok_or(ArchiveError::CountOverflow)?
                != node.count
            {
                return Err(ArchiveError::Invalid("node count recurrence"));
            }
        }
        for index in 0..self.reader.manifest.roots.len() {
            work.step()?;
            let root = &self.reader.manifest.roots[index];
            let id = root.node;
            let declared = root.declared_count;
            if self.reader.node_count(id, work)? != declared {
                return Err(ArchiveError::Invalid("root count"));
            }
        }
        work.check_cancelled()?;
        Ok(VerifiedArchive {
            reader: self.reader,
            full_scan_performed: true,
        })
    }

    /// This capability is created by the writer or a strict scan, never by
    /// decoding a checksum supplied in a document. It binds every directory
    /// entry, opaque atom, metadata byte, root and node block of this artifact.
    pub fn accept_producer_seal(
        self,
        seal: &ProducerSeal,
    ) -> Result<VerifiedArchive<R>, ArchiveError> {
        if self.reader.manifest.digest != seal.manifest_digest
            || self.reader.manifest.archive_bytes != seal.archive_bytes
        {
            return Err(ArchiveError::SealMismatch);
        }
        Ok(VerifiedArchive {
            reader: self.reader,
            full_scan_performed: false,
        })
    }
}

impl<R: ReadAt> VerifiedArchive<R> {
    pub fn info(&self) -> ArchiveInfo<'_> {
        self.reader.info()
    }
    pub fn metrics(&self) -> ArchiveIoMetrics {
        self.reader.metrics()
    }
    pub const fn full_scan_performed(&self) -> bool {
        self.full_scan_performed
    }

    pub fn seal(&self) -> ProducerSeal {
        ProducerSeal {
            manifest_digest: self.reader.manifest.digest,
            archive_bytes: self.reader.manifest.archive_bytes,
        }
    }

    pub fn count(&self, root_index: usize) -> Result<u128, ArchiveError> {
        Ok(self.reader.root(root_index)?.declared_count)
    }

    pub fn atom(
        &mut self,
        variable: u32,
        control: OperationControl<'_>,
    ) -> Result<Vec<u8>, ArchiveError> {
        let mut work = Work::new(control)?;
        if variable >= self.reader.manifest.variables {
            return Err(ArchiveError::Invalid("atom index"));
        }
        let index = self.reader.locate(BlockKind::Atoms, u64::from(variable))?;
        let first = self.reader.manifest.blocks[index].first;
        let bytes = self.reader.block(index, &mut work)?;
        let mut input = Cursor::new(bytes);
        for current in first..=u64::from(variable) {
            work.step()?;
            let length = input.u32()? as usize;
            let atom = input.take(length)?;
            if current == u64::from(variable) {
                return format::copy_bytes(atom);
            }
        }
        Err(ArchiveError::Invalid("atom block index"))
    }

    pub fn contains(
        &mut self,
        root_index: usize,
        variables: &[u32],
        control: OperationControl<'_>,
    ) -> Result<bool, ArchiveError> {
        let mut work = Work::new(control)?;
        if variables.windows(2).any(|pair| pair[0] >= pair[1])
            || variables
                .last()
                .is_some_and(|variable| *variable >= self.reader.manifest.variables)
        {
            return Err(ArchiveError::Invalid(
                "membership variables must be ordered and in range",
            ));
        }
        let mut root = self.reader.root(root_index)?.node;
        let mut cursor = 0;
        while root >= 2 {
            work.step()?;
            let node = self
                .reader
                .node(root, &mut work)?
                .ok_or(ArchiveError::Invalid("membership node"))?;
            match variables.get(cursor).copied() {
                Some(variable) if variable < node.variable => return Ok(false),
                Some(variable) if variable == node.variable => {
                    cursor += 1;
                    root = node.high;
                }
                _ => root = node.low,
            }
        }
        Ok(root == UNIT && cursor == variables.len())
    }

    /// Counted branches are skipped without expanding earlier sets. This order
    /// is low-before-high in the archived variable order, not a CTK1 sort order.
    pub fn page(
        &mut self,
        root_index: usize,
        offset: u128,
        limit: usize,
        control: OperationControl<'_>,
    ) -> Result<ExactPage, ArchiveError> {
        let mut work = Work::new(control)?;
        limited(
            limit as u64,
            self.reader.limits.max_page_items as u64,
            "page items",
        )?;
        let root = self.reader.root(root_index)?;
        let total_count = root.declared_count;
        let root_id = root.node;
        let wanted = total_count.saturating_sub(offset).min(limit as u128) as usize;
        let mut output = Vec::new();
        output
            .try_reserve_exact(wanted)
            .map_err(|_| ArchiveError::Allocation)?;
        if wanted == 0 {
            return Ok(ExactPage {
                root_index,
                offset,
                total_count,
                sets: output,
            });
        }
        let mut stack = Vec::new();
        stack.try_reserve(1).map_err(|_| ArchiveError::Allocation)?;
        stack.push(PageFrame {
            node: root_id,
            prefix: 0,
            selected: None,
        });
        self.reader.metrics.peak_page_stack_entries =
            self.reader.metrics.peak_page_stack_entries.max(stack.len());
        let mut path = Vec::new();
        let mut skip = offset;
        let mut emitted_variables = 0_usize;
        while let Some(frame) = stack.pop() {
            work.step()?;
            path.truncate(frame.prefix);
            if let Some(variable) = frame.selected {
                let length = path
                    .len()
                    .checked_add(1)
                    .ok_or(ArchiveError::IntegerOverflow)?;
                limited(
                    length as u64,
                    self.reader.limits.max_page_variables as u64,
                    "page variables",
                )?;
                path.try_reserve(1).map_err(|_| ArchiveError::Allocation)?;
                path.push(variable);
            }
            if frame.node == EMPTY {
                continue;
            }
            let node = self.reader.node(frame.node, &mut work)?;
            let count = node.map_or(1, |node| node.count);
            if skip >= count {
                skip -= count;
                continue;
            }
            if let Some(node) = node {
                let low_count = self.reader.node_count(node.low, &mut work)?;
                let remaining = (wanted - output.len()) as u128;
                stack.try_reserve(2).map_err(|_| ArchiveError::Allocation)?;
                // Only branches intersecting the requested rank window need
                // frames. In particular, a long low chain of singleton sets
                // must not retain every unrelated high sibling for page 1.
                if skip >= low_count {
                    skip -= low_count;
                    stack.push(PageFrame {
                        node: node.high,
                        prefix: path.len(),
                        selected: Some(node.variable),
                    });
                } else {
                    if remaining > low_count - skip {
                        stack.push(PageFrame {
                            node: node.high,
                            prefix: path.len(),
                            selected: Some(node.variable),
                        });
                    }
                    stack.push(PageFrame {
                        node: node.low,
                        prefix: path.len(),
                        selected: None,
                    });
                }
                self.reader.metrics.peak_page_stack_entries =
                    self.reader.metrics.peak_page_stack_entries.max(stack.len());
            } else {
                emitted_variables = emitted_variables
                    .checked_add(path.len())
                    .ok_or(ArchiveError::IntegerOverflow)?;
                limited(
                    emitted_variables as u64,
                    self.reader.limits.max_page_variables as u64,
                    "page variables",
                )?;
                let mut set = Vec::new();
                set.try_reserve_exact(path.len())
                    .map_err(|_| ArchiveError::Allocation)?;
                set.extend_from_slice(&path);
                output.push(set);
                if output.len() == wanted {
                    break;
                }
            }
        }
        if output.len() != wanted {
            return Err(ArchiveError::Invalid("page did not match verified count"));
        }
        work.check_cancelled()?;
        Ok(ExactPage {
            root_index,
            offset,
            total_count,
            sets: output,
        })
    }

    pub fn clear_cache(&mut self) {
        self.reader.cache = Vec::new();
        self.reader.uncached = Vec::new();
        self.reader.cached_payload_bytes = 0;
    }

    pub fn into_source(self) -> R {
        self.reader.source
    }
}

struct PageFrame {
    node: u64,
    prefix: usize,
    selected: Option<u32>,
}

impl<R: ReadAt> Reader<R> {
    fn info(&self) -> ArchiveInfo<'_> {
        ArchiveInfo {
            variable_count: self.manifest.variables,
            node_count: self.manifest.nodes,
            block_count: self.manifest.blocks.len(),
            archive_bytes: self.manifest.archive_bytes,
            manifest_digest: &self.manifest.digest,
            metadata: &self.manifest.metadata,
            roots: &self.manifest.roots,
        }
    }

    fn metrics(&self) -> ArchiveIoMetrics {
        ArchiveIoMetrics {
            retained_cache_bytes: self.cached_payload_bytes
                + self.cache.capacity() * std::mem::size_of::<CachedBlock>(),
            uncached_block_bytes: self.uncached.capacity(),
            ..self.metrics
        }
    }

    fn root(&self, index: usize) -> Result<&DeclaredRoot, ArchiveError> {
        self.manifest
            .roots
            .get(index)
            .ok_or(ArchiveError::Invalid("root index"))
    }

    fn locate(&self, kind: BlockKind, id: u64) -> Result<usize, ArchiveError> {
        let after = self.manifest.blocks.partition_point(|block| {
            block.kind < kind || (block.kind == kind && block.first <= id)
        });
        let index = after
            .checked_sub(1)
            .ok_or(ArchiveError::Invalid("block address"))?;
        let block = &self.manifest.blocks[index];
        if block.kind != kind || id >= block.first + u64::from(block.items) {
            return Err(ArchiveError::Invalid("block address"));
        }
        Ok(index)
    }

    fn node(&mut self, id: u64, work: &mut Work<'_>) -> Result<Option<Node>, ArchiveError> {
        if id < 2 {
            return Ok(None);
        }
        if id >= self.manifest.nodes + 2 {
            return Err(ArchiveError::Invalid("node address"));
        }
        let index = self.locate(BlockKind::Nodes, id)?;
        let offset = usize::try_from(id - self.manifest.blocks[index].first)
            .map_err(|_| ArchiveError::IntegerOverflow)?
            * NODE_BYTES;
        let variables = self.manifest.variables;
        let bytes = self.block(index, work)?;
        Ok(Some(format::decode_node(
            &bytes[offset..offset + NODE_BYTES],
            id,
            variables,
        )?))
    }

    fn node_count(&mut self, id: u64, work: &mut Work<'_>) -> Result<u128, ArchiveError> {
        if let Some(count) = format::terminal_count(id) {
            return Ok(count);
        }
        self.node(id, work)?
            .map(|node| node.count)
            .ok_or(ArchiveError::Invalid("node count"))
    }

    fn block(&mut self, index: usize, work: &mut Work<'_>) -> Result<&[u8], ArchiveError> {
        work.step()?;
        // A previous oversize/no-cache block is no longer borrowed when a
        // new mutable request begins. Do not retain it beside the live cache.
        self.uncached = Vec::new();
        self.clock = self
            .clock
            .checked_add(1)
            .ok_or(ArchiveError::IntegerOverflow)?;
        if let Ok(position) = self.cache.binary_search_by_key(&index, |block| block.index) {
            self.cache[position].stamp = self.clock;
            self.metrics.cache_hits = self
                .metrics
                .cache_hits
                .checked_add(1)
                .ok_or(ArchiveError::IntegerOverflow)?;
            return Ok(&self.cache[position].bytes);
        }
        let descriptor = &self.manifest.blocks[index];
        let length = descriptor.bytes as usize;
        let mut bytes = zeroed(length)?;
        self.source.read_exact_at(descriptor.offset, &mut bytes)?;
        self.metrics.bytes_read = self
            .metrics
            .bytes_read
            .checked_add(length as u64)
            .ok_or(ArchiveError::IntegerOverflow)?;
        self.metrics.block_fetches = self
            .metrics
            .block_fetches
            .checked_add(1)
            .ok_or(ArchiveError::IntegerOverflow)?;
        if format::digest(&bytes) != descriptor.digest {
            return Err(ArchiveError::ChecksumMismatch);
        }
        match descriptor.kind {
            BlockKind::Atoms => {
                let mut input = Cursor::new(&bytes);
                for _ in 0..descriptor.items {
                    work.step()?;
                    let atom_length = input.u32()? as usize;
                    limited(
                        atom_length as u64,
                        self.limits.max_atom_bytes as u64,
                        "atom bytes",
                    )?;
                    input.take(atom_length)?;
                }
                if !input.is_empty() {
                    return Err(ArchiveError::Invalid("atom block trailing bytes"));
                }
            }
            BlockKind::Nodes => {
                for (local, encoded) in bytes.chunks_exact(NODE_BYTES).enumerate() {
                    work.step()?;
                    format::decode_node(
                        encoded,
                        descriptor.first + local as u64,
                        self.manifest.variables,
                    )?;
                }
            }
        }
        work.check_cancelled()?;
        let entry_bytes = std::mem::size_of::<CachedBlock>();
        let charge = bytes
            .capacity()
            .checked_add(entry_bytes)
            .ok_or(ArchiveError::IntegerOverflow)?;
        if self.limits.cache_bytes == 0 || charge > self.limits.cache_bytes {
            self.uncached = bytes;
            return Ok(&self.uncached);
        }
        // Entry count is additionally bounded by the byte charge, including
        // fixed fields. The vector stays sorted by block ID for logarithmic hits.
        loop {
            if self.cache.is_empty() {
                self.cache = Vec::new();
            }
            let next_capacity = self.cache.capacity().max(self.cache.len() + 1);
            let required = self
                .cached_payload_bytes
                .checked_add(bytes.capacity())
                .and_then(|size| {
                    next_capacity
                        .checked_mul(entry_bytes)
                        .and_then(|entries| size.checked_add(entries))
                })
                .ok_or(ArchiveError::IntegerOverflow)?;
            let slots_full = self.limits.max_cached_blocks != 0
                && self.cache.len() >= self.limits.max_cached_blocks;
            if required <= self.limits.cache_bytes && !slots_full {
                break;
            }
            let mut oldest = None;
            for (position, candidate) in self.cache.iter().enumerate() {
                work.step()?;
                if oldest.is_none_or(|(_, stamp)| candidate.stamp < stamp) {
                    oldest = Some((position, candidate.stamp));
                }
            }
            let oldest = oldest
                .map(|(index, _)| index)
                .ok_or(ArchiveError::Invalid("cache accounting"))?;
            let removed = self.cache.remove(oldest);
            self.cached_payload_bytes -= removed.bytes.capacity();
        }
        self.cache
            .try_reserve_exact(1)
            .map_err(|_| ArchiveError::Allocation)?;
        if self.cached_payload_bytes + bytes.capacity() + self.cache.capacity() * entry_bytes
            > self.limits.cache_bytes
        {
            // An allocator may provide more capacity than requested. Preserve
            // the byte contract by returning this block without caching it.
            self.cache = Vec::new();
            self.cached_payload_bytes = 0;
            self.uncached = bytes;
            return Ok(&self.uncached);
        }
        let position = self
            .cache
            .binary_search_by_key(&index, |block| block.index)
            .unwrap_err();
        self.cached_payload_bytes += bytes.capacity();
        self.metrics.peak_cached_payload_bytes = self
            .metrics
            .peak_cached_payload_bytes
            .max(self.cached_payload_bytes);
        self.cache.insert(
            position,
            CachedBlock {
                index,
                stamp: self.clock,
                bytes,
            },
        );
        Ok(&self.cache[position].bytes)
    }
}
