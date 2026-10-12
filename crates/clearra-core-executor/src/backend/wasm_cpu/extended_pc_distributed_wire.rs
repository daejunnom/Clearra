//! Closed, memory-preflighted raw PC worker transfer. Decoding this wire does
//! not construct any product evidence or a CoreExecutionResult. Its ordinals
//! must still be matched to the coordinator's complete admitted ILC stream.

use std::mem::size_of;

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_replay::{
    RotationRequest, ScoringExecutionEdge, ScoringExecutionNode, ScoringLockEvidence,
    SpinCoverageExecutionGraph,
};
use sha2::{Digest, Sha256};

use super::super::CorePathStep;
use super::{
    invalid, projection_error, receipt_backing, storage_error, ExtendedBuildProbabilitySession,
    ExtendedFamilyPurpose, ExtendedTilingKey, HashSet, PatternBitSet, SearchProblem,
    WasmCpuSearchError,
};

const MAGIC: &[u8; 8] = b"CPC24P01";
const EDGE_BYTES: usize = 22;
const HOLD: [&str; 8] = [
    "use-current",
    "swap-held",
    "store-current-use-next",
    "swap-held-with-unplaced-lookahead",
    "use-unplaced-lookahead",
    "store-current-use-unplaced-lookahead",
    "release-held-at-terminal",
    "none",
];

pub(super) struct Row {
    pub key: String,
    pub coverage: Option<PatternBitSet>,
}

pub(super) struct Partial {
    pub receipts: Vec<(u64, [u8; 32])>,
    pub rows: Vec<Row>,
    pub covered: PatternBitSet,
    pub graphs: Vec<SpinCoverageExecutionGraph>,
    stats: [usize; 8],
    witnessed: u128,
    variants: u128,
    variants_complete: bool,
    representative_rank: Option<u64>,
    representative_pattern: Option<u32>,
    path: Vec<CorePathStep>,
}

impl Partial {
    pub fn checked_retained_bytes(&self) -> Option<u128> {
        let mut bytes = receipt_backing(&self.receipts)?
            .checked_add(receipt_backing(&self.rows)?)?
            .checked_add(self.covered.checked_storage_retained_bytes()?)?
            .checked_add(receipt_backing(&self.graphs)?)?
            .checked_add(receipt_backing(&self.path)?)?;
        for row in &self.rows {
            bytes = bytes.checked_add(row.key.capacity() as u128)?;
            if let Some(bits) = &row.coverage {
                bytes = bytes.checked_add(bits.checked_storage_retained_bytes()?)?;
            }
        }
        for graph in &self.graphs {
            bytes = bytes.checked_add(graph.checked_nested_retained_bytes()?)?;
        }
        Some(bytes)
    }

    pub fn merge_future_bytes(&self) -> Option<u128> {
        let mut bytes = (self.receipts.len() as u128)
            .checked_mul((4 * (size_of::<[u8; 32]>() + 1)) as u128)?
            .checked_add((self.rows.len() as u128).checked_mul(
                (5 * (size_of::<ExtendedTilingKey>()
                    + size_of::<String>()
                    + size_of::<PatternBitSet>()
                    + 1)) as u128,
            )?)?
            .checked_add(
                (self.graphs.len() as u128)
                    .checked_mul(size_of::<SpinCoverageExecutionGraph>() as u128)?,
            )?
            .checked_add(
                self.covered
                    .checked_storage_retained_bytes()?
                    .checked_mul(4)?,
            )?;
        for row in &self.rows {
            // Decoded key, catalog row IDs, reconstructed tiling and parent key
            // coexist. At most 60 placements make this a bounded projection.
            bytes = bytes.checked_add((row.key.len() as u128).checked_mul(3)?)?;
            if let Some(bits) = &row.coverage {
                bytes =
                    bytes.checked_add(bits.checked_storage_retained_bytes()?.checked_mul(2)?)?;
            }
        }
        Some(bytes)
    }

    pub fn validate_union(
        &self,
        engine: &ExtendedBuildProbabilitySession,
    ) -> Result<(), WasmCpuSearchError> {
        if engine.purpose != ExtendedFamilyPurpose::Pc || !self.variants_complete {
            return Err(invalid("extended_pc_distributed_partial_incomplete"));
        }
        let mut keys = HashSet::new();
        keys.try_reserve(self.rows.len())
            .map_err(|_| storage_error())?;
        for row in &self.rows {
            if !keys.insert(row.key.as_str())
                || row.coverage.is_some() != engine.solution_coverage.is_some()
            {
                return Err(invalid("extended_pc_distributed_row_contract_mismatch"));
            }
        }
        if engine.solution_coverage.is_some() {
            let mut union = PatternBitSet::new(self.covered.pattern_count());
            for row in &self.rows {
                union
                    .union_with(
                        row.coverage.as_ref().ok_or_else(|| {
                            invalid("extended_pc_distributed_coverage_row_missing")
                        })?,
                    )
                    .map_err(|_| invalid("extended_pc_distributed_coverage_mismatch"))?;
            }
            if union != self.covered {
                return Err(invalid("extended_pc_distributed_coverage_union_mismatch"));
            }
        }
        if self.rows.is_empty() != self.covered.is_empty() {
            return Err(invalid(
                "extended_pc_distributed_solution_coverage_mismatch",
            ));
        }
        let physical_required = engine.problem.objective().score().requested()
            || engine
                .problem
                .pc_chance_evidence_policy()
                .retains_pc_path_v2_evidence();
        let physical_allowed = physical_required
            || engine
                .problem
                .objective()
                .execution_constraints()
                .requested();
        if !physical_allowed && !self.graphs.is_empty() {
            return Err(invalid(
                "extended_pc_distributed_unrequested_physical_graph",
            ));
        }
        let mut graph_keys = HashSet::new();
        graph_keys
            .try_reserve(self.graphs.len())
            .map_err(|_| storage_error())?;
        for graph in &self.graphs {
            if !keys.contains(graph.candidate_key())
                || !valid_graph(graph, engine.field.target_piece_count())
            {
                return Err(invalid("extended_pc_distributed_physical_graph_invalid"));
            }
            graph_keys.insert(graph.candidate_key());
        }
        if physical_required && graph_keys != keys {
            return Err(invalid(
                "extended_pc_distributed_physical_family_incomplete",
            ));
        }
        match (self.representative_rank, self.representative_pattern) {
            (Some(rank), Some(pattern))
                if !self.path.is_empty()
                    && self.receipts.iter().any(|receipt| receipt.0 == rank)
                    && (pattern as usize) < self.covered.pattern_count()
                    && self.covered.words()[pattern as usize / 64] & (1_u64 << (pattern & 63))
                        != 0 => {}
            (None, None) if self.path.is_empty() && self.rows.is_empty() => {}
            _ => return Err(invalid("extended_pc_distributed_representative_mismatch")),
        }
        Ok(())
    }

    pub fn apply(
        self,
        engine: &mut ExtendedBuildProbabilitySession,
    ) -> Result<(), WasmCpuSearchError> {
        engine.searched_build_nodes = engine
            .searched_build_nodes
            .checked_add(self.stats[0])
            .ok_or_else(projection_error)?;
        engine.reachability_states = engine
            .reachability_states
            .checked_add(self.stats[1])
            .ok_or_else(projection_error)?;
        engine.coverage_product_states = engine
            .coverage_product_states
            .checked_add(self.stats[2])
            .ok_or_else(projection_error)?;
        engine.coverage_product_edge_checks = engine
            .coverage_product_edge_checks
            .checked_add(self.stats[3])
            .ok_or_else(projection_error)?;
        engine.coverage_product_words = engine
            .coverage_product_words
            .checked_add(self.stats[4])
            .ok_or_else(projection_error)?;
        engine.peak_build_order_nodes = engine.peak_build_order_nodes.max(self.stats[5]);
        engine.total_build_order_nodes = engine
            .total_build_order_nodes
            .checked_add(self.stats[6])
            .ok_or_else(projection_error)?;
        engine.peak_build_scratch_bytes = engine.peak_build_scratch_bytes.max(self.stats[7]);
        engine.witnessed_pattern_count = engine
            .witnessed_pattern_count
            .checked_add(self.witnessed)
            .ok_or_else(projection_error)?;
        engine.pc_build_variant_count = engine
            .pc_build_variant_count
            .checked_add(self.variants)
            .ok_or_else(projection_error)?;
        engine.pc_build_variant_count_complete &= self.variants_complete;
        if self.representative_rank.is_some_and(|rank| {
            engine
                .representative_rank
                .is_none_or(|current| rank < current)
        }) {
            engine.representative_rank = self.representative_rank;
            engine.representative_pattern_id = self.representative_pattern;
            engine.representative_path = self.path;
        }
        engine.spin_execution_graphs.extend(self.graphs);
        Ok(())
    }
}

fn valid_graph(graph: &SpinCoverageExecutionGraph, pieces: usize) -> bool {
    if graph.node_count() > u32::MAX as usize || graph.node(graph.root()).is_none() {
        return false;
    }
    for index in 0..graph.node_count() as u32 {
        let Some(node) = graph.node(index) else {
            return false;
        };
        let Some(edges) = graph.checked_edges(node) else {
            return false;
        };
        if node.accepting() && !edges.is_empty() {
            return false;
        }
        for edge in edges {
            if edge.to() <= index
                || graph.node(edge.to()).is_none()
                || usize::from(edge.operation_index()) >= pieces
            {
                return false;
            }
        }
    }
    true
}

pub(super) fn encode(
    engine: &ExtendedBuildProbabilitySession,
    receipts: &[(u64, [u8; 32])],
    mut guard: impl FnMut(u128) -> Result<(), WasmCpuSearchError>,
) -> Result<Vec<u8>, WasmCpuSearchError> {
    let transient = engine
        .buildable_tilings
        .iter()
        .map(|tiling| tiling.canonical_key_len(engine.field.height()))
        .max()
        .unwrap_or(0) as u128;
    guard(transient)?;
    let mut measure = Writer {
        output: None,
        length: 0,
    };
    write(engine, receipts, &mut measure)?;
    guard(
        (measure.length as u128)
            .checked_add(transient)
            .ok_or_else(projection_error)?,
    )?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(measure.length)
        .map_err(|_| storage_error())?;
    guard(
        (output.capacity() as u128)
            .checked_add(transient)
            .ok_or_else(projection_error)?,
    )?;
    let mut writer = Writer {
        output: Some(&mut output),
        length: 0,
    };
    write(engine, receipts, &mut writer)?;
    if writer.length != measure.length {
        return Err(invalid("extended_pc_distributed_wire_length_mismatch"));
    }
    Ok(output)
}

fn write(
    engine: &ExtendedBuildProbabilitySession,
    receipts: &[(u64, [u8; 32])],
    w: &mut Writer<'_>,
) -> Result<(), WasmCpuSearchError> {
    w.bytes(MAGIC)?;
    w.bytes(&snapshot(&engine.problem)?)?;
    w.u64(engine.covered_patterns.pattern_count() as u64)?;
    w.count(receipts.len())?;
    for (ordinal, fingerprint) in receipts {
        w.u64(*ordinal)?;
        w.bytes(fingerprint)?;
    }
    w.count(engine.buildable_tilings.len())?;
    for tiling in &engine.buildable_tilings {
        let key = tiling.canonical_key(engine.catalog.initial_board(), engine.field.height());
        w.string(&key)?;
        match engine
            .solution_coverage
            .as_ref()
            .and_then(|rows| rows.get(&key))
        {
            Some(bits) => {
                w.u8(1)?;
                w.bits(bits)?;
            }
            None => {
                w.u8(0)?;
                if engine.solution_coverage.is_some() {
                    return Err(invalid("extended_pc_distributed_coverage_row_missing"));
                }
            }
        }
    }
    w.bits(&engine.covered_patterns)?;
    w.count(engine.spin_execution_graphs.len())?;
    for graph in &engine.spin_execution_graphs {
        w.string(graph.candidate_key())?;
        w.u32(graph.root())?;
        w.count(graph.node_count())?;
        for index in 0..graph.node_count() as u32 {
            let node = graph
                .node(index)
                .ok_or_else(|| invalid("extended_pc_distributed_physical_graph_invalid"))?;
            w.u32(node.edge_start())?;
            w.u32(node.edge_count())?;
            w.u8(u8::from(node.accepting()))?;
        }
        w.count(graph.all_edges().len())?;
        for edge in graph.all_edges() {
            write_edge(*edge, w)?;
        }
    }
    for value in [
        engine.searched_build_nodes,
        engine.reachability_states,
        engine.coverage_product_states,
        engine.coverage_product_edge_checks,
        engine.coverage_product_words,
        engine.peak_build_order_nodes,
        engine.total_build_order_nodes,
        engine.peak_build_scratch_bytes,
    ] {
        w.u64(value as u64)?;
    }
    w.bytes(&engine.witnessed_pattern_count.to_le_bytes())?;
    w.bytes(&engine.pc_build_variant_count.to_le_bytes())?;
    w.u8(u8::from(engine.pc_build_variant_count_complete))?;
    w.u8(u8::from(engine.representative_rank.is_some()))?;
    if let Some(rank) = engine.representative_rank {
        w.u64(rank)?;
    }
    w.u8(u8::from(engine.representative_pattern_id.is_some()))?;
    if let Some(pattern) = engine.representative_pattern_id {
        w.u32(pattern)?;
    }
    w.count(engine.representative_path.len())?;
    for step in &engine.representative_path {
        w.u8(step.piece().as_ascii() as u8)?;
        w.u8(step.rotation())?;
        w.bytes(&step.x().to_le_bytes())?;
        w.bytes(&step.y().to_le_bytes())?;
        let hold = HOLD
            .iter()
            .position(|text| *text == step.hold())
            .ok_or_else(|| invalid("extended_pc_distributed_hold_invalid"))?;
        w.u8(hold as u8)?;
        w.u8(step.cleared_lines())?;
    }
    Ok(())
}

fn write_edge(edge: ScoringExecutionEdge, w: &mut Writer<'_>) -> Result<(), WasmCpuSearchError> {
    let lock = edge.lock_evidence();
    if lock.first_success_confirmed() != lock.last_action_was_rotation() {
        return Err(invalid("extended_pc_distributed_ordered_kick_invalid"));
    }
    w.u32(edge.to())?;
    w.u8(edge.operation_index())?;
    w.u8(edge.piece().as_ascii() as u8)?;
    w.u8(edge.rotation().quarter_turns())?;
    w.u8(edge.x() as u8)?;
    w.u8(edge.y() as u8)?;
    w.u8(edge.cleared_lines())?;
    w.u8(edge.blocked_t_corners())?;
    w.u8(edge.blocked_t_front_corners())?;
    w.u8(u8::from(edge.perfect_clear()))?;
    w.u8(u8::from(lock.last_action_was_rotation()))?;
    w.u8(lock.from_rotation().quarter_turns())?;
    w.u8(match lock.rotation_request() {
        RotationRequest::None => 0,
        RotationRequest::Clockwise => 1,
        RotationRequest::CounterClockwise => 2,
        RotationRequest::HalfTurn => 3,
    })?;
    w.u8(lock.kick_index())?;
    w.u8(lock.kick_dx() as u8)?;
    w.u8(lock.kick_dy() as u8)?;
    w.u8(lock.predecessor().0 as u8)?;
    w.u8(lock.predecessor().1 as u8)?;
    w.u8(u8::from(lock.immobile_before_clear()))?;
    Ok(())
}

pub(super) fn decode(
    input: &[u8],
    problem: &SearchProblem,
    mut guard: impl FnMut(u128) -> Result<(), WasmCpuSearchError>,
) -> Result<Partial, WasmCpuSearchError> {
    // Inspect lengths using borrowed bytes before allocating any worker-owned
    // container. Counts cannot allocate on the strength of a 4-byte claim.
    let projection = decode_projection(input, problem)?;
    guard(projection)?;
    let mut r = Reader { input, cursor: 0 };
    read_header(&mut r, problem)?;
    let pattern_count = usize::try_from(r.u64()?).map_err(|_| projection_error())?;
    let receipt_count = r.count(40)?;
    let mut receipts = reserved(receipt_count)?;
    for _ in 0..receipt_count {
        receipts.push((r.u64()?, r.array()?));
    }
    if receipts.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
        return Err(invalid("extended_pc_distributed_duplicate_ordinal"));
    }
    let row_count = r.count(5)?;
    let mut rows = reserved(row_count)?;
    for _ in 0..row_count {
        let key = r.string()?;
        let coverage = if r.flag()? {
            Some(r.bits(pattern_count)?)
        } else {
            None
        };
        rows.push(Row { key, coverage });
    }
    let covered = r.bits(pattern_count)?;
    let graph_count = r.count(16)?;
    let mut graphs = reserved(graph_count)?;
    for _ in 0..graph_count {
        let key = r.string()?;
        let root = r.u32()?;
        let count = r.count(9)?;
        let mut nodes = reserved(count)?;
        for _ in 0..count {
            nodes.push(ScoringExecutionNode::new(r.u32()?, r.u32()?, r.flag()?));
        }
        let count = r.count(EDGE_BYTES)?;
        let mut edges = reserved(count)?;
        for _ in 0..count {
            edges.push(read_edge(&mut r)?);
        }
        if key.is_empty() {
            return Err(invalid("extended_pc_distributed_physical_key_invalid"));
        }
        graphs.push(SpinCoverageExecutionGraph::new(0, key, root, nodes, edges));
    }
    let mut stats = [0; 8];
    for value in &mut stats {
        *value = usize::try_from(r.u64()?).map_err(|_| projection_error())?;
    }
    let witnessed = u128::from_le_bytes(r.array()?);
    let variants = u128::from_le_bytes(r.array()?);
    let variants_complete = r.flag()?;
    let representative_rank = if r.flag()? { Some(r.u64()?) } else { None };
    let representative_pattern = if r.flag()? { Some(r.u32()?) } else { None };
    let count = r.count(12)?;
    let mut path = reserved(count)?;
    for _ in 0..count {
        let piece = r.piece()?;
        let rotation = r.u8()?;
        RotationState::from_quarter_turns(rotation)
            .map_err(|_| invalid("extended_pc_distributed_rotation_invalid"))?;
        let x = i32::from_le_bytes(r.array()?);
        let y = i32::from_le_bytes(r.array()?);
        let hold = *HOLD
            .get(usize::from(r.u8()?))
            .ok_or_else(|| invalid("extended_pc_distributed_hold_invalid"))?;
        path.push(CorePathStep::new(piece, rotation, x, y, hold, r.u8()?));
    }
    r.finish()?;
    let partial = Partial {
        receipts,
        rows,
        covered,
        graphs,
        stats,
        witnessed,
        variants,
        variants_complete,
        representative_rank,
        representative_pattern,
        path,
    };
    guard(
        partial
            .checked_retained_bytes()
            .ok_or_else(projection_error)?,
    )?;
    Ok(partial)
}

fn decode_projection(input: &[u8], problem: &SearchProblem) -> Result<u128, WasmCpuSearchError> {
    let mut r = Reader { input, cursor: 0 };
    read_header(&mut r, problem)?;
    let pattern_count = usize::try_from(r.u64()?).map_err(|_| projection_error())?;
    let expected = problem
        .piece_source()
        .materialized_universe()
        .ok_or_else(|| invalid("wasm_piece_source_not_materialized"))?
        .pattern_count();
    if pattern_count != expected {
        return Err(invalid("extended_pc_distributed_pattern_snapshot_mismatch"));
    }
    let receipts = r.count(40)?;
    let mut bytes = (receipts as u128)
        .checked_mul(size_of::<(u64, [u8; 32])>() as u128)
        .ok_or_else(projection_error)?;
    r.take(receipts.checked_mul(40).ok_or_else(projection_error)?)?;
    let rows = r.count(5)?;
    bytes = add_projection(bytes, rows, size_of::<Row>())?;
    for _ in 0..rows {
        bytes = bytes
            .checked_add(r.borrowed_string()?.len() as u128)
            .ok_or_else(projection_error)?;
        if r.flag()? {
            bytes = bytes
                .checked_add(project_bits(&mut r, pattern_count)?)
                .ok_or_else(projection_error)?;
        }
    }
    bytes = bytes
        .checked_add(project_bits(&mut r, pattern_count)?)
        .ok_or_else(projection_error)?;
    let graphs = r.count(16)?;
    bytes = add_projection(bytes, graphs, size_of::<SpinCoverageExecutionGraph>())?;
    for _ in 0..graphs {
        bytes = bytes
            .checked_add(r.borrowed_string()?.len() as u128)
            .ok_or_else(projection_error)?;
        r.u32()?;
        let count = r.count(9)?;
        bytes = add_projection(bytes, count, size_of::<ScoringExecutionNode>())?;
        r.take(count.checked_mul(9).ok_or_else(projection_error)?)?;
        let count = r.count(EDGE_BYTES)?;
        bytes = add_projection(bytes, count, size_of::<ScoringExecutionEdge>())?;
        r.take(count.checked_mul(EDGE_BYTES).ok_or_else(projection_error)?)?;
    }
    r.take(8 * 8 + 2 * 16)?;
    r.flag()?;
    if r.flag()? {
        r.u64()?;
    }
    if r.flag()? {
        r.u32()?;
    }
    let paths = r.count(12)?;
    bytes = add_projection(bytes, paths, size_of::<CorePathStep>())?;
    r.take(paths.checked_mul(12).ok_or_else(projection_error)?)?;
    r.finish()?;
    Ok(bytes)
}

fn project_bits(r: &mut Reader<'_>, patterns: usize) -> Result<u128, WasmCpuSearchError> {
    let words = r.count(8)?;
    if words != patterns.div_ceil(64) {
        return Err(invalid("extended_pc_distributed_coverage_shape_invalid"));
    }
    r.take(words.checked_mul(8).ok_or_else(projection_error)?)?;
    // from_words changes Vec to Arc; both backings may be live together.
    (words as u128).checked_mul(16).ok_or_else(projection_error)
}

fn read_header(r: &mut Reader<'_>, problem: &SearchProblem) -> Result<(), WasmCpuSearchError> {
    if r.take(MAGIC.len())? != MAGIC || r.array::<32>()? != snapshot(problem)? {
        return Err(invalid("extended_pc_distributed_snapshot_mismatch"));
    }
    Ok(())
}

fn snapshot(problem: &SearchProblem) -> Result<[u8; 32], WasmCpuSearchError> {
    let universe = problem
        .piece_source()
        .materialized_universe()
        .ok_or_else(|| invalid("wasm_piece_source_not_materialized"))?;
    let mut h = Sha256::new();
    h.update(b"clearra.pc-full-height-worker-snapshot.v1\0");
    h.update(problem.problem_id().as_str().as_bytes());
    h.update(problem.visible_height().to_le_bytes());
    for word in problem.initial_board().occupied_words() {
        h.update(word.to_le_bytes());
    }
    let (kick, rule) = super::super::replay_profile_ids(problem);
    h.update(kick.to_le_bytes());
    h.update(rule.to_le_bytes());
    h.update(universe.pattern_universe_id().get().to_le_bytes());
    h.update(universe.pattern_weight_model_id().get().to_le_bytes());
    h.update((universe.pattern_count() as u64).to_le_bytes());
    h.update(
        format!(
            "{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{:?}|{}|{}|{}",
            problem.piece_source().id(),
            problem.objective(),
            problem.count_policy(),
            problem.pc_chance_evidence_policy(),
            problem.queue_observation_policy(),
            problem.initial_hold(),
            problem.spawn_profile(),
            problem.output_policy().as_str(),
            problem.supply().hold_enabled(),
            problem.supply().projects_unplaced_lookahead()
        )
        .as_bytes(),
    );
    Ok(h.finalize().into())
}

fn read_edge(r: &mut Reader<'_>) -> Result<ScoringExecutionEdge, WasmCpuSearchError> {
    let to = r.u32()?;
    let operation = r.u8()?;
    let piece = r.piece()?;
    let rotation = r.rotation()?;
    let x = r.u8()? as i8;
    let y = r.u8()? as i8;
    let clear = r.u8()?;
    let corners = r.u8()?;
    let front = r.u8()?;
    let pc = r.flag()?;
    let rotated = r.flag()?;
    let from = r.rotation()?;
    let request = match r.u8()? {
        0 => RotationRequest::None,
        1 => RotationRequest::Clockwise,
        2 => RotationRequest::CounterClockwise,
        3 => RotationRequest::HalfTurn,
        _ => return Err(invalid("extended_pc_distributed_rotation_request_invalid")),
    };
    let index = r.u8()?;
    let dx = r.u8()? as i8;
    let dy = r.u8()? as i8;
    let px = r.u8()? as i8;
    let py = r.u8()? as i8;
    let immobile = r.flag()?;
    let evidence = if rotated {
        if request == RotationRequest::None {
            return Err(invalid("extended_pc_distributed_ordered_kick_invalid"));
        }
        ScoringLockEvidence::rotation(from, request, index, dx, dy, px, py)
    } else {
        if request != RotationRequest::None
            || index != 0
            || dx != 0
            || dy != 0
            || px != 0
            || py != 0
            || from != rotation
        {
            return Err(invalid("extended_pc_distributed_ordered_kick_invalid"));
        }
        ScoringLockEvidence::no_rotation(rotation)
    }
    .with_immobile_before_clear(immobile);
    Ok(ScoringExecutionEdge::new(
        to, operation, piece, rotation, x, y, clear, corners, front, evidence,
    )
    .with_perfect_clear(pc))
}

fn reserved<T>(count: usize) -> Result<Vec<T>, WasmCpuSearchError> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| storage_error())?;
    Ok(values)
}

fn add_projection(bytes: u128, count: usize, size: usize) -> Result<u128, WasmCpuSearchError> {
    bytes
        .checked_add(
            (count as u128)
                .checked_mul(size as u128)
                .ok_or_else(projection_error)?,
        )
        .ok_or_else(projection_error)
}

struct Writer<'a> {
    output: Option<&'a mut Vec<u8>>,
    length: usize,
}
impl Writer<'_> {
    fn bytes(&mut self, bytes: &[u8]) -> Result<(), WasmCpuSearchError> {
        self.length = self
            .length
            .checked_add(bytes.len())
            .ok_or_else(projection_error)?;
        if let Some(output) = self.output.as_mut() {
            output.extend_from_slice(bytes);
        }
        Ok(())
    }
    fn u8(&mut self, n: u8) -> Result<(), WasmCpuSearchError> {
        self.bytes(&[n])
    }
    fn u32(&mut self, n: u32) -> Result<(), WasmCpuSearchError> {
        self.bytes(&n.to_le_bytes())
    }
    fn u64(&mut self, n: u64) -> Result<(), WasmCpuSearchError> {
        self.bytes(&n.to_le_bytes())
    }
    fn count(&mut self, n: usize) -> Result<(), WasmCpuSearchError> {
        self.u32(u32::try_from(n).map_err(|_| projection_error())?)
    }
    fn string(&mut self, n: &str) -> Result<(), WasmCpuSearchError> {
        self.count(n.len())?;
        self.bytes(n.as_bytes())
    }
    fn bits(&mut self, bits: &PatternBitSet) -> Result<(), WasmCpuSearchError> {
        self.count(bits.word_count())?;
        for index in 0..bits.word_count() {
            self.u64(bits.word_at(index))?;
        }
        Ok(())
    }
}

struct Reader<'a> {
    input: &'a [u8],
    cursor: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], WasmCpuSearchError> {
        let end = self
            .cursor
            .checked_add(count)
            .ok_or_else(projection_error)?;
        let bytes = self
            .input
            .get(self.cursor..end)
            .ok_or_else(|| invalid("extended_pc_distributed_wire_truncated"))?;
        self.cursor = end;
        Ok(bytes)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N], WasmCpuSearchError> {
        self.take(N)?
            .try_into()
            .map_err(|_| invalid("extended_pc_distributed_wire_truncated"))
    }
    fn u8(&mut self) -> Result<u8, WasmCpuSearchError> {
        Ok(self.array::<1>()?[0])
    }
    fn u32(&mut self) -> Result<u32, WasmCpuSearchError> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    fn u64(&mut self) -> Result<u64, WasmCpuSearchError> {
        Ok(u64::from_le_bytes(self.array()?))
    }
    fn flag(&mut self) -> Result<bool, WasmCpuSearchError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(invalid("extended_pc_distributed_wire_flag_invalid")),
        }
    }
    fn count(&mut self, minimum_bytes: usize) -> Result<usize, WasmCpuSearchError> {
        let count = usize::try_from(self.u32()?).map_err(|_| projection_error())?;
        if count > (self.input.len() - self.cursor) / minimum_bytes.max(1) {
            return Err(invalid("extended_pc_distributed_wire_count_invalid"));
        }
        Ok(count)
    }
    fn borrowed_string(&mut self) -> Result<&'a str, WasmCpuSearchError> {
        let count = self.count(1)?;
        std::str::from_utf8(self.take(count)?)
            .map_err(|_| invalid("extended_pc_distributed_wire_utf8_invalid"))
    }
    fn string(&mut self) -> Result<String, WasmCpuSearchError> {
        let value = self.borrowed_string()?;
        let mut output = String::new();
        output
            .try_reserve_exact(value.len())
            .map_err(|_| storage_error())?;
        output.push_str(value);
        Ok(output)
    }
    fn bits(&mut self, patterns: usize) -> Result<PatternBitSet, WasmCpuSearchError> {
        let count = self.count(8)?;
        if count != patterns.div_ceil(64) {
            return Err(invalid("extended_pc_distributed_coverage_shape_invalid"));
        }
        let mut words = reserved(count)?;
        for _ in 0..count {
            words.push(self.u64()?);
        }
        if patterns & 63 != 0
            && words
                .last()
                .is_some_and(|word| word >> (patterns & 63) != 0)
        {
            return Err(invalid("extended_pc_distributed_coverage_tail_invalid"));
        }
        PatternBitSet::from_words(patterns, words)
            .map_err(|_| invalid("extended_pc_distributed_coverage_shape_invalid"))
    }
    fn rotation(&mut self) -> Result<RotationState, WasmCpuSearchError> {
        RotationState::from_quarter_turns(self.u8()?)
            .map_err(|_| invalid("extended_pc_distributed_rotation_invalid"))
    }
    fn piece(&mut self) -> Result<PieceKind, WasmCpuSearchError> {
        PieceKind::from_ascii(char::from(self.u8()?))
            .map_err(|_| invalid("extended_pc_distributed_piece_invalid"))
    }
    fn finish(self) -> Result<(), WasmCpuSearchError> {
        if self.cursor != self.input.len() {
            return Err(invalid("extended_pc_distributed_wire_tail_invalid"));
        }
        Ok(())
    }
}
