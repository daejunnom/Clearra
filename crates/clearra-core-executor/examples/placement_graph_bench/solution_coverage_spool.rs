//! Bounded, exact per-identity coverage reduction for the local search benchmark.

use super::solution_coverage_compare::{
    read_queue_set, write_queue_set, BagBlocks, CoverageSummary, CoverageUniverse, CoverageWriter,
};
use clearra_core_executor::backend::P7P4CoverageBlocks;
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BinaryHeap},
    fs::{self, File},
    io::{self, BufReader, BufWriter, Read, Write},
    mem::size_of,
    path::{Path, PathBuf},
    sync::Arc,
};

const MAGIC: &[u8; 8] = b"CLRSP01\0";
const MAX_KEY: usize = 1024;
const IO_BUFFER: usize = 64 * 1024;
// Charges map node occupancy, allocation headers, and fixed key/value fields
// in addition to the exact boxed key and payload lengths.
const MAP_ENTRY_ALLOWANCE: usize = 256;

#[derive(Clone, Copy)]
pub(super) struct CoverageSpoolLimits {
    pub(super) memory_bytes: usize,
    pub(super) disk_bytes: u64,
    pub(super) max_runs: usize,
    pub(super) merge_fan_in: usize,
}

impl Default for CoverageSpoolLimits {
    fn default() -> Self {
        Self {
            memory_bytes: 64 * 1024 * 1024,
            disk_bytes: 16 * 1024 * 1024 * 1024,
            max_runs: 4096,
            merge_fan_in: 16,
        }
    }
}

/// Stores one fragment without retaining the originating worker's workspace or
/// materializing the dense cache of a sparse PatternBitSet. Prepare this before
/// acquiring the shared spool mutex. Transient scratch is O(one universe) per
/// producer, not O(solutions × universe).
pub(super) struct CoverageFragment {
    universe: CoverageUniverse,
    bits: Bits,
}

impl CoverageFragment {
    pub(super) fn from_p7p4_blocks(blocks: Arc<P7P4CoverageBlocks>) -> Self {
        Self {
            universe: CoverageUniverse::P7P4,
            bits: Bits::Blocks(BagBlocks::from_capture(blocks)),
        }
    }

    pub(super) fn from_bitset(universe: CoverageUniverse, set: &PatternBitSet) -> io::Result<Self> {
        let size = universe.queue_count() as usize;
        if set.pattern_count() != size {
            return Err(invalid("captured coverage universe mismatch"));
        }
        let ones = set.count_ones() as usize;
        let bits = if ones < sparse_threshold(universe) {
            Bits::Sparse(
                set.covered_patterns_before(size)
                    .map(|pattern| pattern.index() as u32)
                    .collect(),
            )
        } else {
            Bits::Dense(set.to_owned_words().into_boxed_slice())
        };
        Ok(Self { universe, bits })
    }
}

enum Bits {
    Sparse(Box<[u32]>),
    Dense(Box<[u64]>),
    Blocks(BagBlocks),
}

impl Bits {
    fn bytes(&self) -> usize {
        match self {
            Self::Sparse(ids) => std::mem::size_of_val(ids.as_ref()),
            Self::Dense(words) => std::mem::size_of_val(words.as_ref()),
            Self::Blocks(blocks) => blocks.retained_bytes(),
        }
    }

    fn union_bytes_bound(&self, other: &Self, universe: CoverageUniverse) -> usize {
        let dense_bytes = word_count(universe) * size_of::<u64>();
        match (self, other) {
            (Self::Blocks(a), Self::Blocks(b)) => a.retained_union_bound(b),
            (Self::Blocks(_), _) | (_, Self::Blocks(_)) => BagBlocks::max_retained_bytes(),
            (Self::Sparse(a), Self::Sparse(b)) => {
                ((a.len() + b.len()) * size_of::<u32>()).min(dense_bytes)
            }
            _ => dense_bytes,
        }
    }

    fn union(&mut self, other: Self, universe: CoverageUniverse) {
        if matches!(self, Self::Blocks(_)) || matches!(other, Self::Blocks(_)) {
            debug_assert_eq!(universe, CoverageUniverse::P7P4);
            let left = std::mem::replace(self, Self::Sparse(Box::default())).into_bag_blocks();
            *self = Self::Blocks(left.union(other.into_bag_blocks()));
            return;
        }
        match (&mut *self, other) {
            (Self::Dense(left), Self::Dense(right)) => {
                for (left, right) in left.iter_mut().zip(right.iter()) {
                    *left |= right;
                }
            }
            (Self::Dense(left), Self::Sparse(right)) => set_ids(left, &right),
            (slot @ Self::Sparse(_), Self::Dense(mut right)) => {
                if let Self::Sparse(left) = slot {
                    set_ids(&mut right, left);
                }
                *slot = Self::Dense(right);
            }
            (Self::Sparse(_), Self::Sparse(right)) if right.is_empty() => {}
            (Self::Sparse(left), Self::Sparse(right)) if left.is_empty() => *left = right,
            (Self::Sparse(left), Self::Sparse(right)) => {
                let total = left.len() + right.len();
                if total < sparse_threshold(universe) {
                    let mut result = Vec::with_capacity(total);
                    let mut a = left.iter().copied().peekable();
                    let mut b = right.iter().copied().peekable();
                    while let (Some(x), Some(y)) = (a.peek().copied(), b.peek().copied()) {
                        if x <= y {
                            result.push(x);
                            a.next();
                            if x == y {
                                b.next();
                            }
                        } else {
                            result.push(y);
                            b.next();
                        }
                    }
                    result.extend(a);
                    result.extend(b);
                    *left = result.into_boxed_slice();
                } else {
                    let mut words = vec![0; word_count(universe)];
                    set_ids(&mut words, left);
                    set_ids(&mut words, &right);
                    *self = Self::Dense(words.into_boxed_slice());
                }
            }
            _ => unreachable!("fixed-universe block union handled first"),
        }
    }

    fn into_bag_blocks(self) -> BagBlocks {
        match self {
            Self::Sparse(ids) => BagBlocks::from_sparse(&ids),
            Self::Dense(words) => BagBlocks::from_words(&words),
            Self::Blocks(blocks) => blocks,
        }
    }

    fn write_run(&self, out: &mut impl Write, universe: CoverageUniverse) -> io::Result<()> {
        match self {
            Self::Sparse(ids) => {
                out.write_all(&[0])?;
                out.write_all(&(ids.len() as u32).to_le_bytes())?;
                let mut base = 0;
                for &id in ids.iter() {
                    let gap = id - base;
                    let mut encoded = [0; 5];
                    let len = encode_gap(gap, &mut encoded);
                    out.write_all(&encoded[..len])?;
                    base = id + 1;
                }
                Ok(())
            }
            Self::Dense(words) => {
                out.write_all(&[1])?;
                write_queue_set(out, universe, words).map(|_| ())
            }
            Self::Blocks(blocks) => {
                out.write_all(&[2])?;
                blocks.write(out)
            }
        }
    }

    fn read_run(input: &mut impl Read, universe: CoverageUniverse) -> io::Result<Self> {
        match read_array::<1>(input)?[0] {
            0 => {
                let n = u32::from_le_bytes(read_array(input)?) as usize;
                if n >= sparse_threshold(universe) {
                    return Err(invalid("invalid sparse spool record length"));
                }
                let mut ids = Vec::with_capacity(n);
                let mut base = 0_u32;
                for _ in 0..n {
                    let id = base
                        .checked_add(decode_gap(input)?)
                        .filter(|&id| id < universe.queue_count())
                        .ok_or_else(|| invalid("noncanonical sparse spool record"))?;
                    ids.push(id);
                    base = id + 1;
                }
                Ok(Self::Sparse(ids.into_boxed_slice()))
            }
            1 => Ok(Self::Dense(
                read_queue_set(input, universe)?.into_boxed_slice(),
            )),
            2 if universe == CoverageUniverse::P7P4 => Ok(Self::Blocks(BagBlocks::read(input)?)),
            _ => Err(invalid("unknown spool coverage record")),
        }
    }

    fn write_final<W: Write>(&self, out: &mut CoverageWriter<W>, key: &str) -> io::Result<()> {
        match self {
            Self::Sparse(ids) => {
                let mut ranges: Vec<(u32, u32)> = Vec::new();
                for &id in ids.iter() {
                    if let Some(last) = ranges.last_mut() {
                        if last.1 == id {
                            last.1 = id + 1;
                            continue;
                        }
                    }
                    ranges.push((id, id + 1));
                }
                out.write_ranges(key, &ranges).map(|_| ())
            }
            Self::Dense(words) => out.write_solution(key, words).map(|_| ()),
            Self::Blocks(blocks) => out.write_blocks(key, blocks),
        }
    }
}

pub(super) struct CoverageSpool {
    dir: PathBuf,
    universe: CoverageUniverse,
    limits: CoverageSpoolLimits,
    pending: BTreeMap<Box<str>, Bits>,
    pending_bytes: usize,
    runs: Vec<Run>,
    next_run: usize,
    disk_bytes: u64,
    failed: bool,
}

impl CoverageSpool {
    /// `dir` must be a new leaf inside a manager-validated output root. It is
    /// created exclusively; neither input paths nor preexisting files can be
    /// adopted. An error deliberately retains diagnostic runs.
    pub(super) fn create(
        dir: &Path,
        universe: CoverageUniverse,
        limits: CoverageSpoolLimits,
    ) -> io::Result<Self> {
        let payload_ceiling = if universe == CoverageUniverse::P7P4 {
            BagBlocks::max_retained_bytes()
        } else {
            word_count(universe) * size_of::<u64>()
        };
        let row_ceiling = payload_ceiling + MAX_KEY + MAP_ENTRY_ALLOWANCE;
        if limits.memory_bytes < row_ceiling
            || limits.disk_bytes == 0
            || limits.max_runs == 0
            || !(2..=64).contains(&limits.merge_fan_in)
        {
            return Err(invalid(
                "spool limits do not cover one row and a bounded merge",
            ));
        }
        fs::create_dir(dir)?;
        Ok(Self {
            dir: dir.to_owned(),
            universe,
            limits,
            pending: BTreeMap::new(),
            pending_bytes: 0,
            runs: Vec::new(),
            next_run: 0,
            disk_bytes: 0,
            failed: false,
        })
    }

    pub(super) fn push_fragment(
        &mut self,
        key: &str,
        fragment: CoverageFragment,
    ) -> io::Result<()> {
        if self.failed {
            return Err(invalid(
                "spool failed earlier; no coverage receipt is valid",
            ));
        }
        let result = self.push_inner(key, fragment);
        self.failed = result.is_err();
        result
    }

    fn push_inner(&mut self, key: &str, fragment: CoverageFragment) -> io::Result<()> {
        validate_key(key)?;
        if fragment.universe != self.universe {
            return Err(invalid("fragment uses a different coverage universe"));
        }
        // A union can promote two sparse sets to a dense row. Reserve that
        // cost before replacing an existing sparse value; overestimates only
        // cause an earlier flush, never incomplete output.
        let reserve = if let Some(previous) = self.pending.get(key) {
            previous.union_bytes_bound(&fragment.bits, self.universe) - previous.bytes()
        } else {
            fragment.bits.bytes() + key.len() + MAP_ENTRY_ALLOWANCE
        };
        if self.pending_bytes.saturating_add(reserve) > self.limits.memory_bytes {
            self.flush()?;
        }
        if let Some(previous) = self.pending.get_mut(key) {
            let old_bytes = previous.bytes();
            previous.union(fragment.bits, self.universe);
            self.pending_bytes = self.pending_bytes - old_bytes + previous.bytes();
        } else {
            self.pending_bytes += fragment.bits.bytes() + key.len() + MAP_ENTRY_ALLOWANCE;
            self.pending.insert(key.into(), fragment.bits);
        }
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        if self.runs.len() >= self.limits.max_runs {
            return Err(quota("spool run count exhausted"));
        }
        let pending = std::mem::take(&mut self.pending);
        self.pending_bytes = 0;
        let run = self.new_run(|out| {
            for (key, bits) in pending {
                out.record(&key, &bits)?;
            }
            Ok(())
        })?;
        self.runs.push(run);
        Ok(())
    }

    fn new_run(
        &mut self,
        fill: impl FnOnce(&mut RunWriter<'_>) -> io::Result<()>,
    ) -> io::Result<Run> {
        let path = self.dir.join(format!("run-{:08}.bin", self.next_run));
        self.next_run = self
            .next_run
            .checked_add(1)
            .ok_or_else(|| quota("run sequence overflow"))?;
        let before = self.disk_bytes;
        let file = File::create_new(&path)?;
        let mut out = RunWriter {
            inner: BufWriter::with_capacity(
                IO_BUFFER,
                BudgetedFile {
                    file,
                    used: &mut self.disk_bytes,
                    limit: self.limits.disk_bytes,
                },
            ),
            universe: self.universe,
            rows: 0,
        };
        out.inner.write_all(MAGIC)?;
        out.inner
            .write_all(&self.universe.queue_count().to_le_bytes())?;
        fill(&mut out)?;
        let rows = out.close()?;
        Ok(Run {
            path,
            rows,
            bytes: self.disk_bytes - before,
        })
    }

    #[cfg(test)]
    pub(super) fn finish<W: Write>(self, output: CoverageWriter<W>) -> io::Result<CoverageSummary> {
        self.finish_checked(output, |_| Ok(()))
    }

    pub(super) fn finish_checked<W: Write>(
        mut self,
        mut output: CoverageWriter<W>,
        mut validate_key: impl FnMut(&str) -> io::Result<()>,
    ) -> io::Result<CoverageSummary> {
        if self.failed {
            return Err(invalid(
                "spool failed earlier; no coverage receipt is valid",
            ));
        }
        if self.runs.is_empty() {
            for (key, bits) in self.pending {
                validate_key(&key)?;
                bits.write_final(&mut output, &key)?;
            }
            return output.finish();
        }
        self.flush()?;
        let universe = self.universe;
        let fan_in = self.limits.merge_fan_in;
        let mut runs = std::mem::take(&mut self.runs);
        while runs.len() > fan_in {
            let mut next = Vec::with_capacity(runs.len().div_ceil(fan_in));
            let mut remaining = runs.into_iter();
            while remaining.len() != 0 {
                let batch: Vec<_> = remaining.by_ref().take(fan_in).collect();
                if batch.len() == 1 {
                    next.extend(batch);
                    continue;
                }
                let run = self.new_run(|out| {
                    merge_sorted(&batch, universe, |key, bits| out.record(key, bits))
                })?;
                self.remove_consumed_runs(batch)?;
                next.push(run);
            }
            runs = next;
        }
        merge_sorted(&runs, universe, |key, bits| {
            validate_key(key)?;
            bits.write_final(&mut output, key)
        })?;
        let summary = output.finish()?;
        self.remove_consumed_runs(runs)?;
        Ok(summary)
    }

    fn remove_consumed_runs(&mut self, runs: Vec<Run>) -> io::Result<()> {
        for run in runs {
            // The only cleanups are concrete files created with create_new in
            // this run's exclusive directory; do not remove a directory tree.
            fs::remove_file(run.path)?;
            self.disk_bytes -= run.bytes;
        }
        Ok(())
    }
}

struct Run {
    path: PathBuf,
    rows: u64,
    bytes: u64,
}

struct RunWriter<'a> {
    inner: BufWriter<BudgetedFile<'a>>,
    universe: CoverageUniverse,
    rows: u64,
}

impl RunWriter<'_> {
    fn record(&mut self, key: &str, bits: &Bits) -> io::Result<()> {
        self.inner.write_all(&(key.len() as u16).to_le_bytes())?;
        self.inner.write_all(key.as_bytes())?;
        bits.write_run(&mut self.inner, self.universe)?;
        self.rows += 1;
        Ok(())
    }

    fn close(mut self) -> io::Result<u64> {
        self.inner.write_all(&0_u16.to_le_bytes())?;
        self.inner.write_all(&self.rows.to_le_bytes())?;
        self.inner.flush()?;
        Ok(self.rows)
    }
}

struct BudgetedFile<'a> {
    file: File,
    used: &'a mut u64,
    limit: u64,
}

impl Write for BudgetedFile<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() as u64 > self.limit - *self.used {
            return Err(quota("spool live file bytes exceed the limit"));
        }
        let written = self.file.write(bytes)?;
        *self.used += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

struct RunCursor {
    input: BufReader<File>,
    universe: CoverageUniverse,
    expected_rows: u64,
    rows: u64,
}

impl RunCursor {
    fn open(run: &Run, universe: CoverageUniverse) -> io::Result<Self> {
        let file = File::open(&run.path)?;
        if file.metadata()?.len() != run.bytes {
            return Err(invalid("spool run length changed"));
        }
        let mut input = BufReader::with_capacity(IO_BUFFER, file);
        if &read_array::<8>(&mut input)? != MAGIC
            || u32::from_le_bytes(read_array(&mut input)?) != universe.queue_count()
        {
            return Err(invalid("spool run header mismatch"));
        }
        Ok(Self {
            input,
            universe,
            expected_rows: run.rows,
            rows: 0,
        })
    }

    fn next_key(&mut self) -> io::Result<Option<Box<str>>> {
        let len = u16::from_le_bytes(read_array(&mut self.input)?) as usize;
        if len == 0 {
            let footer_count = u64::from_le_bytes(read_array(&mut self.input)?);
            if footer_count != self.rows
                || footer_count != self.expected_rows
                || self.input.read(&mut [0_u8])? != 0
            {
                return Err(invalid("truncated or modified spool run footer"));
            }
            return Ok(None);
        }
        if len > MAX_KEY || self.rows >= self.expected_rows {
            return Err(invalid("invalid spool key length or record count"));
        }
        let mut bytes = vec![0; len];
        self.input.read_exact(&mut bytes)?;
        let key = String::from_utf8(bytes).map_err(|_| invalid("invalid spool key bytes"))?;
        validate_key(&key)?;
        Ok(Some(key.into_boxed_str()))
    }

    fn bits(&mut self) -> io::Result<Bits> {
        let bits = Bits::read_run(&mut self.input, self.universe)?;
        self.rows += 1;
        Ok(bits)
    }
}

fn merge_sorted(
    runs: &[Run],
    universe: CoverageUniverse,
    mut emit: impl FnMut(&str, &Bits) -> io::Result<()>,
) -> io::Result<()> {
    // Only pending keys and bounded IO buffers are live for all files.
    // Coverage payloads are read after a key wins the heap.
    let mut cursors = Vec::with_capacity(runs.len());
    let mut queue = BinaryHeap::with_capacity(runs.len());
    for (index, run) in runs.iter().enumerate() {
        let mut cursor = RunCursor::open(run, universe)?;
        if let Some(key) = cursor.next_key()? {
            queue.push(Reverse((key, index)));
        }
        cursors.push(cursor);
    }
    let mut active: Option<(Box<str>, Bits)> = None;
    while let Some(Reverse((key, index))) = queue.pop() {
        let cursor = &mut cursors[index];
        let bits = cursor.bits()?;
        if let Some(next) = cursor.next_key()? {
            if next <= key {
                return Err(invalid("spool run keys are not strictly ordered"));
            }
            queue.push(Reverse((next, index)));
        }
        if let Some((current, merged)) = &mut active {
            if *current == key {
                merged.union(bits, universe);
                continue;
            }
            emit(current, merged)?;
        }
        active = Some((key, bits));
    }
    if let Some((key, bits)) = active {
        emit(&key, &bits)?;
    }
    Ok(())
}

fn read_array<const N: usize>(input: &mut impl Read) -> io::Result<[u8; N]> {
    let mut data = [0; N];
    input.read_exact(&mut data)?;
    Ok(data)
}

fn encode_gap(mut gap: u32, out: &mut [u8; 5]) -> usize {
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = gap as u8 & 0x7f;
        gap >>= 7;
        if gap == 0 {
            return index + 1;
        }
        *slot |= 0x80;
    }
    unreachable!("u32 gaps fit in five base-128 octets")
}

fn decode_gap(input: &mut impl Read) -> io::Result<u32> {
    let mut gap = 0;
    for shift in (0..35).step_by(7) {
        let octet = read_array::<1>(input)?[0];
        let bits = u32::from(octet & 0x7f);
        if shift == 28 && bits > 0xf {
            break;
        }
        gap |= bits << shift;
        if octet < 0x80 {
            return if shift != 0 && bits == 0 {
                Err(invalid("noncanonical sparse spool gap"))
            } else {
                Ok(gap)
            };
        }
    }
    Err(invalid("oversized sparse spool gap"))
}

fn validate_key(key: &str) -> io::Result<()> {
    if key.len() <= MAX_KEY
        && key
            .strip_prefix("ctk1|")
            .is_some_and(|body| !body.is_empty())
        && key.bytes().all(|byte| byte.is_ascii_graphic())
    {
        Ok(())
    } else {
        Err(invalid("invalid canonical solution key"))
    }
}

fn set_ids(words: &mut [u64], ids: &[u32]) {
    for &id in ids {
        words[id as usize / 64] |= 1 << (id % 64);
    }
}

fn sparse_threshold(universe: CoverageUniverse) -> usize {
    word_count(universe) * (size_of::<u64>() / size_of::<u32>())
}

fn word_count(universe: CoverageUniverse) -> usize {
    (universe.queue_count() as usize).div_ceil(64)
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn quota(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::StorageFull, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const FIXTURE_KEY: &str = "ctk1|initial=00000000000003f0|placements=I:000000000000000f";
    static NEXT_TEST: AtomicUsize = AtomicUsize::new(0);

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .ancestors()
                .nth(2)
                .expect("repository root")
                .join("_local/artifacts");
            fs::create_dir_all(&root).expect("benchmark test output root");
            loop {
                let n = NEXT_TEST.fetch_add(1, Ordering::Relaxed);
                let path = root.join(format!("coverage-spool-test-{}-{n}", std::process::id()));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(error) => panic!("create exclusive spool test root: {error}"),
                }
            }
        }

        fn spool(&self, universe: CoverageUniverse, limits: CoverageSpoolLimits) -> CoverageSpool {
            CoverageSpool::create(&self.0.join("work"), universe, limits)
                .expect("create test spool")
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let work = self.0.join("work");
            if let Ok(entries) = fs::read_dir(&work) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    if name
                        .to_str()
                        .is_some_and(|name| name.starts_with("run-") && name.ends_with(".bin"))
                    {
                        let _ = fs::remove_file(entry.path());
                    }
                }
                let _ = fs::remove_dir(work);
            }
            let _ = fs::remove_dir(&self.0);
        }
    }

    fn sparse(universe: CoverageUniverse, ids: &[u32]) -> CoverageFragment {
        CoverageFragment::from_bitset(
            universe,
            &PatternBitSet::from_pattern_indices(universe.queue_count() as usize, ids.to_vec())
                .expect("exact test values"),
        )
        .expect("same benchmark universe")
    }

    #[test]
    fn duplicates_across_spilled_runs_merge_by_identity_not_global_union() {
        let dir = TestDir::new();
        let universe = CoverageUniverse::P7P4;
        let mut spool = dir.spool(universe, CoverageSpoolLimits::default());
        spool
            .push_fragment("ctk1|b", sparse(universe, &[1, 8]))
            .unwrap();
        spool
            .push_fragment("ctk1|a", sparse(universe, &[9, 65]))
            .unwrap();
        spool.flush().unwrap();
        spool
            .push_fragment("ctk1|a", sparse(universe, &[0, 9, 129]))
            .unwrap();
        spool
            .push_fragment("ctk1|b", sparse(universe, &[9]))
            .unwrap();
        spool
            .push_fragment("ctk1|b", sparse(universe, &[1]))
            .unwrap();
        spool.flush().unwrap();

        let mut merged = Vec::new();
        merge_sorted(&spool.runs, universe, |key, bits| {
            let Bits::Sparse(ids) = bits else {
                panic!("expected sparse rows");
            };
            merged.push((key.to_owned(), ids.to_vec()));
            Ok(())
        })
        .unwrap();
        assert_eq!(
            merged,
            [
                ("ctk1|a".to_owned(), vec![0, 9, 65, 129]),
                ("ctk1|b".to_owned(), vec![1, 8, 9]),
            ]
        );
    }

    #[test]
    fn compact_disk_rows_union_with_compact_and_legacy_fragments() {
        let dir = TestDir::new();
        let universe = CoverageUniverse::P7P4;
        let mut spool = dir.spool(universe, CoverageSpoolLimits::default());
        let key = "ctk1|a";
        let last = universe.queue_count() - 1;
        for ids in [[2, 840], [840, last]] {
            spool
                .push_fragment(
                    key,
                    CoverageFragment {
                        universe,
                        bits: Bits::Blocks(BagBlocks::from_sparse(&ids)),
                    },
                )
                .unwrap();
            spool.flush().unwrap();
        }
        spool
            .push_fragment(key, sparse(universe, &[1, 840]))
            .unwrap();
        spool.flush().unwrap();
        let mut rows = 0;
        merge_sorted(&spool.runs, universe, |actual_key, bits| {
            assert_eq!(actual_key, key);
            let Bits::Blocks(actual) = bits else {
                panic!("union of compact plus legacy input must stay compact");
            };
            assert_eq!(
                actual.differences(&BagBlocks::from_sparse(&[1, 2, 840, last])),
                (None, None),
            );
            rows += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn finishes_multi_pass_merge_with_bounded_fan_in() {
        let dir = TestDir::new();
        let universe = CoverageUniverse::SmallI;
        let limits = CoverageSpoolLimits {
            merge_fan_in: 2,
            ..Default::default()
        };
        let mut spool = dir.spool(universe, limits);
        for i in 0..7 {
            let ids = if i % 2 == 0 { &[0][..] } else { &[][..] };
            spool
                .push_fragment(FIXTURE_KEY, sparse(universe, ids))
                .unwrap();
            spool.flush().unwrap();
        }
        let mut bytes = Vec::new();
        let writer = CoverageWriter::new(&mut bytes, universe).unwrap();
        spool.finish(writer).unwrap();
        assert!(!bytes.is_empty());
        assert_eq!(fs::read_dir(dir.0.join("work")).unwrap().count(), 0);
    }

    #[test]
    fn quota_error_poisoned_spool_cannot_emit_completed_footer() {
        let dir = TestDir::new();
        let universe = CoverageUniverse::SmallI;
        let limits = CoverageSpoolLimits {
            disk_bytes: 12,
            memory_bytes: 1_300,
            ..Default::default()
        };
        let mut spool = dir.spool(universe, limits);
        let large_key = format!("{FIXTURE_KEY}{}", "a".repeat(400));
        spool
            .push_fragment(&large_key, sparse(universe, &[0]))
            .unwrap();
        let large_key = format!("{FIXTURE_KEY}{}", "b".repeat(400));
        assert_eq!(
            spool
                .push_fragment(&large_key, sparse(universe, &[0]))
                .unwrap_err()
                .kind(),
            io::ErrorKind::StorageFull
        );
        let mut bytes = Vec::new();
        let writer = CoverageWriter::new(&mut bytes, universe).unwrap();
        assert!(spool.finish(writer).is_err());
    }

    #[test]
    fn truncated_spill_fails_closed_instead_of_dropping_last_record() {
        let dir = TestDir::new();
        let universe = CoverageUniverse::SmallI;
        let mut spool = dir.spool(universe, CoverageSpoolLimits::default());
        spool
            .push_fragment(FIXTURE_KEY, sparse(universe, &[0]))
            .unwrap();
        spool.flush().unwrap();
        let run = &spool.runs[0];
        File::options()
            .write(true)
            .open(&run.path)
            .unwrap()
            .set_len(run.bytes - 1)
            .unwrap();
        assert!(merge_sorted(&spool.runs, universe, |_, _| Ok(())).is_err());
    }
}
