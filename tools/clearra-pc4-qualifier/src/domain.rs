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
    collections::{BTreeSet, BinaryHeap},
    fs::{self, OpenOptions},
    io::{BufWriter, Write},
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
    thread,
};

const MAGIC: &[u8; 8] = b"PC4DOM02";
const VERSION: u32 = 2;
const HEADER_BYTES: usize = 128;
const FIELD_MASK: u64 = (1_u64 << 40) - 1;
const MAX_WORKERS: usize = 64;

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
}

impl DomainDerivation {
    fn parse(value: u8) -> Result<Self, String> {
        match value {
            1 => Ok(Self::ReverseSeed),
            2 => Ok(Self::ForwardSeed),
            3 => Ok(Self::ReverseStep),
            4 => Ok(Self::ForwardStep),
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
        (DomainDirection::Forward, None) => {
            return Err("forward domain step requires the reverse layer as --filter".to_owned())
        }
    };
    if output_path.exists() {
        let existing = read(output_path, binding, Some(output_layer))?;
        let expected_derivation = match direction {
            DomainDirection::Reverse => DomainDerivation::ReverseStep,
            DomainDirection::Forward => DomainDerivation::ForwardStep,
        };
        let expected_filter = filter.as_ref().map_or([0; 32], |value| value.file_digest);
        if existing.derivation != expected_derivation
            || existing.input_digest != input.file_digest
            || existing.filter_digest != expected_filter
        {
            return Err("existing domain step is not bound to its current inputs".to_owned());
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
        DomainDirection::Reverse => {
            generate_reverse_layer(binding, output_layer, &input.fields, workers)?
        }
        DomainDirection::Forward => (
            generate_forward_layer(
                binding,
                output_layer,
                &input.fields,
                &filter
                    .as_ref()
                    .expect("forward filter checked above")
                    .fields,
                workers,
            )?,
            0,
        ),
    };
    validate_fields(output_layer, &fields)?;
    let derivation = match direction {
        DomainDirection::Reverse => DomainDerivation::ReverseStep,
        DomainDirection::Forward => DomainDerivation::ForwardStep,
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
    filter: &[u64],
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
                                if filter.binary_search(&candidate_hash).is_ok() {
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

fn generate_reverse_layer(
    binding: DomainBinding,
    output_layer: u8,
    input: &[u64],
    workers: usize,
) -> Result<(Vec<u64>, usize), String> {
    let candidate_cursor = AtomicUsize::new(0);
    let candidate_partials = thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..workers {
            let cursor = &candidate_cursor;
            handles.push(scope.spawn(move || {
                let mut candidates = Vec::new();
                loop {
                    let index = cursor.fetch_add(1, Ordering::Relaxed);
                    let Some(&target_hash) = input.get(index) else {
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
                            let source_hash = clearra_board64_mask_to_hydra_field_hash_v1(source)
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
    let candidate_pair_count = candidate_pairs.len();

    let validation_workers = workers.min(candidate_pair_count.max(1));
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
                        let reaches_domain =
                            enumerate_pc4_ilc_target_fields(source, piece, binding.kick_profile)
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
    Ok((merge_sorted(validated_partials), candidate_pair_count))
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
}
