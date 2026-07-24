use clearra_core_domain::{
    piece::piece_kind::PieceKind,
    solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
};
use clearra_core_executor::{
    CoreExecutionResult, CorePathStep, CorePostProcessScoreCell, CorePostProcessSpinCoverage,
    SolutionCoverage, WasmCandidatePacket,
};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;

const CANDIDATE_MAGIC: u32 = 0x4342_4131;
const PARTIAL_MAGIC: u32 = 0x5052_5431;
const PARTIAL_BATCH_MAGIC: u32 = 0x5052_4231;
const WIRE_VERSION: u32 = 7;
const MAX_WIRE_ITEMS: usize = 16_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DistributedWireError(&'static str);

impl DistributedWireError {
    pub const fn reason(self) -> &'static str {
        self.0
    }
}

pub fn encode_candidate_batch(candidates: &[WasmCandidatePacket]) -> Vec<u8> {
    let row_count = candidates
        .iter()
        .map(|candidate| candidate.row_ids().len())
        .sum::<usize>();
    let mut output = Vec::with_capacity(12 + candidates.len() * 24 + row_count * 4);
    put_u32(&mut output, CANDIDATE_MAGIC);
    put_u32(&mut output, WIRE_VERSION);
    put_u32(&mut output, candidates.len() as u32);
    for candidate in candidates {
        put_u64(&mut output, candidate.ordinal());
        put_u32(&mut output, u32::from(candidate.pass_index()));
        put_u32(&mut output, candidate.target_index());
        put_u32(&mut output, candidate.row_ids().len() as u32);
        for row_id in candidate.row_ids() {
            put_u32(&mut output, *row_id);
        }
    }
    output
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

pub fn encode_partial_result(result: &CoreExecutionResult) -> Vec<u8> {
    let fields = result.summary_fields();
    let path = result.path_steps();
    let identities = result.normalized_solution_identities();
    let normalized_keys = result.normalized_solution_keys();
    let coverage = result.coverage_pattern_words();
    let solution_coverage = result.solution_coverages();
    let score_cells = result.postprocess_score_cells();
    let spin_coverages = result.postprocess_spin_coverages();
    let estimated = fields
        .iter()
        .map(|(key, value)| key.len() + value.len() + 8)
        .sum::<usize>()
        .saturating_add(path.len() * 20)
        .saturating_add(identities.len() * 152)
        .saturating_add(
            normalized_keys
                .iter()
                .map(|key| key.len().saturating_add(4))
                .sum::<usize>(),
        )
        .saturating_add(coverage.len() * 8)
        .saturating_add(
            solution_coverage
                .iter()
                .map(|entry| 160 + entry.covered_patterns().word_count() * 8)
                .sum::<usize>(),
        )
        .saturating_add(
            spin_coverages
                .iter()
                .map(|coverage| {
                    48usize
                        .saturating_add(coverage.target_id().len())
                        .saturating_add(coverage.covered_pattern_words().len() * 8)
                        .saturating_add(
                            coverage
                                .candidate_keys()
                                .iter()
                                .map(|key| key.len().saturating_add(4))
                                .sum::<usize>(),
                        )
                })
                .sum::<usize>(),
        )
        .saturating_add(
            score_cells
                .iter()
                .map(|cell| 181usize.saturating_add(cell.trace_identity().len()))
                .sum::<usize>(),
        )
        .saturating_add(64);
    let mut output = Vec::with_capacity(estimated);
    put_u32(&mut output, PARTIAL_MAGIC);
    put_u32(&mut output, WIRE_VERSION);
    put_u32(&mut output, fields.len() as u32);
    for (key, value) in fields {
        put_bytes(&mut output, key.as_bytes());
        put_bytes(&mut output, value.as_bytes());
    }
    put_u32(&mut output, path.len() as u32);
    for step in path {
        output.push(piece_code(step.piece()));
        output.push(step.rotation());
        output.push(step.cleared_lines());
        output.push(hold_code(step.hold()));
        put_i32(&mut output, step.x());
        put_i32(&mut output, step.y());
    }
    put_u32(&mut output, identities.len() as u32);
    for identity in identities {
        encode_identity(&mut output, *identity);
    }
    put_u32(&mut output, normalized_keys.len() as u32);
    for key in normalized_keys {
        put_bytes(&mut output, key.as_bytes());
    }
    match result.representative_solution_identity() {
        Some(identity) => {
            output.push(1);
            encode_identity(&mut output, identity);
        }
        None => output.push(0),
    }
    put_u32(&mut output, coverage.len() as u32);
    for word in coverage {
        put_u64(&mut output, *word);
    }
    put_u32(&mut output, solution_coverage.len() as u32);
    for entry in solution_coverage {
        encode_identity(&mut output, entry.identity());
        put_u32(&mut output, entry.covered_patterns().pattern_count() as u32);
        put_u32(&mut output, entry.covered_patterns().word_count() as u32);
        for word in entry.covered_patterns().words() {
            put_u64(&mut output, *word);
        }
    }
    match result.postprocess_score_profile_id() {
        Some(profile_id) => {
            output.push(1);
            put_bytes(&mut output, profile_id.as_bytes());
            output.push(u8::from(result.postprocess_score_cells_complete()));
            put_u32(&mut output, score_cells.len() as u32);
            for cell in score_cells {
                encode_identity(&mut output, cell.candidate_identity());
                put_u32(&mut output, cell.pattern_id() as u32);
                put_bytes(&mut output, cell.trace_identity().as_bytes());
                put_u64(&mut output, cell.score());
                put_u32(&mut output, cell.attack());
            }
        }
        None => output.push(0),
    }
    put_u32(&mut output, spin_coverages.len() as u32);
    for coverage in spin_coverages {
        put_bytes(&mut output, coverage.target_id().as_bytes());
        put_u32(&mut output, coverage.pass_index() as u32);
        put_u32(&mut output, coverage.pattern_count() as u32);
        put_u32(&mut output, coverage.covered_pattern_words().len() as u32);
        for word in coverage.covered_pattern_words() {
            put_u64(&mut output, *word);
        }
        put_u32(&mut output, coverage.candidate_keys().len() as u32);
        for key in coverage.candidate_keys() {
            put_bytes(&mut output, key.as_bytes());
        }
        put_u128(&mut output, coverage.witnessed_pattern_count());
        output.push(u8::from(coverage.complete()));
    }
    output
}

pub fn encode_partial_results(results: &[CoreExecutionResult]) -> Vec<u8> {
    let encoded = results
        .iter()
        .map(encode_partial_result)
        .collect::<Vec<_>>();
    let capacity = encoded
        .iter()
        .map(|result| result.len().saturating_add(4))
        .sum::<usize>()
        .saturating_add(12);
    let mut output = Vec::with_capacity(capacity);
    put_u32(&mut output, PARTIAL_BATCH_MAGIC);
    put_u32(&mut output, WIRE_VERSION);
    put_u32(&mut output, encoded.len() as u32);
    for result in encoded {
        put_bytes(&mut output, &result);
    }
    output
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
        for _ in 0..word_count {
            words.push(reader.u64()?);
        }
        let covered_patterns = PatternBitSet::from_words(pattern_count, words)
            .map_err(|_| DistributedWireError("partial_solution_coverage_invalid"))?;
        solution_coverage.push(SolutionCoverage::new(identity, covered_patterns));
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
        for _ in 0..word_count {
            words.push(reader.u64()?);
        }
        let key_count = reader.count()?;
        let mut candidate_keys = Vec::new();
        candidate_keys
            .try_reserve_exact(key_count)
            .map_err(|_| DistributedWireError("partial_spin_key_allocation_failed"))?;
        for _ in 0..key_count {
            candidate_keys.push(reader.string()?);
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
    reader.finish()?;
    let result = CoreExecutionResult::new(fields, path)
        .with_normalized_solution_keys(normalized_keys)
        .with_normalized_solution_identities(identities)
        .with_representative_solution_identity(representative)
        .with_coverage_pattern_words(coverage)
        .with_solution_coverages(solution_coverage)
        .with_postprocess_spin_coverages(spin_coverages);
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
        let length = reader.count()?;
        results.push(decode_partial_result(reader.take(length)?)?);
    }
    reader.finish()?;
    Ok(results)
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
    let pieces = reader.u64()?;
    let placement_count = usize::from(reader.u8()?);
    if placement_count > 16 {
        return Err(DistributedWireError(
            "partial_identity_placement_count_invalid",
        ));
    }
    let mut masks = Vec::with_capacity(placement_count);
    for _ in 0..placement_count {
        masks.push(reader.u64()?);
    }
    StandardBoard64TilingIdentity::from_compact_parts(initial_board, pieces, &masks)
        .map_err(|_| DistributedWireError("partial_identity_invalid"))
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

    fn string(&mut self) -> Result<String, DistributedWireError> {
        let length = self.count()?;
        let bytes = self.take(length)?;
        std::str::from_utf8(bytes)
            .map(str::to_owned)
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
