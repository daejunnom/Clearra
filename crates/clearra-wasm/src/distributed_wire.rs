// SRP rationale: this module has one change reason: the closed distributed-worker wire contract for WASM execution.
use clearra_core_domain::{
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::{
        PiecePlacementMask, StandardBoard64TilingIdentity, STANDARD_BOARD64_TILING_MAX_PLACEMENTS,
    },
};
use clearra_core_executor::{
    core_execution_result::CoreResultFieldReplacementError,
    encode_canonical_wasm_candidate_packet_batch, tiling_solution_store::PackedTilingRows,
    CoreExecutionResult, CorePathStep, CorePostProcessScoreCell, CorePostProcessSpinCoverage,
    DistributedPcChanceCoverageRows, NormalizedSolutionCoverage, SolutionCoverage,
    WasmCandidatePacket, WasmPackedTilingIdentity, WasmTilingRootChunk,
};
use clearra_coverage::{
    pattern::pattern_bitset::PatternBitSet,
    row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind},
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};
use std::sync::Arc;

const CANDIDATE_MAGIC: u32 = 0x4342_4131;
const PARTIAL_MAGIC: u32 = 0x5052_5431;
const PARTIAL_BATCH_MAGIC: u32 = 0x5052_4231;
const TILING_ROOT_CHUNK_MAGIC: u32 = 0x5452_4331;
const WIRE_VERSION: u32 = 8;
const MAX_WIRE_ITEMS: usize = 16_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DistributedWireError(&'static str);

impl DistributedWireError {
    pub const fn reason(self) -> &'static str {
        self.0
    }

    pub(crate) const fn decode_memory_projection_overflow() -> Self {
        Self("partial_decode_memory_projection_overflow")
    }

    pub(crate) const fn candidate_decode_memory_projection_overflow() -> Self {
        Self("candidate_decode_memory_projection_overflow")
    }
}

#[derive(Debug)]
pub enum GuardedDistributedWireError<E> {
    Wire(DistributedWireError),
    MemoryGuard(E),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct PartialResultDecodeProjection {
    nested_retained_bytes: u128,
    constructor_extra_bytes: u128,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct PartialBatchDecodeProjection {
    result_count: usize,
    nested_retained_bytes: u128,
    constructor_extra_bytes: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PartialDecodeContract {
    General,
    BuildProbability,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CandidateBatchDecodeProjection {
    candidate_count: usize,
    row_ids_requested_bytes: u128,
}

pub fn encode_candidate_batch(candidates: &[WasmCandidatePacket]) -> Vec<u8> {
    encode_canonical_wasm_candidate_packet_batch(candidates)
}

pub fn encode_candidate_batch_with_memory_guard<E>(
    candidates: &[WasmCandidatePacket],
    mut memory_guard: impl FnMut(u128) -> Result<(), E>,
) -> Result<Vec<u8>, GuardedDistributedWireError<E>> {
    let required_len =
        checked_candidate_batch_len(candidates).map_err(GuardedDistributedWireError::Wire)?;
    memory_guard(required_len as u128).map_err(GuardedDistributedWireError::MemoryGuard)?;

    let mut output = Vec::new();
    output.try_reserve_exact(required_len).map_err(|_| {
        GuardedDistributedWireError::Wire(DistributedWireError("candidate_batch_allocation_failed"))
    })?;
    memory_guard(output.capacity() as u128).map_err(GuardedDistributedWireError::MemoryGuard)?;

    let mut sink = CheckedWireOutput::new(&mut output);
    checked_put_u32(&mut sink, CANDIDATE_MAGIC).map_err(GuardedDistributedWireError::Wire)?;
    checked_put_u32(&mut sink, WIRE_VERSION).map_err(GuardedDistributedWireError::Wire)?;
    checked_put_u32(
        &mut sink,
        u32::try_from(candidates.len()).map_err(|_| {
            GuardedDistributedWireError::Wire(DistributedWireError(
                "candidate_batch_count_overflow",
            ))
        })?,
    )
    .map_err(GuardedDistributedWireError::Wire)?;
    for candidate in candidates {
        checked_put_u64(&mut sink, candidate.ordinal())
            .map_err(GuardedDistributedWireError::Wire)?;
        checked_put_u32(&mut sink, u32::from(candidate.pass_index()))
            .map_err(GuardedDistributedWireError::Wire)?;
        checked_put_u32(&mut sink, candidate.target_index())
            .map_err(GuardedDistributedWireError::Wire)?;
        checked_put_u32(
            &mut sink,
            u32::try_from(candidate.row_ids().len()).map_err(|_| {
                GuardedDistributedWireError::Wire(DistributedWireError(
                    "candidate_row_count_overflow",
                ))
            })?,
        )
        .map_err(GuardedDistributedWireError::Wire)?;
        for row_id in candidate.row_ids() {
            checked_put_u32(&mut sink, *row_id).map_err(GuardedDistributedWireError::Wire)?;
        }
    }
    drop(sink);
    if output.len() != required_len {
        return Err(GuardedDistributedWireError::Wire(DistributedWireError(
            "candidate_batch_length_mismatch",
        )));
    }
    Ok(output)
}

fn checked_candidate_batch_len(
    candidates: &[WasmCandidatePacket],
) -> Result<usize, DistributedWireError> {
    if candidates.len() > MAX_WIRE_ITEMS {
        return Err(DistributedWireError("candidate_batch_count_exceeded"));
    }
    let mut required_len = 12_usize;
    for candidate in candidates {
        if candidate.row_ids().len() > MAX_WIRE_ITEMS {
            return Err(DistributedWireError("candidate_row_count_exceeded"));
        }
        u32::try_from(candidate.row_ids().len())
            .map_err(|_| DistributedWireError("candidate_row_count_overflow"))?;
        required_len = required_len
            .checked_add(20)
            .and_then(|bytes| {
                bytes.checked_add(
                    candidate
                        .row_ids()
                        .len()
                        .checked_mul(core::mem::size_of::<u32>())?,
                )
            })
            .ok_or(DistributedWireError("candidate_batch_length_overflow"))?;
    }
    Ok(required_len)
}

pub(crate) fn checked_candidate_batch_outer_requested_bytes(
    candidate_count: usize,
) -> Result<u128, DistributedWireError> {
    if candidate_count > MAX_WIRE_ITEMS {
        return Err(DistributedWireError("candidate_batch_count_exceeded"));
    }
    (candidate_count as u128)
        .checked_mul(core::mem::size_of::<WasmCandidatePacket>() as u128)
        .ok_or(DistributedWireError(
            "candidate_decode_memory_projection_overflow",
        ))
}

pub fn decode_candidate_batch(
    input: &[u8],
) -> Result<Vec<WasmCandidatePacket>, DistributedWireError> {
    let mut reader = Reader::new(input);
    reader.require_header(CANDIDATE_MAGIC)?;
    let count = reader.count()?;
    let mut candidates = Vec::new();
    candidates
        .try_reserve_exact(count)
        .map_err(|_| DistributedWireError("candidate_batch_allocation_failed"))?;
    for _ in 0..count {
        let ordinal = reader.u64()?;
        let pass_index = u8::try_from(reader.u32()?)
            .map_err(|_| DistributedWireError("candidate_pass_index_invalid"))?;
        let target_index = reader.u32()?;
        let row_count = reader.count()?;
        let mut row_ids = Vec::new();
        row_ids
            .try_reserve_exact(row_count)
            .map_err(|_| DistributedWireError("candidate_rows_allocation_failed"))?;
        for _ in 0..row_count {
            row_ids.push(reader.u32()?);
        }
        candidates.push(WasmCandidatePacket::for_pass(
            ordinal,
            pass_index,
            target_index,
            row_ids,
        ));
    }
    reader.finish()?;
    Ok(candidates)
}

pub fn decode_build_probability_candidate_batch_with_memory_guard<E>(
    input: &[u8],
    mut memory_guard: impl FnMut(u128) -> Result<(), E>,
) -> Result<Vec<WasmCandidatePacket>, GuardedDistributedWireError<E>> {
    let projection = checked_candidate_batch_decode_projection(input)
        .map_err(GuardedDistributedWireError::Wire)?;
    let requested_outer_bytes =
        checked_candidate_batch_outer_requested_bytes(projection.candidate_count)
            .map_err(GuardedDistributedWireError::Wire)?;
    let requested_batch_bytes = requested_outer_bytes
        .checked_add(projection.row_ids_requested_bytes)
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(
                DistributedWireError::candidate_decode_memory_projection_overflow(),
            )
        })?;
    memory_guard(requested_batch_bytes).map_err(GuardedDistributedWireError::MemoryGuard)?;

    let mut reader = Reader::new(input);
    reader
        .require_header(CANDIDATE_MAGIC)
        .map_err(GuardedDistributedWireError::Wire)?;
    let count = reader.count().map_err(GuardedDistributedWireError::Wire)?;
    debug_assert_eq!(count, projection.candidate_count);
    let mut candidates = Vec::new();
    candidates.try_reserve_exact(count).map_err(|_| {
        GuardedDistributedWireError::Wire(DistributedWireError("candidate_batch_allocation_failed"))
    })?;
    let mut remaining_requested_row_bytes = projection.row_ids_requested_bytes;
    let outer_actual_bytes = (candidates.capacity() as u128)
        .checked_mul(core::mem::size_of::<WasmCandidatePacket>() as u128)
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(
                DistributedWireError::candidate_decode_memory_projection_overflow(),
            )
        })?;
    memory_guard(
        outer_actual_bytes
            .checked_add(remaining_requested_row_bytes)
            .ok_or_else(|| {
                GuardedDistributedWireError::Wire(
                    DistributedWireError::candidate_decode_memory_projection_overflow(),
                )
            })?,
    )
    .map_err(GuardedDistributedWireError::MemoryGuard)?;

    for _ in 0..count {
        let ordinal = reader.u64().map_err(GuardedDistributedWireError::Wire)?;
        let pass_index = u8::try_from(reader.u32().map_err(GuardedDistributedWireError::Wire)?)
            .map_err(|_| {
                GuardedDistributedWireError::Wire(DistributedWireError(
                    "candidate_pass_index_invalid",
                ))
            })?;
        let target_index = reader.u32().map_err(GuardedDistributedWireError::Wire)?;
        let row_count = reader.count().map_err(GuardedDistributedWireError::Wire)?;
        let requested_row_bytes = (row_count as u128)
            .checked_mul(core::mem::size_of::<u32>() as u128)
            .ok_or_else(|| {
                GuardedDistributedWireError::Wire(
                    DistributedWireError::candidate_decode_memory_projection_overflow(),
                )
            })?;
        let remaining_after = remaining_requested_row_bytes
            .checked_sub(requested_row_bytes)
            .ok_or_else(|| {
                GuardedDistributedWireError::Wire(
                    DistributedWireError::candidate_decode_memory_projection_overflow(),
                )
            })?;
        let decoded_before_bytes =
            checked_candidate_vec_retained_bytes(&candidates).ok_or_else(|| {
                GuardedDistributedWireError::Wire(
                    DistributedWireError::candidate_decode_memory_projection_overflow(),
                )
            })?;
        memory_guard(
            decoded_before_bytes
                .checked_add(remaining_requested_row_bytes)
                .ok_or_else(|| {
                    GuardedDistributedWireError::Wire(
                        DistributedWireError::candidate_decode_memory_projection_overflow(),
                    )
                })?,
        )
        .map_err(GuardedDistributedWireError::MemoryGuard)?;

        let mut row_ids = Vec::new();
        row_ids.try_reserve_exact(row_count).map_err(|_| {
            GuardedDistributedWireError::Wire(DistributedWireError(
                "candidate_rows_allocation_failed",
            ))
        })?;
        let actual_row_bytes = (row_ids.capacity() as u128)
            .checked_mul(core::mem::size_of::<u32>() as u128)
            .ok_or_else(|| {
                GuardedDistributedWireError::Wire(
                    DistributedWireError::candidate_decode_memory_projection_overflow(),
                )
            })?;
        memory_guard(
            decoded_before_bytes
                .checked_add(actual_row_bytes)
                .and_then(|bytes| bytes.checked_add(remaining_after))
                .ok_or_else(|| {
                    GuardedDistributedWireError::Wire(
                        DistributedWireError::candidate_decode_memory_projection_overflow(),
                    )
                })?,
        )
        .map_err(GuardedDistributedWireError::MemoryGuard)?;
        for _ in 0..row_count {
            row_ids.push(reader.u32().map_err(GuardedDistributedWireError::Wire)?);
        }
        candidates.push(WasmCandidatePacket::for_pass(
            ordinal,
            pass_index,
            target_index,
            row_ids,
        ));
        remaining_requested_row_bytes = remaining_after;
        let decoded_after_bytes =
            checked_candidate_vec_retained_bytes(&candidates).ok_or_else(|| {
                GuardedDistributedWireError::Wire(
                    DistributedWireError::candidate_decode_memory_projection_overflow(),
                )
            })?;
        memory_guard(
            decoded_after_bytes
                .checked_add(remaining_requested_row_bytes)
                .ok_or_else(|| {
                    GuardedDistributedWireError::Wire(
                        DistributedWireError::candidate_decode_memory_projection_overflow(),
                    )
                })?,
        )
        .map_err(GuardedDistributedWireError::MemoryGuard)?;
    }
    reader.finish().map_err(GuardedDistributedWireError::Wire)?;
    debug_assert_eq!(remaining_requested_row_bytes, 0);
    Ok(candidates)
}

fn checked_candidate_batch_decode_projection(
    input: &[u8],
) -> Result<CandidateBatchDecodeProjection, DistributedWireError> {
    let mut reader = Reader::new(input);
    reader.require_header(CANDIDATE_MAGIC)?;
    let candidate_count = reader.count()?;
    let mut row_ids_requested_bytes = 0_u128;
    for _ in 0..candidate_count {
        reader.u64()?;
        u8::try_from(reader.u32()?)
            .map_err(|_| DistributedWireError("candidate_pass_index_invalid"))?;
        reader.u32()?;
        let row_count = reader.count()?;
        row_ids_requested_bytes = row_ids_requested_bytes
            .checked_add(
                (row_count as u128)
                    .checked_mul(core::mem::size_of::<u32>() as u128)
                    .ok_or_else(
                        DistributedWireError::candidate_decode_memory_projection_overflow,
                    )?,
            )
            .ok_or_else(DistributedWireError::candidate_decode_memory_projection_overflow)?;
        for _ in 0..row_count {
            reader.u32()?;
        }
    }
    reader.finish()?;
    Ok(CandidateBatchDecodeProjection {
        candidate_count,
        row_ids_requested_bytes,
    })
}

pub(crate) fn checked_candidate_vec_retained_bytes(
    candidates: &Vec<WasmCandidatePacket>,
) -> Option<u128> {
    let mut retained = (candidates.capacity() as u128)
        .checked_mul(core::mem::size_of::<WasmCandidatePacket>() as u128)?;
    for candidate in candidates {
        retained = retained.checked_add(candidate.checked_nested_retained_bytes()?)?;
    }
    Some(retained)
}

pub fn encode_tiling_root_chunk(chunk: &WasmTilingRootChunk) -> Vec<u8> {
    let mut output = Vec::with_capacity(91 + chunk.identities().len() * 32);
    put_u32(&mut output, TILING_ROOT_CHUNK_MAGIC);
    put_u32(&mut output, WIRE_VERSION);
    output.push(chunk.pass_index());
    put_u32(&mut output, chunk.root_ordinal().unwrap_or(u32::MAX));
    put_u32(&mut output, chunk.chunk_sequence());
    output.push(u8::from(chunk.root_complete()));
    put_u32(&mut output, chunk.identities().len() as u32);
    put_u64(&mut output, chunk.completed_roots() as u64);
    match chunk.candidate_family_count() {
        Some(count) => {
            output.push(1);
            put_u128(&mut output, count);
        }
        None => {
            output.push(0);
            put_u128(&mut output, 0);
        }
    }
    put_u64(&mut output, chunk.expanded_nodes() as u64);
    put_u64(&mut output, chunk.peak_frontier() as u64);
    put_u64(&mut output, chunk.domain_pruned_states() as u64);
    put_u64(&mut output, chunk.hall_pruned_states() as u64);
    put_u64(&mut output, chunk.column_pruned_states() as u64);
    put_u64(&mut output, chunk.component_compositions() as u64);
    for identity in chunk.identities().iter().copied() {
        put_u64(&mut output, identity.bucket_hash());
        for word in identity.packed_rows() {
            put_u64(&mut output, word);
        }
    }
    output
}

pub fn decode_tiling_root_chunk(input: &[u8]) -> Result<WasmTilingRootChunk, DistributedWireError> {
    let mut reader = Reader::new(input);
    reader.require_header(TILING_ROOT_CHUNK_MAGIC)?;
    let pass_index = reader.u8()?;
    let root_ordinal = reader.u32()?;
    let chunk_sequence = reader.u32()?;
    let root_complete = match reader.u8()? {
        0 => false,
        1 => true,
        _ => return Err(DistributedWireError("tiling_root_complete_flag_invalid")),
    };
    let identity_count = reader.count()?;
    let completed_roots = reader.usize_u64()?;
    let candidate_family_count = match reader.u8()? {
        0 => {
            reader.u128()?;
            None
        }
        1 => Some(reader.u128()?),
        _ => {
            return Err(DistributedWireError(
                "tiling_root_candidate_family_flag_invalid",
            ));
        }
    };
    let expanded_nodes = reader.usize_u64()?;
    let peak_frontier = reader.usize_u64()?;
    let domain_pruned_states = reader.usize_u64()?;
    let hall_pruned_states = reader.usize_u64()?;
    let column_pruned_states = reader.usize_u64()?;
    let component_compositions = reader.usize_u64()?;
    let mut identities = Vec::new();
    identities
        .try_reserve_exact(identity_count)
        .map_err(|_| DistributedWireError("tiling_root_chunk_allocation_failed"))?;
    for _ in 0..identity_count {
        let bucket_hash = reader.u64()?;
        let mut packed_rows = PackedTilingRows::default();
        for word in &mut packed_rows {
            *word = reader.u64()?;
        }
        identities.push(WasmPackedTilingIdentity::new(bucket_hash, packed_rows));
    }
    reader.finish()?;
    Ok(WasmTilingRootChunk::from_wire_parts(
        pass_index,
        root_ordinal,
        chunk_sequence,
        root_complete,
        identities,
        completed_roots,
        candidate_family_count,
        expanded_nodes,
        peak_frontier,
        domain_pruned_states,
        hall_pruned_states,
        column_pruned_states,
        component_compositions,
    ))
}

pub fn is_tiling_root_chunk(input: &[u8]) -> bool {
    input
        .get(..4)
        .and_then(|bytes| bytes.try_into().ok())
        .is_some_and(|bytes| u32::from_le_bytes(bytes) == TILING_ROOT_CHUNK_MAGIC)
}

trait CheckedWireSink {
    fn extend_checked(&mut self, value: &[u8]) -> Result<(), DistributedWireError>;
    fn len(&self) -> usize;
}

#[derive(Default)]
struct CheckedWireLength {
    len: usize,
}

impl CheckedWireSink for CheckedWireLength {
    fn extend_checked(&mut self, value: &[u8]) -> Result<(), DistributedWireError> {
        self.len = self
            .len
            .checked_add(value.len())
            .ok_or(DistributedWireError("partial_result_length_overflow"))?;
        Ok(())
    }

    fn len(&self) -> usize {
        self.len
    }
}

struct CheckedWireOutput<'a> {
    output: &'a mut Vec<u8>,
}

impl<'a> CheckedWireOutput<'a> {
    fn new(output: &'a mut Vec<u8>) -> Self {
        Self { output }
    }
}

impl CheckedWireSink for CheckedWireOutput<'_> {
    fn extend_checked(&mut self, value: &[u8]) -> Result<(), DistributedWireError> {
        let target_len = self
            .output
            .len()
            .checked_add(value.len())
            .ok_or(DistributedWireError("partial_result_length_overflow"))?;
        if target_len > self.output.capacity() {
            return Err(DistributedWireError(
                "partial_result_guarded_capacity_exceeded",
            ));
        }
        self.output.extend_from_slice(value);
        Ok(())
    }

    fn len(&self) -> usize {
        self.output.len()
    }
}

fn checked_put_u8(
    output: &mut impl CheckedWireSink,
    value: u8,
) -> Result<(), DistributedWireError> {
    output.extend_checked(&[value])
}

fn checked_put_u32(
    output: &mut impl CheckedWireSink,
    value: u32,
) -> Result<(), DistributedWireError> {
    output.extend_checked(&value.to_le_bytes())
}

fn checked_put_i32(
    output: &mut impl CheckedWireSink,
    value: i32,
) -> Result<(), DistributedWireError> {
    output.extend_checked(&value.to_le_bytes())
}

fn checked_put_u64(
    output: &mut impl CheckedWireSink,
    value: u64,
) -> Result<(), DistributedWireError> {
    output.extend_checked(&value.to_le_bytes())
}

fn checked_put_u128(
    output: &mut impl CheckedWireSink,
    value: u128,
) -> Result<(), DistributedWireError> {
    output.extend_checked(&value.to_le_bytes())
}

fn checked_put_count(
    output: &mut impl CheckedWireSink,
    value: usize,
) -> Result<(), DistributedWireError> {
    if value > MAX_WIRE_ITEMS {
        return Err(DistributedWireError("partial_result_item_count_exceeded"));
    }
    let value = u32::try_from(value)
        .map_err(|_| DistributedWireError("partial_result_item_count_overflow"))?;
    checked_put_u32(output, value)
}

fn checked_put_bytes(
    output: &mut impl CheckedWireSink,
    value: &[u8],
) -> Result<(), DistributedWireError> {
    let length = u32::try_from(value.len())
        .map_err(|_| DistributedWireError("partial_result_byte_length_overflow"))?;
    checked_put_u32(output, length)?;
    output.extend_checked(value)
}

fn checked_encode_identity(
    output: &mut impl CheckedWireSink,
    identity: StandardBoard64TilingIdentity,
) -> Result<(), DistributedWireError> {
    if identity.placement_count() > 16 {
        return Err(DistributedWireError(
            "partial_identity_placement_count_invalid",
        ));
    }
    let placement_count = u8::try_from(identity.placement_count())
        .map_err(|_| DistributedWireError("partial_identity_placement_count_overflow"))?;
    checked_put_u64(output, identity.initial_board_mask())?;
    checked_put_u64(output, identity.packed_piece_codes())?;
    checked_put_u8(output, placement_count)?;
    for mask in identity.placement_masks() {
        checked_put_u64(output, *mask)?;
    }
    Ok(())
}

pub fn encode_partial_result(
    result: &CoreExecutionResult,
) -> Result<Vec<u8>, DistributedWireError> {
    let required_len = checked_partial_result_len(result)?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(required_len)
        .map_err(|_| DistributedWireError("partial_result_allocation_failed"))?;
    emit_partial_result(&mut CheckedWireOutput::new(&mut output), result)?;
    if output.len() != required_len {
        return Err(DistributedWireError("partial_result_length_mismatch"));
    }
    Ok(output)
}

fn checked_partial_result_len(result: &CoreExecutionResult) -> Result<usize, DistributedWireError> {
    let mut length = CheckedWireLength::default();
    emit_partial_result(&mut length, result)?;
    Ok(length.len())
}

fn emit_partial_result(
    output: &mut impl CheckedWireSink,
    result: &CoreExecutionResult,
) -> Result<(), DistributedWireError> {
    let fields = result.summary_field_entries();
    let path = result.path_steps();
    let identities = result.normalized_solution_identities();
    // The PC merger rebuilds canonical keys from exact identities. Tiling-only
    // workers therefore do not need to duplicate every large string key on
    // the inter-worker wire.
    let normalized_keys = if result.field("objective") == Some("tiling") {
        &[]
    } else {
        result.normalized_solution_keys()
    };
    let coverage = result.coverage_pattern_words();
    let solution_coverage = result.solution_coverages();
    // PC and compact-build workers produce board64 authority and a reconstructable normalized
    // duplicate, while extended-build workers only produce normalized authority. Their mergers
    // consume those respective representations, so carry normalized coverage only when board64
    // coverage is unavailable instead of doubling the worker payload.
    let normalized_solution_coverage = if solution_coverage.is_empty() {
        result.normalized_solution_coverages()
    } else {
        &[]
    };
    let score_cells = result.postprocess_score_cells();
    let spin_coverages = result.postprocess_spin_coverages();
    checked_put_u32(output, PARTIAL_MAGIC)?;
    checked_put_u32(output, WIRE_VERSION)?;
    checked_put_count(output, fields.len())?;
    for (key, value) in fields {
        checked_put_bytes(output, key.as_bytes())?;
        checked_put_bytes(output, value.as_bytes())?;
    }
    checked_put_count(output, path.len())?;
    for step in path {
        let hold = hold_code(step.hold());
        if hold == u8::MAX {
            return Err(DistributedWireError("partial_hold_code_invalid"));
        }
        checked_put_u8(output, piece_code(step.piece()))?;
        checked_put_u8(output, step.rotation())?;
        checked_put_u8(output, step.cleared_lines())?;
        checked_put_u8(output, hold)?;
        checked_put_i32(output, step.x())?;
        checked_put_i32(output, step.y())?;
    }
    checked_put_count(output, identities.len())?;
    for identity in identities {
        checked_encode_identity(output, *identity)?;
    }
    checked_put_count(output, normalized_keys.len())?;
    for key in normalized_keys {
        checked_put_bytes(output, key.as_bytes())?;
    }
    match result.representative_solution_identity() {
        Some(identity) => {
            checked_put_u8(output, 1)?;
            checked_encode_identity(output, identity)?;
        }
        None => checked_put_u8(output, 0)?,
    }
    checked_put_count(output, coverage.len())?;
    for word in coverage {
        checked_put_u64(output, *word)?;
    }
    checked_put_count(output, solution_coverage.len())?;
    for entry in solution_coverage {
        checked_encode_identity(output, entry.identity())?;
        checked_put_count(output, entry.covered_patterns().pattern_count())?;
        checked_put_count(output, entry.covered_patterns().word_count())?;
        for word_index in 0..entry.covered_patterns().word_count() {
            checked_put_u64(output, entry.covered_patterns().word_at(word_index))?;
        }
    }
    checked_put_count(output, normalized_solution_coverage.len())?;
    for entry in normalized_solution_coverage {
        checked_put_bytes(output, entry.solution_key().as_bytes())?;
        checked_put_count(output, entry.covered_patterns().pattern_count())?;
        checked_put_count(output, entry.covered_patterns().word_count())?;
        for word_index in 0..entry.covered_patterns().word_count() {
            checked_put_u64(output, entry.covered_patterns().word_at(word_index))?;
        }
    }
    match result.postprocess_score_profile_id() {
        Some(profile_id) => {
            checked_put_u8(output, 1)?;
            checked_put_bytes(output, profile_id.as_bytes())?;
            checked_put_u8(output, u8::from(result.postprocess_score_cells_complete()))?;
            checked_put_count(output, score_cells.len())?;
            for cell in score_cells {
                checked_encode_identity(output, cell.candidate_identity())?;
                checked_put_count(output, cell.pattern_id())?;
                checked_put_bytes(output, cell.trace_identity().as_bytes())?;
                checked_put_u64(output, cell.score())?;
                checked_put_u32(output, cell.attack())?;
            }
        }
        None => checked_put_u8(output, 0)?,
    }
    checked_put_count(output, spin_coverages.len())?;
    for coverage in spin_coverages {
        checked_put_bytes(output, coverage.target_id().as_bytes())?;
        checked_put_count(output, coverage.pass_index())?;
        checked_put_count(output, coverage.pattern_count())?;
        checked_put_count(output, coverage.covered_pattern_words().len())?;
        for word in coverage.covered_pattern_words() {
            checked_put_u64(output, *word)?;
        }
        checked_put_count(output, coverage.candidate_keys().len())?;
        for key in coverage.candidate_keys() {
            checked_put_bytes(output, key.as_bytes())?;
        }
        checked_put_u128(output, coverage.witnessed_pattern_count())?;
        checked_put_u8(output, u8::from(coverage.complete()))?;
    }
    let typed_chance_evidence = result.pc_chance_coverage_evidence().filter(|evidence| {
        evidence
            .problem()
            .pc_chance_evidence_policy()
            .retains_pc_probability_v2_evidence()
    });
    let transported_chance_rows = result.distributed_pc_chance_coverage_rows();
    if typed_chance_evidence.is_some() && transported_chance_rows.is_some() {
        return Err(DistributedWireError("partial_pc_chance_evidence_ambiguous"));
    }
    let chance_rows = typed_chance_evidence
        .map(|evidence| {
            (
                evidence.piece_source_id(),
                evidence.pattern_universe_id(),
                evidence.pattern_weight_model_id(),
                evidence.pattern_count(),
                evidence.rows(),
                evidence.complete(),
            )
        })
        .or_else(|| {
            transported_chance_rows.map(|transport| {
                (
                    transport.piece_source_id(),
                    transport.pattern_universe_id(),
                    transport.pattern_weight_model_id(),
                    transport.pattern_count(),
                    transport.rows(),
                    transport.complete(),
                )
            })
        });
    match chance_rows {
        None => checked_put_u8(output, 0)?,
        Some((
            piece_source_id,
            pattern_universe_id,
            pattern_weight_model_id,
            pattern_count,
            rows,
            complete,
        )) => {
            if piece_source_id == 0
                || pattern_universe_id.get() == 0
                || pattern_weight_model_id.get() == 0
            {
                return Err(DistributedWireError("partial_pc_chance_identity_invalid"));
            }
            checked_put_u8(output, 1)?;
            checked_put_u64(output, piece_source_id)?;
            checked_put_u64(output, pattern_universe_id.get())?;
            checked_put_u64(output, pattern_weight_model_id.get())?;
            checked_put_count(output, pattern_count)?;
            checked_put_u8(output, u8::from(complete))?;
            checked_put_count(output, rows.len())?;
            let mut previous_candidate_id = None;
            for row in rows {
                if row.row_kind() != &CoverageRowKind::Build
                    || row.piece_source_id() != piece_source_id
                    || row.pattern_universe_id() != pattern_universe_id
                    || row.pattern_weight_model_id() != pattern_weight_model_id
                    || row.pattern_count() != pattern_count
                {
                    return Err(DistributedWireError(
                        "partial_pc_chance_row_identity_invalid",
                    ));
                }
                if previous_candidate_id.is_some_and(|previous| previous >= row.candidate_id()) {
                    return Err(DistributedWireError(
                        "partial_pc_chance_candidate_order_invalid",
                    ));
                }
                checked_put_u64(output, row.candidate_id())?;
                checked_put_count(output, row.coverage_bits().word_count())?;
                for word_index in 0..row.coverage_bits().word_count() {
                    checked_put_u64(output, row.coverage_bits().word_at(word_index))?;
                }
                previous_candidate_id = Some(row.candidate_id());
            }
        }
    }
    Ok(())
}

pub fn encode_partial_results(
    results: &[CoreExecutionResult],
) -> Result<Vec<u8>, DistributedWireError> {
    match encode_partial_results_with_memory_guard(results, |_| {
        Ok::<(), core::convert::Infallible>(())
    }) {
        Ok(output) => Ok(output),
        Err(GuardedDistributedWireError::Wire(error)) => Err(error),
        Err(GuardedDistributedWireError::MemoryGuard(never)) => match never {},
    }
}

pub fn encode_partial_results_with_memory_guard<E>(
    results: &[CoreExecutionResult],
    mut memory_guard: impl FnMut(u128) -> Result<(), E>,
) -> Result<Vec<u8>, GuardedDistributedWireError<E>> {
    if results.len() > MAX_WIRE_ITEMS {
        return Err(GuardedDistributedWireError::Wire(DistributedWireError(
            "partial_batch_result_count_exceeded",
        )));
    }
    let result_count = u32::try_from(results.len()).map_err(|_| {
        GuardedDistributedWireError::Wire(DistributedWireError(
            "partial_batch_result_count_overflow",
        ))
    })?;
    let mut required_len = 12usize;
    for result in results {
        let encoded_len =
            checked_partial_result_len(result).map_err(GuardedDistributedWireError::Wire)?;
        u32::try_from(encoded_len).map_err(|_| {
            GuardedDistributedWireError::Wire(DistributedWireError(
                "partial_batch_result_length_overflow",
            ))
        })?;
        required_len = required_len
            .checked_add(4)
            .and_then(|length| length.checked_add(encoded_len))
            .ok_or_else(|| {
                GuardedDistributedWireError::Wire(DistributedWireError(
                    "partial_batch_length_overflow",
                ))
            })?;
    }

    memory_guard(required_len as u128).map_err(GuardedDistributedWireError::MemoryGuard)?;
    let mut output = Vec::new();
    output.try_reserve_exact(required_len).map_err(|_| {
        GuardedDistributedWireError::Wire(DistributedWireError("partial_batch_allocation_failed"))
    })?;
    memory_guard(output.capacity() as u128).map_err(GuardedDistributedWireError::MemoryGuard)?;

    let mut sink = CheckedWireOutput::new(&mut output);
    checked_put_u32(&mut sink, PARTIAL_BATCH_MAGIC).map_err(GuardedDistributedWireError::Wire)?;
    checked_put_u32(&mut sink, WIRE_VERSION).map_err(GuardedDistributedWireError::Wire)?;
    checked_put_u32(&mut sink, result_count).map_err(GuardedDistributedWireError::Wire)?;
    for result in results {
        let encoded_len =
            checked_partial_result_len(result).map_err(GuardedDistributedWireError::Wire)?;
        checked_put_u32(
            &mut sink,
            u32::try_from(encoded_len).expect("encoded length was checked above"),
        )
        .map_err(GuardedDistributedWireError::Wire)?;
        emit_partial_result(&mut sink, result).map_err(GuardedDistributedWireError::Wire)?;
    }
    drop(sink);
    if output.len() != required_len {
        return Err(GuardedDistributedWireError::Wire(DistributedWireError(
            "partial_batch_length_mismatch",
        )));
    }
    Ok(output)
}

fn checked_partial_batch_decode_projection(
    input: &[u8],
) -> Result<PartialBatchDecodeProjection, DistributedWireError> {
    checked_partial_batch_decode_projection_for_contract(
        input,
        PartialDecodeContract::BuildProbability,
    )
}

fn checked_partial_batch_decode_projection_for_contract(
    input: &[u8],
    contract: PartialDecodeContract,
) -> Result<PartialBatchDecodeProjection, DistributedWireError> {
    let mut reader = Reader::new(input);
    reader.require_header(PARTIAL_BATCH_MAGIC)?;
    let result_count = reader.count()?;
    let mut nested_retained_bytes = 0_u128;
    let mut constructor_extra_bytes = 0_u128;
    for _ in 0..result_count {
        let length = reader.byte_length()?;
        let result_input = reader.take(length)?;
        let projection =
            checked_partial_result_decode_projection_for_contract(result_input, contract)?;
        nested_retained_bytes = nested_retained_bytes
            .checked_add(projection.nested_retained_bytes)
            .ok_or(DistributedWireError(
                "partial_decode_memory_projection_overflow",
            ))?;
        constructor_extra_bytes = constructor_extra_bytes.max(projection.constructor_extra_bytes);
    }
    reader.finish()?;
    Ok(PartialBatchDecodeProjection {
        result_count,
        nested_retained_bytes,
        constructor_extra_bytes,
    })
}

fn checked_partial_result_decode_projection(
    input: &[u8],
) -> Result<PartialResultDecodeProjection, DistributedWireError> {
    checked_partial_result_decode_projection_for_contract(
        input,
        PartialDecodeContract::BuildProbability,
    )
}

fn checked_partial_result_decode_projection_for_contract(
    input: &[u8],
    contract: PartialDecodeContract,
) -> Result<PartialResultDecodeProjection, DistributedWireError> {
    let mut reader = Reader::new(input);
    reader.require_header(PARTIAL_MAGIC)?;
    let mut nested_retained_bytes = 0_u128;
    let mut constructor_extra_bytes = 0_u128;

    let field_count = reader.count()?;
    checked_add_slots::<(String, String)>(&mut nested_retained_bytes, field_count)?;
    let mut backend_requested = None;
    let mut requested_backend = None;
    let mut backend_selected = None;
    let mut selected_backend = None;
    let mut backend_fallback_reason = None;
    let mut coverage_probability = None;
    let mut trace_retention_reason = None;
    for _ in 0..field_count {
        let key = reader.borrowed_string()?;
        let value = reader.borrowed_string()?;
        checked_add_usize(&mut nested_retained_bytes, key.len())?;
        checked_add_usize(&mut nested_retained_bytes, value.len())?;
        match key {
            "backend_requested" if backend_requested.is_none() => backend_requested = Some(value),
            "requested_backend" if requested_backend.is_none() => requested_backend = Some(value),
            "backend_selected" if backend_selected.is_none() => backend_selected = Some(value),
            "selected_backend" if selected_backend.is_none() => selected_backend = Some(value),
            "backend_fallback_reason" if backend_fallback_reason.is_none() => {
                backend_fallback_reason = Some(value)
            }
            "coverage_probability" if coverage_probability.is_none() => {
                coverage_probability = Some(value)
            }
            "trace_retention_reason" if trace_retention_reason.is_none() => {
                trace_retention_reason = Some(value)
            }
            _ => {}
        }
    }
    let field_slot_bytes = (field_count as u128)
        .checked_mul(core::mem::size_of::<(String, String)>() as u128)
        .ok_or(DistributedWireError(
            "partial_decode_memory_projection_overflow",
        ))?;
    constructor_extra_bytes = constructor_extra_bytes.max(field_slot_bytes);

    let path_count = reader.count()?;
    if contract == PartialDecodeContract::BuildProbability && path_count != 0 {
        return Err(DistributedWireError(
            "build_probability_partial_path_invalid",
        ));
    }
    checked_add_slots::<CorePathStep>(&mut nested_retained_bytes, path_count)?;
    for _ in 0..path_count {
        piece_from_code(reader.u8()?)?;
        reader.u8()?;
        reader.u8()?;
        hold_from_code(reader.u8()?)?;
        reader.i32()?;
        reader.i32()?;
    }
    for value in [
        backend_requested.or(requested_backend).unwrap_or("none"),
        backend_selected.or(selected_backend).unwrap_or("none"),
        backend_fallback_reason.unwrap_or("none"),
        coverage_probability.unwrap_or("0.0"),
        trace_retention_reason.unwrap_or("none"),
        trace_retention_reason.unwrap_or("none"),
    ] {
        checked_add_usize(&mut nested_retained_bytes, value.len())?;
    }

    let identity_count = reader.count()?;
    checked_add_slots::<StandardBoard64TilingIdentity>(&mut nested_retained_bytes, identity_count)?;
    for _ in 0..identity_count {
        decode_identity(&mut reader)?;
    }

    let normalized_key_count = reader.count()?;
    checked_add_slots::<String>(&mut nested_retained_bytes, normalized_key_count)?;
    for _ in 0..normalized_key_count {
        let key = reader.borrowed_string()?;
        checked_add_usize(&mut nested_retained_bytes, key.len())?;
    }

    match reader.u8()? {
        0 => {}
        1 => {
            decode_identity(&mut reader)?;
        }
        _ => return Err(DistributedWireError("partial_representative_flag_invalid")),
    }

    let coverage_count = reader.count()?;
    checked_add_slots::<u64>(&mut nested_retained_bytes, coverage_count)?;
    for _ in 0..coverage_count {
        reader.u64()?;
    }

    let solution_coverage_count = reader.count()?;
    checked_add_slots::<SolutionCoverage>(&mut nested_retained_bytes, solution_coverage_count)?;
    for _ in 0..solution_coverage_count {
        decode_identity(&mut reader)?;
        let pattern_count = reader.count()?;
        let word_count = reader.count()?;
        let (storage_bytes, extra_bytes) = checked_pattern_decode_projection(
            &mut reader,
            pattern_count,
            word_count,
            "partial_solution_coverage_shape_invalid",
        )?;
        checked_add_u128(&mut nested_retained_bytes, storage_bytes)?;
        constructor_extra_bytes = constructor_extra_bytes.max(extra_bytes);
    }

    let normalized_solution_coverage_count = reader.count()?;
    checked_add_slots::<NormalizedSolutionCoverage>(
        &mut nested_retained_bytes,
        normalized_solution_coverage_count,
    )?;
    for _ in 0..normalized_solution_coverage_count {
        let solution_key = reader.borrowed_string()?;
        checked_add_usize(&mut nested_retained_bytes, solution_key.len())?;
        let pattern_count = reader.count()?;
        let word_count = reader.count()?;
        let (storage_bytes, extra_bytes) = checked_pattern_decode_projection(
            &mut reader,
            pattern_count,
            word_count,
            "partial_normalized_solution_coverage_shape_invalid",
        )?;
        checked_add_u128(&mut nested_retained_bytes, storage_bytes)?;
        constructor_extra_bytes = constructor_extra_bytes.max(extra_bytes);
    }

    match reader.u8()? {
        0 => {}
        1 => {
            let profile_id = reader.borrowed_string()?;
            checked_add_usize(&mut nested_retained_bytes, profile_id.len())?;
            match reader.u8()? {
                0 | 1 => {}
                _ => return Err(DistributedWireError("partial_score_complete_flag_invalid")),
            }
            let cell_count = reader.count()?;
            checked_add_slots::<CorePostProcessScoreCell>(&mut nested_retained_bytes, cell_count)?;
            for _ in 0..cell_count {
                decode_identity(&mut reader)?;
                reader.count()?;
                let trace_identity = reader.borrowed_string()?;
                checked_add_usize(&mut nested_retained_bytes, trace_identity.len())?;
                reader.u64()?;
                reader.u32()?;
            }
        }
        _ => return Err(DistributedWireError("partial_score_shard_flag_invalid")),
    }

    let spin_coverage_count = reader.count()?;
    checked_add_slots::<CorePostProcessSpinCoverage>(
        &mut nested_retained_bytes,
        spin_coverage_count,
    )?;
    for _ in 0..spin_coverage_count {
        let target_id = reader.borrowed_string()?;
        checked_add_usize(&mut nested_retained_bytes, target_id.len())?;
        reader.count()?;
        let pattern_count = reader.count()?;
        let word_count = reader.count()?;
        if word_count != pattern_count.div_ceil(u64::BITS as usize) {
            return Err(DistributedWireError("partial_spin_coverage_shape_invalid"));
        }
        checked_add_slots::<u64>(&mut nested_retained_bytes, word_count)?;
        for word_index in 0..word_count {
            let word = reader.u64()?;
            validate_pattern_tail_word(
                pattern_count,
                word_index,
                word_count,
                word,
                "partial_spin_coverage_shape_invalid",
            )?;
        }
        let key_count = reader.count()?;
        checked_add_slots::<String>(&mut nested_retained_bytes, key_count)?;
        let mut previous_key = None;
        for _ in 0..key_count {
            let key = reader.borrowed_string()?;
            if previous_key.is_some_and(|previous| previous >= key) {
                return Err(DistributedWireError(
                    "partial_spin_candidate_keys_noncanonical",
                ));
            }
            checked_add_usize(&mut nested_retained_bytes, key.len())?;
            previous_key = Some(key);
        }
        reader.u128()?;
        match reader.u8()? {
            0 | 1 => {}
            _ => return Err(DistributedWireError("partial_spin_complete_flag_invalid")),
        }
    }
    match reader.u8()? {
        0 => {}
        1 => {
            if contract == PartialDecodeContract::BuildProbability {
                // BuildProbability has a distinct closed result contract and
                // must reject product-private PC chance transport before any
                // allocation or caller-memory admission.
                return Err(DistributedWireError(
                    "build_probability_partial_pc_chance_evidence_invalid",
                ));
            }
            let piece_source_id = reader.u64()?;
            let pattern_universe_id = reader.u64()?;
            let pattern_weight_model_id = reader.u64()?;
            if piece_source_id == 0 || pattern_universe_id == 0 || pattern_weight_model_id == 0 {
                return Err(DistributedWireError("partial_pc_chance_identity_invalid"));
            }
            let pattern_count = reader.count()?;
            match reader.u8()? {
                0 | 1 => {}
                _ => {
                    return Err(DistributedWireError(
                        "partial_pc_chance_complete_flag_invalid",
                    ))
                }
            }
            let row_count = reader.count()?;
            checked_add_slots::<CoverageRow>(&mut nested_retained_bytes, row_count)?;
            let mut previous_candidate_id = None;
            for _ in 0..row_count {
                let candidate_id = reader.u64()?;
                if previous_candidate_id.is_some_and(|previous| previous >= candidate_id) {
                    return Err(DistributedWireError(
                        "partial_pc_chance_candidate_order_invalid",
                    ));
                }
                let word_count = reader.count()?;
                let (storage_bytes, extra_bytes) = checked_pattern_decode_projection(
                    &mut reader,
                    pattern_count,
                    word_count,
                    "partial_pc_chance_coverage_shape_invalid",
                )?;
                checked_add_u128(&mut nested_retained_bytes, storage_bytes)?;
                constructor_extra_bytes = constructor_extra_bytes.max(extra_bytes);
                previous_candidate_id = Some(candidate_id);
            }
        }
        _ => {
            return Err(DistributedWireError(
                "partial_pc_chance_evidence_flag_invalid",
            ))
        }
    }
    reader.finish()?;
    Ok(PartialResultDecodeProjection {
        nested_retained_bytes,
        constructor_extra_bytes,
    })
}

fn checked_pattern_decode_projection(
    reader: &mut Reader<'_>,
    pattern_count: usize,
    word_count: usize,
    shape_error: &'static str,
) -> Result<(u128, u128), DistributedWireError> {
    if word_count != pattern_count.div_ceil(u64::BITS as usize) {
        return Err(DistributedWireError(shape_error));
    }
    for word_index in 0..word_count {
        let word = reader.u64()?;
        validate_pattern_tail_word(pattern_count, word_index, word_count, word, shape_error)?;
    }
    let dense_bytes = (word_count as u128)
        .checked_mul(core::mem::size_of::<u64>() as u128)
        .ok_or(DistributedWireError(
            "partial_decode_memory_projection_overflow",
        ))?;
    // BuildProbability ingress deliberately preserves the canonical wire
    // words as dense shared storage. During Vec -> Arc conversion both dense
    // payloads can coexist, so the final Arc is retained storage and the
    // temporary Vec is the constructor-only addition.
    Ok((dense_bytes, dense_bytes))
}

fn validate_pattern_tail_word(
    pattern_count: usize,
    word_index: usize,
    word_count: usize,
    word: u64,
    shape_error: &'static str,
) -> Result<(), DistributedWireError> {
    if word_index + 1 == word_count {
        let remainder = pattern_count % u64::BITS as usize;
        if remainder != 0 && word & !((1_u64 << remainder) - 1) != 0 {
            return Err(DistributedWireError(shape_error));
        }
    }
    Ok(())
}

fn checked_add_slots<T>(total: &mut u128, count: usize) -> Result<(), DistributedWireError> {
    let bytes = (count as u128)
        .checked_mul(core::mem::size_of::<T>() as u128)
        .ok_or(DistributedWireError(
            "partial_decode_memory_projection_overflow",
        ))?;
    checked_add_u128(total, bytes)
}

fn checked_add_usize(total: &mut u128, bytes: usize) -> Result<(), DistributedWireError> {
    checked_add_u128(total, bytes as u128)
}

fn checked_add_u128(total: &mut u128, bytes: u128) -> Result<(), DistributedWireError> {
    *total = total.checked_add(bytes).ok_or(DistributedWireError(
        "partial_decode_memory_projection_overflow",
    ))?;
    Ok(())
}

fn authorize_decode_memory<E>(
    base_bytes: u128,
    local_bytes: u128,
    checked_future_bytes: u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), E>,
) -> Result<(), GuardedDistributedWireError<E>> {
    let observed = base_bytes
        .checked_add(local_bytes)
        .and_then(|bytes| bytes.checked_add(checked_future_bytes))
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(
                DistributedWireError::decode_memory_projection_overflow(),
            )
        })?;
    memory_guard(observed).map_err(GuardedDistributedWireError::MemoryGuard)
}

fn guarded_reserve_exact<T, E>(
    values: &mut Vec<T>,
    count: usize,
    allocation_error: &'static str,
    base_bytes: u128,
    local_bytes: &mut u128,
    remaining_requested_bytes: &mut u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), E>,
) -> Result<(), GuardedDistributedWireError<E>> {
    let requested_bytes = (count as u128)
        .checked_mul(core::mem::size_of::<T>() as u128)
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(
                DistributedWireError::decode_memory_projection_overflow(),
            )
        })?;
    let remaining_after = remaining_requested_bytes
        .checked_sub(requested_bytes)
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(
                DistributedWireError::decode_memory_projection_overflow(),
            )
        })?;
    authorize_decode_memory(
        base_bytes,
        *local_bytes,
        *remaining_requested_bytes,
        memory_guard,
    )?;
    values
        .try_reserve_exact(count)
        .map_err(|_| GuardedDistributedWireError::Wire(DistributedWireError(allocation_error)))?;
    let actual_bytes = (values.capacity() as u128)
        .checked_mul(core::mem::size_of::<T>() as u128)
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(
                DistributedWireError::decode_memory_projection_overflow(),
            )
        })?;
    *remaining_requested_bytes = remaining_after;
    *local_bytes = local_bytes.checked_add(actual_bytes).ok_or_else(|| {
        GuardedDistributedWireError::Wire(DistributedWireError::decode_memory_projection_overflow())
    })?;
    authorize_decode_memory(
        base_bytes,
        *local_bytes,
        *remaining_requested_bytes,
        memory_guard,
    )
}

fn guarded_reserve_exact_temporary<T, E>(
    values: &mut Vec<T>,
    count: usize,
    allocation_error: &'static str,
    base_bytes: u128,
    local_bytes: &mut u128,
    remaining_requested_bytes: u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), E>,
) -> Result<(), GuardedDistributedWireError<E>> {
    let requested_bytes = (count as u128)
        .checked_mul(core::mem::size_of::<T>() as u128)
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(
                DistributedWireError::decode_memory_projection_overflow(),
            )
        })?;
    let requested_future_bytes = remaining_requested_bytes
        .checked_add(requested_bytes)
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(
                DistributedWireError::decode_memory_projection_overflow(),
            )
        })?;
    authorize_decode_memory(
        base_bytes,
        *local_bytes,
        requested_future_bytes,
        memory_guard,
    )?;
    values
        .try_reserve_exact(count)
        .map_err(|_| GuardedDistributedWireError::Wire(DistributedWireError(allocation_error)))?;
    let actual_bytes = (values.capacity() as u128)
        .checked_mul(core::mem::size_of::<T>() as u128)
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(
                DistributedWireError::decode_memory_projection_overflow(),
            )
        })?;
    *local_bytes = local_bytes.checked_add(actual_bytes).ok_or_else(|| {
        GuardedDistributedWireError::Wire(DistributedWireError::decode_memory_projection_overflow())
    })?;
    authorize_decode_memory(
        base_bytes,
        *local_bytes,
        remaining_requested_bytes,
        memory_guard,
    )
}

fn guarded_decode_string<E>(
    reader: &mut Reader<'_>,
    base_bytes: u128,
    local_bytes: &mut u128,
    remaining_requested_bytes: &mut u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), E>,
) -> Result<String, GuardedDistributedWireError<E>> {
    let value = reader
        .borrowed_string()
        .map_err(GuardedDistributedWireError::Wire)?;
    let remaining_after = remaining_requested_bytes
        .checked_sub(value.len() as u128)
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(
                DistributedWireError::decode_memory_projection_overflow(),
            )
        })?;
    authorize_decode_memory(
        base_bytes,
        *local_bytes,
        *remaining_requested_bytes,
        memory_guard,
    )?;
    let mut owned = String::new();
    owned.try_reserve_exact(value.len()).map_err(|_| {
        GuardedDistributedWireError::Wire(DistributedWireError(
            "distributed_wire_string_allocation_failed",
        ))
    })?;
    *local_bytes = local_bytes
        .checked_add(owned.capacity() as u128)
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(
                DistributedWireError::decode_memory_projection_overflow(),
            )
        })?;
    *remaining_requested_bytes = remaining_after;
    authorize_decode_memory(
        base_bytes,
        *local_bytes,
        *remaining_requested_bytes,
        memory_guard,
    )?;
    owned.push_str(value);
    Ok(owned)
}

fn guarded_decode_pattern_bitset<E>(
    reader: &mut Reader<'_>,
    pattern_count: usize,
    word_count: usize,
    shape_error: &'static str,
    allocation_error: &'static str,
    base_bytes: u128,
    local_bytes: &mut u128,
    remaining_requested_bytes: &mut u128,
    memory_guard: &mut impl FnMut(u128) -> Result<(), E>,
) -> Result<PatternBitSet, GuardedDistributedWireError<E>> {
    if word_count != pattern_count.div_ceil(u64::BITS as usize) {
        return Err(GuardedDistributedWireError::Wire(DistributedWireError(
            shape_error,
        )));
    }
    let mut words = Vec::new();
    guarded_reserve_exact_temporary(
        &mut words,
        word_count,
        allocation_error,
        base_bytes,
        local_bytes,
        *remaining_requested_bytes,
        memory_guard,
    )?;
    for _ in 0..word_count {
        words.push(reader.u64().map_err(GuardedDistributedWireError::Wire)?);
    }
    for (word_index, source_word) in words.iter().copied().enumerate() {
        if word_index + 1 == word_count {
            let remainder = pattern_count % u64::BITS as usize;
            if remainder != 0 && source_word & !((1_u64 << remainder) - 1) != 0 {
                return Err(GuardedDistributedWireError::Wire(DistributedWireError(
                    shape_error,
                )));
            }
        }
    }
    let dense_storage_bytes = (word_count as u128)
        .checked_mul(core::mem::size_of::<u64>() as u128)
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(
                DistributedWireError::decode_memory_projection_overflow(),
            )
        })?;
    let remaining_after = remaining_requested_bytes
        .checked_sub(dense_storage_bytes)
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(
                DistributedWireError::decode_memory_projection_overflow(),
            )
        })?;
    // The actual Vec backing is already live. The final dense Arc payload is
    // still in `remaining_requested_bytes`, giving the exact coexistence
    // checkpoint immediately before Vec -> Arc conversion.
    authorize_decode_memory(
        base_bytes,
        *local_bytes,
        *remaining_requested_bytes,
        memory_guard,
    )?;
    let word_storage_bytes = (words.capacity() as u128)
        .checked_mul(core::mem::size_of::<u64>() as u128)
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(
                DistributedWireError::decode_memory_projection_overflow(),
            )
        })?;
    let shared_words: Arc<[u64]> = words.into();
    let covered_patterns = PatternBitSet::from_shared_words(pattern_count, shared_words)
        .map_err(|_| GuardedDistributedWireError::Wire(DistributedWireError(shape_error)))?;
    let actual_storage_bytes = covered_patterns
        .checked_storage_retained_bytes()
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(
                DistributedWireError::decode_memory_projection_overflow(),
            )
        })?;
    if actual_storage_bytes != dense_storage_bytes {
        return Err(GuardedDistributedWireError::Wire(
            DistributedWireError::decode_memory_projection_overflow(),
        ));
    }
    *remaining_requested_bytes = remaining_after;
    *local_bytes = local_bytes
        .checked_sub(word_storage_bytes)
        .and_then(|bytes| bytes.checked_add(actual_storage_bytes))
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(
                DistributedWireError::decode_memory_projection_overflow(),
            )
        })?;
    authorize_decode_memory(
        base_bytes,
        *local_bytes,
        *remaining_requested_bytes,
        memory_guard,
    )?;
    Ok(covered_patterns)
}

fn checked_owned_field_storage_bytes(fields: &Vec<(String, String)>) -> Option<u128> {
    let mut bytes =
        (fields.capacity() as u128).checked_mul(core::mem::size_of::<(String, String)>() as u128)?;
    for (key, value) in fields {
        bytes = bytes
            .checked_add(key.capacity() as u128)?
            .checked_add(value.capacity() as u128)?;
    }
    Some(bytes)
}

fn decode_partial_result_with_memory_guard<E>(
    input: &[u8],
    base_bytes: u128,
    requested_nested_bytes: u128,
    contract: PartialDecodeContract,
    memory_guard: &mut impl FnMut(u128) -> Result<(), E>,
) -> Result<CoreExecutionResult, GuardedDistributedWireError<E>> {
    let mut reader = Reader::new(input);
    reader
        .require_header(PARTIAL_MAGIC)
        .map_err(GuardedDistributedWireError::Wire)?;
    let mut local_bytes = 0_u128;
    let mut remaining_requested_bytes = requested_nested_bytes;

    let field_count = reader.count().map_err(GuardedDistributedWireError::Wire)?;
    let mut fields = Vec::new();
    guarded_reserve_exact(
        &mut fields,
        field_count,
        "partial_fields_allocation_failed",
        base_bytes,
        &mut local_bytes,
        &mut remaining_requested_bytes,
        memory_guard,
    )?;
    for _ in 0..field_count {
        let key = guarded_decode_string(
            &mut reader,
            base_bytes,
            &mut local_bytes,
            &mut remaining_requested_bytes,
            memory_guard,
        )?;
        let value = guarded_decode_string(
            &mut reader,
            base_bytes,
            &mut local_bytes,
            &mut remaining_requested_bytes,
            memory_guard,
        )?;
        fields.push((key, value));
    }

    let path_count = reader.count().map_err(GuardedDistributedWireError::Wire)?;
    if contract == PartialDecodeContract::BuildProbability && path_count != 0 {
        return Err(GuardedDistributedWireError::Wire(DistributedWireError(
            "build_probability_partial_path_invalid",
        )));
    }
    let mut path = Vec::new();
    guarded_reserve_exact(
        &mut path,
        path_count,
        "partial_path_allocation_failed",
        base_bytes,
        &mut local_bytes,
        &mut remaining_requested_bytes,
        memory_guard,
    )?;
    for _ in 0..path_count {
        let piece = piece_from_code(reader.u8().map_err(GuardedDistributedWireError::Wire)?)
            .map_err(GuardedDistributedWireError::Wire)?;
        let rotation = reader.u8().map_err(GuardedDistributedWireError::Wire)?;
        let cleared_lines = reader.u8().map_err(GuardedDistributedWireError::Wire)?;
        let hold = hold_from_code(reader.u8().map_err(GuardedDistributedWireError::Wire)?)
            .map_err(GuardedDistributedWireError::Wire)?;
        let x = reader.i32().map_err(GuardedDistributedWireError::Wire)?;
        let y = reader.i32().map_err(GuardedDistributedWireError::Wire)?;
        path.push(CorePathStep::new(
            piece,
            rotation,
            x,
            y,
            hold,
            cleared_lines,
        ));
    }

    let identity_count = reader.count().map_err(GuardedDistributedWireError::Wire)?;
    let mut identities = Vec::new();
    guarded_reserve_exact(
        &mut identities,
        identity_count,
        "partial_identities_allocation_failed",
        base_bytes,
        &mut local_bytes,
        &mut remaining_requested_bytes,
        memory_guard,
    )?;
    for _ in 0..identity_count {
        identities.push(decode_identity(&mut reader).map_err(GuardedDistributedWireError::Wire)?);
    }

    let normalized_key_count = reader.count().map_err(GuardedDistributedWireError::Wire)?;
    let mut normalized_keys = Vec::new();
    guarded_reserve_exact(
        &mut normalized_keys,
        normalized_key_count,
        "partial_solution_keys_allocation_failed",
        base_bytes,
        &mut local_bytes,
        &mut remaining_requested_bytes,
        memory_guard,
    )?;
    for _ in 0..normalized_key_count {
        normalized_keys.push(guarded_decode_string(
            &mut reader,
            base_bytes,
            &mut local_bytes,
            &mut remaining_requested_bytes,
            memory_guard,
        )?);
    }

    let representative = match reader.u8().map_err(GuardedDistributedWireError::Wire)? {
        0 => None,
        1 => Some(decode_identity(&mut reader).map_err(GuardedDistributedWireError::Wire)?),
        _ => {
            return Err(GuardedDistributedWireError::Wire(DistributedWireError(
                "partial_representative_flag_invalid",
            )))
        }
    };

    let coverage_count = reader.count().map_err(GuardedDistributedWireError::Wire)?;
    let mut coverage = Vec::new();
    guarded_reserve_exact(
        &mut coverage,
        coverage_count,
        "partial_coverage_allocation_failed",
        base_bytes,
        &mut local_bytes,
        &mut remaining_requested_bytes,
        memory_guard,
    )?;
    for _ in 0..coverage_count {
        coverage.push(reader.u64().map_err(GuardedDistributedWireError::Wire)?);
    }

    let solution_coverage_count = reader.count().map_err(GuardedDistributedWireError::Wire)?;
    let mut solution_coverage = Vec::new();
    guarded_reserve_exact(
        &mut solution_coverage,
        solution_coverage_count,
        "partial_solution_coverage_allocation_failed",
        base_bytes,
        &mut local_bytes,
        &mut remaining_requested_bytes,
        memory_guard,
    )?;
    for _ in 0..solution_coverage_count {
        let identity = decode_identity(&mut reader).map_err(GuardedDistributedWireError::Wire)?;
        let pattern_count = reader.count().map_err(GuardedDistributedWireError::Wire)?;
        let word_count = reader.count().map_err(GuardedDistributedWireError::Wire)?;
        let covered_patterns = guarded_decode_pattern_bitset(
            &mut reader,
            pattern_count,
            word_count,
            "partial_solution_coverage_shape_invalid",
            "partial_solution_coverage_allocation_failed",
            base_bytes,
            &mut local_bytes,
            &mut remaining_requested_bytes,
            memory_guard,
        )?;
        solution_coverage.push(SolutionCoverage::new(identity, covered_patterns));
    }

    let normalized_solution_coverage_count =
        reader.count().map_err(GuardedDistributedWireError::Wire)?;
    let mut normalized_solution_coverage = Vec::new();
    guarded_reserve_exact(
        &mut normalized_solution_coverage,
        normalized_solution_coverage_count,
        "partial_normalized_solution_coverage_allocation_failed",
        base_bytes,
        &mut local_bytes,
        &mut remaining_requested_bytes,
        memory_guard,
    )?;
    for _ in 0..normalized_solution_coverage_count {
        let solution_key = guarded_decode_string(
            &mut reader,
            base_bytes,
            &mut local_bytes,
            &mut remaining_requested_bytes,
            memory_guard,
        )?;
        let pattern_count = reader.count().map_err(GuardedDistributedWireError::Wire)?;
        let word_count = reader.count().map_err(GuardedDistributedWireError::Wire)?;
        let covered_patterns = guarded_decode_pattern_bitset(
            &mut reader,
            pattern_count,
            word_count,
            "partial_normalized_solution_coverage_shape_invalid",
            "partial_normalized_solution_coverage_allocation_failed",
            base_bytes,
            &mut local_bytes,
            &mut remaining_requested_bytes,
            memory_guard,
        )?;
        normalized_solution_coverage.push(NormalizedSolutionCoverage::new(
            solution_key,
            covered_patterns,
        ));
    }

    let score_shard = match reader.u8().map_err(GuardedDistributedWireError::Wire)? {
        0 => None,
        1 => {
            let profile_id = guarded_decode_string(
                &mut reader,
                base_bytes,
                &mut local_bytes,
                &mut remaining_requested_bytes,
                memory_guard,
            )?;
            let complete = match reader.u8().map_err(GuardedDistributedWireError::Wire)? {
                0 => false,
                1 => true,
                _ => {
                    return Err(GuardedDistributedWireError::Wire(DistributedWireError(
                        "partial_score_complete_flag_invalid",
                    )))
                }
            };
            let cell_count = reader.count().map_err(GuardedDistributedWireError::Wire)?;
            let mut cells = Vec::new();
            guarded_reserve_exact(
                &mut cells,
                cell_count,
                "partial_score_cells_allocation_failed",
                base_bytes,
                &mut local_bytes,
                &mut remaining_requested_bytes,
                memory_guard,
            )?;
            for _ in 0..cell_count {
                let candidate_identity =
                    decode_identity(&mut reader).map_err(GuardedDistributedWireError::Wire)?;
                let pattern_id = reader.count().map_err(GuardedDistributedWireError::Wire)?;
                let trace_identity = guarded_decode_string(
                    &mut reader,
                    base_bytes,
                    &mut local_bytes,
                    &mut remaining_requested_bytes,
                    memory_guard,
                )?;
                let score = reader.u64().map_err(GuardedDistributedWireError::Wire)?;
                let attack = reader.u32().map_err(GuardedDistributedWireError::Wire)?;
                cells.push(CorePostProcessScoreCell::new(
                    candidate_identity,
                    pattern_id,
                    trace_identity,
                    score,
                    attack,
                ));
            }
            Some((profile_id, complete, cells))
        }
        _ => {
            return Err(GuardedDistributedWireError::Wire(DistributedWireError(
                "partial_score_shard_flag_invalid",
            )))
        }
    };

    let spin_coverage_count = reader.count().map_err(GuardedDistributedWireError::Wire)?;
    let mut spin_coverages = Vec::new();
    guarded_reserve_exact(
        &mut spin_coverages,
        spin_coverage_count,
        "partial_spin_coverage_allocation_failed",
        base_bytes,
        &mut local_bytes,
        &mut remaining_requested_bytes,
        memory_guard,
    )?;
    for _ in 0..spin_coverage_count {
        let target_id = guarded_decode_string(
            &mut reader,
            base_bytes,
            &mut local_bytes,
            &mut remaining_requested_bytes,
            memory_guard,
        )?;
        let pass_index = reader.count().map_err(GuardedDistributedWireError::Wire)?;
        let pattern_count = reader.count().map_err(GuardedDistributedWireError::Wire)?;
        let word_count = reader.count().map_err(GuardedDistributedWireError::Wire)?;
        if word_count != pattern_count.div_ceil(u64::BITS as usize) {
            return Err(GuardedDistributedWireError::Wire(DistributedWireError(
                "partial_spin_coverage_shape_invalid",
            )));
        }
        let mut words = Vec::new();
        guarded_reserve_exact(
            &mut words,
            word_count,
            "partial_spin_coverage_allocation_failed",
            base_bytes,
            &mut local_bytes,
            &mut remaining_requested_bytes,
            memory_guard,
        )?;
        for word_index in 0..word_count {
            let word = reader.u64().map_err(GuardedDistributedWireError::Wire)?;
            validate_pattern_tail_word(
                pattern_count,
                word_index,
                word_count,
                word,
                "partial_spin_coverage_shape_invalid",
            )
            .map_err(GuardedDistributedWireError::Wire)?;
            words.push(word);
        }
        let key_count = reader.count().map_err(GuardedDistributedWireError::Wire)?;
        let mut candidate_keys = Vec::new();
        guarded_reserve_exact(
            &mut candidate_keys,
            key_count,
            "partial_spin_key_allocation_failed",
            base_bytes,
            &mut local_bytes,
            &mut remaining_requested_bytes,
            memory_guard,
        )?;
        for _ in 0..key_count {
            let key = guarded_decode_string(
                &mut reader,
                base_bytes,
                &mut local_bytes,
                &mut remaining_requested_bytes,
                memory_guard,
            )?;
            if candidate_keys
                .last()
                .is_some_and(|previous: &String| previous.as_str() >= key.as_str())
            {
                return Err(GuardedDistributedWireError::Wire(DistributedWireError(
                    "partial_spin_candidate_keys_noncanonical",
                )));
            }
            candidate_keys.push(key);
        }
        let witnessed_pattern_count = reader.u128().map_err(GuardedDistributedWireError::Wire)?;
        let complete = match reader.u8().map_err(GuardedDistributedWireError::Wire)? {
            0 => false,
            1 => true,
            _ => {
                return Err(GuardedDistributedWireError::Wire(DistributedWireError(
                    "partial_spin_complete_flag_invalid",
                )))
            }
        };
        spin_coverages.push(CorePostProcessSpinCoverage::new(
            target_id,
            pass_index,
            pattern_count,
            words,
            candidate_keys,
            witnessed_pattern_count,
            complete,
        ));
    }
    let distributed_pc_chance_rows = match reader.u8().map_err(GuardedDistributedWireError::Wire)? {
        0 => None,
        1 => {
            if contract == PartialDecodeContract::BuildProbability {
                return Err(GuardedDistributedWireError::Wire(DistributedWireError(
                    "build_probability_partial_pc_chance_evidence_invalid",
                )));
            }
            let piece_source_id = reader.u64().map_err(GuardedDistributedWireError::Wire)?;
            let pattern_universe_id =
                PatternUniverseId::new(reader.u64().map_err(GuardedDistributedWireError::Wire)?);
            let pattern_weight_model_id =
                PatternWeightModelId::new(reader.u64().map_err(GuardedDistributedWireError::Wire)?);
            if piece_source_id == 0
                || pattern_universe_id.get() == 0
                || pattern_weight_model_id.get() == 0
            {
                return Err(GuardedDistributedWireError::Wire(DistributedWireError(
                    "partial_pc_chance_identity_invalid",
                )));
            }
            let pattern_count = reader.count().map_err(GuardedDistributedWireError::Wire)?;
            let complete = match reader.u8().map_err(GuardedDistributedWireError::Wire)? {
                0 => false,
                1 => true,
                _ => {
                    return Err(GuardedDistributedWireError::Wire(DistributedWireError(
                        "partial_pc_chance_complete_flag_invalid",
                    )))
                }
            };
            let row_count = reader.count().map_err(GuardedDistributedWireError::Wire)?;
            let mut rows = Vec::new();
            guarded_reserve_exact(
                &mut rows,
                row_count,
                "partial_pc_chance_rows_allocation_failed",
                base_bytes,
                &mut local_bytes,
                &mut remaining_requested_bytes,
                memory_guard,
            )?;
            let mut previous_candidate_id = None;
            for _ in 0..row_count {
                let candidate_id = reader.u64().map_err(GuardedDistributedWireError::Wire)?;
                if previous_candidate_id.is_some_and(|previous| previous >= candidate_id) {
                    return Err(GuardedDistributedWireError::Wire(DistributedWireError(
                        "partial_pc_chance_candidate_order_invalid",
                    )));
                }
                let word_count = reader.count().map_err(GuardedDistributedWireError::Wire)?;
                let coverage = guarded_decode_pattern_bitset(
                    &mut reader,
                    pattern_count,
                    word_count,
                    "partial_pc_chance_coverage_shape_invalid",
                    "partial_pc_chance_coverage_allocation_failed",
                    base_bytes,
                    &mut local_bytes,
                    &mut remaining_requested_bytes,
                    memory_guard,
                )?;
                rows.push(CoverageRow::new_with_piece_source(
                    candidate_id,
                    CoverageRowKind::Build,
                    piece_source_id,
                    pattern_universe_id,
                    pattern_weight_model_id,
                    coverage,
                ));
                previous_candidate_id = Some(candidate_id);
            }
            Some(
                DistributedPcChanceCoverageRows::try_from_untrusted_rows(
                    piece_source_id,
                    pattern_universe_id,
                    pattern_weight_model_id,
                    pattern_count,
                    rows,
                    complete,
                )
                .map_err(|_| {
                    GuardedDistributedWireError::Wire(DistributedWireError(
                        "partial_pc_chance_evidence_invalid",
                    ))
                })?,
            )
        }
        _ => {
            return Err(GuardedDistributedWireError::Wire(DistributedWireError(
                "partial_pc_chance_evidence_flag_invalid",
            )))
        }
    };
    reader.finish().map_err(GuardedDistributedWireError::Wire)?;

    let field_bytes = checked_owned_field_storage_bytes(&fields).ok_or_else(|| {
        GuardedDistributedWireError::Wire(DistributedWireError::decode_memory_projection_overflow())
    })?;
    let path_bytes = (path.capacity() as u128)
        .checked_mul(core::mem::size_of::<CorePathStep>() as u128)
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(
                DistributedWireError::decode_memory_projection_overflow(),
            )
        })?;
    let constructor_owned_bytes = field_bytes.checked_add(path_bytes).ok_or_else(|| {
        GuardedDistributedWireError::Wire(DistributedWireError::decode_memory_projection_overflow())
    })?;
    let other_local_bytes = local_bytes
        .checked_sub(constructor_owned_bytes)
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(
                DistributedWireError::decode_memory_projection_overflow(),
            )
        })?;
    let mut first_constructor_guard = true;
    let mut result = CoreExecutionResult::try_new_with_memory_guard(
        fields,
        path,
        |live_constructor_bytes, checked_future_bytes| {
            if first_constructor_guard {
                first_constructor_guard = false;
                if checked_future_bytes != remaining_requested_bytes {
                    return Err(GuardedDistributedWireError::Wire(
                        DistributedWireError::decode_memory_projection_overflow(),
                    ));
                }
            }
            let construction_bytes = other_local_bytes
                .checked_add(live_constructor_bytes)
                .ok_or_else(|| {
                    GuardedDistributedWireError::Wire(
                        DistributedWireError::decode_memory_projection_overflow(),
                    )
                })?;
            authorize_decode_memory(
                base_bytes,
                construction_bytes,
                checked_future_bytes,
                memory_guard,
            )
        },
    )
    .map_err(|error| match error {
        CoreResultFieldReplacementError::ProjectionOverflow => GuardedDistributedWireError::Wire(
            DistributedWireError::decode_memory_projection_overflow(),
        ),
        CoreResultFieldReplacementError::AllocationFailed { .. } => {
            GuardedDistributedWireError::Wire(DistributedWireError(
                "partial_report_allocation_failed",
            ))
        }
        CoreResultFieldReplacementError::MemoryGuard(error) => error,
    })?
    .with_normalized_solution_keys(normalized_keys)
    .with_normalized_solution_identities(identities)
    .with_representative_solution_identity(representative)
    .with_coverage_pattern_words(coverage)
    .with_solution_coverages(solution_coverage)
    .with_normalized_solution_coverages(normalized_solution_coverage)
    .with_postprocess_spin_coverages(spin_coverages);
    if let Some(rows) = distributed_pc_chance_rows {
        result = result.with_distributed_pc_chance_coverage_rows(rows);
    }
    let result = match score_shard {
        Some((profile_id, complete, cells)) => {
            result.with_postprocess_score_cells(cells, complete, profile_id)
        }
        None => result,
    };
    debug_assert!(!first_constructor_guard);
    let result_nested_bytes = result
        .checked_resource_retained_bytes()
        .and_then(|bytes| bytes.checked_sub(core::mem::size_of::<CoreExecutionResult>() as u128))
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(
                DistributedWireError::decode_memory_projection_overflow(),
            )
        })?;
    authorize_decode_memory(base_bytes, result_nested_bytes, 0, memory_guard)?;
    Ok(result)
}

pub fn decode_partial_result(input: &[u8]) -> Result<CoreExecutionResult, DistributedWireError> {
    let mut reader = Reader::new(input);
    reader.require_header(PARTIAL_MAGIC)?;
    let field_count = reader.count()?;
    let mut fields = Vec::new();
    fields
        .try_reserve_exact(field_count)
        .map_err(|_| DistributedWireError("partial_fields_allocation_failed"))?;
    for _ in 0..field_count {
        fields.push((reader.string()?, reader.string()?));
    }
    let path_count = reader.count()?;
    let mut path = Vec::new();
    path.try_reserve_exact(path_count)
        .map_err(|_| DistributedWireError("partial_path_allocation_failed"))?;
    for _ in 0..path_count {
        let piece = piece_from_code(reader.u8()?)?;
        let rotation = reader.u8()?;
        let cleared_lines = reader.u8()?;
        let hold = hold_from_code(reader.u8()?)?;
        let x = reader.i32()?;
        let y = reader.i32()?;
        path.push(CorePathStep::new(
            piece,
            rotation,
            x,
            y,
            hold,
            cleared_lines,
        ));
    }
    let identity_count = reader.count()?;
    let mut identities = Vec::new();
    identities
        .try_reserve_exact(identity_count)
        .map_err(|_| DistributedWireError("partial_identities_allocation_failed"))?;
    for _ in 0..identity_count {
        identities.push(decode_identity(&mut reader)?);
    }
    let normalized_key_count = reader.count()?;
    let mut normalized_keys = Vec::new();
    normalized_keys
        .try_reserve_exact(normalized_key_count)
        .map_err(|_| DistributedWireError("partial_solution_keys_allocation_failed"))?;
    for _ in 0..normalized_key_count {
        normalized_keys.push(reader.string()?);
    }
    let representative = match reader.u8()? {
        0 => None,
        1 => Some(decode_identity(&mut reader)?),
        _ => return Err(DistributedWireError("partial_representative_flag_invalid")),
    };
    let coverage_count = reader.count()?;
    let mut coverage = Vec::new();
    coverage
        .try_reserve_exact(coverage_count)
        .map_err(|_| DistributedWireError("partial_coverage_allocation_failed"))?;
    for _ in 0..coverage_count {
        coverage.push(reader.u64()?);
    }
    let solution_coverage_count = reader.count()?;
    let mut solution_coverage = Vec::new();
    solution_coverage
        .try_reserve_exact(solution_coverage_count)
        .map_err(|_| DistributedWireError("partial_solution_coverage_allocation_failed"))?;
    for _ in 0..solution_coverage_count {
        let identity = decode_identity(&mut reader)?;
        let pattern_count = reader.count()?;
        let word_count = reader.count()?;
        if word_count != pattern_count.div_ceil(u64::BITS as usize) {
            return Err(DistributedWireError(
                "partial_solution_coverage_shape_invalid",
            ));
        }
        let mut words = Vec::new();
        words
            .try_reserve_exact(word_count)
            .map_err(|_| DistributedWireError("partial_solution_coverage_allocation_failed"))?;
        for word_index in 0..word_count {
            let word = reader.u64()?;
            validate_pattern_tail_word(
                pattern_count,
                word_index,
                word_count,
                word,
                "partial_solution_coverage_shape_invalid",
            )?;
            words.push(word);
        }
        let covered_patterns = PatternBitSet::from_words(pattern_count, words)
            .map_err(|_| DistributedWireError("partial_solution_coverage_invalid"))?;
        solution_coverage.push(SolutionCoverage::new(identity, covered_patterns));
    }
    let normalized_solution_coverage_count = reader.count()?;
    let mut normalized_solution_coverage = Vec::new();
    normalized_solution_coverage
        .try_reserve_exact(normalized_solution_coverage_count)
        .map_err(|_| {
            DistributedWireError("partial_normalized_solution_coverage_allocation_failed")
        })?;
    for _ in 0..normalized_solution_coverage_count {
        let solution_key = reader.string()?;
        let pattern_count = reader.count()?;
        let word_count = reader.count()?;
        if word_count != pattern_count.div_ceil(u64::BITS as usize) {
            return Err(DistributedWireError(
                "partial_normalized_solution_coverage_shape_invalid",
            ));
        }
        let mut words = Vec::new();
        words.try_reserve_exact(word_count).map_err(|_| {
            DistributedWireError("partial_normalized_solution_coverage_allocation_failed")
        })?;
        for word_index in 0..word_count {
            let word = reader.u64()?;
            validate_pattern_tail_word(
                pattern_count,
                word_index,
                word_count,
                word,
                "partial_normalized_solution_coverage_shape_invalid",
            )?;
            words.push(word);
        }
        let covered_patterns = PatternBitSet::from_words(pattern_count, words)
            .map_err(|_| DistributedWireError("partial_normalized_solution_coverage_invalid"))?;
        normalized_solution_coverage.push(NormalizedSolutionCoverage::new(
            solution_key,
            covered_patterns,
        ));
    }
    let score_shard = match reader.u8()? {
        0 => None,
        1 => {
            let profile_id = reader.string()?;
            let complete = match reader.u8()? {
                0 => false,
                1 => true,
                _ => return Err(DistributedWireError("partial_score_complete_flag_invalid")),
            };
            let cell_count = reader.count()?;
            let mut cells = Vec::new();
            cells
                .try_reserve_exact(cell_count)
                .map_err(|_| DistributedWireError("partial_score_cells_allocation_failed"))?;
            for _ in 0..cell_count {
                let candidate_identity = decode_identity(&mut reader)?;
                let pattern_id = reader.count()?;
                let trace_identity = reader.string()?;
                let score = reader.u64()?;
                let attack = reader.u32()?;
                cells.push(CorePostProcessScoreCell::new(
                    candidate_identity,
                    pattern_id,
                    trace_identity,
                    score,
                    attack,
                ));
            }
            Some((profile_id, complete, cells))
        }
        _ => return Err(DistributedWireError("partial_score_shard_flag_invalid")),
    };
    let spin_coverage_count = reader.count()?;
    let mut spin_coverages = Vec::new();
    spin_coverages
        .try_reserve_exact(spin_coverage_count)
        .map_err(|_| DistributedWireError("partial_spin_coverage_allocation_failed"))?;
    for _ in 0..spin_coverage_count {
        let target_id = reader.string()?;
        let pass_index = reader.count()?;
        let pattern_count = reader.count()?;
        let word_count = reader.count()?;
        if word_count != pattern_count.div_ceil(u64::BITS as usize) {
            return Err(DistributedWireError("partial_spin_coverage_shape_invalid"));
        }
        let mut words = Vec::new();
        words
            .try_reserve_exact(word_count)
            .map_err(|_| DistributedWireError("partial_spin_coverage_allocation_failed"))?;
        for word_index in 0..word_count {
            let word = reader.u64()?;
            validate_pattern_tail_word(
                pattern_count,
                word_index,
                word_count,
                word,
                "partial_spin_coverage_shape_invalid",
            )?;
            words.push(word);
        }
        let key_count = reader.count()?;
        let mut candidate_keys = Vec::new();
        candidate_keys
            .try_reserve_exact(key_count)
            .map_err(|_| DistributedWireError("partial_spin_key_allocation_failed"))?;
        for _ in 0..key_count {
            let key = reader.string()?;
            if candidate_keys
                .last()
                .is_some_and(|previous: &String| previous.as_str() >= key.as_str())
            {
                return Err(DistributedWireError(
                    "partial_spin_candidate_keys_noncanonical",
                ));
            }
            candidate_keys.push(key);
        }
        let witnessed_pattern_count = reader.u128()?;
        let complete = match reader.u8()? {
            0 => false,
            1 => true,
            _ => return Err(DistributedWireError("partial_spin_complete_flag_invalid")),
        };
        spin_coverages.push(CorePostProcessSpinCoverage::new(
            target_id,
            pass_index,
            pattern_count,
            words,
            candidate_keys,
            witnessed_pattern_count,
            complete,
        ));
    }
    let distributed_pc_chance_rows = match reader.u8()? {
        0 => None,
        1 => {
            let piece_source_id = reader.u64()?;
            let pattern_universe_id = PatternUniverseId::new(reader.u64()?);
            let pattern_weight_model_id = PatternWeightModelId::new(reader.u64()?);
            if piece_source_id == 0
                || pattern_universe_id.get() == 0
                || pattern_weight_model_id.get() == 0
            {
                return Err(DistributedWireError("partial_pc_chance_identity_invalid"));
            }
            let pattern_count = reader.count()?;
            let complete = match reader.u8()? {
                0 => false,
                1 => true,
                _ => {
                    return Err(DistributedWireError(
                        "partial_pc_chance_complete_flag_invalid",
                    ))
                }
            };
            let row_count = reader.count()?;
            let mut rows = Vec::new();
            rows.try_reserve_exact(row_count)
                .map_err(|_| DistributedWireError("partial_pc_chance_rows_allocation_failed"))?;
            let mut previous_candidate_id = None;
            for _ in 0..row_count {
                let candidate_id = reader.u64()?;
                if previous_candidate_id.is_some_and(|previous| previous >= candidate_id) {
                    return Err(DistributedWireError(
                        "partial_pc_chance_candidate_order_invalid",
                    ));
                }
                let word_count = reader.count()?;
                if word_count != pattern_count.div_ceil(u64::BITS as usize) {
                    return Err(DistributedWireError(
                        "partial_pc_chance_coverage_shape_invalid",
                    ));
                }
                let mut words = Vec::new();
                words.try_reserve_exact(word_count).map_err(|_| {
                    DistributedWireError("partial_pc_chance_coverage_allocation_failed")
                })?;
                for word_index in 0..word_count {
                    let word = reader.u64()?;
                    validate_pattern_tail_word(
                        pattern_count,
                        word_index,
                        word_count,
                        word,
                        "partial_pc_chance_coverage_shape_invalid",
                    )?;
                    words.push(word);
                }
                let coverage = PatternBitSet::from_words(pattern_count, words)
                    .map_err(|_| DistributedWireError("partial_pc_chance_coverage_invalid"))?;
                rows.push(CoverageRow::new_with_piece_source(
                    candidate_id,
                    CoverageRowKind::Build,
                    piece_source_id,
                    pattern_universe_id,
                    pattern_weight_model_id,
                    coverage,
                ));
                previous_candidate_id = Some(candidate_id);
            }
            Some(
                DistributedPcChanceCoverageRows::try_from_untrusted_rows(
                    piece_source_id,
                    pattern_universe_id,
                    pattern_weight_model_id,
                    pattern_count,
                    rows,
                    complete,
                )
                .map_err(|_| DistributedWireError("partial_pc_chance_evidence_invalid"))?,
            )
        }
        _ => {
            return Err(DistributedWireError(
                "partial_pc_chance_evidence_flag_invalid",
            ))
        }
    };
    reader.finish()?;
    let mut result = CoreExecutionResult::new(fields, path)
        .with_normalized_solution_keys(normalized_keys)
        .with_normalized_solution_identities(identities)
        .with_representative_solution_identity(representative)
        .with_coverage_pattern_words(coverage)
        .with_solution_coverages(solution_coverage)
        .with_normalized_solution_coverages(normalized_solution_coverage)
        .with_postprocess_spin_coverages(spin_coverages);
    if let Some(rows) = distributed_pc_chance_rows {
        result = result.with_distributed_pc_chance_coverage_rows(rows);
    }
    Ok(match score_shard {
        Some((profile_id, complete, cells)) => {
            result.with_postprocess_score_cells(cells, complete, profile_id)
        }
        None => result,
    })
}

pub fn decode_partial_results(
    input: &[u8],
) -> Result<Vec<CoreExecutionResult>, DistributedWireError> {
    let mut reader = Reader::new(input);
    reader.require_header(PARTIAL_BATCH_MAGIC)?;
    let count = reader.count()?;
    let mut results = Vec::new();
    results
        .try_reserve_exact(count)
        .map_err(|_| DistributedWireError("partial_batch_allocation_failed"))?;
    for _ in 0..count {
        let length = reader.byte_length()?;
        results.push(decode_partial_result(reader.take(length)?)?);
    }
    reader.finish()?;
    Ok(results)
}

/// Decodes one BuildProbability partial-result batch only after an
/// allocation-free validation/projection pass proves the requested
/// construction peak. Build worker partitions never own public path steps, so
/// a non-empty path fails closed rather than entering an unguarded report
/// reconstruction. The guard is called before and after every owned reserve,
/// through Core's guarded field/report constructor, and after every
/// materialized result while all decoded siblings remain live.
pub fn decode_build_probability_partial_results_with_memory_guard<E>(
    input: &[u8],
    memory_guard: impl FnMut(u128) -> Result<(), E>,
) -> Result<Vec<CoreExecutionResult>, GuardedDistributedWireError<E>> {
    decode_partial_results_with_memory_guard_for_contract(
        input,
        PartialDecodeContract::BuildProbability,
        memory_guard,
    )
}

/// General distributed-result ingress with the same two-pass, whole-live
/// admission discipline as BuildProbability. Unlike the Build contract it
/// admits path/report state and typed PC-chance rows, while still rejecting
/// malformed counts and canonicality before any reserve.
pub fn decode_partial_results_with_memory_guard<E>(
    input: &[u8],
    memory_guard: impl FnMut(u128) -> Result<(), E>,
) -> Result<Vec<CoreExecutionResult>, GuardedDistributedWireError<E>> {
    decode_partial_results_with_memory_guard_for_contract(
        input,
        PartialDecodeContract::General,
        memory_guard,
    )
}

fn decode_partial_results_with_memory_guard_for_contract<E>(
    input: &[u8],
    contract: PartialDecodeContract,
    mut memory_guard: impl FnMut(u128) -> Result<(), E>,
) -> Result<Vec<CoreExecutionResult>, GuardedDistributedWireError<E>> {
    let projection = checked_partial_batch_decode_projection_for_contract(input, contract)
        .map_err(GuardedDistributedWireError::Wire)?;
    let requested_outer_bytes = (projection.result_count as u128)
        .checked_mul(core::mem::size_of::<CoreExecutionResult>() as u128)
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(DistributedWireError(
                "partial_decode_memory_projection_overflow",
            ))
        })?;
    let requested_peak = requested_outer_bytes
        .checked_add(projection.nested_retained_bytes)
        .and_then(|bytes| bytes.checked_add(projection.constructor_extra_bytes))
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(DistributedWireError(
                "partial_decode_memory_projection_overflow",
            ))
        })?;
    memory_guard(requested_peak).map_err(GuardedDistributedWireError::MemoryGuard)?;

    let mut reader = Reader::new(input);
    reader
        .require_header(PARTIAL_BATCH_MAGIC)
        .map_err(GuardedDistributedWireError::Wire)?;
    let count = reader.count().map_err(GuardedDistributedWireError::Wire)?;
    debug_assert_eq!(count, projection.result_count);
    let mut results = Vec::new();
    results.try_reserve_exact(count).map_err(|_| {
        GuardedDistributedWireError::Wire(DistributedWireError("partial_batch_allocation_failed"))
    })?;
    let allocated_outer_bytes = (results.capacity() as u128)
        .checked_mul(core::mem::size_of::<CoreExecutionResult>() as u128)
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(DistributedWireError(
                "partial_decode_memory_projection_overflow",
            ))
        })?;
    let allocated_outer_peak = allocated_outer_bytes
        .checked_add(projection.nested_retained_bytes)
        .and_then(|bytes| bytes.checked_add(projection.constructor_extra_bytes))
        .ok_or_else(|| {
            GuardedDistributedWireError::Wire(DistributedWireError(
                "partial_decode_memory_projection_overflow",
            ))
        })?;
    memory_guard(allocated_outer_peak).map_err(GuardedDistributedWireError::MemoryGuard)?;

    let mut remaining_requested_nested_bytes = projection.nested_retained_bytes;
    for _ in 0..count {
        let length = reader
            .byte_length()
            .map_err(GuardedDistributedWireError::Wire)?;
        let result_input = reader
            .take(length)
            .map_err(GuardedDistributedWireError::Wire)?;
        let result_projection =
            checked_partial_result_decode_projection_for_contract(result_input, contract)
                .map_err(GuardedDistributedWireError::Wire)?;
        let decoded_before =
            checked_decoded_result_vec_retained_bytes(&results).ok_or_else(|| {
                GuardedDistributedWireError::Wire(DistributedWireError(
                    "partial_decode_memory_projection_overflow",
                ))
            })?;
        let construction_peak = decoded_before
            .checked_add(remaining_requested_nested_bytes)
            .and_then(|bytes| bytes.checked_add(result_projection.constructor_extra_bytes))
            .ok_or_else(|| {
                GuardedDistributedWireError::Wire(DistributedWireError(
                    "partial_decode_memory_projection_overflow",
                ))
            })?;
        memory_guard(construction_peak).map_err(GuardedDistributedWireError::MemoryGuard)?;

        let remaining_after_result = remaining_requested_nested_bytes
            .checked_sub(result_projection.nested_retained_bytes)
            .ok_or_else(|| {
                GuardedDistributedWireError::Wire(
                    DistributedWireError::decode_memory_projection_overflow(),
                )
            })?;
        let result_base_bytes = decoded_before
            .checked_add(remaining_after_result)
            .ok_or_else(|| {
                GuardedDistributedWireError::Wire(
                    DistributedWireError::decode_memory_projection_overflow(),
                )
            })?;
        let result = decode_partial_result_with_memory_guard(
            result_input,
            result_base_bytes,
            result_projection.nested_retained_bytes,
            contract,
            &mut memory_guard,
        )?;
        remaining_requested_nested_bytes = remaining_after_result;
        results.push(result);
        let decoded_after =
            checked_decoded_result_vec_retained_bytes(&results).ok_or_else(|| {
                GuardedDistributedWireError::Wire(DistributedWireError(
                    "partial_decode_memory_projection_overflow",
                ))
            })?;
        let actual_with_remaining = decoded_after
            .checked_add(remaining_requested_nested_bytes)
            .ok_or_else(|| {
                GuardedDistributedWireError::Wire(DistributedWireError(
                    "partial_decode_memory_projection_overflow",
                ))
            })?;
        memory_guard(actual_with_remaining).map_err(GuardedDistributedWireError::MemoryGuard)?;
    }
    reader.finish().map_err(GuardedDistributedWireError::Wire)?;
    debug_assert_eq!(remaining_requested_nested_bytes, 0);
    Ok(results)
}

fn checked_decoded_result_vec_retained_bytes(results: &Vec<CoreExecutionResult>) -> Option<u128> {
    let result_inline = core::mem::size_of::<CoreExecutionResult>() as u128;
    let mut retained = (results.capacity() as u128).checked_mul(result_inline)?;
    for result in results {
        retained = retained.checked_add(
            result
                .checked_resource_retained_bytes()?
                .checked_sub(result_inline)?,
        )?;
    }
    Some(retained)
}

fn encode_identity(output: &mut Vec<u8>, identity: StandardBoard64TilingIdentity) {
    put_u64(output, identity.initial_board_mask());
    put_u64(output, identity.packed_piece_codes());
    output.push(identity.placement_count() as u8);
    for mask in identity.placement_masks() {
        put_u64(output, *mask);
    }
}

fn decode_identity(
    reader: &mut Reader<'_>,
) -> Result<StandardBoard64TilingIdentity, DistributedWireError> {
    let initial_board = reader.u64()?;
    let packed_piece_codes = reader.u64()?;
    let placement_count = usize::from(reader.u8()?);
    if placement_count > STANDARD_BOARD64_TILING_MAX_PLACEMENTS {
        return Err(DistributedWireError(
            "partial_identity_placement_count_invalid",
        ));
    }
    let mut placements =
        [PiecePlacementMask::new(PieceKind::I, 0); STANDARD_BOARD64_TILING_MAX_PLACEMENTS];
    for (index, placement) in placements.iter_mut().take(placement_count).enumerate() {
        let piece_code = ((packed_piece_codes >> (index * 3)) & 0x7) as u8;
        let piece = piece_from_code(piece_code)
            .map_err(|_| DistributedWireError("partial_identity_invalid"))?;
        *placement = PiecePlacementMask::new(piece, reader.u64()?);
    }
    let identity = StandardBoard64TilingIdentity::from_placements(
        initial_board,
        placements[..placement_count].iter().copied(),
    )
    .map_err(|_| DistributedWireError("partial_identity_invalid"))?;
    if identity.packed_piece_codes() != packed_piece_codes {
        return Err(DistributedWireError("partial_identity_invalid"));
    }
    Ok(identity)
}

fn piece_code(piece: PieceKind) -> u8 {
    match piece {
        PieceKind::I => 0,
        PieceKind::O => 1,
        PieceKind::T => 2,
        PieceKind::S => 3,
        PieceKind::Z => 4,
        PieceKind::J => 5,
        PieceKind::L => 6,
    }
}

fn piece_from_code(code: u8) -> Result<PieceKind, DistributedWireError> {
    PieceKind::STANDARD_TETROMINOES
        .get(usize::from(code))
        .copied()
        .ok_or(DistributedWireError("partial_piece_code_invalid"))
}

fn hold_code(hold: &str) -> u8 {
    match hold {
        "use-current" => 0,
        "swap-held" => 1,
        "store-current-use-next" => 2,
        "swap-held-with-unplaced-lookahead" => 3,
        "use-unplaced-lookahead" => 4,
        "store-current-use-unplaced-lookahead" => 5,
        "release-held-at-terminal" => 6,
        "none" => 7,
        _ => u8::MAX,
    }
}

fn hold_from_code(code: u8) -> Result<&'static str, DistributedWireError> {
    match code {
        0 => Ok("use-current"),
        1 => Ok("swap-held"),
        2 => Ok("store-current-use-next"),
        3 => Ok("swap-held-with-unplaced-lookahead"),
        4 => Ok("use-unplaced-lookahead"),
        5 => Ok("store-current-use-unplaced-lookahead"),
        6 => Ok("release-held-at-terminal"),
        7 => Ok("none"),
        _ => Err(DistributedWireError("partial_hold_code_invalid")),
    }
}

fn put_u32(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn put_i32(output: &mut Vec<u8>, value: i32) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn put_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn put_u128(output: &mut Vec<u8>, value: u128) {
    output.extend_from_slice(&value.to_le_bytes());
}

fn put_bytes(output: &mut Vec<u8>, value: &[u8]) {
    put_u32(output, value.len() as u32);
    output.extend_from_slice(value);
}

struct Reader<'a> {
    input: &'a [u8],
    cursor: usize,
}

impl<'a> Reader<'a> {
    const fn new(input: &'a [u8]) -> Self {
        Self { input, cursor: 0 }
    }

    fn require_header(&mut self, magic: u32) -> Result<(), DistributedWireError> {
        if self.u32()? != magic {
            return Err(DistributedWireError("distributed_wire_magic_mismatch"));
        }
        if self.u32()? != WIRE_VERSION {
            return Err(DistributedWireError("distributed_wire_version_unsupported"));
        }
        Ok(())
    }

    fn count(&mut self) -> Result<usize, DistributedWireError> {
        let count = self.u32()? as usize;
        if count > MAX_WIRE_ITEMS {
            return Err(DistributedWireError("distributed_wire_count_exceeded"));
        }
        Ok(count)
    }

    fn byte_length(&mut self) -> Result<usize, DistributedWireError> {
        Ok(self.u32()? as usize)
    }

    fn u8(&mut self) -> Result<u8, DistributedWireError> {
        Ok(*self
            .take(1)?
            .first()
            .ok_or(DistributedWireError("distributed_wire_truncated"))?)
    }

    fn u32(&mut self) -> Result<u32, DistributedWireError> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| DistributedWireError("distributed_wire_truncated"))?;
        Ok(u32::from_le_bytes(bytes))
    }

    fn i32(&mut self) -> Result<i32, DistributedWireError> {
        let bytes: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| DistributedWireError("distributed_wire_truncated"))?;
        Ok(i32::from_le_bytes(bytes))
    }

    fn u64(&mut self) -> Result<u64, DistributedWireError> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| DistributedWireError("distributed_wire_truncated"))?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn u128(&mut self) -> Result<u128, DistributedWireError> {
        let bytes: [u8; 16] = self
            .take(16)?
            .try_into()
            .map_err(|_| DistributedWireError("distributed_wire_truncated"))?;
        Ok(u128::from_le_bytes(bytes))
    }

    fn usize_u64(&mut self) -> Result<usize, DistributedWireError> {
        usize::try_from(self.u64()?)
            .map_err(|_| DistributedWireError("distributed_wire_usize_overflow"))
    }

    fn string(&mut self) -> Result<String, DistributedWireError> {
        let value = self.borrowed_string()?;
        let mut owned = String::new();
        owned
            .try_reserve_exact(value.len())
            .map_err(|_| DistributedWireError("distributed_wire_string_allocation_failed"))?;
        owned.push_str(value);
        Ok(owned)
    }

    fn borrowed_string(&mut self) -> Result<&'a str, DistributedWireError> {
        let length = self.byte_length()?;
        let bytes = self.take(length)?;
        std::str::from_utf8(bytes)
            .map_err(|_| DistributedWireError("distributed_wire_utf8_invalid"))
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], DistributedWireError> {
        let end = self
            .cursor
            .checked_add(length)
            .ok_or(DistributedWireError("distributed_wire_length_overflow"))?;
        let bytes = self
            .input
            .get(self.cursor..end)
            .ok_or(DistributedWireError("distributed_wire_truncated"))?;
        self.cursor = end;
        Ok(bytes)
    }

    fn finish(self) -> Result<(), DistributedWireError> {
        if self.cursor == self.input.len() {
            Ok(())
        } else {
            Err(DistributedWireError("distributed_wire_trailing_bytes"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_core_domain::execution_cancellation::ExecutionControl;
    use clearra_core_executor::{
        canonical_wasm_candidate_packet_batch_sha256, encode_canonical_wasm_candidate_packet_batch,
        WasmBuildProbabilityCandidateProducer, WasmCandidateProducerAdvance,
    };
    use clearra_pc_graph::request::{
        PcQueueInput, PcScenarioBoard, PcScenarioQuery, PcSolutionProbabilityPolicy, PieceWindow,
    };
    use clearra_problem::{
        BuildProbabilityAggregation, BuildProbabilityField, FinesseMetric, FinessePatternKnowledge,
        ProblemCompiler,
    };
    #[test]
    fn browser_wire_uses_the_actual_producer_packet_stream_kat() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(1),
        )
        .with_allow_hold(false)
        .with_exact_pieces(Some(1))
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit);
        let problem = ProblemCompiler::compile_scenario_pc(&query).expect("KAT problem");
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("compact one-I field");
        let mut producer =
            WasmBuildProbabilityCandidateProducer::new_with_finesse_and_verifiers_typed(
                &problem,
                field,
                BuildProbabilityAggregation::Buildability,
                FinesseMetric::Inputs,
                FinessePatternKnowledge::Both,
                1,
                0,
            )
            .expect("actual producer");
        let control = ExecutionControl::default();
        let mut packets = Vec::new();
        loop {
            match producer.advance(&control).expect("producer advance") {
                WasmCandidateProducerAdvance::Pending => {}
                WasmCandidateProducerAdvance::Candidate(packet) => packets.push(packet),
                WasmCandidateProducerAdvance::Completed(_) => break,
                WasmCandidateProducerAdvance::Cancelled => panic!("producer cancelled"),
            }
        }
        assert!(!packets.is_empty());
        let browser_wire = encode_candidate_batch(&packets);
        assert_eq!(
            browser_wire,
            encode_canonical_wasm_candidate_packet_batch(&packets)
        );
        assert_eq!(
            canonical_wasm_candidate_packet_batch_sha256(&packets),
            "71cc5dd0ab1d2188d562ab1bddd88ca0e94155e765f07b4d7576fe5a90fb3d9f"
        );
        assert_eq!(
            decode_candidate_batch(&browser_wire).expect("decode browser wire"),
            packets
        );
    }

    fn assert_decode_checkpoint_exact_and_peak_minus_one(
        encoded: &[u8],
        target_call: usize,
        expected_bytes: u128,
        label: &str,
    ) {
        let mut exact_calls = 0_usize;
        let exact =
            decode_build_probability_partial_results_with_memory_guard(encoded, |observed_bytes| {
                let call = exact_calls;
                exact_calls += 1;
                if call == target_call {
                    assert_eq!(observed_bytes, expected_bytes, "{label} exact checkpoint");
                    return (observed_bytes <= expected_bytes)
                        .then_some(())
                        .ok_or(target_call);
                }
                Ok(())
            });
        assert!(exact.is_ok(), "{label} exact checkpoint must succeed");
        assert!(
            exact_calls > target_call,
            "{label} exact checkpoint was not reached"
        );

        let below_cap = expected_bytes
            .checked_sub(1)
            .expect("guarded decode checkpoint is nonzero");
        let mut below_calls = 0_usize;
        let below =
            decode_build_probability_partial_results_with_memory_guard(encoded, |observed_bytes| {
                let call = below_calls;
                below_calls += 1;
                if call == target_call {
                    assert_eq!(observed_bytes, expected_bytes, "{label} below checkpoint");
                    return (observed_bytes <= below_cap)
                        .then_some(())
                        .ok_or(target_call);
                }
                // Earlier checkpoints are deliberately admitted so this
                // regression isolates the named stage instead of merely
                // rediscovering the batch-global maximum.
                Ok(())
            });
        assert!(
            matches!(
                below,
                Err(GuardedDistributedWireError::MemoryGuard(call))
                    if call == target_call
            ),
            "{label} peak-1 checkpoint must fail at the named stage"
        );
        assert_eq!(
            below_calls,
            target_call + 1,
            "{label} must fail before a later allocation checkpoint"
        );
    }

    fn decode_dense_pattern_for_test<E>(
        encoded_words: &[u8],
        memory_guard: &mut impl FnMut(u128) -> Result<(), E>,
    ) -> Result<PatternBitSet, GuardedDistributedWireError<E>> {
        const PATTERN_COUNT: usize = 4_096;
        const WORD_COUNT: usize = PATTERN_COUNT.div_ceil(u64::BITS as usize);
        const BASE_BYTES: u128 = 101;
        const PREEXISTING_LOCAL_BYTES: u128 = 13;

        let mut reader = Reader::new(encoded_words);
        let mut local_bytes = PREEXISTING_LOCAL_BYTES;
        let mut remaining_requested_bytes =
            (WORD_COUNT as u128) * core::mem::size_of::<u64>() as u128;
        let covered_patterns = guarded_decode_pattern_bitset(
            &mut reader,
            PATTERN_COUNT,
            WORD_COUNT,
            "test_dense_pattern_shape_invalid",
            "test_dense_pattern_allocation_failed",
            BASE_BYTES,
            &mut local_bytes,
            &mut remaining_requested_bytes,
            memory_guard,
        )?;
        reader.finish().map_err(GuardedDistributedWireError::Wire)?;
        assert_eq!(remaining_requested_bytes, 0);
        Ok(covered_patterns)
    }

    #[test]
    fn byte_lengths_are_bounded_by_the_input_instead_of_the_item_count_limit() {
        let encoded = u32::try_from(MAX_WIRE_ITEMS + 1)
            .expect("test length fits the wire")
            .to_le_bytes();

        let mut length_reader = Reader::new(&encoded);
        assert_eq!(
            length_reader.byte_length(),
            Ok(MAX_WIRE_ITEMS + 1),
            "a large result blob is not a collection with that many entries"
        );

        let mut count_reader = Reader::new(&encoded);
        assert_eq!(
            count_reader.count(),
            Err(DistributedWireError("distributed_wire_count_exceeded"))
        );
    }

    #[test]
    fn tiling_partial_wire_omits_reconstructable_string_keys() {
        let result = CoreExecutionResult::new(
            vec![("objective".to_owned(), "tiling".to_owned())],
            Vec::new(),
        )
        .with_normalized_solution_keys(vec!["reconstructed-at-merge".to_owned()]);

        let encoded = encode_partial_result(&result).expect("encode tiling partial result");
        let decoded = decode_partial_result(&encoded).expect("tiling partial result");

        assert!(decoded.normalized_solution_keys().is_empty());
    }

    #[test]
    fn partial_wire_round_trips_extended_normalized_solution_coverage() {
        let coverage = NormalizedSolutionCoverage::new(
            "extended-board-candidate",
            PatternBitSet::from_words(65, vec![0x5, 0x1]).expect("coverage bitset"),
        );
        let result = CoreExecutionResult::new(Vec::new(), Vec::new())
            .with_normalized_solution_coverages(vec![coverage.clone()]);

        let encoded = encode_partial_result(&result).expect("encode partial result");
        let decoded = decode_partial_result(&encoded).expect("partial result");

        assert_eq!(decoded.normalized_solution_coverages(), &[coverage]);
        assert!(decoded.solution_coverages().is_empty());
    }

    #[test]
    fn partial_wire_prefers_board64_solution_coverage_over_its_normalized_duplicate() {
        let identity = StandardBoard64TilingIdentity::from_placements(0, std::iter::empty())
            .expect("empty identity");
        let patterns = PatternBitSet::from_words(65, vec![0x5, 0x1]).expect("coverage bitset");
        let result = CoreExecutionResult::new(Vec::new(), Vec::new())
            .with_solution_coverages(vec![SolutionCoverage::new(identity, patterns.clone())])
            .with_normalized_solution_coverages(vec![NormalizedSolutionCoverage::new(
                "reconstructable-duplicate",
                patterns,
            )]);

        let encoded = encode_partial_result(&result).expect("encode partial result");
        let decoded = decode_partial_result(&encoded).expect("partial result");

        assert_eq!(decoded.solution_coverages(), result.solution_coverages());
        assert!(decoded.normalized_solution_coverages().is_empty());
    }

    fn typed_chance_transport_fixture() -> CoreExecutionResult {
        let piece_source_id = 11;
        let pattern_universe_id = PatternUniverseId::new(12);
        let pattern_weight_model_id = PatternWeightModelId::new(13);
        let rows = vec![
            CoverageRow::new_with_piece_source(
                7,
                CoverageRowKind::Build,
                piece_source_id,
                pattern_universe_id,
                pattern_weight_model_id,
                PatternBitSet::from_words(2, vec![0b01]).expect("first chance row"),
            ),
            CoverageRow::new_with_piece_source(
                11,
                CoverageRowKind::Build,
                piece_source_id,
                pattern_universe_id,
                pattern_weight_model_id,
                PatternBitSet::from_words(2, vec![0b10]).expect("second chance row"),
            ),
        ];
        let transport = DistributedPcChanceCoverageRows::try_from_untrusted_rows(
            piece_source_id,
            pattern_universe_id,
            pattern_weight_model_id,
            2,
            rows,
            true,
        )
        .expect("typed chance transport");
        CoreExecutionResult::new(Vec::new(), Vec::new())
            .with_coverage_pattern_words(vec![0b11])
            .with_distributed_pc_chance_coverage_rows(transport)
    }

    #[test]
    fn typed_chance_partial_wire_round_trips_byte_exact_with_closed_rows() {
        let result = typed_chance_transport_fixture();
        let encoded = encode_partial_result(&result).expect("encode typed chance partial");
        let decoded = decode_partial_result(&encoded).expect("decode typed chance partial");

        assert_eq!(
            decoded.distributed_pc_chance_coverage_rows(),
            result.distributed_pc_chance_coverage_rows()
        );
        assert!(decoded.pc_chance_coverage_evidence().is_none());
        assert_eq!(
            encode_partial_result(&decoded).expect("re-encode typed chance partial"),
            encoded
        );
    }

    #[test]
    fn typed_chance_guarded_batch_round_trips_and_counts_all_live_siblings() {
        let result = typed_chance_transport_fixture().with_path_steps(vec![CorePathStep::new(
            PieceKind::I,
            0,
            3,
            0,
            "none",
            0,
        )]);
        let encoded = encode_partial_results(&[result.clone(), result])
            .expect("encode typed chance sibling batch");
        let mut observations = Vec::new();
        let decoded = decode_partial_results_with_memory_guard(&encoded, |observed| {
            observations.push(observed);
            Ok::<(), ()>(())
        })
        .expect("guarded typed chance decode");
        let actual = checked_decoded_result_vec_retained_bytes(&decoded)
            .expect("checked typed chance result storage");

        assert_eq!(decoded.len(), 2);
        assert_eq!(observations.last(), Some(&actual));
        assert_eq!(
            encode_partial_results(&decoded).expect("re-encode guarded chance batch"),
            encoded
        );
        assert!(
            actual
                > decoded[0]
                    .checked_resource_retained_bytes()
                    .expect("single chance result storage"),
            "outer result capacity and the decoded sibling must remain admitted"
        );
    }

    #[test]
    fn guarded_reserve_rejection_happens_before_prefill() {
        let mut values = Vec::<u64>::new();
        let mut local_bytes = 0_u128;
        let mut remaining_requested_bytes = (4 * core::mem::size_of::<u64>()) as u128;
        let mut calls = 0_usize;
        let error = guarded_reserve_exact(
            &mut values,
            4,
            "test_allocation_failed",
            0,
            &mut local_bytes,
            &mut remaining_requested_bytes,
            &mut |_| {
                let call = calls;
                calls += 1;
                if call == 1 {
                    Err(call)
                } else {
                    Ok(())
                }
            },
        )
        .expect_err("post-reserve admission must be able to reject");

        assert!(matches!(error, GuardedDistributedWireError::MemoryGuard(1)));
        assert_eq!(calls, 2, "no later allocation may follow the rejection");
        assert_eq!(values.len(), 0, "reserved storage must not be filled first");
        assert!(values.capacity() >= 4, "the rejected call is post-reserve");
    }

    #[test]
    fn typed_chance_partial_wire_rejects_missing_duplicate_and_dirty_row_bytes() {
        let mut encoded =
            encode_partial_result(&typed_chance_transport_fixture()).expect("typed chance wire");
        let (flag_offset, complete_offset, second_candidate_offset, first_word_offset) = {
            let mut reader = Reader::new(&encoded);
            reader
                .require_header(PARTIAL_MAGIC)
                .expect("partial header");
            assert_eq!(reader.count().expect("fields"), 0);
            assert_eq!(reader.count().expect("path"), 0);
            assert_eq!(reader.count().expect("identities"), 0);
            assert_eq!(reader.count().expect("keys"), 0);
            assert_eq!(reader.u8().expect("representative"), 0);
            assert_eq!(reader.count().expect("aggregate words"), 1);
            reader.u64().expect("aggregate word");
            assert_eq!(reader.count().expect("solution coverage"), 0);
            assert_eq!(reader.count().expect("normalized coverage"), 0);
            assert_eq!(reader.u8().expect("score shard"), 0);
            assert_eq!(reader.count().expect("spin coverage"), 0);
            let flag_offset = reader.cursor;
            assert_eq!(reader.u8().expect("chance flag"), 1);
            reader.u64().expect("piece source");
            reader.u64().expect("pattern universe");
            reader.u64().expect("weight model");
            assert_eq!(reader.count().expect("pattern count"), 2);
            let complete_offset = reader.cursor;
            assert_eq!(reader.u8().expect("complete"), 1);
            assert_eq!(reader.count().expect("row count"), 2);
            assert_eq!(reader.u64().expect("first candidate"), 7);
            assert_eq!(reader.count().expect("first row words"), 1);
            let first_word_offset = reader.cursor;
            reader.u64().expect("first row word");
            let second_candidate_offset = reader.cursor;
            assert_eq!(reader.u64().expect("second candidate"), 11);
            assert_eq!(reader.count().expect("second row words"), 1);
            reader.u64().expect("second row word");
            reader.finish().expect("complete chance wire");
            (
                flag_offset,
                complete_offset,
                second_candidate_offset,
                first_word_offset,
            )
        };

        assert!(matches!(
            decode_partial_result(&encoded[..flag_offset]),
            Err(DistributedWireError("distributed_wire_truncated"))
        ));

        encoded[second_candidate_offset..second_candidate_offset + 8]
            .copy_from_slice(&7_u64.to_le_bytes());
        assert!(matches!(
            decode_partial_result(&encoded),
            Err(DistributedWireError(
                "partial_pc_chance_candidate_order_invalid"
            ))
        ));

        let mut invalid_complete =
            encode_partial_result(&typed_chance_transport_fixture()).expect("chance complete wire");
        invalid_complete[complete_offset] = 2;
        assert!(matches!(
            decode_partial_result(&invalid_complete),
            Err(DistributedWireError(
                "partial_pc_chance_complete_flag_invalid"
            ))
        ));

        let mut dirty_tail =
            encode_partial_result(&typed_chance_transport_fixture()).expect("chance tail wire");
        dirty_tail[first_word_offset + 7] |= 0x80;
        assert!(matches!(
            decode_partial_result(&dirty_tail),
            Err(DistributedWireError(
                "partial_pc_chance_coverage_shape_invalid"
            ))
        ));
    }

    #[test]
    fn guarded_partial_batch_encoder_rejects_peak_minus_one_and_round_trips_at_the_peak() {
        let sparse = PatternBitSet::from_pattern_indices(4_096, vec![1, 2_001])
            .expect("sparse coverage bitset");
        let sparse_cache_probe = sparse.clone();
        assert_eq!(sparse_cache_probe.storage_component_count(), 2);
        let results = vec![CoreExecutionResult::new(
            vec![("search_kind".to_owned(), "build-probability".to_owned())],
            Vec::new(),
        )
        .with_normalized_solution_coverages(vec![NormalizedSolutionCoverage::new(
            "sparse-candidate",
            sparse,
        )])];
        let mut observed = [0_u128; 2];
        let mut observed_count = 0usize;
        let encoded = encode_partial_results_with_memory_guard(&results, |future| {
            if observed_count < observed.len() {
                observed[observed_count] = future;
            }
            observed_count += 1;
            Ok::<(), ()>(())
        })
        .expect("guarded partial batch");
        assert_eq!(
            sparse_cache_probe.storage_component_count(),
            2,
            "wire sizing and emission must not populate a sparse dense-word cache"
        );
        assert_eq!(
            observed_count, 2,
            "requested and actual capacity are guarded"
        );
        let peak = observed.into_iter().max().expect("guard observations");
        let decoded = decode_partial_results(&encoded).expect("decode guarded batch");
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0].field("search_kind"), Some("build-probability"));
        assert_eq!(
            decoded[0].normalized_solution_coverages()[0]
                .covered_patterns()
                .word_at(31),
            1_u64 << 17
        );

        let exact = encode_partial_results_with_memory_guard(&results, |future| {
            (future <= peak)
                .then_some(())
                .ok_or("memory-budget-exceeded")
        });
        assert!(
            exact.is_ok(),
            "the observed exact peak must remain admissible"
        );

        let below = encode_partial_results_with_memory_guard(&results, |future| {
            (future < peak)
                .then_some(())
                .ok_or("memory-budget-exceeded")
        });
        assert!(matches!(
            below,
            Err(GuardedDistributedWireError::MemoryGuard(
                "memory-budget-exceeded"
            ))
        ));
    }

    #[test]
    fn guarded_dense_pattern_conversion_authorizes_vec_arc_coexistence_at_its_own_boundary() {
        const PATTERN_COUNT: usize = 4_096;
        const WORD_COUNT: usize = PATTERN_COUNT.div_ceil(u64::BITS as usize);
        let mut encoded_words = Vec::new();
        for word_index in 0..WORD_COUNT {
            let word = match word_index {
                0 => 1_u64 << 3,
                32 => 1_u64 << 1,
                _ => 0,
            };
            put_u64(&mut encoded_words, word);
        }

        let mut observations = Vec::new();
        let covered_patterns = decode_dense_pattern_for_test(&encoded_words, &mut |future| {
            observations.push(future);
            Ok::<(), ()>(())
        })
        .expect("guarded dense pattern");
        assert_eq!(
            observations.len(),
            4,
            "temporary requested/actual, pre-Arc coexistence, and final dense storage are distinct checkpoints"
        );
        let pre_arc_call = 2_usize;
        let pre_arc_bytes = observations[pre_arc_call];
        assert_eq!(
            observations[1], pre_arc_bytes,
            "allocator-actual Vec backing remains live at Arc conversion"
        );
        assert!(
            pre_arc_bytes > observations[3],
            "the conversion checkpoint must include both dense payloads"
        );
        assert_eq!(covered_patterns.storage_component_count(), 1);
        assert_eq!(
            covered_patterns.checked_storage_retained_bytes(),
            Some((WORD_COUNT * core::mem::size_of::<u64>()) as u128)
        );
        assert_eq!(covered_patterns.word_at(0), 1_u64 << 3);
        assert_eq!(covered_patterns.word_at(32), 1_u64 << 1);

        let mut exact_calls = 0_usize;
        let exact = decode_dense_pattern_for_test(&encoded_words, &mut |future| {
            let call = exact_calls;
            exact_calls += 1;
            if call == pre_arc_call {
                assert_eq!(future, pre_arc_bytes);
                return (future <= pre_arc_bytes).then_some(()).ok_or(pre_arc_call);
            }
            Ok(())
        });
        assert!(exact.is_ok(), "exact Vec-to-Arc coexistence must fit");

        let below_cap = pre_arc_bytes - 1;
        let mut below_calls = 0_usize;
        let below = decode_dense_pattern_for_test(&encoded_words, &mut |future| {
            let call = below_calls;
            below_calls += 1;
            if call == pre_arc_call {
                assert_eq!(future, pre_arc_bytes);
                return (future <= below_cap).then_some(()).ok_or(pre_arc_call);
            }
            Ok(())
        });
        assert!(matches!(
            below,
            Err(GuardedDistributedWireError::MemoryGuard(call)) if call == pre_arc_call
        ));
        assert_eq!(below_calls, pre_arc_call + 1);
    }

    #[test]
    fn guarded_partial_batch_decoder_guards_each_nested_stage_and_both_siblings() {
        let result = CoreExecutionResult::new(
            vec![
                ("search_kind".to_owned(), "build-probability".to_owned()),
                ("backend_requested".to_owned(), "wasm-cpu".to_owned()),
                ("backend_selected".to_owned(), "wasm-cpu".to_owned()),
                ("backend_fallback_reason".to_owned(), "none".to_owned()),
                ("coverage_probability".to_owned(), "0.500".to_owned()),
                (
                    "trace_retention_reason".to_owned(),
                    "distributed".to_owned(),
                ),
            ],
            Vec::new(),
        )
        .with_normalized_solution_coverages(vec![NormalizedSolutionCoverage::new(
            "wire-dense-candidate",
            PatternBitSet::from_pattern_indices(4_096, vec![3, 2_049])
                .expect("sparse source coverage"),
        )]);
        let results = vec![result.clone(), result];
        let encoded = encode_partial_results(&results).expect("encode sibling batch");
        let batch_projection =
            checked_partial_batch_decode_projection(&encoded).expect("batch projection");
        let (first_projection, second_projection) = {
            let mut reader = Reader::new(&encoded);
            reader
                .require_header(PARTIAL_BATCH_MAGIC)
                .expect("batch header");
            assert_eq!(reader.count().expect("batch count"), 2);
            let first_length = reader.byte_length().expect("first length");
            let first = reader.take(first_length).expect("first partial");
            let first_projection =
                checked_partial_result_decode_projection(first).expect("first projection");
            let second_length = reader.byte_length().expect("second length");
            let second = reader.take(second_length).expect("second partial");
            let second_projection =
                checked_partial_result_decode_projection(second).expect("second projection");
            reader.finish().expect("complete batch");
            (first_projection, second_projection)
        };
        assert_eq!(first_projection, second_projection);

        let mut observations = Vec::new();
        let decoded =
            decode_build_probability_partial_results_with_memory_guard(&encoded, |future| {
                observations.push(future);
                Ok::<(), ()>(())
            })
            .expect("guarded sibling decode");
        let actual = checked_decoded_result_vec_retained_bytes(&decoded)
            .expect("checked decoded sibling storage");
        assert_eq!(decoded.len(), 2);
        assert!(
            observations.len() >= 40,
            "Strings, Vec capacities, dense conversion, and rebuilt report strings must each re-enter the guard"
        );
        assert_eq!(observations.last(), Some(&actual));
        assert_eq!(
            encode_partial_results(&decoded).expect("re-encode dense sibling batch"),
            encoded,
            "guarded dense decoding must preserve the wire format exactly"
        );
        assert!(
            actual
                > decoded[0]
                    .checked_resource_retained_bytes()
                    .expect("first result storage"),
            "the guarded batch retains its outer Vec and the second decoded sibling"
        );
        for result in &decoded {
            assert_eq!(
                result.normalized_solution_coverages()[0]
                    .covered_patterns()
                    .storage_component_count(),
                1,
                "guarded wire coverage remains dense even when the producer was sparse"
            );
        }
        assert_eq!(
            decoded[0].normalized_solution_coverages()[0]
                .covered_patterns()
                .word_at(32),
            1_u64 << 1
        );
        assert_eq!(
            decoded[1].normalized_solution_coverages()[0]
                .covered_patterns()
                .word_at(32),
            1_u64 << 1
        );

        let outer_actual_bytes =
            (decoded.capacity() as u128) * core::mem::size_of::<CoreExecutionResult>() as u128;
        let expected_outer_actual = outer_actual_bytes
            + batch_projection.nested_retained_bytes
            + batch_projection.constructor_extra_bytes;
        assert_eq!(observations[1], expected_outer_actual);
        assert_decode_checkpoint_exact_and_peak_minus_one(
            &encoded,
            1,
            expected_outer_actual,
            "outer Vec allocator-actual",
        );

        let result_segments = observations
            .len()
            .checked_sub(2)
            .expect("batch request/outer checkpoints");
        assert_eq!(
            result_segments % 2,
            0,
            "identical siblings must emit identical checkpoint counts"
        );
        let calls_per_result = result_segments / 2;
        assert!(calls_per_result > 4, "each sibling has nested checkpoints");
        let first_segment_start = 2_usize;
        let first_segment_end = first_segment_start + calls_per_result;
        let second_segment_start = first_segment_end;
        let second_segment_end = second_segment_start + calls_per_result;
        assert_eq!(second_segment_end, observations.len());

        let first_nested_call = first_segment_start + 1;
        let second_nested_call = second_segment_start + 1;
        let first_actual_nested = decoded[0]
            .checked_resource_retained_bytes()
            .and_then(|bytes| {
                bytes.checked_sub(core::mem::size_of::<CoreExecutionResult>() as u128)
            })
            .expect("first actual nested bytes");
        assert_eq!(
            observations[first_nested_call],
            outer_actual_bytes
                + first_projection.nested_retained_bytes
                + second_projection.nested_retained_bytes,
            "the first sibling's first nested allocation includes the remaining sibling projection"
        );
        assert_eq!(
            observations[second_nested_call],
            outer_actual_bytes + first_actual_nested + second_projection.nested_retained_bytes,
            "the second sibling's first nested allocation includes the first sibling's allocator-actual storage"
        );

        // Isolate every callback inside both partial builders. This includes
        // requested/actual String reserves, every incremental Core report
        // String callback, Vec capacities, and the dense Vec-to-Arc stages.
        // Construction and post-result checkpoints bound each open range.
        for target_call in (first_segment_start + 1)..(first_segment_end - 1) {
            assert_decode_checkpoint_exact_and_peak_minus_one(
                &encoded,
                target_call,
                observations[target_call],
                &format!("first sibling nested checkpoint {target_call}"),
            );
        }
        for target_call in (second_segment_start + 1)..(second_segment_end - 1) {
            assert_decode_checkpoint_exact_and_peak_minus_one(
                &encoded,
                target_call,
                observations[target_call],
                &format!("second sibling nested checkpoint {target_call}"),
            );
        }
    }

    #[test]
    fn guarded_partial_batch_decoder_rejects_malformed_and_oversized_counts_before_guarding() {
        let mut oversized = Vec::new();
        put_u32(&mut oversized, PARTIAL_BATCH_MAGIC);
        put_u32(&mut oversized, WIRE_VERSION);
        put_u32(&mut oversized, 1);
        put_u32(&mut oversized, 12);
        put_u32(&mut oversized, PARTIAL_MAGIC);
        put_u32(&mut oversized, WIRE_VERSION);
        put_u32(
            &mut oversized,
            u32::try_from(MAX_WIRE_ITEMS + 1).expect("oversized count fits the wire"),
        );
        let mut guard_calls = 0_usize;
        let error = decode_build_probability_partial_results_with_memory_guard(&oversized, |_| {
            guard_calls += 1;
            Ok::<(), ()>(())
        })
        .expect_err("oversized inner count must fail closed");
        assert_eq!(guard_calls, 0, "malformed input must allocate nothing");
        assert!(matches!(
            error,
            GuardedDistributedWireError::Wire(DistributedWireError(
                "distributed_wire_count_exceeded"
            ))
        ));

        let mut truncated = Vec::new();
        put_u32(&mut truncated, PARTIAL_BATCH_MAGIC);
        put_u32(&mut truncated, WIRE_VERSION);
        put_u32(&mut truncated, 1);
        put_u32(&mut truncated, u32::MAX);
        let mut guard_calls = 0_usize;
        let error = decode_build_probability_partial_results_with_memory_guard(&truncated, |_| {
            guard_calls += 1;
            Ok::<(), ()>(())
        })
        .expect_err("truncated batch must fail closed");
        assert_eq!(guard_calls, 0, "truncated input must allocate nothing");
        assert!(matches!(
            error,
            GuardedDistributedWireError::Wire(DistributedWireError("distributed_wire_truncated"))
        ));

        let canonical_tail = CoreExecutionResult::new(Vec::new(), Vec::new())
            .with_normalized_solution_coverages(vec![NormalizedSolutionCoverage::new(
                "tail-candidate",
                PatternBitSet::from_words(65, vec![0x5, 0x1]).expect("canonical tail bitset"),
            )]);
        let mut dirty_tail =
            encode_partial_results(&[canonical_tail]).expect("encode canonical tail batch");
        let (result_start, tail_word_offset) = {
            let mut batch_reader = Reader::new(&dirty_tail);
            batch_reader
                .require_header(PARTIAL_BATCH_MAGIC)
                .expect("batch header");
            assert_eq!(batch_reader.count().expect("batch count"), 1);
            let result_length = batch_reader.byte_length().expect("partial length");
            let result_start = batch_reader.cursor;
            let result_input = batch_reader.take(result_length).expect("partial input");
            batch_reader.finish().expect("complete batch");

            let mut result_reader = Reader::new(result_input);
            result_reader
                .require_header(PARTIAL_MAGIC)
                .expect("partial header");
            assert_eq!(result_reader.count().expect("field count"), 0);
            assert_eq!(result_reader.count().expect("path count"), 0);
            assert_eq!(result_reader.count().expect("identity count"), 0);
            assert_eq!(result_reader.count().expect("key count"), 0);
            assert_eq!(result_reader.u8().expect("representative flag"), 0);
            assert_eq!(result_reader.count().expect("coverage count"), 0);
            assert_eq!(result_reader.count().expect("solution coverage count"), 0);
            assert_eq!(result_reader.count().expect("normalized coverage count"), 1);
            assert_eq!(
                result_reader
                    .borrowed_string()
                    .expect("normalized solution key"),
                "tail-candidate"
            );
            assert_eq!(result_reader.count().expect("pattern count"), 65);
            assert_eq!(result_reader.count().expect("word count"), 2);
            result_reader.u64().expect("first word");
            (result_start, result_reader.cursor)
        };
        dirty_tail[result_start + tail_word_offset] |= 0b10;
        let mut guard_calls = 0_usize;
        let error = decode_build_probability_partial_results_with_memory_guard(&dirty_tail, |_| {
            guard_calls += 1;
            Ok::<(), ()>(())
        })
        .expect_err("noncanonical tail bits must fail before dense conversion");
        assert_eq!(
            guard_calls, 0,
            "the allocation-free prepass rejects dirty tail bits"
        );
        assert!(matches!(
            error,
            GuardedDistributedWireError::Wire(DistributedWireError(
                "partial_normalized_solution_coverage_shape_invalid"
            ))
        ));
    }

    #[test]
    fn guarded_partial_spin_decoder_rejects_dirty_tail_and_noncanonical_candidate_keys_in_prepass()
    {
        let canonical = CoreExecutionResult::new(Vec::new(), Vec::new())
            .with_postprocess_spin_coverages(vec![CorePostProcessSpinCoverage::new(
                "spin-target",
                0,
                65,
                vec![0x5, 0x1],
                vec!["bravo".to_owned(), "alpha".to_owned()],
                2,
                true,
            )]);
        let encoded = encode_partial_results(&[canonical]).expect("encode canonical spin batch");
        let (result_start, tail_word_offset, second_key_offset, second_key_len) = {
            let mut batch_reader = Reader::new(&encoded);
            batch_reader
                .require_header(PARTIAL_BATCH_MAGIC)
                .expect("batch header");
            assert_eq!(batch_reader.count().expect("batch count"), 1);
            let result_length = batch_reader.byte_length().expect("partial length");
            let result_start = batch_reader.cursor;
            let result_input = batch_reader.take(result_length).expect("partial input");
            batch_reader.finish().expect("complete batch");

            let mut result_reader = Reader::new(result_input);
            result_reader
                .require_header(PARTIAL_MAGIC)
                .expect("partial header");
            assert_eq!(result_reader.count().expect("field count"), 0);
            assert_eq!(result_reader.count().expect("path count"), 0);
            assert_eq!(result_reader.count().expect("identity count"), 0);
            assert_eq!(result_reader.count().expect("key count"), 0);
            assert_eq!(result_reader.u8().expect("representative flag"), 0);
            assert_eq!(result_reader.count().expect("coverage count"), 0);
            assert_eq!(result_reader.count().expect("solution coverage count"), 0);
            assert_eq!(result_reader.count().expect("normalized coverage count"), 0);
            assert_eq!(result_reader.u8().expect("score shard flag"), 0);
            assert_eq!(result_reader.count().expect("spin coverage count"), 1);
            assert_eq!(
                result_reader.borrowed_string().expect("spin target"),
                "spin-target"
            );
            assert_eq!(result_reader.count().expect("pass index"), 0);
            assert_eq!(result_reader.count().expect("pattern count"), 65);
            assert_eq!(result_reader.count().expect("word count"), 2);
            result_reader.u64().expect("first word");
            let tail_word_offset = result_reader.cursor;
            assert_eq!(result_reader.u64().expect("tail word"), 1);
            assert_eq!(result_reader.count().expect("candidate key count"), 2);
            assert_eq!(
                result_reader
                    .borrowed_string()
                    .expect("first candidate key"),
                "alpha"
            );
            let second_key_len = result_reader.byte_length().expect("second key length");
            let second_key_offset = result_reader.cursor;
            assert_eq!(
                std::str::from_utf8(
                    result_reader
                        .take(second_key_len)
                        .expect("second candidate key bytes")
                )
                .expect("candidate key utf8"),
                "bravo"
            );
            (
                result_start,
                tail_word_offset,
                second_key_offset,
                second_key_len,
            )
        };

        let mut dirty_tail = encoded.clone();
        dirty_tail[result_start + tail_word_offset] |= 0b10;
        let mut guard_calls = 0_usize;
        let error = decode_build_probability_partial_results_with_memory_guard(&dirty_tail, |_| {
            guard_calls += 1;
            Ok::<(), ()>(())
        })
        .expect_err("dirty spin tail must fail closed");
        assert_eq!(guard_calls, 0, "the prepass rejects dirty spin tail bits");
        assert!(matches!(
            error,
            GuardedDistributedWireError::Wire(DistributedWireError(
                "partial_spin_coverage_shape_invalid"
            ))
        ));

        let mut duplicate_key = encoded;
        assert_eq!(second_key_len, "alpha".len());
        duplicate_key
            [result_start + second_key_offset..result_start + second_key_offset + second_key_len]
            .copy_from_slice(b"alpha");
        let mut guard_calls = 0_usize;
        let error =
            decode_build_probability_partial_results_with_memory_guard(&duplicate_key, |_| {
                guard_calls += 1;
                Ok::<(), ()>(())
            })
            .expect_err("duplicate spin candidate key must fail closed");
        assert_eq!(
            guard_calls, 0,
            "the prepass rejects noncanonical spin candidate keys"
        );
        assert!(matches!(
            error,
            GuardedDistributedWireError::Wire(DistributedWireError(
                "partial_spin_candidate_keys_noncanonical"
            ))
        ));
    }

    #[test]
    fn tiling_root_chunk_round_trips_fixed_size_identities() {
        let chunk = WasmTilingRootChunk::from_wire_parts(
            2,
            7,
            3,
            true,
            vec![
                WasmPackedTilingIdentity::new(11, [1, 2, 3]),
                WasmPackedTilingIdentity::new(19, [4, 5, 6]),
            ],
            2,
            Some(17),
            31,
            7,
            5,
            3,
            2,
            1,
        );

        let encoded = encode_tiling_root_chunk(&chunk);
        assert!(is_tiling_root_chunk(&encoded));
        assert_eq!(
            decode_tiling_root_chunk(&encoded).expect("tiling root chunk"),
            chunk
        );
    }
}
