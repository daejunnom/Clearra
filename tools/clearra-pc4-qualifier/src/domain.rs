// SRP rationale: this module has one change reason: producing and validating
// profile-bound, resumable PC4 forward and reverse domain layers.
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_core_executor::{
    enumerate_pc4_ilc_geometric_predecessor_fields, enumerate_pc4_ilc_target_fields,
    Pc4IlcForwardMembershipWorkspace,
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
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
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
// Generator-only semi-join filter. False positives are checked against the
// complete sorted F_k file; false negatives are impossible for inserted keys.
const FORWARD_BLOOM_MAX_BYTES: usize = 128 * 1024 * 1024;
const FORWARD_BLOOM_HASHES: u64 = 4;
const REVERSE_VALIDATION_BATCH_SIZE: usize = 131_072;
#[cfg(not(test))]
const PAIR_RUN_FAN_IN: usize = 32;
#[cfg(test)]
const PAIR_RUN_FAN_IN: usize = 2;
#[cfg(not(test))]
const REVERSE_TARGET_CHUNK_SIZE: usize = 512;
#[cfg(test)]
const REVERSE_TARGET_CHUNK_SIZE: usize = 2;
// L_m uses a verified Bloom semi-join, so larger target chunks avoid tens of
// thousands of tiny checkpoint files without retaining a whole reverse layer.
#[cfg(not(test))]
const LEGAL_PREDECESSOR_TARGET_CHUNK_SIZE: usize = 4096;
#[cfg(test)]
const LEGAL_PREDECESSOR_TARGET_CHUNK_SIZE: usize = 2;
#[cfg(not(test))]
const FORWARD_SOURCE_CHUNK_SIZE: usize = 1024;
#[cfg(test)]
const FORWARD_SOURCE_CHUNK_SIZE: usize = 2;

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

pub(crate) struct DomainSummary {
    pub(crate) layer: u8,
    pub(crate) field_count: usize,
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
    if direction == DomainDirection::Forward && filter_path.is_none() {
        return step_unfiltered_forward(binding, input_path, output_path, requested_workers);
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
        } else {
            cleanup_forward_spill(output_path)?;
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
    if direction == DomainDirection::Forward {
        let filter_digest = filter.as_ref().map_or([0; 32], |value| value.file_digest);
        let derivation = if filter.is_some() {
            DomainDerivation::ForwardStep
        } else {
            DomainDerivation::ForwardReachableStep
        };
        let (identity, output_field_count) = write_streamed_domain(
            output_path,
            binding,
            output_layer,
            derivation,
            input.file_digest,
            filter_digest,
            |emit| {
                visit_forward_layer_spilled(
                    binding,
                    output_layer,
                    ForwardSource::Fields(&input.fields),
                    input.file_digest,
                    filter.as_ref().map(|value| value.fields.as_slice()),
                    filter_digest,
                    output_path,
                    workers,
                    emit,
                )
            },
        )?;
        cleanup_forward_spill(output_path)?;
        return Ok(StepReport {
            disposition: "created",
            input_layer: input.layer,
            output_layer,
            input_field_count: input.fields.len(),
            output_field_count,
            candidate_pair_count: 0,
            workers,
            file_identity: identity,
        });
    }
    let mut candidate_pair_count = 0_usize;
    let (identity, output_field_count) = write_streamed_domain(
        output_path,
        binding,
        output_layer,
        DomainDerivation::ReverseStep,
        input.file_digest,
        [0; 32],
        |emit| {
            let (count, pairs) = visit_reverse_layer_spilled(
                binding,
                output_layer,
                &input.fields,
                input.file_digest,
                output_path,
                workers,
                REVERSE_TARGET_CHUNK_SIZE,
                None,
                emit,
            )?;
            candidate_pair_count = pairs;
            Ok(count)
        },
    )?;
    cleanup_reverse_spill(output_path)?;
    Ok(StepReport {
        disposition: "created",
        input_layer: input.layer,
        output_layer,
        input_field_count: input.fields.len(),
        output_field_count,
        candidate_pair_count,
        workers,
        file_identity: identity,
    })
}

/// The product F_k path validates the input without materialising it, then
/// rechecks the same immutable digest while feeding bounded source chunks into
/// pair runs. Historical filtered steps retain their in-memory input path.
fn step_unfiltered_forward(
    binding: DomainBinding,
    input_path: &Path,
    output_path: &Path,
    requested_workers: usize,
) -> Result<StepReport, String> {
    let input = inspect_domain_file(input_path, binding, None)?;
    let output_layer = input
        .layer
        .checked_add(1)
        .filter(|layer| *layer <= 10)
        .ok_or("forward domain is already at layer ten")?;
    if output_path.exists() {
        let existing = inspect_domain_file(output_path, binding, Some(output_layer))?;
        if existing.derivation != DomainDerivation::ForwardReachableStep
            || existing.input_digest != input.file_digest
            || existing.filter_digest != [0; 32]
        {
            return Err("existing domain step is not bound to its current inputs".to_owned());
        }
        cleanup_forward_spill(output_path)?;
        return Ok(StepReport {
            disposition: "already-complete",
            input_layer: input.layer,
            output_layer,
            input_field_count: input.field_count,
            output_field_count: existing.field_count,
            candidate_pair_count: 0,
            workers: 0,
            file_identity: existing.file_identity,
        });
    }
    let available = thread::available_parallelism().map_or(1, usize::from);
    let workers = requested_workers
        .min(available)
        .min(input.field_count.max(1));
    let (identity, output_field_count) = write_streamed_domain(
        output_path,
        binding,
        output_layer,
        DomainDerivation::ForwardReachableStep,
        input.file_digest,
        [0; 32],
        |emit| {
            visit_forward_layer_spilled(
                binding,
                output_layer,
                ForwardSource::VerifiedFile {
                    path: input_path,
                    summary: &input,
                },
                input.file_digest,
                None,
                [0; 32],
                output_path,
                workers,
                emit,
            )
        },
    )?;
    cleanup_forward_spill(output_path)?;
    Ok(StepReport {
        disposition: "created",
        input_layer: input.layer,
        output_layer,
        input_field_count: input.field_count,
        output_field_count,
        candidate_pair_count: 0,
        workers,
        file_identity: identity,
    })
}

/// The production forward path emits its final sorted union directly into a
/// domain writer. Tests retain a collecting adapter to compare the spill merge
/// with the independent in-memory implementation on bounded fixtures.
#[cfg(test)]
fn generate_forward_layer_spilled(
    binding: DomainBinding,
    output_layer: u8,
    input: &[u64],
    input_digest: [u8; 32],
    filter: Option<&[u64]>,
    filter_digest: [u8; 32],
    output_path: &Path,
    workers: usize,
) -> Result<Vec<u64>, String> {
    let mut fields = Vec::new();
    visit_forward_layer_spilled(
        binding,
        output_layer,
        ForwardSource::Fields(input),
        input_digest,
        filter,
        filter_digest,
        output_path,
        workers,
        &mut |field| {
            fields.push(field);
            Ok(())
        },
    )?;
    Ok(fields)
}

enum ForwardSource<'a> {
    Fields(&'a [u64]),
    VerifiedFile {
        path: &'a Path,
        summary: &'a DomainSummary,
    },
}

fn visit_forward_layer_spilled(
    binding: DomainBinding,
    output_layer: u8,
    source: ForwardSource<'_>,
    input_digest: [u8; 32],
    filter: Option<&[u64]>,
    filter_digest: [u8; 32],
    output_path: &Path,
    workers: usize,
    emit: &mut dyn FnMut(u64) -> Result<(), String>,
) -> Result<usize, String> {
    let spill_root = prepare_forward_spill(output_path)?;
    let spill_identity: [u8; 32] = Sha256::new()
        .chain_update(b"clearra.legal-forward.spill.v1\0")
        .chain_update(input_digest)
        .chain_update(filter_digest)
        .finalize()
        .into();
    let mut run_paths = Vec::new();
    let mut created_runs = 0_usize;
    let mut reused_runs = 0_usize;
    let source_len = match &source {
        ForwardSource::Fields(fields) => fields.len(),
        ForwardSource::VerifiedFile { summary, .. } => summary.field_count,
    };
    let mut write_chunk = |chunk_index: usize, input_chunk: &[u64]| -> Result<(), String> {
        let start = chunk_index
            .checked_mul(FORWARD_SOURCE_CHUNK_SIZE)
            .ok_or("forward run start overflow")?;
        let end = start
            .checked_add(input_chunk.len())
            .ok_or("forward run end overflow")?;
        let path = pair_run_path(&spill_root, start, end);
        if path.exists() {
            PairRunReader::open(&path, binding, spill_identity, output_layer, start, end)?;
            reused_runs += 1;
        } else {
            let fields = generate_forward_layer(
                binding,
                output_layer,
                input_chunk,
                filter,
                workers.min(input_chunk.len().max(1)),
            )?;
            let marked_fields = fields
                .into_iter()
                .map(|field| (field, PieceKind::I))
                .collect::<Vec<_>>();
            write_pair_run(
                &path,
                binding,
                spill_identity,
                output_layer,
                start,
                end,
                &marked_fields,
            )?;
            created_runs += 1;
        }
        if (created_runs + reused_runs) % 128 == 0 || end == source_len {
            eprintln!(
                "legal_board_forward_run=progress layer={} created={} reused={} end={}",
                output_layer, created_runs, reused_runs, end
            );
        }
        run_paths.push((path, start, end));
        Ok(())
    };
    match source {
        ForwardSource::Fields(input) => {
            for (chunk_index, input_chunk) in input.chunks(FORWARD_SOURCE_CHUNK_SIZE).enumerate() {
                write_chunk(chunk_index, input_chunk)?;
            }
        }
        ForwardSource::VerifiedFile { path, summary } => {
            let mut chunk = Vec::with_capacity(FORWARD_SOURCE_CHUNK_SIZE);
            let mut chunk_index = 0_usize;
            let mut visit = |field| -> Result<(), String> {
                chunk.push(field);
                if chunk.len() == FORWARD_SOURCE_CHUNK_SIZE {
                    write_chunk(chunk_index, &chunk)?;
                    chunk.clear();
                    chunk_index = chunk_index
                        .checked_add(1)
                        .ok_or("forward run index overflow")?;
                }
                Ok(())
            };
            let (observed, _) =
                scan_domain_file(path, binding, Some(summary.layer), Some(&mut visit))?;
            drop(visit);
            if !chunk.is_empty() {
                write_chunk(chunk_index, &chunk)?;
            }
            if observed.file_digest != summary.file_digest
                || observed.field_count != summary.field_count
                || observed.derivation != summary.derivation
                || observed.input_digest != summary.input_digest
                || observed.filter_digest != summary.filter_digest
            {
                return Err("forward source changed during its verified scan".to_owned());
            }
        }
    }
    drop(write_chunk);
    let run_paths = reduce_pair_runs(
        &spill_root,
        run_paths,
        binding,
        spill_identity,
        output_layer,
    )?;
    let mut readers = run_paths
        .iter()
        .map(|(path, start, end)| {
            PairRunReader::open(path, binding, spill_identity, output_layer, *start, *end)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let marker = PieceKind::STANDARD_TETROMINOES
        .iter()
        .position(|piece| *piece == PieceKind::I)
        .ok_or("forward run marker is not standard")? as u8;
    let mut heap = BinaryHeap::new();
    for (run_index, reader) in readers.iter_mut().enumerate() {
        if let Some(pair) = reader.next_pair()? {
            heap.push(Reverse((pair, run_index)));
        }
    }
    let mut count = 0_usize;
    let mut previous = None;
    while let Some(Reverse((pair, run_index))) = heap.pop() {
        if pair.1 != marker {
            return Err("forward run contains a non-marker piece".to_owned());
        }
        if previous != Some(pair.0) {
            emit(pair.0)?;
            previous = Some(pair.0);
            count = count.checked_add(1).ok_or("forward field count overflow")?;
        }
        if let Some(next) = readers[run_index].next_pair()? {
            heap.push(Reverse((next, run_index)));
        }
    }
    Ok(count)
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
    let forward = inspect_domain_file(forward_source_path, binding, None)?;
    if forward.layer >= 10 {
        return Err("legal predecessor source must be below layer ten".to_owned());
    }
    let target_layer = forward.layer + 1;
    let target = read(legal_target_path, binding, Some(target_layer))?;
    if output_path.exists() {
        let existing = inspect_domain_file(output_path, binding, Some(forward.layer))?;
        if existing.derivation != DomainDerivation::LegalPredecessorStep
            || existing.input_digest != forward.file_digest
            || existing.filter_digest != target.file_digest
        {
            return Err(
                "existing legal predecessor layer is not bound to its current inputs".to_owned(),
            );
        }
        cleanup_reverse_spill(output_path)?;
        return Ok(StepReport {
            disposition: "already-complete",
            input_layer: target_layer,
            output_layer: forward.layer,
            input_field_count: target.fields.len(),
            output_field_count: existing.field_count,
            candidate_pair_count: 0,
            workers: 0,
            file_identity: existing.file_identity,
        });
    }
    let available = thread::available_parallelism().map_or(1, usize::from);
    let workers = requested_workers
        .min(available)
        .min(target.fields.len().max(1));
    let forward_bloom = ForwardBloom::from_verified_domain(forward_source_path, binding, &forward)?;
    // Enumerate geometric predecessors of the already-complete legal target,
    // merge their pair runs, intersect the sorted stream with complete F_k in
    // one verified file pass, then exact-ILC-check each surviving source.
    // A bounded no-false-negative Bloom semi-join removes most impossible
    // sources before spill. The sorted F_k cursor remains the exact authority
    // after merge; Bloom admission alone never certifies membership.
    let spill_identity: [u8; 32] = Sha256::new()
        .chain_update(b"clearra.legal-predecessor.spill.v2.bloom4\0")
        .chain_update(target.file_digest)
        .chain_update(forward.file_digest)
        .finalize()
        .into();
    let mut candidate_pair_count = 0_usize;
    let (identity, output_field_count) = write_streamed_domain(
        output_path,
        binding,
        forward.layer,
        DomainDerivation::LegalPredecessorStep,
        forward.file_digest,
        target.file_digest,
        |emit| {
            let (count, pairs) = visit_reverse_layer_spilled(
                binding,
                forward.layer,
                &target.fields,
                spill_identity,
                output_path,
                workers,
                LEGAL_PREDECESSOR_TARGET_CHUNK_SIZE,
                Some(ForwardMembership::SortedDomain {
                    path: forward_source_path,
                    summary: &forward,
                    prefilter: &forward_bloom,
                }),
                emit,
            )?;
            candidate_pair_count = pairs;
            Ok(count)
        },
    )?;
    cleanup_reverse_spill(output_path)?;
    Ok(StepReport {
        disposition: "created",
        input_layer: target_layer,
        output_layer: forward.layer,
        input_field_count: target.fields.len(),
        output_field_count,
        candidate_pair_count,
        workers,
        file_identity: identity,
    })
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
    validate_spill_root_parent(&root, output_path)?;
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
    validate_spill_root_parent(&root, output_path)?;
    fs::remove_dir_all(root).map_err(io_error)
}

fn validate_spill_root_parent(root: &Path, output_path: &Path) -> Result<(), String> {
    let expected_parent = fs::canonicalize(
        output_path
            .parent()
            .ok_or("legal-board spill output has no parent")?,
    )
    .map_err(io_error)?;
    let resolved_root = fs::canonicalize(root).map_err(io_error)?;
    if resolved_root.parent() != Some(expected_parent.as_path()) {
        return Err("legal-board spill root escapes its output parent".to_owned());
    }
    Ok(())
}

fn forward_spill_root(output_path: &Path) -> Result<PathBuf, String> {
    let parent = output_path
        .parent()
        .ok_or("legal-board forward output has no parent")?;
    let output_name = output_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("legal-board forward output name must be UTF-8")?;
    Ok(parent.join(format!(".{output_name}.legal-board-forward-spill-v1")))
}

fn prepare_forward_spill(output_path: &Path) -> Result<PathBuf, String> {
    let root = forward_spill_root(output_path)?;
    if root.exists() {
        let metadata = fs::symlink_metadata(&root).map_err(io_error)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("legal-board forward spill root must be a real directory".to_owned());
        }
    } else {
        fs::create_dir(&root).map_err(io_error)?;
    }
    validate_spill_root_parent(&root, output_path)?;
    Ok(root)
}

fn cleanup_forward_spill(output_path: &Path) -> Result<(), String> {
    let root = forward_spill_root(output_path)?;
    if !root.exists() {
        return Ok(());
    }
    let metadata = fs::symlink_metadata(&root).map_err(io_error)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("legal-board forward spill cleanup rejected non-directory".to_owned());
    }
    validate_spill_root_parent(&root, output_path)?;
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
    if let Err(error) = fs::hard_link(&pending, path) {
        let _ = fs::remove_file(&pending);
        return Err(io_error(error));
    }
    fs::remove_file(&pending).map_err(io_error)
}

fn piece_from_index(index: u8) -> Result<PieceKind, String> {
    PieceKind::STANDARD_TETROMINOES
        .get(usize::from(index))
        .copied()
        .ok_or("legal-board pair run piece index invalid".to_owned())
}

/// Merge one bounded fan-in directly into an immutable run. A merged layer may
/// contain millions of pairs; retaining the entire result before writing it
/// defeats the source-chunk spill boundary.
fn write_merged_pair_run_group(
    output: &Path,
    runs: &[(PathBuf, usize, usize)],
    binding: DomainBinding,
    input_digest: [u8; 32],
    output_layer: u8,
) -> Result<usize, String> {
    if output.exists() {
        return Err("refusing to overwrite a legal-board merged run".to_owned());
    }
    let (start, end) = match (runs.first(), runs.last()) {
        (Some((_, start, _)), Some((_, _, end))) => (*start, *end),
        _ => return Err("legal-board merge group empty".to_owned()),
    };
    let name = output
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("legal-board merged run name must be UTF-8")?;
    let pending = output.with_file_name(format!(".{name}.pending-{}", std::process::id()));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)
        .map_err(io_error)?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, file);
    let result = (|| {
        writer
            .write_all(&[0_u8; PAIR_RUN_HEADER_BYTES])
            .map_err(io_error)?;
        let mut readers = runs
            .iter()
            .map(|(path, run_start, run_end)| {
                PairRunReader::open(
                    path,
                    binding,
                    input_digest,
                    output_layer,
                    *run_start,
                    *run_end,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut heap = BinaryHeap::new();
        for (run_index, reader) in readers.iter_mut().enumerate() {
            if let Some(pair) = reader.next_pair()? {
                heap.push(Reverse((pair, run_index)));
            }
        }
        let mut count = 0_usize;
        let mut payload_digest = Sha256::new();
        let mut last_pair = None;
        while let Some(Reverse((pair, run_index))) = heap.pop() {
            if let Some(next) = readers[run_index].next_pair()? {
                heap.push(Reverse((next, run_index)));
            }
            if last_pair == Some(pair) {
                continue;
            }
            last_pair = Some(pair);
            let mut encoded = [0_u8; PAIR_RUN_RECORD_BYTES];
            encoded[..8].copy_from_slice(&pair.0.to_le_bytes());
            encoded[8] = piece_from_index(pair.1)?.as_ascii() as u8;
            writer.write_all(&encoded).map_err(io_error)?;
            payload_digest.update(&encoded);
            count = count.checked_add(1).ok_or("pair run count overflow")?;
        }
        let mut header = [0_u8; PAIR_RUN_HEADER_BYTES];
        header[..8].copy_from_slice(PAIR_RUN_MAGIC);
        header[8..12].copy_from_slice(&VERSION.to_le_bytes());
        header[12..16].copy_from_slice(&u32::from(output_layer).to_le_bytes());
        header[16..24].copy_from_slice(
            &u64::try_from(start)
                .map_err(|_| "pair run start overflow")?
                .to_le_bytes(),
        );
        header[24..32].copy_from_slice(
            &u64::try_from(end)
                .map_err(|_| "pair run end overflow")?
                .to_le_bytes(),
        );
        header[32..40].copy_from_slice(
            &u64::try_from(count)
                .map_err(|_| "pair run count overflow")?
                .to_le_bytes(),
        );
        header[40..72].copy_from_slice(&binding.identity);
        header[72..104].copy_from_slice(&input_digest);
        header[104..136].copy_from_slice(&payload_digest.finalize());
        writer.seek(SeekFrom::Start(0)).map_err(io_error)?;
        writer.write_all(&header).map_err(io_error)?;
        writer.flush().map_err(io_error)?;
        writer.get_ref().sync_all().map_err(io_error)?;
        Ok::<usize, String>(count)
    })();
    drop(writer);
    let count = match result {
        Ok(count) => count,
        Err(error) => {
            let _ = fs::remove_file(&pending);
            return Err(error);
        }
    };
    if let Err(error) = fs::hard_link(&pending, output) {
        let _ = fs::remove_file(&pending);
        return Err(io_error(error));
    }
    fs::remove_file(&pending).map_err(io_error)?;
    Ok(count)
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
                let count =
                    write_merged_pair_run_group(&path, group, binding, input_digest, output_layer)?;
                eprintln!(
                    "legal_board_pair_merge=created layer={} pass={} start={} end={} pairs={}",
                    output_layer, pass, start, end, count
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
    target_prefilter: &ForwardBloom,
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
    let fields = validate_reverse_sources(binding, input, target_prefilter, candidates, workers)?;
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
    target_prefilter: &ForwardBloom,
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
                let mut workspace = Pc4IlcForwardMembershipWorkspace::default();
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
                            reaches_domain = reaches_any_domain_target(
                                &mut workspace,
                                source,
                                piece,
                                binding.kick_profile,
                                input,
                                target_prefilter,
                            )?;
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

fn reaches_any_domain_target(
    workspace: &mut Pc4IlcForwardMembershipWorkspace,
    source: u64,
    piece: PieceKind,
    kick_profile: KickTableProfileId,
    sorted_target_hashes: &[u64],
    target_prefilter: &ForwardBloom,
) -> Result<bool, String> {
    let mut invalid_target = None;
    let found = workspace
        .any_target(source, piece, kick_profile, |target| {
            match clearra_board64_mask_to_hydra_field_hash_v1(target) {
                Ok(hash) => {
                    target_prefilter.may_contain(hash)
                        && sorted_target_hashes.binary_search(&hash).is_ok()
                }
                Err(error) => {
                    invalid_target = Some(error.reason());
                    true
                }
            }
        })
        .map_err(|error| error.reason().to_owned())?;
    if let Some(reason) = invalid_target {
        return Err(reason.to_owned());
    }
    Ok(found)
}

/// Probabilistic *admission* for the large disk-backed F_k semi-join. Exact
/// membership is still proved by SortedDomainCursor before ILC validation.
struct ForwardBloom {
    words: Vec<u64>,
    bit_mask: u64,
}

impl ForwardBloom {
    fn from_fields(fields: &[u64]) -> Result<Self, String> {
        let mut filter = Self::new(fields.len())?;
        for &field in fields {
            filter.insert(field);
        }
        Ok(filter)
    }

    fn new(expected_fields: usize) -> Result<Self, String> {
        let maximum_bits = FORWARD_BLOOM_MAX_BYTES * 8;
        let wanted_bits = expected_fields.saturating_mul(8).max(64);
        let bit_count = wanted_bits
            .checked_next_power_of_two()
            .unwrap_or(maximum_bits)
            .min(maximum_bits);
        let word_count = bit_count / 64;
        let mut words = Vec::new();
        words
            .try_reserve_exact(word_count)
            .map_err(|_| "forward membership prefilter allocation failed".to_owned())?;
        words.resize(word_count, 0);
        Ok(Self {
            words,
            bit_mask: u64::try_from(bit_count - 1)
                .map_err(|_| "forward membership prefilter width overflow".to_owned())?,
        })
    }

    fn from_verified_domain(
        path: &Path,
        binding: DomainBinding,
        summary: &DomainSummary,
    ) -> Result<Self, String> {
        let mut filter = Self::new(summary.field_count)?;
        visit_verified_domain_fields(path, binding, summary, &mut |field| {
            filter.insert(field);
            Ok(())
        })?;
        eprintln!(
            "legal_board_forward_prefilter=ready layer={} fields={} bytes={}",
            summary.layer,
            summary.field_count,
            filter.words.len() * 8
        );
        Ok(filter)
    }

    fn insert(&mut self, field: u64) {
        for bit in bloom_bit_positions(field, self.bit_mask) {
            self.words[bit >> 6] |= 1_u64 << (bit & 63);
        }
    }

    fn may_contain(&self, field: u64) -> bool {
        bloom_bit_positions(field, self.bit_mask)
            .all(|bit| self.words[bit >> 6] & (1_u64 << (bit & 63)) != 0)
    }
}

fn bloom_bit_positions(field: u64, bit_mask: u64) -> impl Iterator<Item = usize> {
    let first = bloom_mix(field ^ 0x243f_6a88_85a3_08d3);
    let step = bloom_mix(field ^ 0x1319_8a2e_0370_7344) | 1;
    (0..FORWARD_BLOOM_HASHES)
        .map(move |index| ((first.wrapping_add(index.wrapping_mul(step))) & bit_mask) as usize)
}

fn bloom_mix(mut value: u64) -> u64 {
    value ^= value >> 30;
    value = value.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

#[cfg(test)]
mod forward_bloom_tests {
    use super::ForwardBloom;

    #[test]
    fn semi_join_never_rejects_an_inserted_field_even_with_collisions() {
        for count in [0, 1, 64, 10_000] {
            let fields = (0..count)
                .map(|index| (index as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15))
                .collect::<Vec<_>>();
            let filter = ForwardBloom::from_fields(&fields).unwrap();
            for field in fields {
                assert!(filter.may_contain(field));
            }
        }
    }
}

#[derive(Clone, Copy)]
enum ForwardMembership<'a> {
    InMemory(&'a [u64]),
    SortedDomain {
        path: &'a Path,
        summary: &'a DomainSummary,
        prefilter: &'a ForwardBloom,
    },
}

fn visit_reverse_layer_spilled(
    binding: DomainBinding,
    output_layer: u8,
    input: &[u64],
    input_digest: [u8; 32],
    output_path: &Path,
    workers: usize,
    target_chunk_size: usize,
    forward_filter: Option<ForwardMembership<'_>>,
    emit: &mut dyn FnMut(u64) -> Result<(), String>,
) -> Result<(usize, usize), String> {
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
                                let keep = match forward_filter {
                                    None => true,
                                    Some(ForwardMembership::SortedDomain { prefilter, .. }) => {
                                        prefilter.may_contain(source_hash)
                                    }
                                    Some(ForwardMembership::InMemory(allowed)) => {
                                        allowed.binary_search(&source_hash).is_ok()
                                    }
                                };
                                if keep {
                                    candidates.push((source_hash, piece));
                                }
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
    // R_(k+1) is already loaded and verified. A small no-false-negative
    // admission filter avoids cache-missing binary searches for the many
    // geometric locks that are not in this exact target domain. The sorted
    // lookup remains the authority on every possible hit.
    let target_prefilter = ForwardBloom::from_fields(input)?;
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
    let mut disk_membership = match forward_filter {
        Some(ForwardMembership::SortedDomain { path, summary, .. }) => {
            Some(SortedDomainCursor::open(path, binding, summary)?)
        }
        _ => None,
    };
    let mut field_count = 0_usize;
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
        if let Some(cursor) = disk_membership.as_mut() {
            if !cursor.contains(pair.0)? {
                continue;
            }
        }
        candidate_pair_count = candidate_pair_count
            .checked_add(1)
            .ok_or("candidate pair count overflow")?;
        if current_source.is_some_and(|source| source != pair.0) {
            batch.push((current_source.expect("source exists"), current_piece_bits));
            source_count += 1;
            if batch.len() == REVERSE_VALIDATION_BATCH_SIZE {
                let start = source_count - batch.len();
                for field in validate_reverse_checkpoint(
                    &spill_root,
                    binding,
                    input,
                    &target_prefilter,
                    input_digest,
                    output_layer,
                    &batch,
                    workers,
                    start,
                    source_count,
                )? {
                    emit(field)?;
                    field_count = field_count
                        .checked_add(1)
                        .ok_or("reverse field count overflow")?;
                }
                batch.clear();
                eprintln!(
                    "legal_board_validation=progress layer={} sources={} fields={}",
                    output_layer, source_count, field_count
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
        for field in validate_reverse_checkpoint(
            &spill_root,
            binding,
            input,
            &target_prefilter,
            input_digest,
            output_layer,
            &batch,
            workers,
            start,
            source_count,
        )? {
            emit(field)?;
            field_count = field_count
                .checked_add(1)
                .ok_or("reverse field count overflow")?;
        }
    }
    if let Some(cursor) = disk_membership {
        cursor.finish()?;
    }
    eprintln!(
        "legal_board_validation=complete layer={} pairs={} sources={} fields={}",
        output_layer, candidate_pair_count, source_count, field_count
    );
    Ok((field_count, candidate_pair_count))
}

#[cfg(test)]
fn generate_reverse_layer_spilled(
    binding: DomainBinding,
    output_layer: u8,
    input: &[u64],
    input_digest: [u8; 32],
    output_path: &Path,
    workers: usize,
    target_chunk_size: usize,
    forward_filter: Option<&[u64]>,
) -> Result<(Vec<u64>, usize), String> {
    let mut fields = Vec::new();
    let (count, pairs) = visit_reverse_layer_spilled(
        binding,
        output_layer,
        input,
        input_digest,
        output_path,
        workers,
        target_chunk_size,
        forward_filter.map(ForwardMembership::InMemory),
        &mut |field| {
            fields.push(field);
            Ok(())
        },
    )?;
    if count != fields.len() {
        return Err("reverse validation reported the wrong field count".to_owned());
    }
    Ok((fields, pairs))
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
    let target_prefilter = ForwardBloom::from_fields(input)?;

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
                    let mut workspace = Pc4IlcForwardMembershipWorkspace::default();
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
                            let reaches_domain = reaches_any_domain_target(
                                &mut workspace,
                                source,
                                piece,
                                binding.kick_profile,
                                input,
                                &target_prefilter,
                            )?;
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
    let (summary, fields) = scan_domain_file(path, binding, expected_layer, None)?;
    Ok(DomainFile {
        layer: summary.layer,
        fields,
        file_identity: summary.file_identity,
        file_digest: summary.file_digest,
        derivation: summary.derivation,
        input_digest: summary.input_digest,
        filter_digest: summary.filter_digest,
    })
}

pub(crate) fn inspect_domain_file(
    path: &Path,
    binding: DomainBinding,
    expected_layer: Option<u8>,
) -> Result<DomainSummary, String> {
    let mut ignore = |_field| Ok(());
    let (summary, _) = scan_domain_file(path, binding, expected_layer, Some(&mut ignore))?;
    Ok(summary)
}

pub(crate) fn visit_verified_domain_fields(
    path: &Path,
    binding: DomainBinding,
    expected: &DomainSummary,
    visitor: &mut dyn FnMut(u64) -> Result<(), String>,
) -> Result<(), String> {
    let (observed, _) = scan_domain_file(path, binding, Some(expected.layer), Some(visitor))?;
    if observed.file_digest != expected.file_digest
        || observed.field_count != expected.field_count
        || observed.derivation != expected.derivation
        || observed.input_digest != expected.input_digest
        || observed.filter_digest != expected.filter_digest
    {
        return Err("domain layer changed during verified visitation".to_owned());
    }
    Ok(())
}

/// Verify the whole immutable layer while optionally visiting fields instead
/// of allocating a second full domain vector. The streaming caller compares
/// the observed digest again before publishing any derived output.
fn scan_domain_file(
    path: &Path,
    binding: DomainBinding,
    expected_layer: Option<u8>,
    mut visitor: Option<&mut dyn FnMut(u64) -> Result<(), String>>,
) -> Result<(DomainSummary, Vec<u64>), String> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("domain file symlink or non-file rejected".to_owned());
    }
    if metadata.len() < HEADER_BYTES as u64 {
        return Err("domain file header invalid".to_owned());
    }
    let mut reader = BufReader::with_capacity(64 * 1024, File::open(path).map_err(io_error)?);
    let mut header = [0_u8; HEADER_BYTES];
    reader.read_exact(&mut header).map_err(io_error)?;
    if header.get(..8) != Some(MAGIC.as_slice()) {
        return Err("domain file header invalid".to_owned());
    }
    if read_u32(&header[8..12])? != VERSION {
        return Err("domain file version invalid".to_owned());
    }
    let layer_u32 = read_u32(&header[12..16])?;
    let layer = u8::try_from(layer_u32).map_err(|_| "domain layer overflow")?;
    if layer > 10 || expected_layer.is_some_and(|expected| expected != layer) {
        return Err("domain file layer mismatch".to_owned());
    }
    let count =
        usize::try_from(read_u64(&header[16..24])?).map_err(|_| "domain field count overflow")?;
    let expected_len = HEADER_BYTES
        .checked_add(count.checked_mul(8).ok_or("domain file length overflow")?)
        .ok_or("domain file length overflow")?;
    if header.get(24..56) != Some(binding.identity.as_slice())
        || metadata.len()
            != u64::try_from(expected_len).map_err(|_| "domain file length overflow")?
    {
        return Err("domain file binding or length mismatch".to_owned());
    }
    let derivation = DomainDerivation::parse(header[56])?;
    if header[57..64].iter().any(|byte| *byte != 0) {
        return Err("domain file reserved header bytes are nonzero".to_owned());
    }
    let input_digest: [u8; 32] = header[64..96]
        .try_into()
        .map_err(|_| "domain input digest width mismatch")?;
    let filter_digest: [u8; 32] = header[96..128]
        .try_into()
        .map_err(|_| "domain filter digest width mismatch")?;
    let mut digest = Sha256::new();
    digest.update(header);
    let mut fields = Vec::new();
    if visitor.is_none() {
        fields
            .try_reserve_exact(count)
            .map_err(|_| "domain file allocation failed")?;
    }
    let expected_cells = u32::from(layer) * 4;
    let mut prior = None;
    let mut batch = vec![0_u8; 1024 * 1024];
    let mut remaining = count;
    while remaining > 0 {
        let batch_fields = remaining.min(batch.len() / 8);
        let batch_bytes = batch_fields * 8;
        reader
            .read_exact(&mut batch[..batch_bytes])
            .map_err(io_error)?;
        digest.update(&batch[..batch_bytes]);
        for encoded in batch[..batch_bytes].chunks_exact(8) {
            let field = read_u64(encoded)?;
            if field & !FIELD_MASK != 0 || field.count_ones() != expected_cells {
                return Err("domain field outside its exact area layer".to_owned());
            }
            if prior.is_some_and(|value| value >= field) {
                return Err("domain fields must be strictly sorted and unique".to_owned());
            }
            if let Some(visit) = visitor.as_mut() {
                visit(field)?;
            } else {
                fields.push(field);
            }
            prior = Some(field);
        }
        remaining -= batch_fields;
    }
    let mut trailing = [0_u8; 1];
    if reader.read(&mut trailing).map_err(io_error)? != 0 {
        return Err("domain file length changed while reading".to_owned());
    }
    let file_digest: [u8; 32] = digest.finalize().into();
    Ok((
        DomainSummary {
            layer,
            field_count: count,
            file_identity: format!("sha256:{}", hex(&file_digest)),
            file_digest,
            derivation,
            input_digest,
            filter_digest,
        },
        fields,
    ))
}

/// Exact, monotonically queried membership for a validated sorted domain.
/// Geometric predecessor pairs are globally merged before querying this
/// cursor, so one sequential pass over F_k replaces the whole F_k vector.
struct SortedDomainCursor {
    reader: BufReader<File>,
    digest: Sha256,
    expected_digest: [u8; 32],
    remaining: usize,
    layer: u8,
    prior_field: Option<u64>,
    current: Option<u64>,
    prior_query: Option<u64>,
}

impl SortedDomainCursor {
    fn open(path: &Path, binding: DomainBinding, summary: &DomainSummary) -> Result<Self, String> {
        let metadata = fs::symlink_metadata(path).map_err(io_error)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("domain membership source must be a real file".to_owned());
        }
        let expected_len = HEADER_BYTES
            .checked_add(
                summary
                    .field_count
                    .checked_mul(8)
                    .ok_or("domain membership length overflow")?,
            )
            .ok_or("domain membership length overflow")?;
        if metadata.len()
            != u64::try_from(expected_len).map_err(|_| "domain membership length overflow")?
        {
            return Err("domain membership length mismatch".to_owned());
        }
        let mut reader = BufReader::with_capacity(64 * 1024, File::open(path).map_err(io_error)?);
        let mut header = [0_u8; HEADER_BYTES];
        reader.read_exact(&mut header).map_err(io_error)?;
        if header[..8] != *MAGIC
            || read_u32(&header[8..12])? != VERSION
            || read_u32(&header[12..16])? != u32::from(summary.layer)
            || read_u64(&header[16..24])?
                != u64::try_from(summary.field_count)
                    .map_err(|_| "domain membership count overflow")?
            || header[24..56] != binding.identity
            || header[56] != summary.derivation as u8
            || header[57..64].iter().any(|byte| *byte != 0)
            || header[64..96] != summary.input_digest
            || header[96..128] != summary.filter_digest
        {
            return Err("domain membership header changed".to_owned());
        }
        let mut digest = Sha256::new();
        digest.update(header);
        Ok(Self {
            reader,
            digest,
            expected_digest: summary.file_digest,
            remaining: summary.field_count,
            layer: summary.layer,
            prior_field: None,
            current: None,
            prior_query: None,
        })
    }

    fn next_field(&mut self) -> Result<Option<u64>, String> {
        if self.remaining == 0 {
            return Ok(None);
        }
        let mut encoded = [0_u8; 8];
        self.reader.read_exact(&mut encoded).map_err(io_error)?;
        self.digest.update(encoded);
        let field = u64::from_le_bytes(encoded);
        if field & !FIELD_MASK != 0 || field.count_ones() != u32::from(self.layer) * 4 {
            return Err("domain membership field outside its area layer".to_owned());
        }
        if self.prior_field.is_some_and(|prior| prior >= field) {
            return Err("domain membership fields are not strictly sorted".to_owned());
        }
        self.prior_field = Some(field);
        self.remaining -= 1;
        Ok(Some(field))
    }

    fn contains(&mut self, wanted: u64) -> Result<bool, String> {
        if self.prior_query.is_some_and(|prior| prior > wanted) {
            return Err("domain membership queries must be sorted".to_owned());
        }
        self.prior_query = Some(wanted);
        while self.current.is_none_or(|field| field < wanted) {
            self.current = self.next_field()?;
            if self.current.is_none() {
                return Ok(false);
            }
        }
        Ok(self.current == Some(wanted))
    }

    fn finish(mut self) -> Result<(), String> {
        while self.next_field()?.is_some() {}
        let mut trailing = [0_u8; 1];
        if self.reader.read(&mut trailing).map_err(io_error)? != 0 {
            return Err("domain membership length changed during lookup".to_owned());
        }
        let observed: [u8; 32] = self.digest.finalize().into();
        if observed != self.expected_digest {
            return Err("domain membership digest changed during lookup".to_owned());
        }
        Ok(())
    }
}

/// Validate a completed legal layer against one full sorted domain without
/// retaining that full domain beside the legal layer being encoded.
#[cfg(test)]
pub(crate) fn verify_subset_of_file(
    legal_fields: &[u64],
    complete_path: &Path,
    binding: DomainBinding,
    layer: u8,
) -> Result<(String, [u8; 32]), String> {
    validate_fields(layer, legal_fields)?;
    let complete = inspect_domain_file(complete_path, binding, Some(layer))?;
    let mut cursor = SortedDomainCursor::open(complete_path, binding, &complete)?;
    for &field in legal_fields {
        if !cursor.contains(field)? {
            return Err("legal-board layer is not a subset of its complete domain".to_owned());
        }
    }
    cursor.finish()?;
    Ok((complete.file_identity, complete.file_digest))
}

pub(crate) fn verify_subset_files(
    legal_path: &Path,
    complete_path: &Path,
    binding: DomainBinding,
    layer: u8,
) -> Result<(DomainSummary, DomainSummary), String> {
    let complete = inspect_domain_file(complete_path, binding, Some(layer))?;
    let mut cursor = SortedDomainCursor::open(complete_path, binding, &complete)?;
    let mut check = |field| -> Result<(), String> {
        if cursor.contains(field)? {
            Ok(())
        } else {
            Err("legal-board layer is not a subset of its complete domain".to_owned())
        }
    };
    let (legal, _) = scan_domain_file(legal_path, binding, Some(layer), Some(&mut check))?;
    drop(check);
    cursor.finish()?;
    Ok((legal, complete))
}

/// Publish a final domain union without collecting it in RAM. The count is
/// only known after the merge, so fill its header in the pending file, then
/// hash the exact on-disk bytes before immutable publication.
fn write_streamed_domain<F>(
    path: &Path,
    binding: DomainBinding,
    layer: u8,
    derivation: DomainDerivation,
    input_digest: [u8; 32],
    filter_digest: [u8; 32],
    mut visit: F,
) -> Result<(String, usize), String>
where
    F: FnMut(&mut dyn FnMut(u64) -> Result<(), String>) -> Result<usize, String>,
{
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
        writer.write_all(&[0_u8; HEADER_BYTES]).map_err(io_error)?;
        let mut count = 0_usize;
        let mut prior = None;
        let produced = {
            let mut emit = |field: u64| -> Result<(), String> {
                if field & !FIELD_MASK != 0 || field.count_ones() != u32::from(layer) * 4 {
                    return Err("domain field outside its exact area layer".to_owned());
                }
                if prior.is_some_and(|value| value >= field) {
                    return Err("domain fields must be strictly sorted and unique".to_owned());
                }
                writer.write_all(&field.to_le_bytes()).map_err(io_error)?;
                prior = Some(field);
                count = count.checked_add(1).ok_or("domain field count overflow")?;
                Ok(())
            };
            visit(&mut emit)?
        };
        if produced != count {
            return Err("streamed domain merge reported the wrong field count".to_owned());
        }
        let mut header = Vec::with_capacity(HEADER_BYTES);
        header.extend_from_slice(MAGIC);
        header.extend_from_slice(&VERSION.to_le_bytes());
        header.extend_from_slice(&u32::from(layer).to_le_bytes());
        header.extend_from_slice(
            &u64::try_from(count)
                .map_err(|_| "domain field count overflow")?
                .to_le_bytes(),
        );
        header.extend_from_slice(&binding.identity);
        header.push(derivation as u8);
        header.extend_from_slice(&[0; 7]);
        header.extend_from_slice(&input_digest);
        header.extend_from_slice(&filter_digest);
        if header.len() != HEADER_BYTES {
            return Err("domain header width mismatch".to_owned());
        }
        writer.seek(SeekFrom::Start(0)).map_err(io_error)?;
        writer.write_all(&header).map_err(io_error)?;
        writer.flush().map_err(io_error)?;
        writer.get_ref().sync_all().map_err(io_error)?;
        Ok::<usize, String>(count)
    })();
    drop(writer);
    let count = match result {
        Ok(count) => count,
        Err(error) => {
            let _ = fs::remove_file(&pending);
            return Err(error);
        }
    };
    let result = (|| {
        let mut file = File::open(&pending).map_err(io_error)?;
        let expected_len = HEADER_BYTES
            .checked_add(count.checked_mul(8).ok_or("domain file length overflow")?)
            .ok_or("domain file length overflow")?;
        if file.metadata().map_err(io_error)?.len()
            != u64::try_from(expected_len).map_err(|_| "domain file length overflow")?
        {
            return Err("domain file length changed before publication".to_owned());
        }
        let mut digest = Sha256::new();
        let mut batch = [0_u8; 64 * 1024];
        loop {
            let bytes = file.read(&mut batch).map_err(io_error)?;
            if bytes == 0 {
                break;
            }
            digest.update(&batch[..bytes]);
        }
        drop(file);
        fs::hard_link(&pending, path).map_err(io_error)?;
        Ok::<String, String>(format!("sha256:{}", hex(&digest.finalize())))
    })();
    let cleanup = fs::remove_file(&pending).map_err(io_error);
    let identity = result?;
    cleanup?;
    Ok((identity, count))
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
        fs::hard_link(&pending, path).map_err(io_error)?;
        fs::remove_file(&pending).map_err(io_error)?;
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

    #[test]
    fn reverse_domain_membership_matches_complete_forward_enumeration() {
        let mut workspace = Pc4IlcForwardMembershipWorkspace::default();
        for profile in [
            KickTableProfileId::Srs90,
            KickTableProfileId::SrsPlus,
            KickTableProfileId::SrsX,
            KickTableProfileId::Jstris180,
            KickTableProfileId::NoKick,
        ] {
            for source in [0, (0b1111111110_u64) << 20] {
                for piece in [PieceKind::J, PieceKind::L, PieceKind::T] {
                    let complete = enumerate_pc4_ilc_target_fields(source, piece, profile).unwrap();
                    let mut hashes = complete
                        .into_iter()
                        .map(clearra_board64_mask_to_hydra_field_hash_v1)
                        .collect::<Result<Vec<_>, _>>()
                        .unwrap();
                    hashes.sort_unstable();
                    for selected in [hashes.first(), hashes.last()].into_iter().flatten() {
                        let target_prefilter = ForwardBloom::from_fields(&[*selected]).unwrap();
                        assert!(reaches_any_domain_target(
                            &mut workspace,
                            source,
                            piece,
                            profile,
                            &[*selected],
                            &target_prefilter,
                        )
                        .unwrap());
                    }
                    let absent_prefilter = ForwardBloom::from_fields(&[u64::MAX]).unwrap();
                    assert!(!reaches_any_domain_target(
                        &mut workspace,
                        source,
                        piece,
                        profile,
                        &[u64::MAX],
                        &absent_prefilter,
                    )
                    .unwrap());
                }
            }
        }
    }

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
        let summary = inspect_domain_file(&path, binding(), Some(1)).unwrap();
        assert_eq!(summary.field_count, fields.len());
        assert_eq!(summary.file_digest, observed.file_digest);
        assert_eq!(observed.fields, fields);
        let expected_digest: [u8; 32] = Sha256::digest(fs::read(&path).unwrap()).into();
        assert_eq!(observed.file_digest, expected_digest);
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
        OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(&[0])
            .unwrap();
        assert!(read(&path, binding(), Some(1)).is_err());
        assert!(inspect_domain_file(&path, binding(), Some(1)).is_err());
        fs::remove_file(path).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn sorted_domain_cursor_is_exact_and_rejects_a_changed_file() {
        let root = test_root("sorted-domain-cursor");
        fs::create_dir(&root).unwrap();
        let path = root.join("forward.bin");
        write(
            &path,
            binding(),
            1,
            &[0b1111, 0b1111_0000, 0b1111_0000_0000],
            DomainDerivation::ForwardReachableStep,
            [1; 32],
            [0; 32],
        )
        .unwrap();
        let summary = inspect_domain_file(&path, binding(), Some(1)).unwrap();
        let mut cursor = SortedDomainCursor::open(&path, binding(), &summary).unwrap();
        for (field, expected) in [
            (0, false),
            (0b1111, true),
            (0b1_0000, false),
            (0b1111_0000, true),
            (0b1_0000_0000, false),
            (0b1111_0000_0000, true),
        ] {
            assert_eq!(cursor.contains(field).unwrap(), expected);
        }
        cursor.finish().unwrap();
        let mut cursor = SortedDomainCursor::open(&path, binding(), &summary).unwrap();
        assert!(cursor.contains(0b1111_0000).unwrap());
        assert!(cursor.contains(0b1111).is_err());
        drop(cursor);
        fs::remove_file(&path).unwrap();
        write(
            &path,
            binding(),
            1,
            &[0b1111, 0b1111_0000, 0b1111_0000_0000_0000],
            DomainDerivation::ForwardReachableStep,
            [1; 32],
            [0; 32],
        )
        .unwrap();
        let cursor = SortedDomainCursor::open(&path, binding(), &summary).unwrap();
        assert!(cursor.finish().is_err());
        fs::remove_file(path).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn complete_domain_subset_check_streams_and_rejects_missing_fields() {
        let root = test_root("complete-domain-subset");
        fs::create_dir(&root).unwrap();
        let path = root.join("complete.bin");
        let legal_path = root.join("legal.bin");
        write(
            &path,
            binding(),
            1,
            &[0b1111, 0b1111_0000, 0b1111_0000_0000],
            DomainDerivation::ForwardReachableStep,
            [1; 32],
            [0; 32],
        )
        .unwrap();
        let (identity, digest) =
            verify_subset_of_file(&[0b1111, 0b1111_0000_0000], &path, binding(), 1).unwrap();
        assert_eq!(identity, format!("sha256:{}", hex(&digest)));
        assert!(verify_subset_of_file(&[0b1111_0000_0000_0000], &path, binding(), 1).is_err());
        write(
            &legal_path,
            binding(),
            1,
            &[0b1111, 0b1111_0000_0000],
            DomainDerivation::LegalPredecessorStep,
            [2; 32],
            [3; 32],
        )
        .unwrap();
        let (legal, complete) = verify_subset_files(&legal_path, &path, binding(), 1).unwrap();
        assert_eq!(legal.field_count, 2);
        assert_eq!(complete.file_digest, digest);
        fs::remove_file(&legal_path).unwrap();
        write(
            &legal_path,
            binding(),
            1,
            &[0b1111_0000_0000_0000],
            DomainDerivation::LegalPredecessorStep,
            [2; 32],
            [3; 32],
        )
        .unwrap();
        assert!(verify_subset_files(&legal_path, &path, binding(), 1).is_err());
        fs::remove_file(legal_path).unwrap();
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
        let expected_nine = root.join("expected-09.bin");

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

        write(
            &expected_nine,
            binding,
            9,
            &selected,
            DomainDerivation::LegalPredecessorStep,
            observed.input_digest,
            observed.filter_digest,
        )
        .unwrap();
        assert_eq!(
            fs::read(&legal_nine).unwrap(),
            fs::read(&expected_nine).unwrap()
        );

        for path in [
            forward_ten,
            legal_ten,
            forward_nine,
            legal_nine,
            expected_nine,
        ] {
            fs::remove_file(path).unwrap();
        }
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn disk_prefilter_preserves_exact_predecessors_across_target_chunks() {
        let binding = DomainBinding::legal_board(KickTableProfileId::SrsPlus).unwrap();
        let root = test_root("disk-prefilter-chunks");
        fs::create_dir(&root).unwrap();
        let (reverse_nine, _) =
            generate_reverse_layer_bounded(binding, 9, &[FIELD_MASK], 2, 1).unwrap();
        let targets = &reverse_nine[..5];
        let (reverse_eight, _) = generate_reverse_layer_bounded(binding, 8, targets, 2, 2).unwrap();
        let selected = reverse_eight.iter().step_by(3).copied().collect::<Vec<_>>();
        let forward_path = root.join("forward-08.bin");
        let target_path = root.join("legal-09.bin");
        let output_path = root.join("legal-08.bin");
        write(
            &forward_path,
            binding,
            8,
            &selected,
            DomainDerivation::ForwardReachableStep,
            [1; 32],
            [0; 32],
        )
        .unwrap();
        write(
            &target_path,
            binding,
            9,
            targets,
            DomainDerivation::LegalPredecessorStep,
            [2; 32],
            [3; 32],
        )
        .unwrap();
        legal_predecessor_step(binding, &forward_path, &target_path, &output_path, 2).unwrap();
        assert_eq!(
            read(&output_path, binding, Some(8)).unwrap().fields,
            selected
        );
        fs::remove_file(output_path).unwrap();
        fs::remove_file(target_path).unwrap();
        fs::remove_file(forward_path).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn streamed_reverse_step_matches_direct_domain_bytes() {
        let binding = DomainBinding::legal_board(KickTableProfileId::SrsPlus).unwrap();
        let root = test_root("streamed-reverse-domain");
        fs::create_dir(&root).unwrap();
        let source = root.join("source.bin");
        let expected = root.join("expected.bin");
        let observed = root.join("observed.bin");
        seed(binding, DomainDirection::Reverse, &source).unwrap();
        let source_digest = read(&source, binding, Some(10)).unwrap().file_digest;
        let (fields, _) = generate_reverse_layer_bounded(binding, 9, &[FIELD_MASK], 2, 1).unwrap();
        write(
            &expected,
            binding,
            9,
            &fields,
            DomainDerivation::ReverseStep,
            source_digest,
            [0; 32],
        )
        .unwrap();
        let report = step(
            binding,
            DomainDirection::Reverse,
            &source,
            None,
            &observed,
            2,
        )
        .unwrap();
        assert_eq!(report.output_field_count, fields.len());
        assert_eq!(fs::read(&observed).unwrap(), fs::read(&expected).unwrap());
        for path in [source, expected, observed] {
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
    fn streamed_pair_merge_preserves_sorted_unique_records_and_digest() {
        let binding = DomainBinding::legal_board(KickTableProfileId::SrsPlus).unwrap();
        let root = test_root("streamed-pair-merge");
        fs::create_dir(&root).unwrap();
        let first = pair_run_path(&root, 0, 2);
        let second = pair_run_path(&root, 2, 4);
        let output = root.join("merged-00-0000000000-0000000004.bin");
        let digest = [9; 32];
        write_pair_run(
            &first,
            binding,
            digest,
            2,
            0,
            2,
            &[(1, PieceKind::I), (2, PieceKind::T), (4, PieceKind::O)],
        )
        .unwrap();
        write_pair_run(
            &second,
            binding,
            digest,
            2,
            2,
            4,
            &[(2, PieceKind::T), (3, PieceKind::O), (4, PieceKind::O)],
        )
        .unwrap();
        let count = write_merged_pair_run_group(
            &output,
            &[(first.clone(), 0, 2), (second.clone(), 2, 4)],
            binding,
            digest,
            2,
        )
        .unwrap();
        assert_eq!(count, 4);
        let mut reader = PairRunReader::open(&output, binding, digest, 2, 0, 4).unwrap();
        let mut observed = Vec::new();
        while let Some(pair) = reader.next_pair().unwrap() {
            observed.push(pair);
        }
        assert_eq!(observed, vec![(1, 0), (2, 2), (3, 1), (4, 1)]);
        assert!(write_merged_pair_run_group(
            &output,
            &[(first.clone(), 0, 2), (second.clone(), 2, 4)],
            binding,
            digest,
            2,
        )
        .is_err());
        for path in [first, second, output] {
            fs::remove_file(path).unwrap();
        }
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn spill_cleanup_requires_the_exact_output_parent() {
        let root = test_root("spill-parent");
        let nested = root.join("nested");
        let child = nested.join("child");
        fs::create_dir_all(&child).unwrap();
        assert!(validate_spill_root_parent(&nested, &root.join("layer.bin")).is_ok());
        assert!(validate_spill_root_parent(&child, &root.join("layer.bin")).is_err());
        fs::remove_dir(&child).unwrap();
        fs::remove_dir(&nested).unwrap();
        fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn spilled_forward_domain_matches_direct_and_filtered_layers() {
        let binding = DomainBinding::legal_board(KickTableProfileId::Jstris180).unwrap();
        let first_layer = generate_forward_layer(binding, 1, &[0], None, 1).unwrap();
        let sources = &first_layer[..first_layer.len().min(6)];
        assert!(sources.len() > FORWARD_SOURCE_CHUNK_SIZE);
        let expected = generate_forward_layer(binding, 2, sources, None, 2).unwrap();
        let root = test_root("spilled-forward");
        fs::create_dir(&root).unwrap();
        let output = root.join("forward2.bin");
        let observed =
            generate_forward_layer_spilled(binding, 2, sources, [3; 32], None, [0; 32], &output, 2)
                .unwrap();
        let resumed =
            generate_forward_layer_spilled(binding, 2, sources, [3; 32], None, [0; 32], &output, 2)
                .unwrap();
        assert_eq!(observed, expected);
        assert_eq!(resumed, expected);
        cleanup_forward_spill(&output).unwrap();

        let filter = expected.iter().step_by(4).copied().collect::<Vec<_>>();
        let filtered_output = root.join("filtered2.bin");
        let filtered = generate_forward_layer_spilled(
            binding,
            2,
            sources,
            [3; 32],
            Some(&filter),
            [4; 32],
            &filtered_output,
            2,
        )
        .unwrap();
        assert_eq!(filtered, filter);
        cleanup_forward_spill(&filtered_output).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn streamed_forward_step_matches_direct_domain_bytes_and_resumes() {
        let binding = DomainBinding::legal_board(KickTableProfileId::Jstris180).unwrap();
        let root = test_root("streamed-forward-domain");
        fs::create_dir(&root).unwrap();
        let source = root.join("source.bin");
        let expected = root.join("expected.bin");
        let observed = root.join("observed.bin");
        write(
            &source,
            binding,
            0,
            &[0],
            DomainDerivation::ForwardSeed,
            [0; 32],
            [0; 32],
        )
        .unwrap();
        let source_digest = read(&source, binding, Some(0)).unwrap().file_digest;
        let fields = generate_forward_layer(binding, 1, &[0], None, 1).unwrap();
        write(
            &expected,
            binding,
            1,
            &fields,
            DomainDerivation::ForwardReachableStep,
            source_digest,
            [0; 32],
        )
        .unwrap();
        let report = step(
            binding,
            DomainDirection::Forward,
            &source,
            None,
            &observed,
            2,
        )
        .unwrap();
        assert_eq!(report.output_field_count, fields.len());
        assert_eq!(fs::read(&observed).unwrap(), fs::read(&expected).unwrap());
        let resumed = step(
            binding,
            DomainDirection::Forward,
            &source,
            None,
            &observed,
            2,
        )
        .unwrap();
        assert_eq!(resumed.disposition, "already-complete");
        assert_eq!(resumed.file_identity, report.file_identity);
        for path in [source, expected, observed] {
            fs::remove_file(path).unwrap();
        }
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn streamed_forward_writer_does_not_publish_invalid_order() {
        let root = test_root("streamed-forward-invalid-order");
        fs::create_dir(&root).unwrap();
        let output = root.join("forward.bin");
        let result = write_streamed_domain(
            &output,
            binding(),
            1,
            DomainDerivation::ForwardReachableStep,
            [1; 32],
            [0; 32],
            |emit| {
                emit(0b1111_0000)?;
                emit(0b1111)?;
                Ok(2)
            },
        );
        assert!(result.is_err());
        assert!(!output.exists());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn streamed_filtered_forward_step_matches_direct_domain_bytes() {
        let binding = DomainBinding::legal_board(KickTableProfileId::Jstris180).unwrap();
        let root = test_root("streamed-forward-filtered");
        fs::create_dir(&root).unwrap();
        let source = root.join("source.bin");
        let filter = root.join("filter.bin");
        let expected = root.join("expected.bin");
        let observed = root.join("observed.bin");
        let first_layer = generate_forward_layer(binding, 1, &[0], None, 1).unwrap();
        let sources = &first_layer[..first_layer.len().min(2)];
        let second_layer = generate_forward_layer(binding, 2, sources, None, 2).unwrap();
        let selected = second_layer.iter().step_by(3).copied().collect::<Vec<_>>();
        write(
            &source,
            binding,
            1,
            sources,
            DomainDerivation::ForwardReachableStep,
            [1; 32],
            [0; 32],
        )
        .unwrap();
        write(
            &filter,
            binding,
            2,
            &selected,
            DomainDerivation::ReverseStep,
            [2; 32],
            [0; 32],
        )
        .unwrap();
        let source_digest = read(&source, binding, Some(1)).unwrap().file_digest;
        let filter_digest = read(&filter, binding, Some(2)).unwrap().file_digest;
        write(
            &expected,
            binding,
            2,
            &selected,
            DomainDerivation::ForwardStep,
            source_digest,
            filter_digest,
        )
        .unwrap();
        let report = step(
            binding,
            DomainDirection::Forward,
            &source,
            Some(&filter),
            &observed,
            2,
        )
        .unwrap();
        assert_eq!(report.output_field_count, selected.len());
        assert_eq!(fs::read(&observed).unwrap(), fs::read(&expected).unwrap());
        for path in [source, filter, expected, observed] {
            fs::remove_file(path).unwrap();
        }
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn streamed_forward_step_reads_multiple_verified_source_chunks() {
        let binding = DomainBinding::legal_board(KickTableProfileId::Jstris180).unwrap();
        let root = test_root("streamed-forward-source-chunks");
        fs::create_dir(&root).unwrap();
        let source = root.join("source.bin");
        let expected = root.join("expected.bin");
        let observed = root.join("observed.bin");
        let first_layer = generate_forward_layer(binding, 1, &[0], None, 1).unwrap();
        let sources = &first_layer[..first_layer.len().min(6)];
        assert!(sources.len() > FORWARD_SOURCE_CHUNK_SIZE);
        write(
            &source,
            binding,
            1,
            sources,
            DomainDerivation::ForwardReachableStep,
            [1; 32],
            [0; 32],
        )
        .unwrap();
        let source_digest = inspect_domain_file(&source, binding, Some(1))
            .unwrap()
            .file_digest;
        let fields = generate_forward_layer(binding, 2, sources, None, 2).unwrap();
        write(
            &expected,
            binding,
            2,
            &fields,
            DomainDerivation::ForwardReachableStep,
            source_digest,
            [0; 32],
        )
        .unwrap();
        let report = step(
            binding,
            DomainDirection::Forward,
            &source,
            None,
            &observed,
            2,
        )
        .unwrap();
        assert_eq!(report.input_field_count, sources.len());
        assert_eq!(report.output_field_count, fields.len());
        assert_eq!(fs::read(&observed).unwrap(), fs::read(&expected).unwrap());
        for path in [source, expected, observed] {
            fs::remove_file(path).unwrap();
        }
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn streamed_forward_step_rejects_a_changed_source_before_publication() {
        let binding = DomainBinding::legal_board(KickTableProfileId::Jstris180).unwrap();
        let root = test_root("streamed-forward-changed-source");
        fs::create_dir(&root).unwrap();
        let source = root.join("source.bin");
        let output = root.join("output.bin");
        write(
            &source,
            binding,
            1,
            &[0b1111],
            DomainDerivation::ForwardReachableStep,
            [1; 32],
            [0; 32],
        )
        .unwrap();
        let original = inspect_domain_file(&source, binding, Some(1)).unwrap();
        fs::remove_file(&source).unwrap();
        write(
            &source,
            binding,
            1,
            &[0b1111_0000],
            DomainDerivation::ForwardReachableStep,
            [1; 32],
            [0; 32],
        )
        .unwrap();
        let result = write_streamed_domain(
            &output,
            binding,
            2,
            DomainDerivation::ForwardReachableStep,
            original.file_digest,
            [0; 32],
            |emit| {
                visit_forward_layer_spilled(
                    binding,
                    2,
                    ForwardSource::VerifiedFile {
                        path: &source,
                        summary: &original,
                    },
                    original.file_digest,
                    None,
                    [0; 32],
                    &output,
                    1,
                    emit,
                )
            },
        );
        assert!(result.is_err());
        assert!(!output.exists());
        cleanup_forward_spill(&output).unwrap();
        fs::remove_file(source).unwrap();
        fs::remove_dir(root).unwrap();
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
            generate_reverse_layer_spilled(binding, 8, input, [9; 32], &output, 3, 1, None)
                .unwrap();
        let (resumed, _) =
            generate_reverse_layer_spilled(binding, 8, input, [9; 32], &output, 3, 1, None)
                .unwrap();

        assert_eq!(observed, expected);
        assert_eq!(resumed, expected);
        cleanup_reverse_spill(&output).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn filtered_spilled_predecessors_match_exact_intersection_across_chunk_boundaries() {
        let binding = DomainBinding::legal_board(KickTableProfileId::SrsPlus).unwrap();
        let (layer_nine, _) =
            generate_reverse_layer_bounded(binding, 9, &[FIELD_MASK], 2, 1).unwrap();
        let targets = &layer_nine[..5];
        let (all_predecessors, _) =
            generate_reverse_layer_bounded(binding, 8, targets, 2, 2).unwrap();
        let forward = all_predecessors
            .iter()
            .step_by(3)
            .copied()
            .collect::<Vec<_>>();
        let root = test_root("filtered-spill");
        fs::create_dir(&root).unwrap();
        let output = root.join("legal8.bin");

        let (observed, _) = generate_reverse_layer_spilled(
            binding,
            8,
            targets,
            [7; 32],
            &output,
            3,
            1,
            Some(&forward),
        )
        .unwrap();
        let expected = forward
            .iter()
            .copied()
            .filter(|&source_hash| {
                let source = hydra_field_hash_v1_to_clearra_board64_mask(source_hash).unwrap();
                PieceKind::STANDARD_TETROMINOES
                    .iter()
                    .copied()
                    .any(|piece| {
                        enumerate_pc4_ilc_target_fields(source, piece, binding.kick_profile)
                            .unwrap()
                            .into_iter()
                            .any(|target| {
                                let hash =
                                    clearra_board64_mask_to_hydra_field_hash_v1(target).unwrap();
                                targets.binary_search(&hash).is_ok()
                            })
                    })
            })
            .collect::<Vec<_>>();
        assert_eq!(observed, expected);

        cleanup_reverse_spill(&output).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
