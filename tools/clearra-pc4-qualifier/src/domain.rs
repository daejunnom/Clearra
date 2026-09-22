use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_executor::{
    enumerate_pc4_ilc_geometric_predecessor_fields, enumerate_pc4_ilc_target_fields,
};
use clearra_pc4_tablebase::{
    clearra_board64_mask_to_hydra_field_hash_v1, hydra_field_hash_v1_to_clearra_board64_mask,
};
use clearra_rules::kicks::KickTableProfileId;
use sha2::{Digest, Sha256};
use std::{
    cmp::Reverse,
    collections::{BTreeSet, BinaryHeap, HashSet},
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
};

const MAGIC: &[u8; 8] = b"PC4DOM02";
const VERSION: u32 = 2;
const HEADER_BYTES: usize = 128;
const PAIR_RUN_MAGIC: &[u8; 8] = b"LBRUN001";
const PAIR_RUN_HEADER_BYTES: usize = 136;
const PAIR_RUN_RECORD_BYTES: usize = 9;
const VALIDATION_RUN_MAGIC: &[u8; 8] = b"LBVAL001";
const VALIDATION_RUN_HEADER_BYTES: usize = 168;
const FIELD_MASK: u64 = (1_u64 << 40) - 1;
const MAX_WORKERS: usize = 64;
const REVERSE_VALIDATION_BATCH_SIZE: usize = 131_072;
#[cfg(not(test))]
const PAIR_RUN_FAN_IN: usize = 32;
#[cfg(test)]
const PAIR_RUN_FAN_IN: usize = 2;
#[cfg(not(test))]
const REVERSE_TARGET_CHUNK_SIZE: usize = 512;
#[cfg(test)]
const REVERSE_TARGET_CHUNK_SIZE: usize = 2;

#[derive(Clone, Copy)]
pub(crate) struct DomainBinding {
    identity: [u8; 32],
    kick_profile: KickTableProfileId,
}

impl DomainBinding {
    pub(crate) const fn new(identity: [u8; 32], kick_profile: KickTableProfileId) -> Self {
        Self {
            identity,
            kick_profile,
        }
    }

    pub(crate) fn identity_string(self) -> String {
        format!("sha256:{}", hex(&self.identity))
    }

    pub(crate) const fn raw_identity(self) -> [u8; 32] {
        self.identity
    }

    /// Binds an independently generated legal-board domain to the exact
    /// Clearra movement table rather than to an upstream dataset generation.
    /// Any ordered kick-offset change produces a different identity and makes
    /// existing layer files fail closed instead of being silently reused.
    pub(crate) fn legal_board(kick_profile: KickTableProfileId) -> Result<Self, String> {
        let identity = clearra_core_executor::built_in_legal_board_rule_identity(kick_profile)
            .map_err(|_| {
                "legal-board generation requires a connected built-in profile".to_owned()
            })?;
        Ok(Self::new(identity, kick_profile))
    }

    pub(crate) const fn legal_board_binding(self) -> clearra_core_executor::LegalBoardBinding {
        clearra_core_executor::LegalBoardBinding {
            kick_profile: self.kick_profile,
            rule_identity: self.identity,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DomainDirection {
    Reverse,
    Forward,
}

impl DomainDirection {
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "reverse" => Ok(Self::Reverse),
            "forward" => Ok(Self::Forward),
            _ => Err("domain direction must be reverse or forward".to_owned()),
        }
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Reverse => "reverse",
            Self::Forward => "forward",
        }
    }
}

pub(crate) struct DomainFile {
    pub(crate) layer: u8,
    pub(crate) fields: Vec<u64>,
    pub(crate) file_identity: String,
    pub(crate) file_digest: [u8; 32],
    pub(crate) derivation: DomainDerivation,
    pub(crate) input_digest: [u8; 32],
    pub(crate) filter_digest: [u8; 32],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum DomainDerivation {
    ReverseSeed = 1,
    ForwardSeed = 2,
    ReverseStep = 3,
    ForwardStep = 4,
    ForwardReachableStep = 5,
    LegalTerminalSeed = 6,
    LegalPredecessorStep = 7,
}

impl DomainDerivation {
    fn parse(value: u8) -> Result<Self, String> {
        match value {
            1 => Ok(Self::ReverseSeed),
            2 => Ok(Self::ForwardSeed),
            3 => Ok(Self::ReverseStep),
            4 => Ok(Self::ForwardStep),
            5 => Ok(Self::ForwardReachableStep),
            6 => Ok(Self::LegalTerminalSeed),
            7 => Ok(Self::LegalPredecessorStep),
            _ => Err("domain derivation invalid".to_owned()),
        }
    }
}

pub(crate) struct SeedReport {
    pub(crate) disposition: &'static str,
    pub(crate) layer: u8,
    pub(crate) field_count: usize,
    pub(crate) file_identity: String,
}

pub(crate) struct StepReport {
    pub(crate) disposition: &'static str,
    pub(crate) input_layer: u8,
    pub(crate) output_layer: u8,
    pub(crate) input_field_count: usize,
    pub(crate) output_field_count: usize,
    pub(crate) candidate_pair_count: usize,
    pub(crate) workers: usize,
    pub(crate) file_identity: String,
}

pub(crate) fn seed(
    binding: DomainBinding,
    direction: DomainDirection,
    output: &Path,
) -> Result<SeedReport, String> {
    let (layer, field) = match direction {
        DomainDirection::Reverse => (10, FIELD_MASK),
        DomainDirection::Forward => (0, 0),
    };
    if output.exists() {
        let existing = read(output, binding, Some(layer))?;
        let expected_derivation = match direction {
            DomainDirection::Reverse => DomainDerivation::ReverseSeed,
            DomainDirection::Forward => DomainDerivation::ForwardSeed,
        };
        if existing.fields != [field]
            || existing.derivation != expected_derivation
            || existing.input_digest != [0; 32]
            || existing.filter_digest != [0; 32]
        {
            return Err("existing domain seed has the wrong field".to_owned());
        }
        return Ok(SeedReport {
            disposition: "already-complete",
            layer,
            field_count: 1,
            file_identity: existing.file_identity,
        });
    }
    let derivation = match direction {
        DomainDirection::Reverse => DomainDerivation::ReverseSeed,
        DomainDirection::Forward => DomainDerivation::ForwardSeed,
    };
    let identity = write(
        output,
        binding,
        layer,
        &[field],
        derivation,
        [0; 32],
        [0; 32],
    )?;
    Ok(SeedReport {
        disposition: "created",
        layer,
        field_count: 1,
        file_identity: identity,
    })
}

pub(crate) fn step(
    binding: DomainBinding,
    direction: DomainDirection,
    input_path: &Path,
    filter_path: Option<&Path>,
    output_path: &Path,
    requested_workers: usize,
) -> Result<StepReport, String> {
    if requested_workers == 0 || requested_workers > MAX_WORKERS {
        return Err("domain worker count outside 1..=64".to_owned());
    }
    let input = read(input_path, binding, None)?;
    let output_layer = match direction {
        DomainDirection::Reverse => input
            .layer
            .checked_sub(1)
            .ok_or("reverse domain is already at layer zero")?,
        DomainDirection::Forward => input
            .layer
            .checked_add(1)
            .filter(|layer| *layer <= 10)
            .ok_or("forward domain is already at layer ten")?,
    };
    let filter = match (direction, filter_path) {
        (DomainDirection::Reverse, None) => None,
        (DomainDirection::Reverse, Some(_)) => {
            return Err("reverse domain step does not accept --filter".to_owned())
        }
        (DomainDirection::Forward, Some(path)) => Some(read(path, binding, Some(output_layer))?),
        // An unfiltered forward step is the product generator's F_k domain.
        // The older filtered form remains readable for historical evidence,
        // but exact legal layers are now derived in a separate backward pass
        // that is restricted to these complete forward layers.
        (DomainDirection::Forward, None) => None,
    };
    if output_path.exists() {
        let existing = read(output_path, binding, Some(output_layer))?;
        let expected_derivation = match direction {
            DomainDirection::Reverse => DomainDerivation::ReverseStep,
            DomainDirection::Forward if filter.is_some() => DomainDerivation::ForwardStep,
            DomainDirection::Forward => DomainDerivation::ForwardReachableStep,
        };
        let expected_filter = filter.as_ref().map_or([0; 32], |value| value.file_digest);
        if existing.derivation != expected_derivation
            || existing.input_digest != input.file_digest
            || existing.filter_digest != expected_filter
        {
            return Err("existing domain step is not bound to its current inputs".to_owned());
        }
        if direction == DomainDirection::Reverse {
            cleanup_reverse_spill(output_path)?;
        }
        return Ok(StepReport {
            disposition: "already-complete",
            input_layer: input.layer,
            output_layer,
            input_field_count: input.fields.len(),
            output_field_count: existing.fields.len(),
            candidate_pair_count: 0,
            workers: 0,
            file_identity: existing.file_identity,
        });
    }

    let available = thread::available_parallelism().map_or(1, usize::from);
    let workers = requested_workers
        .min(available)
        .min(input.fields.len().max(1));
    let (fields, candidate_pair_count) = match direction {
        DomainDirection::Reverse => generate_reverse_layer(
            binding,
            output_layer,
            &input.fields,
            input.file_digest,
            output_path,
            workers,
        )?,
        DomainDirection::Forward => (
            generate_forward_layer(
                binding,
                output_layer,
                &input.fields,
                filter.as_ref().map(|value| value.fields.as_slice()),
                workers,
            )?,
            0,
        ),
    };
    validate_fields(output_layer, &fields)?;
    let derivation = match direction {
        DomainDirection::Reverse => DomainDerivation::ReverseStep,
        DomainDirection::Forward if filter.is_some() => DomainDerivation::ForwardStep,
        DomainDirection::Forward => DomainDerivation::ForwardReachableStep,
    };
    let filter_digest = filter.as_ref().map_or([0; 32], |value| value.file_digest);
    let identity = write(
        output_path,
        binding,
        output_layer,
        &fields,
        derivation,
        input.file_digest,
        filter_digest,
    )?;
    if direction == DomainDirection::Reverse {
        cleanup_reverse_spill(output_path)?;
    }
    Ok(StepReport {
        disposition: "created",
        input_layer: input.layer,
        output_layer,
        input_field_count: input.fields.len(),
        output_field_count: fields.len(),
        candidate_pair_count,
        workers,
        file_identity: identity,
    })
}

fn generate_forward_layer(
    binding: DomainBinding,
    output_layer: u8,
    input: &[u64],
    filter: Option<&[u64]>,
    workers: usize,
) -> Result<Vec<u64>, String> {
    let cursor = AtomicUsize::new(0);
    let partials = thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..workers {
            let cursor = &cursor;
            handles.push(scope.spawn(move || {
                let mut output = BTreeSet::new();
                loop {
                    let begin = cursor.fetch_add(8, Ordering::Relaxed);
                    if begin >= input.len() {
                        break;
                    }
                    for &field_hash in &input[begin..input.len().min(begin + 8)] {
                        let cells = hydra_field_hash_v1_to_clearra_board64_mask(field_hash)
                            .map_err(|error| error.reason().to_owned())?;
                        for piece in PieceKind::STANDARD_TETROMINOES {
                            for candidate in
                                enumerate_pc4_ilc_target_fields(cells, piece, binding.kick_profile)
                                    .map_err(|error| error.reason().to_owned())?
                            {
                                if candidate.count_ones() != u32::from(output_layer) * 4 {
                                    return Err("generated field belongs to the wrong area layer"
                                        .to_owned());
                                }
                                let candidate_hash =
                                    clearra_board64_mask_to_hydra_field_hash_v1(candidate)
                                        .map_err(|error| error.reason().to_owned())?;
                                if filter.is_none_or(|allowed| {
                                    allowed.binary_search(&candidate_hash).is_ok()
                                }) {
                                    output.insert(candidate_hash);
                                }
                            }
                        }
                    }
                }
                Ok(output.into_iter().collect::<Vec<_>>())
            }));
        }
        join_workers(handles)
    })?;
    Ok(merge_sorted(partials))
}

/// Seed `L_10` from the complete forward domain. This deliberately does not
/// trust a reverse seed: a terminal field is a legal board only when it was
/// reached from the empty origin under the exact profile.
pub(crate) fn legal_terminal_seed(
    binding: DomainBinding,
    forward_path: &Path,
    output_path: &Path,
) -> Result<SeedReport, String> {
    let forward = read(forward_path, binding, Some(10))?;
    if output_path.exists() {
        let existing = read(output_path, binding, Some(10))?;
        if existing.derivation != DomainDerivation::LegalTerminalSeed
            || existing.input_digest != forward.file_digest
            || existing.filter_digest != [0; 32]
        {
            return Err(
                "existing legal terminal is not bound to the current forward domain".to_owned(),
            );
        }
        return Ok(SeedReport {
            disposition: "already-complete",
            layer: 10,
            field_count: existing.fields.len(),
            file_identity: existing.file_identity,
        });
    }
    if forward.fields.binary_search(&FIELD_MASK).is_err() {
        return Err("complete forward domain does not reach the full four-line field".to_owned());
    }
    let identity = write(
        output_path,
        binding,
        10,
        &[FIELD_MASK],
        DomainDerivation::LegalTerminalSeed,
        forward.file_digest,
        [0; 32],
    )?;
    Ok(SeedReport {
        disposition: "created",
        layer: 10,
        field_count: 1,
        file_identity: identity,
    })
}

/// Derive `L_k` without materialising the much larger unrestricted `R_k`.
/// A source is admitted iff it belongs to complete `F_k` and one exact ILC
/// placement reaches the already-complete `L_(k+1)` layer.
pub(crate) fn legal_predecessor_step(
    binding: DomainBinding,
    forward_source_path: &Path,
    legal_target_path: &Path,
    output_path: &Path,
    requested_workers: usize,
) -> Result<StepReport, String> {
    if requested_workers == 0 || requested_workers > MAX_WORKERS {
        return Err("domain worker count outside 1..=64".to_owned());
    }
    let forward = read(forward_source_path, binding, None)?;
    if forward.layer >= 10 {
        return Err("legal predecessor source must be below layer ten".to_owned());
    }
    let target_layer = forward.layer + 1;
    let target = read(legal_target_path, binding, Some(target_layer))?;
    if output_path.exists() {
        let existing = read(output_path, binding, Some(forward.layer))?;
        if existing.derivation != DomainDerivation::LegalPredecessorStep
            || existing.input_digest != forward.file_digest
            || existing.filter_digest != target.file_digest
        {
            return Err(
                "existing legal predecessor layer is not bound to its current inputs".to_owned(),
            );
        }
        return Ok(StepReport {
            disposition: "already-complete",
            input_layer: target_layer,
            output_layer: forward.layer,
            input_field_count: target.fields.len(),
            output_field_count: existing.fields.len(),
            candidate_pair_count: 0,
            workers: 0,
            file_identity: existing.file_identity,
        });
    }
    let available = thread::available_parallelism().map_or(1, usize::from);
    let workers = requested_workers
        .min(available)
        .min(forward.fields.len().max(1));
    let cursor = AtomicUsize::new(0);
    let partials = thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..workers {
            let cursor = &cursor;
            let sources = &forward.fields;
            let targets = &target.fields;
            handles.push(scope.spawn(move || {
                let mut admitted = Vec::new();
                loop {
                    let begin = cursor.fetch_add(8, Ordering::Relaxed);
                    if begin >= sources.len() {
                        break;
                    }
                    for &field_hash in &sources[begin..sources.len().min(begin + 8)] {
                        let cells = hydra_field_hash_v1_to_clearra_board64_mask(field_hash)
                            .map_err(|error| error.reason().to_owned())?;
                        let mut reaches_legal_target = false;
                        'pieces: for piece in PieceKind::STANDARD_TETROMINOES {
                            for candidate in
                                enumerate_pc4_ilc_target_fields(cells, piece, binding.kick_profile)
                                    .map_err(|error| error.reason().to_owned())?
                            {
                                let candidate_hash =
                                    clearra_board64_mask_to_hydra_field_hash_v1(candidate)
                                        .map_err(|error| error.reason().to_owned())?;
                                if targets.binary_search(&candidate_hash).is_ok() {
                                    reaches_legal_target = true;
                                    break 'pieces;
                                }
                            }
                        }
                        if reaches_legal_target {
                            admitted.push(field_hash);
                        }
                    }
                }
                Ok(admitted)
            }));
        }
        join_workers(handles)
    })?;
    let fields = merge_sorted(partials);
    validate_fields(forward.layer, &fields)?;
    let identity = write(
        output_path,
        binding,
        forward.layer,
        &fields,
        DomainDerivation::LegalPredecessorStep,
        forward.file_digest,
        target.file_digest,
    )?;
    Ok(StepReport {
        disposition: "created",
        input_layer: target_layer,
        output_layer: forward.layer,
        input_field_count: target.fields.len(),
        output_field_count: fields.len(),
        candidate_pair_count: forward.fields.len(),
        workers,
        file_identity: identity,
    })
}

fn generate_reverse_layer(
    binding: DomainBinding,
    output_layer: u8,
    input: &[u64],
    input_digest: [u8; 32],
    output_path: &Path,
    workers: usize,
) -> Result<(Vec<u64>, usize), String> {
    generate_reverse_layer_spilled(
        binding,
        output_layer,
        input,
        input_digest,
        output_path,
        workers,
        REVERSE_TARGET_CHUNK_SIZE,
    )
}

struct PairRunReader {
    reader: BufReader<File>,
    remaining: usize,
    expected_payload_digest: [u8; 32],
    payload_digest: Sha256,
    verified: bool,
}

impl PairRunReader {
    fn open(
        path: &Path,
        binding: DomainBinding,
        input_digest: [u8; 32],
        output_layer: u8,
        expected_start: usize,
        expected_end: usize,
    ) -> Result<Self, String> {
        let metadata = fs::symlink_metadata(path).map_err(io_error)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("legal-board pair run symlink or non-file rejected".to_owned());
        }
        let file = File::open(path).map_err(io_error)?;
        // Thousands of checkpoint runs may participate in a large layer.
        // Keep the per-run buffer modest so resumability does not turn into a
        // new aggregate memory spike during the k-way merge.
        let mut reader = BufReader::with_capacity(8 * 1024, file);
        let mut header = [0_u8; PAIR_RUN_HEADER_BYTES];
        reader.read_exact(&mut header).map_err(io_error)?;
        if header[..8] != *PAIR_RUN_MAGIC
            || read_u32(&header[8..12])? != VERSION
            || read_u32(&header[12..16])? != u32::from(output_layer)
            || read_u64(&header[16..24])?
                != u64::try_from(expected_start).map_err(|_| "pair run start overflow")?
            || read_u64(&header[24..32])?
                != u64::try_from(expected_end).map_err(|_| "pair run end overflow")?
            || header[40..72] != binding.identity
            || header[72..104] != input_digest
        {
            return Err("legal-board pair run binding mismatch".to_owned());
        }
        let count =
            usize::try_from(read_u64(&header[32..40])?).map_err(|_| "pair run count overflow")?;
        let expected_len = PAIR_RUN_HEADER_BYTES
            .checked_add(
                count
                    .checked_mul(PAIR_RUN_RECORD_BYTES)
                    .ok_or("pair run length overflow")?,
            )
            .ok_or("pair run length overflow")?;
        if metadata.len() != u64::try_from(expected_len).map_err(|_| "pair run length overflow")? {
            return Err("legal-board pair run length mismatch".to_owned());
        }
        Ok(Self {
            reader,
            remaining: count,
            expected_payload_digest: header[104..136]
                .try_into()
                .map_err(|_| "pair run digest width mismatch")?,
            payload_digest: Sha256::new(),
            verified: false,
        })
    }

    fn next_pair(&mut self) -> Result<Option<(u64, u8)>, String> {
        if self.remaining == 0 {
            if !self.verified {
                let observed: [u8; 32] = self.payload_digest.clone().finalize().into();
                if observed != self.expected_payload_digest {
                    return Err("legal-board pair run digest mismatch".to_owned());
                }
                self.verified = true;
            }
            return Ok(None);
        }
        let mut encoded = [0_u8; PAIR_RUN_RECORD_BYTES];
        self.reader.read_exact(&mut encoded).map_err(io_error)?;
        self.payload_digest.update(encoded);
        self.remaining -= 1;
        let source_hash = read_u64(&encoded[..8])?;
        let piece = PieceKind::from_ascii(char::from(encoded[8]))
            .map_err(|_| "legal-board pair run piece invalid".to_owned())?;
        let piece_index = PieceKind::STANDARD_TETROMINOES
            .iter()
            .position(|candidate| *candidate == piece)
            .ok_or("legal-board pair run piece is not standard")?;
        Ok(Some((
            source_hash,
            u8::try_from(piece_index).map_err(|_| "piece index overflow")?,
        )))
    }
}

fn reverse_spill_root(output_path: &Path) -> Result<PathBuf, String> {
    let parent = output_path
        .parent()
        .ok_or("legal-board output has no parent")?;
    let output_name = output_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("legal-board output name must be UTF-8")?;
    Ok(parent.join(format!(".{output_name}.legal-board-spill-v1")))
}

fn prepare_reverse_spill(output_path: &Path) -> Result<PathBuf, String> {
    let root = reverse_spill_root(output_path)?;
    if root.exists() {
        let metadata = fs::symlink_metadata(&root).map_err(io_error)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("legal-board spill root must be a real directory".to_owned());
        }
    } else {
        fs::create_dir(&root).map_err(io_error)?;
    }
    Ok(root)
}

fn cleanup_reverse_spill(output_path: &Path) -> Result<(), String> {
    let root = reverse_spill_root(output_path)?;
    if !root.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(&root).map_err(io_error)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("legal-board spill cleanup rejected non-directory".to_owned());
    }
    fs::remove_dir_all(root).map_err(io_error)
}

fn pair_run_path(root: &Path, start: usize, end: usize) -> PathBuf {
    root.join(format!("pairs-{start:010}-{end:010}.bin"))
}

fn write_pair_run(
    path: &Path,
    binding: DomainBinding,
    input_digest: [u8; 32],
    output_layer: u8,
    start: usize,
    end: usize,
    pairs: &[(u64, PieceKind)],
) -> Result<(), String> {
    if path.exists() {
        return Err("refusing to overwrite a legal-board pair run".to_owned());
    }
    if pairs.windows(2).any(|window| window[0] >= window[1]) {
        return Err("legal-board pair run is not sorted and unique".to_owned());
    }
    let mut payload_digest = Sha256::new();
    for &(source_hash, piece) in pairs {
        payload_digest.update(source_hash.to_le_bytes());
        payload_digest.update([piece.as_ascii() as u8]);
    }
    let payload_digest: [u8; 32] = payload_digest.finalize().into();
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("legal-board pair run name must be UTF-8")?;
    let pending = path.with_file_name(format!(".{name}.pending-{}", std::process::id()));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)
        .map_err(io_error)?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, file);
    let result = (|| {
        writer.write_all(PAIR_RUN_MAGIC).map_err(io_error)?;
        writer.write_all(&VERSION.to_le_bytes()).map_err(io_error)?;
        writer
            .write_all(&u32::from(output_layer).to_le_bytes())
            .map_err(io_error)?;
        writer
            .write_all(
                &u64::try_from(start)
                    .map_err(|_| "pair run start overflow")?
                    .to_le_bytes(),
            )
            .map_err(io_error)?;
        writer
            .write_all(
                &u64::try_from(end)
                    .map_err(|_| "pair run end overflow")?
                    .to_le_bytes(),
            )
            .map_err(io_error)?;
        writer
            .write_all(
                &u64::try_from(pairs.len())
                    .map_err(|_| "pair run count overflow")?
                    .to_le_bytes(),
            )
            .map_err(io_error)?;
        writer.write_all(&binding.identity).map_err(io_error)?;
        writer.write_all(&input_digest).map_err(io_error)?;
        writer.write_all(&payload_digest).map_err(io_error)?;
        for &(source_hash, piece) in pairs {
            writer
                .write_all(&source_hash.to_le_bytes())
                .map_err(io_error)?;
            writer
                .write_all(&[piece.as_ascii() as u8])
                .map_err(io_error)?;
        }
        writer.flush().map_err(io_error)?;
        writer.get_ref().sync_all().map_err(io_error)
    })();
    if let Err(error) = result {
        drop(writer);
        let _ = fs::remove_file(&pending);
        return Err(error);
    }
    drop(writer);
    fs::rename(&pending, path).map_err(io_error)
}

fn piece_from_index(index: u8) -> Result<PieceKind, String> {
    PieceKind::STANDARD_TETROMINOES
        .get(usize::from(index))
        .copied()
        .ok_or("legal-board pair run piece index invalid".to_owned())
}

fn merge_pair_run_group(
    runs: &[(PathBuf, usize, usize)],
    binding: DomainBinding,
    input_digest: [u8; 32],
    output_layer: u8,
) -> Result<Vec<(u64, PieceKind)>, String> {
    let mut readers = runs
        .iter()
        .map(|(path, start, end)| {
            PairRunReader::open(path, binding, input_digest, output_layer, *start, *end)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut heap = BinaryHeap::new();
    for (run_index, reader) in readers.iter_mut().enumerate() {
        if let Some(pair) = reader.next_pair()? {
            heap.push(Reverse((pair, run_index)));
        }
    }
    let mut merged = Vec::new();
    let mut last_pair = None;
    while let Some(Reverse((pair, run_index))) = heap.pop() {
        if let Some(next) = readers[run_index].next_pair()? {
            heap.push(Reverse((next, run_index)));
        }
        if last_pair == Some(pair) {
            continue;
        }
        last_pair = Some(pair);
        merged.push((pair.0, piece_from_index(pair.1)?));
    }
    Ok(merged)
}

fn reduce_pair_runs(
    root: &Path,
    mut runs: Vec<(PathBuf, usize, usize)>,
    binding: DomainBinding,
    input_digest: [u8; 32],
    output_layer: u8,
) -> Result<Vec<(PathBuf, usize, usize)>, String> {
    let mut pass = 0_usize;
    while runs.len() > PAIR_RUN_FAN_IN {
        let mut reduced = Vec::with_capacity(runs.len().div_ceil(PAIR_RUN_FAN_IN));
        for group in runs.chunks(PAIR_RUN_FAN_IN) {
            let start = group
                .first()
                .map(|(_, start, _)| *start)
                .ok_or("legal-board merge group empty")?;
            let end = group
                .last()
                .map(|(_, _, end)| *end)
                .ok_or("legal-board merge group empty")?;
            let path = root.join(format!("merged-{pass:02}-{start:010}-{end:010}.bin"));
            if path.exists() {
                PairRunReader::open(&path, binding, input_digest, output_layer, start, end)?;
                eprintln!(
                    "legal_board_pair_merge=reused layer={} pass={} start={} end={}",
                    output_layer, pass, start, end
                );
            } else {
                let merged = merge_pair_run_group(group, binding, input_digest, output_layer)?;
                write_pair_run(
                    &path,
                    binding,
                    input_digest,
                    output_layer,
                    start,
                    end,
                    &merged,
                )?;
                eprintln!(
                    "legal_board_pair_merge=created layer={} pass={} start={} end={} pairs={}",
                    output_layer,
                    pass,
                    start,
                    end,
                    merged.len()
                );
            }
            reduced.push((path, start, end));
        }
        runs = reduced;
        pass += 1;
    }
    Ok(runs)
}

fn validation_run_path(root: &Path, start: usize, end: usize) -> PathBuf {
    root.join(format!("validated-{start:012}-{end:012}.bin"))
}

fn validation_candidate_digest(candidates: &[(u64, u8)]) -> [u8; 32] {
    let mut digest = Sha256::new();
    for &(source_hash, piece_bits) in candidates {
        digest.update(source_hash.to_le_bytes());
        digest.update([piece_bits]);
    }
    digest.finalize().into()
}

fn write_validation_run(
    path: &Path,
    binding: DomainBinding,
    input_digest: [u8; 32],
    output_layer: u8,
    start: usize,
    end: usize,
    candidate_digest: [u8; 32],
    fields: &[u64],
) -> Result<(), String> {
    if path.exists() {
        return Err("refusing to overwrite a legal-board validation run".to_owned());
    }
    validate_fields(output_layer, fields)?;
    let mut payload_digest = Sha256::new();
    for &field in fields {
        payload_digest.update(field.to_le_bytes());
    }
    let payload_digest: [u8; 32] = payload_digest.finalize().into();
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("legal-board validation run name must be UTF-8")?;
    let pending = path.with_file_name(format!(".{name}.pending-{}", std::process::id()));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)
        .map_err(io_error)?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, file);
    let result = (|| {
        writer.write_all(VALIDATION_RUN_MAGIC).map_err(io_error)?;
        writer.write_all(&VERSION.to_le_bytes()).map_err(io_error)?;
        writer
            .write_all(&u32::from(output_layer).to_le_bytes())
            .map_err(io_error)?;
        writer
            .write_all(
                &u64::try_from(start)
                    .map_err(|_| "validation run start overflow")?
                    .to_le_bytes(),
            )
            .map_err(io_error)?;
        writer
            .write_all(
                &u64::try_from(end)
                    .map_err(|_| "validation run end overflow")?
                    .to_le_bytes(),
            )
            .map_err(io_error)?;
        writer
            .write_all(
                &u64::try_from(fields.len())
                    .map_err(|_| "validation run count overflow")?
                    .to_le_bytes(),
            )
            .map_err(io_error)?;
        writer.write_all(&binding.identity).map_err(io_error)?;
        writer.write_all(&input_digest).map_err(io_error)?;
        writer.write_all(&candidate_digest).map_err(io_error)?;
        writer.write_all(&payload_digest).map_err(io_error)?;
        for &field in fields {
            writer.write_all(&field.to_le_bytes()).map_err(io_error)?;
        }
        writer.flush().map_err(io_error)?;
        writer.get_ref().sync_all().map_err(io_error)
    })();
    if let Err(error) = result {
        drop(writer);
        let _ = fs::remove_file(&pending);
        return Err(error);
    }
    drop(writer);
    fs::rename(&pending, path).map_err(io_error)
}

fn read_validation_run(
    path: &Path,
    binding: DomainBinding,
    input_digest: [u8; 32],
    output_layer: u8,
    start: usize,
    end: usize,
    candidate_digest: [u8; 32],
) -> Result<Vec<u64>, String> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("legal-board validation run symlink or non-file rejected".to_owned());
    }
    let mut reader = BufReader::with_capacity(64 * 1024, File::open(path).map_err(io_error)?);
    let mut header = [0_u8; VALIDATION_RUN_HEADER_BYTES];
    reader.read_exact(&mut header).map_err(io_error)?;
    if header[..8] != *VALIDATION_RUN_MAGIC
        || read_u32(&header[8..12])? != VERSION
        || read_u32(&header[12..16])? != u32::from(output_layer)
        || read_u64(&header[16..24])?
            != u64::try_from(start).map_err(|_| "validation run start overflow")?
        || read_u64(&header[24..32])?
            != u64::try_from(end).map_err(|_| "validation run end overflow")?
        || header[40..72] != binding.identity
        || header[72..104] != input_digest
        || header[104..136] != candidate_digest
    {
        return Err("legal-board validation run binding mismatch".to_owned());
    }
    let count =
        usize::try_from(read_u64(&header[32..40])?).map_err(|_| "validation run count overflow")?;
    let expected_len = VALIDATION_RUN_HEADER_BYTES
        .checked_add(
            count
                .checked_mul(8)
                .ok_or("validation run length overflow")?,
        )
        .ok_or("validation run length overflow")?;
    if metadata.len()
        != u64::try_from(expected_len).map_err(|_| "validation run length overflow")?
    {
        return Err("legal-board validation run length mismatch".to_owned());
    }
    let mut payload_digest = Sha256::new();
    let mut fields = Vec::new();
    fields
        .try_reserve_exact(count)
        .map_err(|_| "validation run allocation failed")?;
    for _ in 0..count {
        let mut encoded = [0_u8; 8];
        reader.read_exact(&mut encoded).map_err(io_error)?;
        payload_digest.update(encoded);
        fields.push(u64::from_le_bytes(encoded));
    }
    let observed_digest: [u8; 32] = payload_digest.finalize().into();
    if observed_digest != header[136..168] {
        return Err("legal-board validation run digest mismatch".to_owned());
    }
    validate_fields(output_layer, &fields)?;
    Ok(fields)
}

fn validate_reverse_checkpoint(
    root: &Path,
    binding: DomainBinding,
    input: &[u64],
    input_digest: [u8; 32],
    output_layer: u8,
    candidates: &[(u64, u8)],
    workers: usize,
    start: usize,
    end: usize,
) -> Result<Vec<u64>, String> {
    let candidate_digest = validation_candidate_digest(candidates);
    let path = validation_run_path(root, start, end);
    if path.exists() {
        let fields = read_validation_run(
            &path,
            binding,
            input_digest,
            output_layer,
            start,
            end,
            candidate_digest,
        )?;
        eprintln!(
            "legal_board_validation_run=reused layer={} start={} end={} fields={}",
            output_layer,
            start,
            end,
            fields.len()
        );
        return Ok(fields);
    }
    let fields = validate_reverse_sources(binding, input, candidates, workers)?;
    write_validation_run(
        &path,
        binding,
        input_digest,
        output_layer,
        start,
        end,
        candidate_digest,
        &fields,
    )?;
    eprintln!(
        "legal_board_validation_run=created layer={} start={} end={} fields={}",
        output_layer,
        start,
        end,
        fields.len()
    );
    Ok(fields)
}

fn validate_reverse_sources(
    binding: DomainBinding,
    input: &[u64],
    candidates: &[(u64, u8)],
    workers: usize,
) -> Result<Vec<u64>, String> {
    let cursor = AtomicUsize::new(0);
    let partials = thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..workers.min(candidates.len().max(1)) {
            let cursor = &cursor;
            handles.push(scope.spawn(move || {
                let mut validated = Vec::new();
                loop {
                    let begin = cursor.fetch_add(16, Ordering::Relaxed);
                    if begin >= candidates.len() {
                        break;
                    }
                    for (offset, &(source_hash, piece_bits)) in candidates
                        [begin..candidates.len().min(begin + 16)]
                        .iter()
                        .enumerate()
                    {
                        let source = hydra_field_hash_v1_to_clearra_board64_mask(source_hash)
                            .map_err(|error| error.reason().to_owned())?;
                        let mut reaches_domain = false;
                        for (piece_index, piece) in
                            PieceKind::STANDARD_TETROMINOES.iter().copied().enumerate()
                        {
                            if piece_bits & (1_u8 << piece_index) == 0 {
                                continue;
                            }
                            reaches_domain = enumerate_pc4_ilc_target_fields(
                                source,
                                piece,
                                binding.kick_profile,
                            )
                            .map_err(|error| error.reason().to_owned())?
                            .into_iter()
                            .map(clearra_board64_mask_to_hydra_field_hash_v1)
                            .collect::<Result<Vec<_>, _>>()
                            .map_err(|error| error.reason().to_owned())?
                            .into_iter()
                            .any(|target_hash| input.binary_search(&target_hash).is_ok());
                            if reaches_domain {
                                break;
                            }
                        }
                        if reaches_domain {
                            validated.push((begin + offset, source_hash));
                        }
                    }
                }
                Ok(validated)
            }));
        }
        join_workers(handles)
    })?;
    let mut indexed = partials.into_iter().flatten().collect::<Vec<_>>();
    indexed.sort_unstable_by_key(|(index, _)| *index);
    Ok(indexed.into_iter().map(|(_, field)| field).collect())
}

fn generate_reverse_layer_spilled(
    binding: DomainBinding,
    output_layer: u8,
    input: &[u64],
    input_digest: [u8; 32],
    output_path: &Path,
    workers: usize,
    target_chunk_size: usize,
) -> Result<(Vec<u64>, usize), String> {
    if target_chunk_size == 0 {
        return Err("reverse target chunk size must be positive".to_owned());
    }
    let spill_root = prepare_reverse_spill(output_path)?;
    let mut run_paths = Vec::new();
    let mut created_run_count = 0_usize;
    let mut reused_run_count = 0_usize;
    for (chunk_index, input_chunk) in input.chunks(target_chunk_size).enumerate() {
        let start = chunk_index
            .checked_mul(target_chunk_size)
            .ok_or("pair run start overflow")?;
        let end = start
            .checked_add(input_chunk.len())
            .ok_or("pair run end overflow")?;
        let path = pair_run_path(&spill_root, start, end);
        if path.exists() {
            PairRunReader::open(&path, binding, input_digest, output_layer, start, end)?;
            reused_run_count += 1;
            run_paths.push((path, start, end));
            continue;
        }
        let candidate_cursor = AtomicUsize::new(0);
        let candidate_partials = thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 0..workers.min(input_chunk.len().max(1)) {
                let cursor = &candidate_cursor;
                handles.push(scope.spawn(move || {
                    let mut candidates = Vec::new();
                    loop {
                        let target_index = cursor.fetch_add(1, Ordering::Relaxed);
                        let Some(&target_hash) = input_chunk.get(target_index) else {
                            break;
                        };
                        let cells = hydra_field_hash_v1_to_clearra_board64_mask(target_hash)
                            .map_err(|error| error.reason().to_owned())?;
                        for piece in PieceKind::STANDARD_TETROMINOES {
                            for source in enumerate_pc4_ilc_geometric_predecessor_fields(
                                cells,
                                piece,
                                binding.kick_profile,
                            )
                            .map_err(|error| error.reason().to_owned())?
                            {
                                if source.count_ones() != u32::from(output_layer) * 4 {
                                    return Err(
                                        "geometric predecessor belongs to the wrong area layer"
                                            .to_owned(),
                                    );
                                }
                                let source_hash =
                                    clearra_board64_mask_to_hydra_field_hash_v1(source)
                                        .map_err(|error| error.reason().to_owned())?;
                                candidates.push((source_hash, piece));
                            }
                        }
                    }
                    candidates.sort_unstable();
                    candidates.dedup();
                    Ok(candidates)
                }));
            }
            join_workers(handles)
        })?;
        let candidate_pairs = merge_sorted(candidate_partials);
        write_pair_run(
            &path,
            binding,
            input_digest,
            output_layer,
            start,
            end,
            &candidate_pairs,
        )?;
        created_run_count += 1;
        if created_run_count % 128 == 0 || end == input.len() {
            eprintln!(
                "legal_board_pair_run=progress layer={} created={} reused={} end={} pairs_in_latest={}",
                output_layer,
                created_run_count,
                reused_run_count,
                end,
                candidate_pairs.len()
            );
        }
        run_paths.push((path, start, end));
    }
    eprintln!(
        "legal_board_pair_run=complete layer={} created={} reused={} total={}",
        output_layer,
        created_run_count,
        reused_run_count,
        run_paths.len()
    );

    let run_paths = reduce_pair_runs(&spill_root, run_paths, binding, input_digest, output_layer)?;
    let mut readers = run_paths
        .iter()
        .map(|(path, start, end)| {
            PairRunReader::open(path, binding, input_digest, output_layer, *start, *end)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut heap = BinaryHeap::new();
    for (run_index, reader) in readers.iter_mut().enumerate() {
        if let Some(pair) = reader.next_pair()? {
            heap.push(Reverse((pair, run_index)));
        }
    }
    let mut fields = Vec::new();
    let mut batch = Vec::with_capacity(REVERSE_VALIDATION_BATCH_SIZE);
    let mut current_source = None;
    let mut current_piece_bits = 0_u8;
    let mut last_pair = None;
    let mut candidate_pair_count = 0_usize;
    let mut source_count = 0_usize;
    while let Some(Reverse((pair, run_index))) = heap.pop() {
        if let Some(next) = readers[run_index].next_pair()? {
            heap.push(Reverse((next, run_index)));
        }
        if last_pair == Some(pair) {
            continue;
        }
        last_pair = Some(pair);
        candidate_pair_count = candidate_pair_count
            .checked_add(1)
            .ok_or("candidate pair count overflow")?;
        if current_source.is_some_and(|source| source != pair.0) {
            batch.push((current_source.expect("source exists"), current_piece_bits));
            source_count += 1;
            if batch.len() == REVERSE_VALIDATION_BATCH_SIZE {
                let start = source_count - batch.len();
                fields.extend(validate_reverse_checkpoint(
                    &spill_root,
                    binding,
                    input,
                    input_digest,
                    output_layer,
                    &batch,
                    workers,
                    start,
                    source_count,
                )?);
                batch.clear();
                eprintln!(
                    "legal_board_validation=progress layer={} sources={} fields={}",
                    output_layer,
                    source_count,
                    fields.len()
                );
            }
            current_piece_bits = 0;
        }
        current_source = Some(pair.0);
        current_piece_bits |= 1_u8 << pair.1;
    }
    if let Some(source) = current_source {
        batch.push((source, current_piece_bits));
        source_count += 1;
    }
    if !batch.is_empty() {
        let start = source_count - batch.len();
        fields.extend(validate_reverse_checkpoint(
            &spill_root,
            binding,
            input,
            input_digest,
            output_layer,
            &batch,
            workers,
            start,
            source_count,
        )?);
    }
    eprintln!(
        "legal_board_validation=complete layer={} pairs={} sources={} fields={}",
        output_layer,
        candidate_pair_count,
        source_count,
        fields.len()
    );
    Ok((fields, candidate_pair_count))
}

fn generate_reverse_layer_bounded(
    binding: DomainBinding,
    output_layer: u8,
    input: &[u64],
    workers: usize,
    target_chunk_size: usize,
) -> Result<(Vec<u64>, usize), String> {
    if target_chunk_size == 0 {
        return Err("reverse target chunk size must be positive".to_owned());
    }
    // A complete 4L layer can expand to hundreds of millions of geometric
    // `(source, piece)` pairs. Keeping every pair, every worker partial, and
    // the merged copy live at once makes the generator depend on the host's
    // free working set even though the published domain is much smaller.
    //
    // Bound that transient set by target chunks. Each chunk is still claimed
    // dynamically by all requested workers, validated against the complete
    // input layer, and deduplicated exactly. Only the final source-field set
    // crosses chunk boundaries, so chunking changes neither membership nor
    // the deterministic sorted file identity.
    let mut validated_fields = HashSet::new();
    let mut candidate_pair_count = 0_usize;

    for input_chunk in input.chunks(target_chunk_size) {
        let candidate_cursor = AtomicUsize::new(0);
        let candidate_partials = thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 0..workers {
                let cursor = &candidate_cursor;
                handles.push(scope.spawn(move || {
                    let mut candidates = Vec::new();
                    loop {
                        let index = cursor.fetch_add(1, Ordering::Relaxed);
                        let Some(&target_hash) = input_chunk.get(index) else {
                            break;
                        };
                        let target = hydra_field_hash_v1_to_clearra_board64_mask(target_hash)
                            .map_err(|error| error.reason().to_owned())?;
                        for piece in PieceKind::STANDARD_TETROMINOES {
                            for source in enumerate_pc4_ilc_geometric_predecessor_fields(
                                target,
                                piece,
                                binding.kick_profile,
                            )
                            .map_err(|error| error.reason().to_owned())?
                            {
                                if source.count_ones() != u32::from(output_layer) * 4 {
                                    return Err(
                                        "geometric predecessor belongs to the wrong area layer"
                                            .to_owned(),
                                    );
                                }
                                let source_hash =
                                    clearra_board64_mask_to_hydra_field_hash_v1(source)
                                        .map_err(|error| error.reason().to_owned())?;
                                candidates.push((source_hash, piece));
                            }
                        }
                    }
                    candidates.sort_unstable();
                    candidates.dedup();
                    Ok(candidates)
                }));
            }
            join_workers(handles)
        })?;
        let candidate_pairs = merge_sorted(candidate_partials);
        candidate_pair_count = candidate_pair_count
            .checked_add(candidate_pairs.len())
            .ok_or("candidate pair count overflow")?;

        let validation_workers = workers.min(candidate_pairs.len().max(1));
        let validation_cursor = AtomicUsize::new(0);
        let validated_partials = thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 0..validation_workers {
                let cursor = &validation_cursor;
                let candidate_pairs = &candidate_pairs;
                handles.push(scope.spawn(move || {
                    let mut validated = Vec::new();
                    loop {
                        let begin = cursor.fetch_add(4, Ordering::Relaxed);
                        if begin >= candidate_pairs.len() {
                            break;
                        }
                        for &(source_hash, piece) in
                            &candidate_pairs[begin..candidate_pairs.len().min(begin + 4)]
                        {
                            let source = hydra_field_hash_v1_to_clearra_board64_mask(source_hash)
                                .map_err(|error| error.reason().to_owned())?;
                            let reaches_domain = enumerate_pc4_ilc_target_fields(
                                source,
                                piece,
                                binding.kick_profile,
                            )
                            .map_err(|error| error.reason().to_owned())?
                            .into_iter()
                            .map(clearra_board64_mask_to_hydra_field_hash_v1)
                            .collect::<Result<Vec<_>, _>>()
                            .map_err(|error| error.reason().to_owned())?
                            .into_iter()
                            .any(|target_hash| input.binary_search(&target_hash).is_ok());
                            if reaches_domain {
                                validated.push(source_hash);
                            }
                        }
                    }
                    validated.sort_unstable();
                    validated.dedup();
                    Ok(validated)
                }));
            }
            join_workers(handles)
        })?;
        for field in merge_sorted(validated_partials) {
            validated_fields.insert(field);
        }
    }

    let mut fields = validated_fields.into_iter().collect::<Vec<_>>();
    fields.sort_unstable();
    Ok((fields, candidate_pair_count))
}

fn join_workers<T>(
    handles: Vec<thread::ScopedJoinHandle<'_, Result<T, String>>>,
) -> Result<Vec<T>, String> {
    handles
        .into_iter()
        .map(|handle| {
            handle
                .join()
                .map_err(|_| "domain worker panicked".to_owned())?
        })
        .collect()
}

fn merge_sorted<T: Copy + Ord>(partials: Vec<Vec<T>>) -> Vec<T> {
    let capacity = partials.iter().map(Vec::len).sum();
    let mut merged = Vec::with_capacity(capacity);
    let mut heap = BinaryHeap::new();
    for (partition, values) in partials.iter().enumerate() {
        if let Some(&value) = values.first() {
            heap.push(Reverse((value, partition, 0_usize)));
        }
    }
    while let Some(Reverse((value, partition, index))) = heap.pop() {
        if merged.last().is_none_or(|prior| *prior != value) {
            merged.push(value);
        }
        let next = index + 1;
        if let Some(&next_value) = partials[partition].get(next) {
            heap.push(Reverse((next_value, partition, next)));
        }
    }
    merged.shrink_to_fit();
    merged
}

pub(crate) fn read(
    path: &Path,
    binding: DomainBinding,
    expected_layer: Option<u8>,
) -> Result<DomainFile, String> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("domain file symlink or non-file rejected".to_owned());
    }
    let bytes = fs::read(path).map_err(io_error)?;
    if bytes.len() < HEADER_BYTES || bytes.get(..8) != Some(MAGIC.as_slice()) {
        return Err("domain file header invalid".to_owned());
    }
    if read_u32(&bytes[8..12])? != VERSION {
        return Err("domain file version invalid".to_owned());
    }
    let layer_u32 = read_u32(&bytes[12..16])?;
    let layer = u8::try_from(layer_u32).map_err(|_| "domain layer overflow")?;
    if layer > 10 || expected_layer.is_some_and(|expected| expected != layer) {
        return Err("domain file layer mismatch".to_owned());
    }
    let count =
        usize::try_from(read_u64(&bytes[16..24])?).map_err(|_| "domain field count overflow")?;
    if bytes.get(24..56) != Some(binding.identity.as_slice())
        || bytes.len() != HEADER_BYTES.saturating_add(count.saturating_mul(8))
    {
        return Err("domain file binding or length mismatch".to_owned());
    }
    let derivation = DomainDerivation::parse(bytes[56])?;
    if bytes[57..64].iter().any(|byte| *byte != 0) {
        return Err("domain file reserved header bytes are nonzero".to_owned());
    }
    let input_digest: [u8; 32] = bytes[64..96]
        .try_into()
        .map_err(|_| "domain input digest width mismatch")?;
    let filter_digest: [u8; 32] = bytes[96..128]
        .try_into()
        .map_err(|_| "domain filter digest width mismatch")?;
    let mut fields = Vec::new();
    fields
        .try_reserve_exact(count)
        .map_err(|_| "domain file allocation failed")?;
    for encoded in bytes[HEADER_BYTES..].chunks_exact(8) {
        fields.push(read_u64(encoded)?);
    }
    validate_fields(layer, &fields)?;
    let file_digest: [u8; 32] = Sha256::digest(&bytes).into();
    Ok(DomainFile {
        layer,
        fields,
        file_identity: format!("sha256:{}", hex(&file_digest)),
        file_digest,
        derivation,
        input_digest,
        filter_digest,
    })
}

fn write(
    path: &Path,
    binding: DomainBinding,
    layer: u8,
    fields: &[u64],
    derivation: DomainDerivation,
    input_digest: [u8; 32],
    filter_digest: [u8; 32],
) -> Result<String, String> {
    validate_fields(layer, fields)?;
    if path.exists() {
        return Err("refusing to overwrite an existing domain file".to_owned());
    }
    let parent = path.parent().ok_or("domain output has no parent")?;
    let parent_metadata = fs::symlink_metadata(parent).map_err(io_error)?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
        return Err("domain output parent must be a real directory".to_owned());
    }
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("domain output name must be UTF-8")?;
    let pending = parent.join(format!(".{name}.pending-{}", std::process::id()));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)
        .map_err(io_error)?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, file);
    let result = (|| {
        let mut digest = Sha256::new();
        let mut header = Vec::with_capacity(HEADER_BYTES);
        header.extend_from_slice(MAGIC);
        header.extend_from_slice(&VERSION.to_le_bytes());
        header.extend_from_slice(&u32::from(layer).to_le_bytes());
        header.extend_from_slice(&(fields.len() as u64).to_le_bytes());
        header.extend_from_slice(&binding.identity);
        header.push(derivation as u8);
        header.extend_from_slice(&[0; 7]);
        header.extend_from_slice(&input_digest);
        header.extend_from_slice(&filter_digest);
        writer.write_all(&header).map_err(io_error)?;
        digest.update(&header);
        for field in fields {
            let encoded = field.to_le_bytes();
            writer.write_all(&encoded).map_err(io_error)?;
            digest.update(encoded);
        }
        writer.flush().map_err(io_error)?;
        writer.get_ref().sync_all().map_err(io_error)?;
        drop(writer);
        fs::rename(&pending, path).map_err(io_error)?;
        Ok(format!("sha256:{}", hex(digest.finalize().as_slice())))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&pending);
    }
    result
}

fn validate_fields(layer: u8, fields: &[u64]) -> Result<(), String> {
    let expected_cells = u32::from(layer) * 4;
    let mut prior = None;
    for &field in fields {
        if field & !FIELD_MASK != 0 || field.count_ones() != expected_cells {
            return Err("domain field outside its exact area layer".to_owned());
        }
        if prior.is_some_and(|value| value >= field) {
            return Err("domain fields must be strictly sorted and unique".to_owned());
        }
        prior = Some(field);
    }
    Ok(())
}

fn read_u32(bytes: &[u8]) -> Result<u32, String> {
    Ok(u32::from_le_bytes(
        bytes.try_into().map_err(|_| "u32 width mismatch")?,
    ))
}

fn read_u64(bytes: &[u8]) -> Result<u64, String> {
    Ok(u64::from_le_bytes(
        bytes.try_into().map_err(|_| "u64 width mismatch")?,
    ))
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 15)]));
    }
    output
}

fn io_error(error: std::io::Error) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding() -> DomainBinding {
        DomainBinding::new([7; 32], KickTableProfileId::Jstris180)
    }

    #[test]
    fn legal_board_binding_is_stable_and_profile_specific() {
        let srs_plus_a = DomainBinding::legal_board(KickTableProfileId::SrsPlus).unwrap();
        let srs_plus_b = DomainBinding::legal_board(KickTableProfileId::SrsPlus).unwrap();
        let jstris = DomainBinding::legal_board(KickTableProfileId::Jstris180).unwrap();

        assert_eq!(srs_plus_a.raw_identity(), srs_plus_b.raw_identity());
        assert_ne!(srs_plus_a.raw_identity(), jstris.raw_identity());
        assert!(DomainBinding::legal_board(KickTableProfileId::Custom).is_err());
    }

    fn test_root(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("clearra-pc4-domain-{name}-{}", std::process::id()))
    }

    #[test]
    fn domain_file_round_trip_rejects_wrong_binding_and_layer() {
        let root = test_root("roundtrip");
        fs::create_dir(&root).unwrap();
        let path = root.join("layer.bin");
        let fields = [0b1111_u64, 0b1111_0000_u64];
        write(
            &path,
            binding(),
            1,
            &fields,
            DomainDerivation::ReverseStep,
            [1; 32],
            [0; 32],
        )
        .unwrap();
        let observed = read(&path, binding(), Some(1)).unwrap();
        assert_eq!(observed.fields, fields);
        assert_eq!(observed.derivation, DomainDerivation::ReverseStep);
        assert_eq!(observed.input_digest, [1; 32]);
        assert_eq!(observed.filter_digest, [0; 32]);
        assert!(read(
            &path,
            DomainBinding::new([8; 32], KickTableProfileId::Jstris180),
            Some(1)
        )
        .is_err());
        assert!(read(&path, binding(), Some(2)).is_err());
        fs::remove_file(path).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn seed_fields_occupy_the_terminal_layers() {
        let root = test_root("seed");
        fs::create_dir(&root).unwrap();
        let reverse = root.join("reverse.bin");
        let forward = root.join("forward.bin");
        seed(binding(), DomainDirection::Reverse, &reverse).unwrap();
        seed(binding(), DomainDirection::Forward, &forward).unwrap();
        assert_eq!(
            read(&reverse, binding(), Some(10)).unwrap().fields,
            [FIELD_MASK]
        );
        let forward_file = read(&forward, binding(), Some(0)).unwrap();
        assert_eq!(forward_file.fields, [0]);
        assert_eq!(forward_file.derivation, DomainDerivation::ForwardSeed);
        fs::remove_file(reverse).unwrap();
        fs::remove_file(forward).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn legal_backtrace_is_restricted_to_the_complete_forward_source() {
        let binding = DomainBinding::legal_board(KickTableProfileId::SrsPlus).unwrap();
        let root = test_root("legal-backtrace");
        fs::create_dir(&root).unwrap();
        let forward_ten = root.join("forward-10.bin");
        let legal_ten = root.join("legal-10.bin");
        let forward_nine = root.join("forward-09.bin");
        let legal_nine = root.join("legal-09.bin");

        write(
            &forward_ten,
            binding,
            10,
            &[FIELD_MASK],
            DomainDerivation::ForwardReachableStep,
            [1; 32],
            [0; 32],
        )
        .unwrap();
        legal_terminal_seed(binding, &forward_ten, &legal_ten).unwrap();

        let (all_predecessors, _) =
            generate_reverse_layer_bounded(binding, 9, &[FIELD_MASK], 2, 1).unwrap();
        let selected = [
            all_predecessors[0],
            all_predecessors[all_predecessors.len() - 1],
        ];
        write(
            &forward_nine,
            binding,
            9,
            &selected,
            DomainDerivation::ForwardReachableStep,
            [2; 32],
            [0; 32],
        )
        .unwrap();
        legal_predecessor_step(binding, &forward_nine, &legal_ten, &legal_nine, 2).unwrap();
        let observed = read(&legal_nine, binding, Some(9)).unwrap();

        assert_eq!(observed.fields, selected);
        assert_eq!(observed.derivation, DomainDerivation::LegalPredecessorStep);
        assert_eq!(
            observed.input_digest,
            read(&forward_nine, binding, Some(9)).unwrap().file_digest
        );
        assert_eq!(
            observed.filter_digest,
            read(&legal_ten, binding, Some(10)).unwrap().file_digest
        );

        for path in [forward_ten, legal_ten, forward_nine, legal_nine] {
            fs::remove_file(path).unwrap();
        }
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn sorted_worker_partitions_use_a_unique_k_way_merge() {
        assert_eq!(
            merge_sorted(vec![
                vec![1_u64, 3, 8],
                vec![1, 2, 8],
                Vec::new(),
                vec![4, 9]
            ]),
            vec![1, 2, 3, 4, 8, 9]
        );
    }

    #[test]
    fn reverse_domain_membership_is_independent_of_target_chunk_boundaries() {
        let binding = DomainBinding::legal_board(KickTableProfileId::SrsPlus).unwrap();
        let (layer_nine, _) =
            generate_reverse_layer_bounded(binding, 9, &[FIELD_MASK], 2, 1).unwrap();
        let input = &layer_nine[..5];

        let (one_target_per_chunk, _) =
            generate_reverse_layer_bounded(binding, 8, input, 3, 1).unwrap();
        let (single_chunk, _) =
            generate_reverse_layer_bounded(binding, 8, input, 3, input.len()).unwrap();

        assert_eq!(one_target_per_chunk, single_chunk);
    }

    #[test]
    fn spilled_reverse_domain_matches_in_memory_membership() {
        let binding = DomainBinding::legal_board(KickTableProfileId::SrsPlus).unwrap();
        let (layer_nine, _) =
            generate_reverse_layer_bounded(binding, 9, &[FIELD_MASK], 2, 1).unwrap();
        let input = &layer_nine[..5];
        let root = test_root("spilled-reverse");
        fs::create_dir(&root).unwrap();
        let output = root.join("layer8.bin");

        let (expected, _) =
            generate_reverse_layer_bounded(binding, 8, input, 3, input.len()).unwrap();
        let (observed, _) =
            generate_reverse_layer_spilled(binding, 8, input, [9; 32], &output, 3, 1).unwrap();
        let (resumed, _) =
            generate_reverse_layer_spilled(binding, 8, input, [9; 32], &output, 3, 1).unwrap();

        assert_eq!(observed, expected);
        assert_eq!(resumed, expected);
        cleanup_reverse_spill(&output).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
