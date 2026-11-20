//! Per-solution benchmark evidence. Exact queue ordinals are bound to the
//! original uniform supply universe, not to the union of all solutions.

use std::{
    collections::HashMap,
    io::{self, Read, Write},
};

use clearra_core_domain::solution::normalized_tiling_solution::NormalizedTilingSolutionKey;

const MAGIC: &[u8; 8] = b"CLRCOV1\0";
const MAX_KEY_BYTES: usize = 512;
const SUFFIX_BYTES: usize = 105; // 840 possible four-draw tails following each first bag.
const SUFFIX_BITS: usize = SUFFIX_BYTES * 8;

/// Tags freeze the fixture's rule, board, hold, CountUnique, output mode,
/// supply order, queue length and unit weight. P7P4 uses the original
/// StandardTetrominoes order over 7! x 7P4, before hold projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CoverageUniverse {
    P7P4,   // Empty 10x4, Jstris180, enabled-empty hold, 10 locks and 11 draws.
    SmallI, // Board 0x3f0, Jstris180, no hold, fixed I.
    #[cfg(test)]
    Tiny,
}

impl CoverageUniverse {
    pub(super) const fn queue_count(self) -> u32 {
        match self {
            Self::P7P4 => 4_233_600,
            Self::SmallI => 1,
            #[cfg(test)]
            Self::Tiny => 4,
        }
    }

    const fn solution_count(self) -> u32 {
        match self {
            Self::P7P4 => 456_459,
            Self::SmallI => 1,
            #[cfg(test)]
            Self::Tiny => 2,
        }
    }

    const fn tag(self) -> u8 {
        match self {
            Self::P7P4 => 1,
            Self::SmallI => 2,
            #[cfg(test)]
            Self::Tiny => 255,
        }
    }

    fn from_tag(tag: u8) -> io::Result<Self> {
        match tag {
            1 => Ok(Self::P7P4),
            2 => Ok(Self::SmallI),
            #[cfg(test)]
            255 => Ok(Self::Tiny),
            _ => Err(invalid("unknown coverage universe")),
        }
    }

    const fn byte_count(self) -> usize {
        (self.queue_count() as usize).div_ceil(8)
    }

    const fn word_count(self) -> usize {
        (self.queue_count() as usize).div_ceil(64)
    }

    const fn block_count(self) -> usize {
        self.queue_count() as usize / SUFFIX_BITS
    }

    fn validate_key(self, key: &str) -> io::Result<()> {
        if !key.starts_with("ctk1|") || key.len() > MAX_KEY_BYTES {
            return Err(invalid("expected a bounded canonical CTK1 identity"));
        }
        let canonical = NormalizedTilingSolutionKey::parse_canonical(key)
            .map_err(|_| invalid("invalid canonical CTK1 identity"))?;
        let identity = canonical
            .standard_board64_identity()
            .map_err(|_| invalid("identity is outside the Board64 fixture"))?;
        let expected = match self {
            Self::P7P4 => Some((0, 10)),
            Self::SmallI => Some((0x3f0, 1)),
            #[cfg(test)]
            Self::Tiny => None,
        };
        if let Some((initial, placements)) = expected {
            if identity.initial_board_mask() != initial || identity.placement_count() != placements
            {
                return Err(invalid("identity does not belong to this coverage fixture"));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct CoverageSummary {
    pub(super) universe: CoverageUniverse,
    pub(super) solutions: u32,
    pub(super) covered_queues: u32,
    pub(super) covered_pairs: u64,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum CoverageComparison {
    Equal(CoverageSummary),
    Different(CoverageDifference),
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct CoverageDifference {
    pub(super) reference_identity: Option<String>,
    pub(super) candidate_identity: Option<String>,
    pub(super) first_missing_queue: Option<u32>,
    pub(super) first_added_queue: Option<u32>,
    pub(super) reference_numerator: Option<u32>,
    pub(super) candidate_numerator: Option<u32>,
    pub(super) denominator: u32,
}

pub(super) struct CoverageWriter<W> {
    out: W,
    evidence: Evidence,
}

impl<W: Write> CoverageWriter<W> {
    pub(super) fn new(mut out: W, universe: CoverageUniverse) -> io::Result<Self> {
        out.write_all(MAGIC)?;
        out.write_all(&[universe.tag()])?;
        put_u32(&mut out, universe.queue_count())?;
        put_u32(&mut out, universe.solution_count())?;
        put_u32(&mut out, 1)?; // Each original supply outcome has integer weight 1.
        Ok(Self {
            out,
            evidence: Evidence::new(universe),
        })
    }

    pub(super) fn write_solution(&mut self, key: &str, words: &[u64]) -> io::Result<()> {
        let set = QueueSet::from_words(self.evidence.summary.universe, words)?;
        self.write_row(key, &set)
    }

    pub(super) fn write_ranges(&mut self, key: &str, ranges: &[(u32, u32)]) -> io::Result<()> {
        let universe = self.evidence.summary.universe;
        let numerator = validate_ranges(universe, ranges)?;
        let payload = QueuePayload::Ranges(ranges.to_vec());
        if universe.block_count() != 0 && packed_range_size(ranges) > 2_048 {
            return self.write_solution(key, &payload.words(universe));
        }
        self.write_row(key, &QueueSet { numerator, payload })
    }

    fn write_row(&mut self, key: &str, set: &QueueSet) -> io::Result<()> {
        self.evidence.accept(key, set)?;
        self.out.write_all(&[1])?;
        self.out.write_all(&(key.len() as u16).to_le_bytes())?;
        self.out.write_all(key.as_bytes())?;
        set.payload.write(&mut self.out)
    }

    pub(super) fn finish(mut self) -> io::Result<CoverageSummary> {
        let summary = self.evidence.complete()?;
        self.out.write_all(&[0])?;
        put_u32(&mut self.out, summary.solutions)?;
        put_u32(&mut self.out, summary.covered_queues)?;
        self.out.write_all(&summary.covered_pairs.to_le_bytes())?;
        self.out.flush()?;
        Ok(summary)
    }
}

struct Evidence {
    summary: CoverageSummary,
    previous: String,
    union: Vec<u8>,
}

impl Evidence {
    fn new(universe: CoverageUniverse) -> Self {
        Self {
            summary: CoverageSummary {
                universe,
                solutions: 0,
                covered_queues: 0,
                covered_pairs: 0,
            },
            previous: String::new(),
            union: vec![0; universe.byte_count()],
        }
    }

    fn accept(&mut self, key: &str, set: &QueueSet) -> io::Result<()> {
        let universe = self.summary.universe;
        if self.summary.solutions >= universe.solution_count() {
            return Err(invalid("more solution rows than the fixture permits"));
        }
        universe.validate_key(key)?;
        if key <= self.previous.as_str() {
            return Err(invalid(
                "solution identities are duplicated or out of order",
            ));
        }
        if set.numerator == 0 {
            return Err(invalid(
                "a solved identity cannot have empty queue coverage",
            ));
        }
        self.previous.clear();
        self.previous.push_str(key);
        self.summary.solutions += 1;
        self.summary.covered_pairs += u64::from(set.numerator);
        if self.summary.covered_queues < universe.queue_count() {
            self.summary.covered_queues += set.payload.or_into(&mut self.union);
        }
        Ok(())
    }

    fn complete(&self) -> io::Result<CoverageSummary> {
        if self.summary.solutions != self.summary.universe.solution_count()
            || self.summary.covered_queues != self.summary.universe.queue_count()
        {
            return Err(invalid("incomplete solution family or global queue union"));
        }
        Ok(self.summary)
    }
}

struct CoverageReader<R> {
    input: R,
    evidence: Evidence,
}

impl<R: Read> CoverageReader<R> {
    fn new(mut input: R) -> io::Result<Self> {
        if &read_array::<8>(&mut input)? != MAGIC {
            return Err(invalid("unsupported solution coverage evidence format"));
        }
        let universe = CoverageUniverse::from_tag(read_u8(&mut input)?)?;
        if get_u32(&mut input)? != universe.queue_count()
            || get_u32(&mut input)? != universe.solution_count()
            || get_u32(&mut input)? != 1
        {
            return Err(invalid(
                "coverage header violates the fixed universe contract",
            ));
        }
        Ok(Self {
            input,
            evidence: Evidence::new(universe),
        })
    }

    fn next(&mut self) -> io::Result<Option<(String, QueueSet)>> {
        match read_u8(&mut self.input)? {
            1 => {
                let length = usize::from(get_u16(&mut self.input)?);
                if length == 0 || length > MAX_KEY_BYTES {
                    return Err(invalid("invalid solution identity size"));
                }
                let mut key = [0; MAX_KEY_BYTES];
                self.input.read_exact(&mut key[..length])?;
                let key = std::str::from_utf8(&key[..length])
                    .map_err(|_| invalid("non-UTF-8 solution identity"))?;
                let set = QueueSet::read(&mut self.input, self.evidence.summary.universe)?;
                self.evidence.accept(key, &set)?;
                Ok(Some((key.to_owned(), set)))
            }
            0 => {
                let expected = self.evidence.complete()?;
                if (
                    get_u32(&mut self.input)?,
                    get_u32(&mut self.input)?,
                    u64::from_le_bytes(read_array(&mut self.input)?),
                ) != (
                    expected.solutions,
                    expected.covered_queues,
                    expected.covered_pairs,
                ) {
                    return Err(invalid("solution coverage footer does not match its rows"));
                }
                if self.input.read(&mut [0])? != 0 {
                    return Err(invalid("trailing bytes after solution coverage footer"));
                }
                Ok(None)
            }
            _ => Err(invalid("invalid solution coverage record tag")),
        }
    }
}

/// Buffer file inputs in the caller. Equal requires both complete footers and
/// EOF. Early disagreement is only negative evidence, not proof of completion.
pub(super) fn compare(
    reference: impl Read,
    candidate: impl Read,
) -> io::Result<CoverageComparison> {
    let mut reference = CoverageReader::new(reference)?;
    let mut candidate = CoverageReader::new(candidate)?;
    let universe = reference.evidence.summary.universe;
    if candidate.evidence.summary.universe != universe {
        return Err(invalid("solution coverage universe mismatch"));
    }
    loop {
        let left = reference.next()?;
        let right = candidate.next()?;
        match (left, right) {
            (None, None) => return Ok(CoverageComparison::Equal(reference.evidence.summary)),
            (Some((left_key, left)), Some((right_key, right))) if left_key == right_key => {
                if left == right {
                    continue;
                }
                // Encoding may differ without any semantic change.
                let left_words = left.payload.words(universe);
                let right_words = right.payload.words(universe);
                let first_missing_queue = first_difference(&left_words, &right_words);
                let first_added_queue = first_difference(&right_words, &left_words);
                if first_missing_queue.is_some() || first_added_queue.is_some() {
                    return Ok(CoverageComparison::Different(CoverageDifference {
                        reference_identity: Some(left_key),
                        candidate_identity: Some(right_key),
                        first_missing_queue,
                        first_added_queue,
                        reference_numerator: Some(left.numerator),
                        candidate_numerator: Some(right.numerator),
                        denominator: universe.queue_count(),
                    }));
                }
            }
            (left, right) => {
                return Ok(CoverageComparison::Different(CoverageDifference {
                    reference_numerator: left.as_ref().map(|(_, set)| set.numerator),
                    candidate_numerator: right.as_ref().map(|(_, set)| set.numerator),
                    reference_identity: left.map(|(key, _)| key),
                    candidate_identity: right.map(|(key, _)| key),
                    first_missing_queue: None,
                    first_added_queue: None,
                    denominator: universe.queue_count(),
                }));
            }
        }
    }
}

/// Shared exact spill codec; its caller owns identity and completion framing.
pub(super) fn write_queue_set(
    mut out: impl Write,
    universe: CoverageUniverse,
    words: &[u64],
) -> io::Result<u32> {
    let set = QueueSet::from_words(universe, words)?;
    set.payload.write(&mut out)?;
    Ok(set.numerator)
}

pub(super) fn read_queue_set(
    mut input: impl Read,
    universe: CoverageUniverse,
) -> io::Result<Vec<u64>> {
    Ok(QueueSet::read(&mut input, universe)?
        .payload
        .words(universe))
}

#[derive(Debug, Eq, PartialEq)]
struct QueueSet {
    numerator: u32,
    payload: QueuePayload,
}

#[derive(Debug, Eq, PartialEq)]
enum QueuePayload {
    Ranges(Vec<(u32, u32)>),
    BagBlocks {
        blocks: Vec<[u8; SUFFIX_BYTES]>,
        runs: Vec<(u16, u16)>, // (dictionary index, number of prefixes)
    },
}

impl QueueSet {
    fn from_words(universe: CoverageUniverse, words: &[u64]) -> io::Result<Self> {
        if words.len() != universe.word_count() {
            return Err(invalid("solution has the wrong coverage word count"));
        }
        let tail = universe.queue_count() % 64;
        if tail != 0 && words.last().is_some_and(|last| *last >> tail != 0) {
            return Err(invalid(
                "coverage contains a queue beyond the fixed universe",
            ));
        }
        let numerator = words.iter().map(|word| word.count_ones()).sum();
        let range_limit = if universe.block_count() == 0 {
            usize::MAX
        } else {
            2_048
        };
        if let Some(ranges) = collect_ranges(words, range_limit) {
            return Ok(Self {
                numerator,
                payload: QueuePayload::Ranges(ranges),
            });
        }
        // In a fixed P7P4 universe, each complete first bag occupies exactly
        // one 840-bit block. Equivalent tail sets need storing only once.
        let payload = QueuePayload::from_bag_words(words, universe);
        let QueuePayload::BagBlocks { blocks, runs } = &payload else {
            unreachable!("bag dictionary returns blocks");
        };
        let limit = 4 + blocks.len() * SUFFIX_BYTES + runs.len() * 4;
        let payload = collect_ranges(words, limit).map_or(payload, QueuePayload::Ranges);
        Ok(Self { numerator, payload })
    }

    fn read(input: &mut impl Read, universe: CoverageUniverse) -> io::Result<Self> {
        match read_u8(input)? {
            0 => {
                let count = get_u32(input)?;
                if count > universe.queue_count().div_ceil(2) {
                    return Err(invalid("too many solution coverage ranges"));
                }
                let mut ranges = reserve(count as usize)?;
                let mut previous_end = 0_u32;
                for _ in 0..count {
                    let gap = read_varint(input)?;
                    let length = read_varint(input)?;
                    let start = previous_end
                        .checked_add(gap)
                        .ok_or_else(|| invalid("range start overflow"))?;
                    let end = start
                        .checked_add(length)
                        .ok_or_else(|| invalid("range end overflow"))?;
                    ranges.push((start, end));
                    previous_end = end;
                }
                let numerator = validate_ranges(universe, &ranges)?;
                Ok(Self {
                    numerator,
                    payload: QueuePayload::Ranges(ranges),
                })
            }
            1 => {
                let total_blocks = universe.block_count();
                let block_count = usize::from(get_u16(input)?);
                let run_count = usize::from(get_u16(input)?);
                if block_count == 0
                    || block_count > total_blocks
                    || run_count == 0
                    || run_count > total_blocks
                {
                    return Err(invalid("invalid bag block or run count"));
                }
                let mut blocks = reserve(block_count)?;
                let mut counts = reserve(block_count)?;
                for _ in 0..block_count {
                    let block = read_array::<SUFFIX_BYTES>(input)?;
                    counts.push(block.iter().map(|byte| byte.count_ones()).sum::<u32>());
                    blocks.push(block);
                }
                let mut runs = reserve(run_count)?;
                let mut seen = 0;
                let mut numerator = 0;
                for _ in 0..run_count {
                    let index = get_u16(input)?;
                    let repeat = get_u16(input)?;
                    let Some(&count) = counts.get(usize::from(index)) else {
                        return Err(invalid("invalid bag block dictionary index"));
                    };
                    seen += usize::from(repeat);
                    if repeat == 0
                        || seen > total_blocks
                        || runs.last().is_some_and(|&(last, _)| index == last)
                    {
                        return Err(invalid("invalid bag block run"));
                    }
                    numerator += u32::from(repeat) * count;
                    runs.push((index, repeat));
                }
                if seen != total_blocks {
                    return Err(invalid("incomplete bag block prefix coverage"));
                }
                Ok(Self {
                    numerator,
                    payload: QueuePayload::BagBlocks { blocks, runs },
                })
            }
            _ => Err(invalid("invalid queue-set encoding")),
        }
    }
}

impl QueuePayload {
    fn from_bag_words(words: &[u64], universe: CoverageUniverse) -> Self {
        let mut bytes = Vec::with_capacity(universe.byte_count());
        for word in words {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        assert_eq!(bytes.len(), universe.block_count() * SUFFIX_BYTES);
        let mut indexed = HashMap::new();
        let mut blocks = Vec::new();
        let mut runs: Vec<(u16, u16)> = Vec::new();
        for block in bytes.chunks_exact(SUFFIX_BYTES) {
            let index = *indexed.entry(block).or_insert_with(|| {
                let index = blocks.len() as u16;
                blocks.push(block.try_into().expect("exactly one 7P4 block"));
                index
            });
            match runs.last_mut() {
                Some((previous, count)) if *previous == index => *count += 1,
                _ => runs.push((index, 1)),
            }
        }
        Self::BagBlocks { blocks, runs }
    }

    fn write(&self, out: &mut impl Write) -> io::Result<()> {
        match self {
            Self::Ranges(ranges) => {
                out.write_all(&[0])?;
                put_u32(out, ranges.len() as u32)?;
                let mut previous_end = 0;
                for &(start, end) in ranges {
                    write_varint(out, start - previous_end)?;
                    write_varint(out, end - start)?;
                    previous_end = end;
                }
            }
            Self::BagBlocks { blocks, runs } => {
                out.write_all(&[1])?;
                out.write_all(&(blocks.len() as u16).to_le_bytes())?;
                out.write_all(&(runs.len() as u16).to_le_bytes())?;
                for block in blocks {
                    out.write_all(block)?;
                }
                for &(index, repeat) in runs {
                    out.write_all(&index.to_le_bytes())?;
                    out.write_all(&repeat.to_le_bytes())?;
                }
            }
        }
        Ok(())
    }

    fn or_into(&self, output: &mut [u8]) -> u32 {
        match self {
            Self::Ranges(ranges) => ranges
                .iter()
                .map(|&(start, end)| mark_range(output, start as usize, end as usize))
                .sum(),
            Self::BagBlocks { blocks, runs } => {
                let mut added = 0;
                let mut offset = 0;
                for &(index, repeat) in runs {
                    let block = &blocks[usize::from(index)];
                    for _ in 0..repeat {
                        let next = offset + SUFFIX_BYTES;
                        for (old, bits) in output[offset..next].iter_mut().zip(block) {
                            added += (bits & !*old).count_ones();
                            *old |= bits;
                        }
                        offset = next;
                    }
                }
                added
            }
        }
    }

    fn words(&self, universe: CoverageUniverse) -> Vec<u64> {
        let mut output = vec![0; universe.byte_count()];
        self.or_into(&mut output);
        output
            .chunks(8)
            .map(|bytes| {
                let mut word = [0; 8];
                word[..bytes.len()].copy_from_slice(bytes);
                u64::from_le_bytes(word)
            })
            .collect()
    }
}

fn mark_range(output: &mut [u8], start: usize, end: usize) -> u32 {
    let mut added = 0;
    let first = start / 8;
    let last = (end - 1) / 8;
    for (index, old) in output[first..=last].iter_mut().enumerate() {
        let absolute = first + index;
        let mut mask = u8::MAX;
        if absolute == first {
            mask &= u8::MAX << (start % 8);
        }
        if absolute == last {
            mask &= u8::MAX >> (7 - (end - 1) % 8);
        }
        added += (mask & !*old).count_ones();
        *old |= mask;
    }
    added
}

fn first_difference(left: &[u64], right: &[u64]) -> Option<u32> {
    left.iter()
        .zip(right)
        .enumerate()
        .find_map(|(index, (&left, &right))| {
            let difference = left & !right;
            (difference != 0).then(|| index as u32 * 64 + difference.trailing_zeros())
        })
}

fn validate_ranges(universe: CoverageUniverse, ranges: &[(u32, u32)]) -> io::Result<u32> {
    let mut count = 0;
    let mut previous_end = None;
    for &(start, end) in ranges {
        if start >= end
            || end > universe.queue_count()
            || previous_end.is_some_and(|last| start <= last)
        {
            return Err(invalid(
                "coverage ranges must be maximal, disjoint, sorted, and in bounds",
            ));
        }
        count += end - start;
        previous_end = Some(end);
    }
    Ok(count)
}

fn collect_ranges(words: &[u64], limit: usize) -> Option<Vec<(u32, u32)>> {
    let mut ranges = Vec::new();
    let mut current: Option<(u32, u32)> = None;
    let mut previous_end = 0;
    let mut length = 4;
    for (index, &word) in words.iter().enumerate() {
        let mut pending = word;
        let base = index as u32 * 64;
        while pending != 0 {
            let bit = pending.trailing_zeros();
            let run = (pending >> bit).trailing_ones();
            let start = base + bit;
            let end = start + run;
            pending &= if bit + run == 64 {
                0
            } else {
                u64::MAX << (bit + run)
            };
            match current {
                Some((start_before, end_before)) if end_before == start => {
                    current = Some((start_before, end));
                }
                Some((start_before, end_before)) => {
                    length += varint_size(start_before - previous_end)
                        + varint_size(end_before - start_before);
                    if length > limit {
                        return None;
                    }
                    ranges.push((start_before, end_before));
                    previous_end = end_before;
                    current = Some((start, end));
                }
                None => current = Some((start, end)),
            }
        }
    }
    if let Some((start, end)) = current {
        length += varint_size(start - previous_end) + varint_size(end - start);
        if length > limit {
            return None;
        }
        ranges.push((start, end));
    }
    Some(ranges)
}

fn packed_range_size(ranges: &[(u32, u32)]) -> usize {
    let mut previous_end = 0;
    4 + ranges
        .iter()
        .map(|&(start, end)| {
            let bytes = varint_size(start - previous_end) + varint_size(end - start);
            previous_end = end;
            bytes
        })
        .sum::<usize>()
}

fn reserve<T>(length: usize) -> io::Result<Vec<T>> {
    let mut values = Vec::new();
    values.try_reserve_exact(length).map_err(io::Error::other)?;
    Ok(values)
}

fn read_array<const N: usize>(input: &mut impl Read) -> io::Result<[u8; N]> {
    let mut bytes = [0; N];
    input.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn read_u8(input: &mut impl Read) -> io::Result<u8> {
    Ok(read_array::<1>(input)?[0])
}

fn get_u16(input: &mut impl Read) -> io::Result<u16> {
    Ok(u16::from_le_bytes(read_array(input)?))
}

fn get_u32(input: &mut impl Read) -> io::Result<u32> {
    Ok(u32::from_le_bytes(read_array(input)?))
}

fn put_u32(out: &mut impl Write, value: u32) -> io::Result<()> {
    out.write_all(&value.to_le_bytes())
}

fn varint_size(value: u32) -> usize {
    (32 - value.leading_zeros()).max(1).div_ceil(7) as usize
}

fn write_varint(out: &mut impl Write, mut value: u32) -> io::Result<()> {
    let mut bytes = [0_u8; 5];
    let size = varint_size(value);
    for (index, byte) in bytes[..size].iter_mut().enumerate() {
        *byte = (value as u8 & 0x7f) | if index + 1 < size { 0x80 } else { 0 };
        value >>= 7;
    }
    out.write_all(&bytes[..size])
}

fn read_varint(input: &mut impl Read) -> io::Result<u32> {
    let mut value = 0_u32;
    for shift in (0..=28).step_by(7) {
        let byte = read_u8(input)?;
        if shift == 28 && byte > 0x0f {
            return Err(invalid("coverage varint exceeds u32"));
        }
        value |= u32::from(byte & 0x7f) << shift;
        if byte < 0x80 {
            if shift != 0 && byte == 0 {
                return Err(invalid("noncanonical coverage varint"));
            }
            return Ok(value);
        }
    }
    Err(invalid("unterminated coverage varint"))
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "ctk1|initial=0000000000000000|placements=I:000000000000000f";
    const B: &str = "ctk1|initial=0000000000000000|placements=I:00000000000000f0";

    fn fixture(first: u64, second: u64) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut writer = CoverageWriter::new(&mut bytes, CoverageUniverse::Tiny).unwrap();
        writer.write_solution(A, &[first]).unwrap();
        writer.write_solution(B, &[second]).unwrap();
        writer.finish().unwrap();
        bytes
    }

    #[test]
    fn same_union_and_probabilities_do_not_hide_swapped_solution_queues() {
        let first = fixture(0b0011, 0b1100);
        let swapped = fixture(0b1100, 0b0011);
        assert_eq!(
            compare(first.as_slice(), first.as_slice()).unwrap(),
            CoverageComparison::Equal(CoverageSummary {
                universe: CoverageUniverse::Tiny,
                solutions: 2,
                covered_queues: 4,
                covered_pairs: 4,
            })
        );
        let CoverageComparison::Different(diff) =
            compare(first.as_slice(), swapped.as_slice()).unwrap()
        else {
            panic!("global coverage alone must not authorize a per-solution match");
        };
        assert_eq!(diff.reference_identity.as_deref(), Some(A));
        assert_eq!(diff.candidate_identity.as_deref(), Some(A));
        assert_eq!(
            (diff.first_missing_queue, diff.first_added_queue),
            (Some(0), Some(2))
        );
        assert_eq!(
            (
                diff.reference_numerator,
                diff.candidate_numerator,
                diff.denominator
            ),
            (Some(2), Some(2), 4)
        );
    }

    #[test]
    fn missing_forged_or_nonfinal_footers_do_not_validate() {
        let good = fixture(0b0011, 0b1100);
        for cut in 0..good.len() {
            assert!(compare(good.as_slice(), &good[..cut]).is_err(), "cut={cut}");
        }
        let mut bad_footer = good.clone();
        *bad_footer.last_mut().unwrap() = 1;
        assert!(compare(good.as_slice(), bad_footer.as_slice()).is_err());
        let mut trailing = good.clone();
        trailing.push(0);
        assert!(compare(good.as_slice(), trailing.as_slice()).is_err());
    }

    #[test]
    fn scope_weight_order_and_range_guards_fail_closed() {
        let mut writer = CoverageWriter::new(Vec::new(), CoverageUniverse::Tiny).unwrap();
        assert!(writer.write_solution(A, &[0x10]).is_err());
        assert!(writer.write_ranges(A, &[(0, 1), (1, 2)]).is_err());
        assert!(writer.write_ranges(A, &[(2, 1)]).is_err());
        assert!(writer.write_ranges(A, &[(0, 5)]).is_err());
        writer.write_ranges(B, &[(1, 4)]).unwrap();
        assert!(writer.write_solution(B, &[1]).is_err());
        assert!(writer.write_solution(A, &[1]).is_err());
        assert!(writer.finish().is_err());
        let mut wrong_weight = fixture(0b0011, 0b1100);
        wrong_weight[8 + 1 + 4 + 4] = 2;
        assert!(compare(wrong_weight.as_slice(), wrong_weight.as_slice()).is_err());
    }

    #[test]
    fn ranges_coalesce_across_words_and_preserve_universe_last_bit() {
        let universe = CoverageUniverse::P7P4;
        let mut words = vec![0; universe.word_count()];
        words[0] = 1 << 63;
        words[1] = u64::MAX;
        words[2] = 0b11;
        *words.last_mut().unwrap() = 1 << 63;
        let set = QueueSet::from_words(universe, &words).unwrap();
        assert_eq!(set.numerator, 68);
        assert_eq!(
            set.payload,
            QueuePayload::Ranges(vec![(63, 130), (4_233_599, 4_233_600)])
        );
        let mut packed = Vec::new();
        assert_eq!(write_queue_set(&mut packed, universe, &words).unwrap(), 68);
        assert_eq!(read_queue_set(packed.as_slice(), universe).unwrap(), words);
    }

    #[test]
    fn repeated_first_bag_suffixes_use_exact_bounded_dictionary() {
        let universe = CoverageUniverse::P7P4;
        let words = vec![0x000f_000f_000f_000f; universe.word_count()];
        let mut packed = Vec::new();
        let count = write_queue_set(&mut packed, universe, &words).unwrap();
        assert_eq!(count, universe.queue_count() / 4);
        assert_eq!(packed[0], 1);
        assert!(packed.len() < 24 * 1024);
        assert_eq!(read_queue_set(packed.as_slice(), universe).unwrap(), words);
        packed.pop();
        assert!(read_queue_set(packed.as_slice(), universe).is_err());
    }

    #[test]
    fn codecs_match_an_independent_tiny_set_oracle() {
        let universe = CoverageUniverse::Tiny;
        for words in 0..16 {
            let mut bytes = Vec::new();
            let numerator = write_queue_set(&mut bytes, universe, &[words]).unwrap();
            let actual = read_queue_set(bytes.as_slice(), universe).unwrap();
            let expected: Vec<_> = (0..4).filter(|bit| words & (1_u64 << bit) != 0).collect();
            assert_eq!(numerator as usize, expected.len());
            for bit in 0..4 {
                assert_eq!(actual[0] & (1 << bit) != 0, expected.contains(&bit));
            }
        }
    }
}
