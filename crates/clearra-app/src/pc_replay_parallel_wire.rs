//! Closed transport for the existing replay source, cells and projected page.
//! Allocation is admitted before every reserve; wire input never mints a
//! complete-path owner, which remains in PcReplaySourceBuildSession.
use super::*;
use crate::pc_path_result::{PcPathBoardMask, PcPathStepV2};
use clearra_core_domain::{
    board::standard_pc_board::Board256Mask,
    piece::{piece_kind::PieceKind, rotation::RotationState},
    solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
};
use clearra_geometry::layout::board64_layout::Board64Layout;
use clearra_replay::{
    ExactScoringExecutionGraph, FullHeightExecutionBatch, RotationRequest, ScoringExecutionEdge,
    ScoringExecutionNode, ScoringLockEvidence, SpinCoverageExecutionBatch,
    SpinCoverageExecutionGraph,
};

const MAGIC: &[u8; 8] = b"CRRPC001";
const MAX_BYTES: usize = 512 * 1024 * 1024;
const HOLD: [&str; 4] = [
    "none",
    "swap-with-hold",
    "store-incoming",
    "release-held-at-terminal",
];

struct Writer {
    bytes: Option<Vec<u8>>,
    length: usize,
    hash: Option<Sha256>,
}
impl Writer {
    fn counting() -> Self {
        Self {
            bytes: None,
            length: 0,
            hash: None,
        }
    }
    fn put(&mut self, bytes: &[u8]) -> ReplayResult<()> {
        self.length = self.length.checked_add(bytes.len()).ok_or(OVERFLOW)?;
        if self.length > MAX_BYTES - 32 {
            return Err("complete_replay_parallel_packet_too_large".into());
        }
        if let Some(output) = &mut self.bytes {
            output.extend_from_slice(bytes);
        }
        if let Some(hash) = &mut self.hash {
            hash.update(bytes);
        }
        Ok(())
    }
    fn byte(&mut self, value: u8) -> ReplayResult<()> {
        self.put(&[value])
    }
    fn u16(&mut self, value: u16) -> ReplayResult<()> {
        self.put(&value.to_le_bytes())
    }
    fn u32(&mut self, value: u32) -> ReplayResult<()> {
        self.put(&value.to_le_bytes())
    }
    fn u64(&mut self, value: u64) -> ReplayResult<()> {
        self.put(&value.to_le_bytes())
    }
    fn count(&mut self, value: usize) -> ReplayResult<()> {
        self.u64(u64::try_from(value).map_err(|_| OVERFLOW)?)
    }
    fn piece(&mut self, value: Option<PieceKind>) -> ReplayResult<()> {
        self.byte(value.map_or(0, |p| p.as_ascii() as u8))
    }
    fn string(&mut self, value: &str) -> ReplayResult<()> {
        self.count(value.len())?;
        self.put(value.as_bytes())
    }
    fn board(&mut self, board: PcPathBoardMask) -> ReplayResult<()> {
        self.byte(match board {
            PcPathBoardMask::Compact(_) => 0,
            PcPathBoardMask::FullHeight { height, .. } => height,
        })?;
        for word in board.words() {
            self.u64(word)?;
        }
        Ok(())
    }
}

fn encode(
    kind: u8,
    size: usize,
    guard: &mut impl FnMut(u128) -> bool,
    write: impl FnOnce(&mut Writer) -> ReplayResult<()>,
) -> ReplayResult<Vec<u8>> {
    if size > MAX_BYTES || !guard(size as u128) {
        return Err(PcReplayPageError::HostMemoryLimit {
            required: size as u128,
        });
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(|_| "complete_replay_allocation_failed")?;
    if !guard(bytes.capacity() as u128) {
        return Err(PcReplayPageError::HostMemoryLimit {
            required: bytes.capacity() as u128,
        });
    }
    let mut writer = Writer {
        bytes: Some(bytes),
        length: 0,
        hash: None,
    };
    writer.put(MAGIC)?;
    writer.byte(kind)?;
    write(&mut writer)?;
    let mut bytes = writer.bytes.expect("encoding output");
    let hash = Sha256::digest(&bytes);
    bytes.extend_from_slice(&hash);
    if bytes.len() != size {
        return Err(mismatch());
    }
    Ok(bytes)
}
fn size(kind: u8, write: impl FnOnce(&mut Writer) -> ReplayResult<()>) -> ReplayResult<usize> {
    let mut writer = Writer::counting();
    writer.put(MAGIC)?;
    writer.byte(kind)?;
    write(&mut writer)?;
    writer.length.checked_add(32).ok_or_else(|| OVERFLOW.into())
}

struct Reader<'a, 'g> {
    bytes: &'a [u8],
    position: usize,
    owned: u128,
    guard: &'g mut dyn FnMut(u128) -> bool,
}
impl<'a, 'g> Reader<'a, 'g> {
    fn new(
        bytes: &'a [u8],
        kind: u8,
        guard: &'g mut dyn FnMut(u128) -> bool,
    ) -> ReplayResult<Self> {
        if bytes.len() < 41
            || bytes.len() > MAX_BYTES
            || bytes.get(..8) != Some(MAGIC)
            || bytes[8] != kind
        {
            return Err(mismatch());
        }
        let length = bytes.len() - 32;
        if Sha256::digest(&bytes[..length]).as_slice() != &bytes[length..] {
            return Err(mismatch());
        }
        Ok(Self {
            bytes: &bytes[..length],
            position: 9,
            owned: 0,
            guard,
        })
    }
    fn take(&mut self, length: usize) -> ReplayResult<&'a [u8]> {
        let end = self.position.checked_add(length).ok_or(OVERFLOW)?;
        let bytes = self.bytes.get(self.position..end).ok_or_else(mismatch)?;
        self.position = end;
        Ok(bytes)
    }
    fn byte(&mut self) -> ReplayResult<u8> {
        Ok(self.take(1)?[0])
    }
    fn bool(&mut self) -> ReplayResult<bool> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(mismatch()),
        }
    }
    fn u16(&mut self) -> ReplayResult<u16> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("fixed u16"),
        ))
    }
    fn u32(&mut self) -> ReplayResult<u32> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("fixed u32"),
        ))
    }
    fn u64(&mut self) -> ReplayResult<u64> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().expect("fixed u64"),
        ))
    }
    fn count(&mut self) -> ReplayResult<usize> {
        usize::try_from(self.u64()?).map_err(|_| OVERFLOW.into())
    }
    fn piece(&mut self) -> ReplayResult<Option<PieceKind>> {
        match self.byte()? {
            0 => Ok(None),
            value => PieceKind::from_ascii(value as char)
                .map(Some)
                .map_err(|_| mismatch()),
        }
    }
    fn required_piece(&mut self) -> ReplayResult<PieceKind> {
        self.piece()?.ok_or_else(mismatch)
    }
    fn admit(&mut self, bytes: u128) -> ReplayResult<()> {
        self.owned = self.owned.checked_add(bytes).ok_or(OVERFLOW)?;
        if !(self.guard)(self.owned) {
            return Err(PcReplayPageError::HostMemoryLimit {
                required: self.owned,
            });
        }
        Ok(())
    }
    fn vec<T>(&mut self, count: usize, minimum_wire_item: usize) -> ReplayResult<Vec<T>> {
        if count > self.bytes.len().saturating_sub(self.position) / minimum_wire_item.max(1) {
            return Err(mismatch());
        }
        self.admit(bytes::<T>(count)?)?;
        let mut values = Vec::new();
        values
            .try_reserve_exact(count)
            .map_err(|_| "complete_replay_allocation_failed")?;
        self.admit(bytes::<T>(values.capacity().saturating_sub(count))?)?;
        Ok(values)
    }
    fn string(&mut self) -> ReplayResult<String> {
        let length = self.count()?;
        let input = core::str::from_utf8(self.take(length)?).map_err(|_| mismatch())?;
        self.admit(length as u128)?;
        let mut output = String::new();
        output
            .try_reserve_exact(length)
            .map_err(|_| "complete_replay_allocation_failed")?;
        self.admit(output.capacity().saturating_sub(length) as u128)?;
        output.push_str(input);
        Ok(output)
    }
    fn board(&mut self) -> ReplayResult<PcPathBoardMask> {
        let height = self.byte()?;
        let words = [self.u64()?, self.u64()?, self.u64()?, self.u64()?];
        match height {
            0 if words[1..] == [0; 3] => Ok(PcPathBoardMask::Compact(words[0])),
            7..=24 => {
                let occupied = Board256Mask::from_words(words);
                clearra_replay::FullHeightReplayProjector::validate_board(height, occupied)
                    .map_err(|_| mismatch())?;
                Ok(PcPathBoardMask::FullHeight { height, occupied })
            }
            _ => Err(mismatch()),
        }
    }
    fn finish(self) -> ReplayResult<()> {
        if self.position == self.bytes.len() {
            Ok(())
        } else {
            Err(mismatch())
        }
    }
}

fn write_edge(w: &mut Writer, edge: ScoringExecutionEdge) -> ReplayResult<()> {
    w.u32(edge.to())?;
    w.byte(edge.operation_index())?;
    w.piece(Some(edge.piece()))?;
    w.byte(edge.rotation().quarter_turns())?;
    w.byte(edge.x() as u8)?;
    w.byte(edge.y() as u8)?;
    w.byte(edge.cleared_lines())?;
    w.byte(edge.blocked_t_corners())?;
    w.byte(edge.blocked_t_front_corners())?;
    let lock = edge.lock_evidence();
    w.byte(lock.last_action_was_rotation() as u8)?;
    w.byte(lock.from_rotation().quarter_turns())?;
    w.byte(match lock.rotation_request() {
        RotationRequest::None => 0,
        RotationRequest::Clockwise => 1,
        RotationRequest::CounterClockwise => 2,
        RotationRequest::HalfTurn => 3,
    })?;
    w.byte(lock.kick_index())?;
    w.byte(lock.kick_dx() as u8)?;
    w.byte(lock.kick_dy() as u8)?;
    w.byte(lock.predecessor().0 as u8)?;
    w.byte(lock.predecessor().1 as u8)?;
    w.byte(lock.immobile_before_clear() as u8)?;
    w.byte(edge.perfect_clear() as u8)
}
fn read_edge(r: &mut Reader<'_, '_>) -> ReplayResult<ScoringExecutionEdge> {
    let to = r.u32()?;
    let operation = r.byte()?;
    let piece = r.required_piece()?;
    let rotation = RotationState::from_quarter_turns(r.byte()?).map_err(|_| mismatch())?;
    let x = r.byte()? as i8;
    let y = r.byte()? as i8;
    let cleared = r.byte()?;
    let corners = r.byte()?;
    let front = r.byte()?;
    let rotated = r.bool()?;
    let from = RotationState::from_quarter_turns(r.byte()?).map_err(|_| mismatch())?;
    let request = match r.byte()? {
        0 => RotationRequest::None,
        1 => RotationRequest::Clockwise,
        2 => RotationRequest::CounterClockwise,
        3 => RotationRequest::HalfTurn,
        _ => return Err(mismatch()),
    };
    let kick = r.byte()?;
    let dx = r.byte()? as i8;
    let dy = r.byte()? as i8;
    let px = r.byte()? as i8;
    let py = r.byte()? as i8;
    let immobile = r.bool()?;
    let pc = r.bool()?;
    let lock = if rotated {
        if matches!(request, RotationRequest::None) {
            return Err(mismatch());
        }
        ScoringLockEvidence::rotation(from, request, kick, dx, dy, px, py)
    } else {
        if !matches!(request, RotationRequest::None)
            || kick != 0
            || dx != 0
            || dy != 0
            || px != 0
            || py != 0
        {
            return Err(mismatch());
        }
        ScoringLockEvidence::no_rotation(from)
    }
    .with_immobile_before_clear(immobile);
    Ok(ScoringExecutionEdge::new(
        to, operation, piece, rotation, x, y, cleared, corners, front, lock,
    )
    .with_perfect_clear(pc))
}
fn write_nodes(
    w: &mut Writer,
    count: usize,
    node: impl Fn(u32) -> Option<ScoringExecutionNode>,
    edges: &[ScoringExecutionEdge],
) -> ReplayResult<()> {
    w.count(count)?;
    for i in 0..count {
        let n = node(u32::try_from(i).map_err(|_| OVERFLOW)?).ok_or_else(mismatch)?;
        w.u32(n.edge_start())?;
        w.u32(n.edge_count())?;
        w.byte(n.accepting() as u8)?;
    }
    w.count(edges.len())?;
    for &edge in edges {
        write_edge(w, edge)?;
    }
    Ok(())
}
fn read_nodes(
    r: &mut Reader<'_, '_>,
    root: u32,
) -> ReplayResult<(Vec<ScoringExecutionNode>, Vec<ScoringExecutionEdge>)> {
    let count = r.count()?;
    let mut nodes = r.vec(count, 9)?;
    for _ in 0..count {
        nodes.push(ScoringExecutionNode::new(r.u32()?, r.u32()?, r.bool()?));
    }
    let count = r.count()?;
    let mut edges = r.vec(count, 22)?;
    for _ in 0..count {
        edges.push(read_edge(r)?);
    }
    if nodes.get(root as usize).is_none() {
        return Err(mismatch());
    }
    for (index, node) in nodes.iter().enumerate() {
        let last = (node.edge_start() as usize)
            .checked_add(node.edge_count() as usize)
            .ok_or(OVERFLOW)?;
        let span = edges
            .get(node.edge_start() as usize..last)
            .ok_or_else(mismatch)?;
        if node.accepting() && !span.is_empty()
            || span
                .iter()
                .any(|edge| edge.to() as usize <= index || edge.to() as usize >= nodes.len())
        {
            return Err(mismatch());
        }
    }
    Ok((nodes, edges))
}

#[allow(clippy::too_many_arguments)]
fn write_header(
    w: &mut Writer,
    patterns: &[Vec<PieceKind>],
    cursor: u16,
    hold: Option<PieceKind>,
    enabled: bool,
    lookahead: bool,
    standard: bool,
    kick: u64,
    rules: u64,
    complete: bool,
) -> ReplayResult<()> {
    w.count(patterns.len())?;
    for pattern in patterns {
        w.count(pattern.len())?;
        for &piece in pattern {
            w.piece(Some(piece))?;
        }
    }
    w.u16(cursor)?;
    w.piece(hold)?;
    w.byte(enabled as u8)?;
    w.byte(lookahead as u8)?;
    w.byte(standard as u8)?;
    w.u64(kick)?;
    w.u64(rules)?;
    w.byte(complete as u8)
}
struct Header {
    patterns: Vec<Vec<PieceKind>>,
    cursor: u16,
    hold: Option<PieceKind>,
    enabled: bool,
    lookahead: bool,
    standard: bool,
    kick: u64,
    rules: u64,
}
fn read_header(
    r: &mut Reader<'_, '_>,
    expected_patterns: usize,
    projection: PcPathProjectionContext,
) -> ReplayResult<Header> {
    let count = r.count()?;
    if count == 0 || count != expected_patterns {
        return Err(mismatch());
    }
    let mut patterns = r.vec(count, 8)?;
    for _ in 0..count {
        let length = r.count()?;
        let mut pattern = r.vec(length, 1)?;
        for _ in 0..length {
            pattern.push(r.required_piece()?);
        }
        patterns.push(pattern);
    }
    let cursor = r.u16()?;
    let hold = r.piece()?;
    let enabled = r.bool()?;
    let lookahead = r.bool()?;
    let standard = r.bool()?;
    let kick = r.u64()?;
    let rules = r.u64()?;
    if !r.bool()?
        || kick == 0
        || rules == 0
        || usize::from(cursor) != projection.initial_cursor
        || hold != projection.initial_hold
    {
        return Err(mismatch());
    }
    Ok(Header {
        patterns,
        cursor,
        hold,
        enabled,
        lookahead,
        standard,
        kick,
        rules,
    })
}
fn write_query(w: &mut Writer, q: &PcReplayParallelQuery) -> ReplayResult<()> {
    w.put(&q.identity)?;
    write_query_source(w, q)
}

pub(super) fn source_identity(q: &PcReplayParallelQuery) -> ReplayResult<[u8; 32]> {
    let mut writer = Writer {
        bytes: None,
        length: 0,
        hash: Some(Sha256::new()),
    };
    writer.put(b"clearra.replay-completion-source.v1\0")?;
    write_query_source(&mut writer, q)?;
    Ok(writer
        .hash
        .expect("source identity writer")
        .finalize()
        .into())
}

fn write_query_source(w: &mut Writer, q: &PcReplayParallelQuery) -> ReplayResult<()> {
    w.put(&q.owner_identity)?;
    w.u64(q.generation)?;
    w.count(q.patterns)?;
    w.board(q.projection.initial_board)?;
    w.count(q.projection.initial_cursor)?;
    w.piece(q.projection.initial_hold)?;
    match &q.batches {
        ReplayBatches::Compact(batches) => {
            w.byte(0)?;
            w.count(batches.len())?;
            for b in batches.iter() {
                w.byte(u8::try_from(b.layout().height()).map_err(|_| OVERFLOW)?)?;
                w.u64(b.initial_occupied())?;
                write_header(
                    w,
                    b.patterns(),
                    b.initial_cursor(),
                    b.initial_hold(),
                    b.hold_enabled(),
                    b.projects_unplaced_lookahead(),
                    b.projects_standard_bag_lookahead(),
                    b.kick_table_id(),
                    b.rule_profile_id(),
                    b.complete(),
                )?;
                w.count(b.graphs().len())?;
                for g in b.graphs() {
                    let identity = g.identity();
                    w.u64(g.candidate_id())?;
                    w.u64(identity.initial_board_mask())?;
                    w.u64(identity.packed_piece_codes())?;
                    w.count(identity.placement_count())?;
                    for &mask in identity.placement_masks() {
                        w.u64(mask)?;
                    }
                    w.u32(g.root())?;
                    write_nodes(w, g.node_count(), |i| g.node(i), g.all_edges())?;
                }
            }
        }
        ReplayBatches::FullHeight(batches) => {
            w.byte(1)?;
            w.count(batches.len())?;
            for b in batches.iter() {
                w.byte(b.height())?;
                for word in b.initial().words() {
                    w.u64(word)?;
                }
                let e = b.execution();
                write_header(
                    w,
                    e.patterns(),
                    e.initial_cursor(),
                    e.initial_hold(),
                    e.hold_enabled(),
                    e.projects_unplaced_lookahead(),
                    e.projects_standard_bag_lookahead(),
                    e.kick_table_id(),
                    e.rule_profile_id(),
                    e.complete(),
                )?;
                w.count(e.graphs().len())?;
                for g in e.graphs() {
                    w.u64(g.candidate_id())?;
                    w.string(g.candidate_key())?;
                    w.u32(g.root())?;
                    write_nodes(w, g.node_count(), |i| g.node(i), g.all_edges())?;
                }
            }
        }
    }
    w.count(q.groups.len())?;
    for (candidate, locations) in q.groups.iter() {
        w.u64(*candidate)?;
        w.count(locations.len())?;
        for location in locations {
            w.count(location.batch)?;
            w.count(location.graph)?;
        }
    }
    Ok(())
}
pub(super) fn query_size(q: &PcReplayParallelQuery) -> ReplayResult<usize> {
    size(1, |w| write_query(w, q))
}
pub(super) fn encode_query(
    q: &PcReplayParallelQuery,
    guard: &mut impl FnMut(u128) -> bool,
) -> ReplayResult<Vec<u8>> {
    encode(1, query_size(q)?, guard, |w| write_query(w, q))
}

pub(super) fn decode_query(
    input: &[u8],
    guard: &mut impl FnMut(u128) -> bool,
) -> ReplayResult<PcReplayParallelQuery> {
    let mut r = Reader::new(input, 1, guard)?;
    r.admit(core::mem::size_of::<PcReplayParallelQuery>() as u128)?;
    let identity = r.take(32)?.try_into().expect("fixed identity");
    let owner_identity = r.take(32)?.try_into().expect("fixed owner identity");
    let generation = r.u64()?;
    let patterns = r.count()?;
    if generation == 0 || patterns == 0 {
        return Err(mismatch());
    }
    let projection = PcPathProjectionContext {
        initial_board: r.board()?,
        initial_cursor: r.count()?,
        initial_hold: r.piece()?,
    };
    let kind = r.byte()?;
    let count = r.count()?;
    let batches = match kind {
        0 => {
            let mut batches = r.vec::<ExactScoringExecutionBatch>(count, 38)?;
            for _ in 0..count {
                let height = r.byte()?;
                let initial = r.u64()?;
                let layout = Board64Layout::standard_10_by_lines(height).map_err(|_| mismatch())?;
                if projection.initial_board.compact() != Some(initial) {
                    return Err(mismatch());
                }
                let h = read_header(&mut r, patterns, projection)?;
                let count = r.count()?;
                let mut graphs = r.vec(count, 36)?;
                for _ in 0..count {
                    let candidate = r.u64()?;
                    let initial = r.u64()?;
                    let codes = r.u64()?;
                    let count = r.count()?;
                    let mut masks = r.vec(count, 8)?;
                    for _ in 0..count {
                        masks.push(r.u64()?);
                    }
                    let identity =
                        StandardBoard64TilingIdentity::from_compact_parts(initial, codes, &masks)
                            .map_err(|_| mismatch())?;
                    let root = r.u32()?;
                    let (nodes, edges) = read_nodes(&mut r, root)?;
                    graphs.push(ExactScoringExecutionGraph::new(
                        candidate, identity, root, nodes, edges,
                    ));
                }
                batches.push(ExactScoringExecutionBatch::new(
                    layout,
                    initial,
                    h.patterns,
                    h.cursor,
                    h.hold,
                    h.enabled,
                    h.lookahead,
                    h.standard,
                    h.kick,
                    h.rules,
                    graphs,
                    true,
                ));
            }
            r.admit(
                bytes::<ExactScoringExecutionBatch>(batches.len())?
                    .checked_add(16)
                    .ok_or(OVERFLOW)?,
            )?;
            ReplayBatches::Compact(batches.into())
        }
        1 => {
            let mut batches = r.vec::<FullHeightExecutionBatch>(count, 62)?;
            for _ in 0..count {
                let height = r.byte()?;
                let initial = Board256Mask::from_words([r.u64()?, r.u64()?, r.u64()?, r.u64()?]);
                if projection.initial_board
                    != (PcPathBoardMask::FullHeight {
                        height,
                        occupied: initial,
                    })
                {
                    return Err(mismatch());
                }
                let h = read_header(&mut r, patterns, projection)?;
                let count = r.count()?;
                let mut graphs = r.vec(count, 28)?;
                for _ in 0..count {
                    let candidate = r.u64()?;
                    let key = r.string()?;
                    if key.is_empty() {
                        return Err(mismatch());
                    }
                    let root = r.u32()?;
                    let (nodes, edges) = read_nodes(&mut r, root)?;
                    graphs.push(SpinCoverageExecutionGraph::new(
                        candidate, key, root, nodes, edges,
                    ));
                }
                let execution = SpinCoverageExecutionBatch::new(
                    h.patterns,
                    h.cursor,
                    h.hold,
                    h.enabled,
                    h.lookahead,
                    h.standard,
                    h.kick,
                    h.rules,
                    graphs,
                    true,
                );
                batches.push(
                    FullHeightExecutionBatch::from_spin_coverage(height, initial, execution)
                        .map_err(|_| mismatch())?,
                );
            }
            r.admit(
                bytes::<FullHeightExecutionBatch>(batches.len())?
                    .checked_add(16)
                    .ok_or(OVERFLOW)?,
            )?;
            ReplayBatches::FullHeight(batches.into())
        }
        _ => return Err(mismatch()),
    };
    let count = r.count()?;
    let mut groups = r.vec(count, 16)?;
    let mut previous = None;
    for _ in 0..count {
        let candidate = r.u64()?;
        if previous.is_some_and(|p| p >= candidate) {
            return Err(mismatch());
        }
        previous = Some(candidate);
        let count = r.count()?;
        if count == 0 {
            return Err(mismatch());
        }
        let mut locations = r.vec(count, 16)?;
        for _ in 0..count {
            let location = ExactReplayGraphLocation {
                batch: r.count()?,
                graph: r.count()?,
            };
            let source_candidate = match &batches {
                ReplayBatches::Compact(b) => b
                    .get(location.batch)
                    .and_then(|b| b.graphs().get(location.graph))
                    .map(|g| g.candidate_id()),
                ReplayBatches::FullHeight(b) => b
                    .get(location.batch)
                    .and_then(|b| b.execution().graphs().get(location.graph))
                    .map(|g| g.candidate_id()),
            };
            if source_candidate != Some(candidate)
                || locations
                    .last()
                    .is_some_and(|p: &ExactReplayGraphLocation| {
                        (p.batch, p.graph) >= (location.batch, location.graph)
                    })
            {
                return Err(mismatch());
            }
            locations.push(location);
        }
        groups.push((candidate, locations));
    }
    let graph_count = match &batches {
        ReplayBatches::Compact(b) => b.iter().map(|b| b.graphs().len()).sum::<usize>(),
        ReplayBatches::FullHeight(b) => b
            .iter()
            .map(|b| b.execution().graphs().len())
            .sum::<usize>(),
    };
    if groups.iter().map(|(_, l)| l.len()).sum::<usize>() != graph_count {
        return Err(mismatch());
    }
    r.admit(
        bytes::<(u64, Vec<ExactReplayGraphLocation>)>(groups.len())?
            .checked_add(16)
            .ok_or(OVERFLOW)?,
    )?;
    let query = PcReplayParallelQuery {
        identity,
        owner_identity,
        generation,
        batches,
        groups: groups.into(),
        projection,
        patterns,
    };
    if source_identity(&query)? != query.identity {
        return Err(mismatch());
    }
    let actual = query.checked_retained_bytes().ok_or(OVERFLOW)?;
    if !(r.guard)(actual.max(r.owned)) {
        return Err(PcReplayPageError::HostMemoryLimit {
            required: actual.max(r.owned),
        });
    }
    r.finish()?;
    Ok(query)
}

fn write_task(w: &mut Writer, t: &PcReplayParallelTask) -> ReplayResult<()> {
    w.put(&t.identity)?;
    w.u64(t.generation)?;
    w.u64(t.kind)?;
    w.count(t.ordinal)?;
    w.count(t.first)?;
    w.count(t.length)
}
fn read_task(r: &mut Reader<'_, '_>) -> ReplayResult<PcReplayParallelTask> {
    let task = PcReplayParallelTask {
        identity: r.take(32)?.try_into().expect("fixed identity"),
        generation: r.u64()?,
        kind: r.u64()?,
        ordinal: r.count()?,
        first: r.count()?,
        length: r.count()?,
    };
    if task.generation == 0 || !matches!(task.kind, COUNT | PAGE) {
        return Err(mismatch());
    }
    Ok(task)
}
pub(super) fn encode_task(task: &PcReplayParallelTask) -> Vec<u8> {
    encode(
        2,
        size(2, |w| write_task(w, task)).expect("fixed task size"),
        &mut |_| true,
        |w| write_task(w, task),
    )
    .expect("fixed task fits")
}
pub(super) fn decode_task(bytes: &[u8]) -> ReplayResult<PcReplayParallelTask> {
    let mut guard = |_| true;
    let mut r = Reader::new(bytes, 2, &mut guard)?;
    let task = read_task(&mut r)?;
    r.finish()?;
    Ok(task)
}

fn write_member(w: &mut Writer, m: &PcPathWitnessV2) -> ReplayResult<()> {
    w.u64(m.candidate_id)?;
    w.u64(m.producer_candidate_id)?;
    w.count(m.pattern_id)?;
    w.string(&m.trace_identity)?;
    w.string(&m.normalized_trace_key)?;
    w.count(m.consumed_piece_count)?;
    w.piece(m.terminal_hold_piece)?;
    w.count(m.steps.len())?;
    for s in &m.steps {
        w.count(s.step_index)?;
        w.u16(s.operation_id)?;
        w.piece(Some(s.active_piece))?;
        w.count(s.input_cursor)?;
        w.count(s.output_cursor)?;
        w.piece(s.input_hold_piece)?;
        w.piece(s.output_hold_piece)?;
        w.byte(
            u8::try_from(
                HOLD.iter()
                    .position(|&value| value == s.hold_decision)
                    .ok_or_else(mismatch)?,
            )
            .map_err(|_| OVERFLOW)?,
        )?;
        w.byte(s.rotation)?;
        w.u16(s.x)?;
        w.u16(s.y)?;
        w.board(s.placement_mask)?;
        w.board(s.board_before_mask)?;
        w.board(s.board_after_placement_mask)?;
        w.board(s.board_after_line_clear_mask)?;
        w.u64(s.cleared_row_mask)?;
        w.byte(s.cleared_lines)?;
        w.string(&s.line_clear_identity)?;
    }
    Ok(())
}
fn read_member(r: &mut Reader<'_, '_>) -> ReplayResult<PcPathWitnessV2> {
    let candidate_id = r.u64()?;
    let producer_candidate_id = r.u64()?;
    let pattern_id = r.count()?;
    let trace_identity = r.string()?;
    let normalized_trace_key = r.string()?;
    let consumed_piece_count = r.count()?;
    let terminal_hold_piece = r.piece()?;
    let count = r.count()?;
    if count > 60 || trace_identity.is_empty() || trace_identity != normalized_trace_key {
        return Err(mismatch());
    }
    let mut steps = r.vec(count, 170)?;
    for index in 0..count {
        let step = PcPathStepV2 {
            step_index: r.count()?,
            operation_id: r.u16()?,
            active_piece: r.required_piece()?,
            input_cursor: r.count()?,
            output_cursor: r.count()?,
            input_hold_piece: r.piece()?,
            output_hold_piece: r.piece()?,
            hold_decision: *HOLD.get(r.byte()? as usize).ok_or_else(mismatch)?,
            rotation: r.byte()?,
            x: r.u16()?,
            y: r.u16()?,
            placement_mask: r.board()?,
            board_before_mask: r.board()?,
            board_after_placement_mask: r.board()?,
            board_after_line_clear_mask: r.board()?,
            cleared_row_mask: r.u64()?,
            cleared_lines: r.byte()?,
            line_clear_identity: r.string()?,
        };
        if step.step_index != index
            || step.rotation > 3
            || step.x >= 10
            || step.y >= 24
            || steps.last().is_some_and(|previous: &PcPathStepV2| {
                previous.board_after_line_clear_mask != step.board_before_mask
                    || previous.output_cursor != step.input_cursor
                    || previous.output_hold_piece != step.input_hold_piece
            })
        {
            return Err(mismatch());
        }
        steps.push(step);
    }
    if steps.last().is_some_and(|s| {
        !s.board_after_line_clear_mask.is_empty()
            || s.output_hold_piece != terminal_hold_piece
            || s.output_cursor != consumed_piece_count
    }) {
        return Err(mismatch());
    }
    Ok(PcPathWitnessV2 {
        candidate_id,
        producer_candidate_id,
        pattern_id,
        trace_identity,
        normalized_trace_key,
        consumed_piece_count,
        terminal_hold_piece,
        steps,
    })
}
fn write_receipt(w: &mut Writer, r: &PcReplayParallelReceipt) -> ReplayResult<()> {
    write_task(w, &r.task)?;
    w.byte(r.count.is_some() as u8)?;
    w.count(r.count.unwrap_or(0))?;
    w.count(r.members.len())?;
    for member in &r.members {
        write_member(w, member)?;
    }
    Ok(())
}
pub(super) fn encode_receipt(
    r: &PcReplayParallelReceipt,
    guard: &mut impl FnMut(u128) -> bool,
) -> ReplayResult<Vec<u8>> {
    encode(3, size(3, |w| write_receipt(w, r))?, guard, |w| {
        write_receipt(w, r)
    })
}
pub(super) fn decode_receipt(
    bytes: &[u8],
    guard: &mut impl FnMut(u128) -> bool,
) -> ReplayResult<PcReplayParallelReceipt> {
    let mut r = Reader::new(bytes, 3, guard)?;
    r.admit(core::mem::size_of::<PcReplayParallelReceipt>() as u128)?;
    let task = read_task(&mut r)?;
    let complete = r.bool()?;
    let count = r.count()?;
    let length = r.count()?;
    if length > PC_REPLAY_MEMBER_PAGE_SIZE
        || (!complete && (count != 0 || length != 0))
        || (task.kind == COUNT && length != 0)
        || (task.kind == PAGE && complete && length != task.length)
    {
        return Err(mismatch());
    }
    let mut members = r.vec(length, 57)?;
    for _ in 0..length {
        members.push(read_member(&mut r)?);
    }
    r.finish()?;
    Ok(PcReplayParallelReceipt {
        task,
        count: complete.then_some(count),
        members,
    })
}
