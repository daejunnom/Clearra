use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    cmp::Reverse,
    collections::{BTreeSet, BinaryHeap},
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

const SORT_RUN_FIELDS: usize = 262_144;
const MERGE_FAN_IN: usize = 32;
const IO_BUFFER_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct RunFile {
    pub(crate) path: PathBuf,
    pub(crate) count: u64,
    pub(crate) identity: String,
}

pub(crate) struct Workspace {
    root: PathBuf,
    marker: Value,
}

impl Workspace {
    pub(crate) fn prepare(root: &Path, marker: Value) -> Result<Self, String> {
        let parent = root.parent().ok_or("boundary workspace has no parent")?;
        let parent_metadata = fs::symlink_metadata(parent).map_err(io_error)?;
        if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
            return Err("boundary workspace parent must be a real directory".to_owned());
        }
        let marker_path = root.join("workspace.json");
        if root.exists() {
            let metadata = fs::symlink_metadata(root).map_err(io_error)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err("boundary workspace must be a real directory".to_owned());
            }
            let encoded = fs::read(&marker_path).map_err(io_error)?;
            if encoded.len() > 64 * 1024 {
                return Err("boundary workspace marker is too large".to_owned());
            }
            let existing: Value =
                serde_json::from_slice(&encoded).map_err(|error| error.to_string())?;
            if existing != marker {
                return Err("boundary workspace belongs to different proof inputs".to_owned());
            }
            return Ok(Self {
                root: root.to_path_buf(),
                marker,
            });
        }
        fs::create_dir(root).map_err(io_error)?;
        let mut marker_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker_path)
            .map_err(io_error)?;
        serde_json::to_writer_pretty(&mut marker_file, &marker)
            .map_err(|error| error.to_string())?;
        marker_file.write_all(b"\n").map_err(io_error)?;
        marker_file.sync_all().map_err(io_error)?;
        Ok(Self {
            root: root.to_path_buf(),
            marker,
        })
    }

    pub(crate) fn accumulator(
        &self,
        layer: u8,
        worker: usize,
    ) -> Result<RunAccumulator<'_>, String> {
        RunAccumulator::new(self, layer, worker)
    }

    pub(crate) fn writer(&self, name: &str) -> Result<RunWriter, String> {
        validate_run_name(name)?;
        RunWriter::new(&self.root, name)
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn write_sorted(&self, name: &str, values: &[u64]) -> Result<RunFile, String> {
        validate_run_name(name)?;
        validate_sorted(values)?;
        let path = self.root.join(name);
        self.write_values_atomic(&path, values.iter().copied())
    }

    pub(crate) fn merge_runs(
        &self,
        label: &str,
        layer: u8,
        mut runs: Vec<RunFile>,
    ) -> Result<RunFile, String> {
        validate_run_name(label)?;
        if runs.is_empty() {
            return self.write_sorted(&format!("{label}-empty.bin"), &[]);
        }
        let mut pass = 0_usize;
        while runs.len() > 1 {
            let mut next = Vec::with_capacity(runs.len().div_ceil(MERGE_FAN_IN));
            for (group_index, group) in runs.chunks(MERGE_FAN_IN).enumerate() {
                let path = self.root.join(format!(
                    "{label}-layer-{layer:02}-pass-{pass:03}-group-{group_index:06}.bin"
                ));
                let merged = self.merge_group(&path, group)?;
                for input in group {
                    self.remove_run(input)?;
                }
                next.push(merged);
            }
            runs = next;
            pass = pass
                .checked_add(1)
                .ok_or("boundary run merge pass overflow")?;
        }
        runs.pop()
            .ok_or("boundary run merge output missing".to_owned())
    }

    pub(crate) fn merge_runs_preserving_inputs(
        &self,
        label: &str,
        layer: u8,
        mut runs: Vec<RunFile>,
    ) -> Result<RunFile, String> {
        validate_run_name(label)?;
        if runs.is_empty() {
            return self.write_sorted(&format!("{label}-empty.bin"), &[]);
        }
        let original_paths = runs
            .iter()
            .map(|run| run.path.clone())
            .collect::<BTreeSet<_>>();
        let mut pass = 0_usize;
        while runs.len() > 1 {
            let mut next = Vec::with_capacity(runs.len().div_ceil(MERGE_FAN_IN));
            for (group_index, group) in runs.chunks(MERGE_FAN_IN).enumerate() {
                let path = self.root.join(format!(
                    "{label}-layer-{layer:02}-pass-{pass:03}-group-{group_index:06}.bin"
                ));
                let merged = self.merge_group(&path, group)?;
                for input in group {
                    if !original_paths.contains(&input.path) {
                        self.remove_run(input)?;
                    }
                }
                next.push(merged);
            }
            runs = next;
            pass = pass
                .checked_add(1)
                .ok_or("boundary run merge pass overflow")?;
        }
        runs.pop()
            .ok_or("boundary run merge output missing".to_owned())
    }

    pub(crate) fn restore_run(
        &self,
        name: &str,
        count: u64,
        identity: &str,
    ) -> Result<RunFile, String> {
        validate_run_name(name)?;
        let run = RunFile {
            path: self.root.join(name),
            count,
            identity: identity.to_owned(),
        };
        self.validate_owned_run(&run)?;
        let mut reader =
            BufReader::with_capacity(IO_BUFFER_BYTES, File::open(&run.path).map_err(io_error)?);
        let mut digest = Sha256::new();
        let mut encoded = [0_u8; 8];
        let mut prior = None;
        for _ in 0..count {
            reader.read_exact(&mut encoded).map_err(io_error)?;
            digest.update(encoded);
            let value = u64::from_le_bytes(encoded);
            if prior.is_some_and(|previous| previous >= value) {
                return Err("restored boundary run is not strictly sorted".to_owned());
            }
            prior = Some(value);
        }
        let actual = format!("sha256:{}", hex(digest.finalize().as_slice()));
        if actual != identity {
            return Err("restored boundary run identity mismatch".to_owned());
        }
        Ok(run)
    }

    pub(crate) fn retain_entries(&self, names: &BTreeSet<String>) -> Result<(), String> {
        let marker_path = self.root.join("workspace.json");
        let encoded = fs::read(&marker_path).map_err(io_error)?;
        let current: Value = serde_json::from_slice(&encoded).map_err(|error| error.to_string())?;
        if current != self.marker {
            return Err("boundary workspace marker changed before recovery".to_owned());
        }
        for name in names {
            validate_run_name(name)?;
        }
        for entry in fs::read_dir(&self.root).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let name = entry
                .file_name()
                .to_str()
                .ok_or("boundary workspace entry name must be UTF-8")?
                .to_owned();
            if name == "workspace.json" || names.contains(&name) {
                continue;
            }
            let metadata = fs::symlink_metadata(entry.path()).map_err(io_error)?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err("boundary workspace contains a non-file recovery entry".to_owned());
            }
            fs::remove_file(entry.path()).map_err(io_error)?;
        }
        Ok(())
    }

    pub(crate) fn visit_range<F>(
        &self,
        run: &RunFile,
        start: u64,
        end: u64,
        mut visit: F,
    ) -> Result<(), String>
    where
        F: FnMut(u64) -> Result<(), String>,
    {
        if start > end || end > run.count {
            return Err("boundary run range outside file".to_owned());
        }
        self.validate_owned_run(run)?;
        let mut file = File::open(&run.path).map_err(io_error)?;
        file.seek(SeekFrom::Start(
            start.checked_mul(8).ok_or("boundary run seek overflow")?,
        ))
        .map_err(io_error)?;
        let mut reader = BufReader::with_capacity(IO_BUFFER_BYTES, file);
        let mut encoded = [0_u8; 8];
        let mut prior = None;
        for _ in start..end {
            reader.read_exact(&mut encoded).map_err(io_error)?;
            let value = u64::from_le_bytes(encoded);
            if prior.is_some_and(|previous| previous >= value) {
                return Err("boundary run range is not strictly sorted".to_owned());
            }
            prior = Some(value);
            visit(value)?;
        }
        Ok(())
    }

    pub(crate) fn remove_run(&self, run: &RunFile) -> Result<(), String> {
        self.validate_owned_run(run)?;
        fs::remove_file(&run.path).map_err(io_error)
    }

    pub(crate) fn cleanup(self) -> Result<(), String> {
        let marker_path = self.root.join("workspace.json");
        let encoded = fs::read(&marker_path).map_err(io_error)?;
        let current: Value = serde_json::from_slice(&encoded).map_err(|error| error.to_string())?;
        if current != self.marker {
            return Err("boundary workspace marker changed before cleanup".to_owned());
        }
        let metadata = fs::symlink_metadata(&self.root).map_err(io_error)?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("boundary workspace changed before cleanup".to_owned());
        }
        fs::remove_dir_all(&self.root).map_err(io_error)
    }

    fn merge_group(&self, path: &Path, inputs: &[RunFile]) -> Result<RunFile, String> {
        if inputs.is_empty() || inputs.len() > MERGE_FAN_IN {
            return Err("boundary run merge group outside supported fan-in".to_owned());
        }
        let mut cursors = inputs
            .iter()
            .map(|input| RunCursor::open(self, input))
            .collect::<Result<Vec<_>, String>>()?;
        let mut heap = BinaryHeap::new();
        for (partition, cursor) in cursors.iter().enumerate() {
            if let Some(value) = cursor.current {
                heap.push(Reverse((value, partition)));
            }
        }
        let mut emitted_prior = None;
        self.write_values_atomic(
            path,
            std::iter::from_fn(|| {
                while let Some(Reverse((value, partition))) = heap.pop() {
                    let cursor = &mut cursors[partition];
                    if cursor.advance().is_err() {
                        return Some(Err("boundary run merge cursor failed".to_owned()));
                    }
                    if let Some(next) = cursor.current {
                        heap.push(Reverse((next, partition)));
                    }
                    if emitted_prior != Some(value) {
                        emitted_prior = Some(value);
                        return Some(Ok(value));
                    }
                }
                None
            }),
        )
    }

    fn write_values_atomic<I, T>(&self, path: &Path, values: I) -> Result<RunFile, String>
    where
        I: IntoIterator<Item = T>,
        T: IntoRunValue,
    {
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or("boundary run name must be UTF-8")?;
        validate_run_name(name)?;
        if path.parent() != Some(self.root.as_path()) {
            return Err("boundary run escaped workspace".to_owned());
        }
        let mut writer = self.writer(name)?;
        for item in values {
            writer.push(item.into_run_value()?)?;
        }
        writer.finish()
    }

    fn validate_owned_run(&self, run: &RunFile) -> Result<(), String> {
        if run.path.parent() != Some(self.root.as_path()) {
            return Err("boundary run escaped workspace".to_owned());
        }
        let metadata = fs::symlink_metadata(&run.path).map_err(io_error)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len()
                != run
                    .count
                    .checked_mul(8)
                    .ok_or("boundary run byte length overflow")?
        {
            return Err("boundary run metadata invalid".to_owned());
        }
        Ok(())
    }
}

pub(crate) struct RunWriter {
    path: PathBuf,
    pending: PathBuf,
    writer: Option<BufWriter<File>>,
    count: u64,
    prior: Option<u64>,
    digest: Sha256,
    finished: bool,
}

impl RunWriter {
    fn new(root: &Path, name: &str) -> Result<Self, String> {
        let path = root.join(name);
        if path.exists() {
            return Err("refusing to overwrite an existing boundary run".to_owned());
        }
        let pending = root.join(format!(".{name}.pending-{}", std::process::id()));
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)
            .map_err(io_error)?;
        Ok(Self {
            path,
            pending,
            writer: Some(BufWriter::with_capacity(IO_BUFFER_BYTES, file)),
            count: 0,
            prior: None,
            digest: Sha256::new(),
            finished: false,
        })
    }

    pub(crate) fn push(&mut self, value: u64) -> Result<(), String> {
        if self.finished || self.prior.is_some_and(|previous| previous >= value) {
            return Err("boundary run output is not strictly sorted".to_owned());
        }
        let encoded = value.to_le_bytes();
        self.writer
            .as_mut()
            .ok_or("boundary run writer already closed")?
            .write_all(&encoded)
            .map_err(io_error)?;
        self.digest.update(encoded);
        self.prior = Some(value);
        self.count = self
            .count
            .checked_add(1)
            .ok_or("boundary run field count overflow")?;
        Ok(())
    }

    pub(crate) fn finish(mut self) -> Result<RunFile, String> {
        let mut writer = self
            .writer
            .take()
            .ok_or("boundary run writer already closed")?;
        writer.flush().map_err(io_error)?;
        writer.get_ref().sync_all().map_err(io_error)?;
        drop(writer);
        fs::rename(&self.pending, &self.path).map_err(io_error)?;
        self.finished = true;
        Ok(RunFile {
            path: self.path.clone(),
            count: self.count,
            identity: format!("sha256:{}", hex(self.digest.clone().finalize().as_slice())),
        })
    }
}

impl Drop for RunWriter {
    fn drop(&mut self) {
        if !self.finished {
            let _ = fs::remove_file(&self.pending);
        }
    }
}

pub(crate) struct RunAccumulator<'a> {
    workspace: &'a Workspace,
    layer: u8,
    worker: usize,
    sequence: usize,
    pending: Vec<u64>,
    runs: Vec<RunFile>,
}

impl<'a> RunAccumulator<'a> {
    fn new(workspace: &'a Workspace, layer: u8, worker: usize) -> Result<Self, String> {
        let mut pending = Vec::new();
        pending
            .try_reserve_exact(SORT_RUN_FIELDS)
            .map_err(|_| "boundary disk-run allocation failed")?;
        Ok(Self {
            workspace,
            layer,
            worker,
            sequence: 0,
            pending,
            runs: Vec::new(),
        })
    }

    pub(crate) fn push(&mut self, value: u64) -> Result<(), String> {
        self.pending.push(value);
        if self.pending.len() == SORT_RUN_FIELDS {
            self.flush()?;
        }
        Ok(())
    }

    pub(crate) fn finish(mut self) -> Result<Vec<RunFile>, String> {
        self.flush()?;
        Ok(self.runs)
    }

    fn flush(&mut self) -> Result<(), String> {
        if self.pending.is_empty() {
            return Ok(());
        }
        self.pending.sort_unstable();
        self.pending.dedup();
        let values = std::mem::take(&mut self.pending);
        let name = format!(
            "generated-layer-{:02}-worker-{:02}-run-{:08}.bin",
            self.layer, self.worker, self.sequence
        );
        let run = self.workspace.write_sorted(&name, &values)?;
        self.runs.push(run);
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("boundary disk-run sequence overflow")?;
        self.pending = Vec::new();
        self.pending
            .try_reserve_exact(SORT_RUN_FIELDS)
            .map_err(|_| "boundary disk-run allocation failed")?;
        Ok(())
    }
}

struct RunCursor {
    reader: BufReader<File>,
    remaining: u64,
    prior: Option<u64>,
    current: Option<u64>,
}

impl RunCursor {
    fn open(workspace: &Workspace, run: &RunFile) -> Result<Self, String> {
        workspace.validate_owned_run(run)?;
        let mut cursor = Self {
            reader: BufReader::with_capacity(
                IO_BUFFER_BYTES,
                File::open(&run.path).map_err(io_error)?,
            ),
            remaining: run.count,
            prior: None,
            current: None,
        };
        cursor.advance()?;
        Ok(cursor)
    }

    fn advance(&mut self) -> Result<(), String> {
        if self.remaining == 0 {
            self.current = None;
            return Ok(());
        }
        let mut encoded = [0_u8; 8];
        self.reader.read_exact(&mut encoded).map_err(io_error)?;
        let value = u64::from_le_bytes(encoded);
        if self.prior.is_some_and(|previous| previous >= value) {
            return Err("boundary run input is not strictly sorted".to_owned());
        }
        self.prior = Some(value);
        self.current = Some(value);
        self.remaining -= 1;
        Ok(())
    }
}

trait IntoRunValue {
    fn into_run_value(self) -> Result<u64, String>;
}

impl IntoRunValue for u64 {
    fn into_run_value(self) -> Result<u64, String> {
        Ok(self)
    }
}

impl IntoRunValue for Result<u64, String> {
    fn into_run_value(self) -> Result<u64, String> {
        self
    }
}

fn validate_sorted(values: &[u64]) -> Result<(), String> {
    if values.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err("boundary run values must be strictly sorted".to_owned());
    }
    Ok(())
}

fn validate_run_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err("boundary run name invalid".to_owned());
    }
    Ok(())
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
    use serde_json::json;

    #[test]
    fn disk_runs_merge_sorted_unique_values_and_ranges() {
        let root =
            std::env::temp_dir().join(format!("clearra-pc4-boundary-store-{}", std::process::id()));
        if root.exists() {
            fs::remove_dir_all(&root).unwrap();
        }
        let workspace = Workspace::prepare(&root, json!({ "test": "disk-runs" })).unwrap();
        let first = workspace.write_sorted("first.bin", &[1, 3, 8]).unwrap();
        let second = workspace
            .write_sorted("second.bin", &[1, 2, 8, 13])
            .unwrap();
        let merged = workspace
            .merge_runs("merged", 4, vec![first, second])
            .unwrap();
        assert_eq!(merged.count, 5);
        let mut observed = Vec::new();
        workspace
            .visit_range(&merged, 1, 4, |value| {
                observed.push(value);
                Ok(())
            })
            .unwrap();
        assert_eq!(observed, [2, 3, 8]);
        workspace.remove_run(&merged).unwrap();
        workspace.cleanup().unwrap();
    }
}
