use sha2::{Digest, Sha256};
use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

const MAGIC: &[u8; 8] = b"PC4BND02";
const VERSION: u32 = 2;
const HEADER_BYTES: usize = 64;
const FIELD_MASK: u64 = (1_u64 << 40) - 1;

pub(crate) struct BoundaryFile {
    pub(crate) fields: Vec<u64>,
    pub(crate) file_identity: String,
}

pub(crate) struct MergeInput {
    pub(crate) path: PathBuf,
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) field_count: u64,
    pub(crate) file_identity: String,
}

pub(crate) struct BoundarySummary {
    pub(crate) field_count: u64,
    pub(crate) file_identity: String,
}

pub(crate) fn write(
    path: &Path,
    binding: [u8; 32],
    start: u32,
    end: u32,
    fields: &[u64],
) -> Result<String, String> {
    if start >= end {
        return Err("outside-boundary source range invalid".to_owned());
    }
    validate_fields(fields)?;
    if path.exists() {
        return Err("refusing to overwrite an existing outside-boundary file".to_owned());
    }
    let parent = path
        .parent()
        .ok_or("outside-boundary output has no parent")?;
    let metadata = fs::symlink_metadata(parent).map_err(io_error)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("outside-boundary parent must be a real directory".to_owned());
    }
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("outside-boundary output name must be UTF-8")?;
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
        header.extend_from_slice(&start.to_le_bytes());
        header.extend_from_slice(&end.to_le_bytes());
        header.extend_from_slice(&0_u32.to_le_bytes());
        header.extend_from_slice(&binding);
        header.extend_from_slice(&(fields.len() as u64).to_le_bytes());
        writer.write_all(&header).map_err(io_error)?;
        digest.update(&header);
        for field in fields {
            let bytes = field.to_le_bytes();
            writer.write_all(&bytes).map_err(io_error)?;
            digest.update(bytes);
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

pub(crate) fn read(
    path: &Path,
    binding: [u8; 32],
    expected_range: Option<(u32, u32)>,
) -> Result<BoundaryFile, String> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("outside-boundary symlink or non-file rejected".to_owned());
    }
    let bytes = fs::read(path).map_err(io_error)?;
    if bytes.len() < HEADER_BYTES || bytes.get(..8) != Some(MAGIC.as_slice()) {
        return Err("outside-boundary header invalid".to_owned());
    }
    if read_u32(&bytes[8..12])? != VERSION || read_u32(&bytes[20..24])? != 0 {
        return Err("outside-boundary version or reserved bytes invalid".to_owned());
    }
    let start = read_u32(&bytes[12..16])?;
    let end = read_u32(&bytes[16..20])?;
    if start >= end || expected_range.is_some_and(|expected| expected != (start, end)) {
        return Err("outside-boundary source range mismatch".to_owned());
    }
    if bytes.get(24..56) != Some(binding.as_slice()) {
        return Err("outside-boundary generation binding mismatch".to_owned());
    }
    let count = usize::try_from(read_u64(&bytes[56..64])?)
        .map_err(|_| "outside-boundary field count overflow")?;
    if bytes.len() != HEADER_BYTES.saturating_add(count.saturating_mul(8)) {
        return Err("outside-boundary length mismatch".to_owned());
    }
    let mut fields = Vec::new();
    fields
        .try_reserve_exact(count)
        .map_err(|_| "outside-boundary allocation failed")?;
    for encoded in bytes[HEADER_BYTES..].chunks_exact(8) {
        fields.push(read_u64(encoded)?);
    }
    validate_fields(&fields)?;
    Ok(BoundaryFile {
        fields,
        file_identity: format!("sha256:{}", hex(Sha256::digest(&bytes).as_slice())),
    })
}

pub(crate) fn merge_files(
    output: &Path,
    binding: [u8; 32],
    start: u32,
    end: u32,
    inputs: &[MergeInput],
) -> Result<BoundarySummary, String> {
    if start >= end || inputs.is_empty() {
        return Err("merged outside-boundary range or inputs invalid".to_owned());
    }
    if inputs.iter().any(|input| input.path == output) {
        return Err("merged outside-boundary output aliases an input".to_owned());
    }
    let parent = output
        .parent()
        .ok_or("merged outside-boundary output has no parent")?;
    let metadata = fs::symlink_metadata(parent).map_err(io_error)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("merged outside-boundary parent must be a real directory".to_owned());
    }
    let name = output
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("merged outside-boundary output name must be UTF-8")?;
    let pending = parent.join(format!(".{name}.merge-pending-{}", std::process::id()));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&pending)
        .map_err(io_error)?;
    let result = (|| {
        let mut writer = BufWriter::with_capacity(1024 * 1024, file);
        writer
            .write_all(&encoded_header(binding, start, end, 0))
            .map_err(io_error)?;

        let mut cursors = inputs
            .iter()
            .map(|input| BoundaryCursor::open(input, binding))
            .collect::<Result<Vec<_>, String>>()?;
        let mut heap = BinaryHeap::new();
        for (partition, cursor) in cursors.iter().enumerate() {
            if let Some(value) = cursor.current {
                heap.push(Reverse((value, partition)));
            }
        }
        let mut prior = None;
        let mut field_count = 0_u64;
        while let Some(Reverse((value, partition))) = heap.pop() {
            if prior != Some(value) {
                writer.write_all(&value.to_le_bytes()).map_err(io_error)?;
                field_count = field_count
                    .checked_add(1)
                    .ok_or("merged outside-boundary field count overflow")?;
                prior = Some(value);
            }
            let cursor = cursors
                .get_mut(partition)
                .ok_or("outside-boundary merge cursor missing")?;
            cursor.advance()?;
            if let Some(next) = cursor.current {
                heap.push(Reverse((next, partition)));
            }
        }
        for cursor in &mut cursors {
            cursor.finish()?;
        }
        writer.flush().map_err(io_error)?;
        let mut file = writer
            .into_inner()
            .map_err(|error| io_error(error.into_error()))?;
        file.seek(SeekFrom::Start(56)).map_err(io_error)?;
        file.write_all(&field_count.to_le_bytes())
            .map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
        drop(file);

        let candidate_identity = hash_file(&pending)?;
        if output.exists() {
            let existing = read(output, binding, Some((start, end)))?;
            let existing_count = u64::try_from(existing.fields.len())
                .map_err(|_| "existing merged outside-boundary count overflow")?;
            if existing_count != field_count || existing.file_identity != candidate_identity {
                return Err("existing merged outside-boundary differs from exact inputs".to_owned());
            }
            fs::remove_file(&pending).map_err(io_error)?;
            return Ok(BoundarySummary {
                field_count,
                file_identity: existing.file_identity,
            });
        }
        fs::rename(&pending, output).map_err(io_error)?;
        Ok(BoundarySummary {
            field_count,
            file_identity: candidate_identity,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&pending);
    }
    result
}

struct BoundaryCursor {
    reader: BufReader<File>,
    expected_identity: String,
    expected_count: u64,
    read_count: u64,
    digest: Sha256,
    prior: Option<u64>,
    current: Option<u64>,
}

impl BoundaryCursor {
    fn open(input: &MergeInput, binding: [u8; 32]) -> Result<Self, String> {
        let metadata = fs::symlink_metadata(&input.path).map_err(io_error)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("outside-boundary merge input symlink or non-file rejected".to_owned());
        }
        let mut reader =
            BufReader::with_capacity(1024 * 1024, File::open(&input.path).map_err(io_error)?);
        let mut header = [0_u8; HEADER_BYTES];
        reader.read_exact(&mut header).map_err(io_error)?;
        validate_header(&header, binding, Some((input.start, input.end)))?;
        let encoded_count = read_u64(&header[56..64])?;
        if encoded_count != input.field_count
            || metadata.len()
                != (HEADER_BYTES as u64).saturating_add(encoded_count.saturating_mul(8))
        {
            return Err("outside-boundary merge input count or length mismatch".to_owned());
        }
        let mut digest = Sha256::new();
        digest.update(header);
        let mut cursor = Self {
            reader,
            expected_identity: input.file_identity.clone(),
            expected_count: encoded_count,
            read_count: 0,
            digest,
            prior: None,
            current: None,
        };
        cursor.advance()?;
        Ok(cursor)
    }

    fn advance(&mut self) -> Result<(), String> {
        if self.read_count == self.expected_count {
            self.current = None;
            return Ok(());
        }
        let mut encoded = [0_u8; 8];
        self.reader.read_exact(&mut encoded).map_err(io_error)?;
        self.digest.update(encoded);
        let value = u64::from_le_bytes(encoded);
        validate_field(value)?;
        if self.prior.is_some_and(|prior| prior >= value) {
            return Err("outside-boundary merge input is not strictly sorted".to_owned());
        }
        self.prior = Some(value);
        self.current = Some(value);
        self.read_count += 1;
        Ok(())
    }

    fn finish(&mut self) -> Result<(), String> {
        if self.current.is_some() || self.read_count != self.expected_count {
            return Err("outside-boundary merge input was not fully consumed".to_owned());
        }
        let mut trailing = [0_u8; 1];
        if self.reader.read(&mut trailing).map_err(io_error)? != 0 {
            return Err("outside-boundary merge input has trailing bytes".to_owned());
        }
        let observed = format!("sha256:{}", hex(self.digest.clone().finalize().as_slice()));
        if observed != self.expected_identity {
            return Err("outside-boundary merge input identity mismatch".to_owned());
        }
        Ok(())
    }
}

fn encoded_header(binding: [u8; 32], start: u32, end: u32, count: u64) -> [u8; HEADER_BYTES] {
    let mut header = [0_u8; HEADER_BYTES];
    header[..8].copy_from_slice(MAGIC);
    header[8..12].copy_from_slice(&VERSION.to_le_bytes());
    header[12..16].copy_from_slice(&start.to_le_bytes());
    header[16..20].copy_from_slice(&end.to_le_bytes());
    header[24..56].copy_from_slice(&binding);
    header[56..64].copy_from_slice(&count.to_le_bytes());
    header
}

fn validate_header(
    header: &[u8; HEADER_BYTES],
    binding: [u8; 32],
    expected_range: Option<(u32, u32)>,
) -> Result<(), String> {
    if header.get(..8) != Some(MAGIC.as_slice())
        || read_u32(&header[8..12])? != VERSION
        || read_u32(&header[20..24])? != 0
    {
        return Err("outside-boundary header invalid".to_owned());
    }
    let start = read_u32(&header[12..16])?;
    let end = read_u32(&header[16..20])?;
    if start >= end || expected_range.is_some_and(|expected| expected != (start, end)) {
        return Err("outside-boundary source range mismatch".to_owned());
    }
    if header.get(24..56) != Some(binding.as_slice()) {
        return Err("outside-boundary generation binding mismatch".to_owned());
    }
    Ok(())
}

fn validate_field(field: u64) -> Result<(), String> {
    if field & !FIELD_MASK != 0 || !field.count_ones().is_multiple_of(4) {
        return Err("outside-boundary field outside PC4 area layers".to_owned());
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(io_error)?;
    let mut digest = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(io_error)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("sha256:{}", hex(digest.finalize().as_slice())))
}

#[cfg(test)]
fn merge_sorted(partials: &[Vec<u64>]) -> Vec<u64> {
    let mut merged = Vec::new();
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
    merged
}

fn validate_fields(fields: &[u64]) -> Result<(), String> {
    let mut prior = None;
    for &field in fields {
        validate_field(field)?;
        if prior.is_some_and(|value| value >= field) {
            return Err("outside-boundary fields must be strictly sorted and unique".to_owned());
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
    fn boundary_round_trip_binds_generation_and_source_range() {
        let root =
            std::env::temp_dir().join(format!("clearra-pc4-boundary-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let path = root.join("boundary.bin");
        write(&path, [7; 32], 4, 9, &[0, 15, 255]).unwrap();
        let observed = read(&path, [7; 32], Some((4, 9))).unwrap();
        assert_eq!(observed.fields, [0, 15, 255]);
        assert!(read(&path, [8; 32], Some((4, 9))).is_err());
        assert!(read(&path, [7; 32], Some((5, 9))).is_err());
        fs::remove_file(path).unwrap();
        fs::remove_dir(root).unwrap();
    }

    #[test]
    fn boundary_merge_is_sorted_and_unique_across_shards() {
        assert_eq!(
            merge_sorted(&[vec![1, 3, 8], vec![1, 2, 8], Vec::new(), vec![4, 9]]),
            vec![1, 2, 3, 4, 8, 9]
        );
    }

    #[test]
    fn boundary_streaming_merge_rehashes_and_deduplicates_inputs() {
        let root =
            std::env::temp_dir().join(format!("clearra-pc4-boundary-merge-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let first = root.join("first.bin");
        let second = root.join("second.bin");
        let output = root.join("merged.bin");
        let first_identity = write(&first, [9; 32], 0, 3, &[15, 85, 255]).unwrap();
        let second_identity = write(&second, [9; 32], 3, 8, &[15, 51, 255, 3855]).unwrap();
        let summary = merge_files(
            &output,
            [9; 32],
            0,
            8,
            &[
                MergeInput {
                    path: first.clone(),
                    start: 0,
                    end: 3,
                    field_count: 3,
                    file_identity: first_identity,
                },
                MergeInput {
                    path: second.clone(),
                    start: 3,
                    end: 8,
                    field_count: 4,
                    file_identity: second_identity,
                },
            ],
        )
        .unwrap();
        assert_eq!(summary.field_count, 5);
        let observed = read(&output, [9; 32], Some((0, 8))).unwrap();
        assert_eq!(observed.fields, [15, 51, 85, 255, 3855]);
        assert_eq!(summary.file_identity, observed.file_identity);
        fs::remove_dir_all(root).unwrap();
    }
}
