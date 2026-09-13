//! SRP: explicit native dataset lifecycle; never called by a search request.
//! curl is a transport-only system dependency, invoked without a shell or user
//! curl configuration. SHA-256, publication and retention are owned here.
use crate::{error::CliErrorCode, output::CliOutput};
use clearra_i18n::LanguageId;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[path = "tablebase_download_format.rs"]
mod format;
const REPOSITORY: &str = "muse918/tetris-4lpc-mdp-vstar-policy";
const FILES: [&str; 3] = [
    "field_hash_to_id.v1.bin",
    "graph_offsets.u32.bin",
    "graph.bin",
];
type Result<T> = std::result::Result<T, &'static str>;

#[derive(Clone, Debug)]
pub(super) struct Artifact {
    path: &'static str,
    size: u64,
    digest: String,
}
impl Artifact {
    fn value(&self) -> Value {
        json!({ "path": self.path, "byte_length": self.size, "content_identity": format!("sha256:{}", self.digest) })
    }
}

pub(crate) fn run(args: &[String], language: LanguageId, json_output: bool) -> CliOutput {
    let help = args.is_empty() || args.iter().any(|v| matches!(v.as_str(), "--help" | "-h"));
    if help {
        let (note, unavailable, location) = match language {
            LanguageId::Ko => ("선택한 킥테이블의 그래프와 두 인덱스를 명시적으로 다운로드합니다. curl이 필요합니다. check는 용량만 확인하며 파일을 받지 않습니다. download는 최신 데이터로 갱신하고 이전 데이터는 성공 후 정리합니다. Ctrl+C로 중단할 수 있습니다.",
                "현재 Jstris 180만 사용 가능합니다. SRS / SRS+ / SRS-X / No kick은 각 프로필의 그래프와 인덱스가 검증되면 별도로 제공됩니다.",
                "CLEARRA_PC4_DIRECTORY로 다운로드와 로컬 탐색의 공통 저장 위치를 지정합니다. 다운로드는 선택 사항입니다. --tablebase 탐색은 저장된 데이터가 없으면 필요한 구간만 온라인으로 조회합니다(curl 7.84 이상). 조회 실패 시 오프라인 탐색은 자동 시작하지 않습니다."),
            LanguageId::Ja => ("選択したキックテーブルのグラフと2つのインデックスを明示的にダウンロードします。curlが必要です。checkは容量のみ確認します。downloadは最新データへ更新し、成功後に旧データを削除します。Ctrl+Cで中止できます。",
                "現在はJstris 180のみ利用できます。SRS / SRS+ / SRS-X / No kickはそれぞれのグラフとインデックスが確認され次第、個別に提供します。",
                "CLEARRA_PC4_DIRECTORYで共通の保存先を指定します。ダウンロードは任意です。--tablebase検索は保存済みデータがなければ必要な範囲だけオンラインで取得します(curl 7.84以降)。失敗してもオフライン検索は自動開始しません。"),
            _ => ("Explicitly download the selected kick table's graph and two indexes. Requires curl. check retrieves sizes only. download updates to the latest data and removes the old copy only after success. Ctrl+C cancels.",
                "Jstris 180 is currently available. SRS / SRS+ / SRS-X / No kick require their own independently qualified graph and indexes.",
                "CLEARRA_PC4_DIRECTORY selects the shared storage base. Downloads are optional: --tablebase searches read online ranges when no data is installed (curl 7.84 or later). A lookup failure never starts offline search automatically.")
        };
        return CliOutput::success(format!("clearra tablebase <check|download|status|remove> --profile srs|srs-plus|srs-x|jstris-180|no-kick [--directory DIRECTORY]\n{note}\n{unavailable}\n{location}"));
    }
    match parse_and_execute(args) {
        Ok(value) => {
            if json_output {
                return CliOutput::success(value.to_string());
            }
            let (available, absent, bytes, revision) = match language {
                LanguageId::Ko => ("Jstris 180 TB", "저장된 TB 없음", "용량", "데이터 버전"),
                LanguageId::Ja => (
                    "Jstris 180 TB",
                    "保存済みTBなし",
                    "容量",
                    "データのバージョン",
                ),
                _ => ("Jstris 180 TB", "No saved TB", "Size", "Data revision"),
            };
            if value["installed"] == false {
                return CliOutput::success(absent);
            }
            let size = value["stored_bytes"]
                .as_u64()
                .or_else(|| value["download_bytes"].as_u64())
                .unwrap_or(0);
            let cleanup = if value["cleanup_pending"] == true {
                match language {
                    LanguageId::Ko => "\n새 TB 저장은 완료했지만 이전 파일 정리가 남았습니다. 다음 업데이트 또는 remove로 다시 정리할 수 있습니다.",
                    LanguageId::Ja => "\n新しいTBの保存は完了しましたが、旧ファイルの削除が残っています。次回の更新またはremoveで再試行できます。",
                    _ => "\nNew TB saved; old-file cleanup is pending. The next update or remove retries cleanup.",
                }
            } else {
                ""
            };
            CliOutput::success(format!(
                "{available}\n{bytes}: {:.1} MiB\n{revision}: {}{cleanup}",
                size as f64 / 1_048_576.0,
                value["revision"].as_str().unwrap_or("")
            ))
        }
        Err(reason) => CliOutput::error(CliErrorCode::CliInvalidValue, reason),
    }
}

fn parse_and_execute(args: &[String]) -> Result<Value> {
    let action = args
        .first()
        .ok_or("tablebase: an action is required")?
        .as_str();
    if !matches!(action, "check" | "download" | "status" | "remove") {
        return Err("tablebase: use check, download, status or remove");
    }
    let mut directory = None;
    let mut profile = None;
    let mut i = 1;
    while i < args.len() {
        let value = args
            .get(i + 1)
            .ok_or("tablebase: option requires a value")?;
        match args[i].as_str() {
            "--profile" if profile.is_none() => profile = Some(value.as_str()),
            "--directory" if directory.is_none() => directory = Some(PathBuf::from(value)),
            _ => return Err("tablebase: unknown or repeated option"),
        }
        i += 2;
    }
    let profile = profile.ok_or(
        "tablebase: --profile is required; choose srs, srs-plus, srs-x, jstris-180 or no-kick",
    )?;
    if !["srs", "srs-plus", "srs-x", "jstris-180", "no-kick"].contains(&profile) {
        return Err("tablebase: unknown profile");
    }
    if profile != "jstris-180" && matches!(action, "check" | "download") {
        return Err("tablebase: this profile is not yet qualified for complete local download");
    }
    if action == "check" {
        let (revision, files) = discover()?;
        return Ok(json!({ "profile": "jstris-180", "revision": revision,
            "download_bytes": files.iter().map(|f| f.size).sum::<u64>(), "files": files.iter().map(Artifact::value).collect::<Vec<_>>() }));
    }
    let base = directory.map(Ok).unwrap_or_else(default_directory)?;
    let root = base.join("pc4-v1").join(profile);
    reject_links(&root)?;
    if !root.exists() && action != "download" {
        return Ok(json!({ "installed": false, "profile": profile }));
    }
    // No alternate profile may interpret canonical Jstris files. Its future
    // independent format adapter must be qualified before opening its store.
    if profile != "jstris-180" {
        return Err("tablebase: this profile has no qualified local reader");
    }
    fs::create_dir_all(&root).map_err(|_| "tablebase: could not create the storage directory")?;
    reject_links(&root)?;
    let lock_path = root.join("store.lock");
    reject_links(&lock_path)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
        .map_err(|_| "tablebase: could not open storage lock")?;
    lock.try_lock()
        .map_err(|_| "tablebase: storage is in use by another process")?;
    match action {
        "status" => status(&root),
        "remove" => {
            active(&root)?; // reject an unrecognized pointer before deletion
            remove_file_if_exists(&root.join("active.json"))?;
            clean_generations(&root, None)?;
            Ok(json!({ "installed": false, "removed": true }))
        }
        "download" => {
            let (revision, files) = discover()?;
            install(&root, &revision, &files, curl_stream)
        }
        _ => unreachable!(),
    }
}

fn default_directory() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("CLEARRA_PC4_DIRECTORY") {
        return Ok(PathBuf::from(path));
    }
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(|p| PathBuf::from(p).join("Clearra").join("tablebase"))
            .ok_or("tablebase: supply --directory when LOCALAPPDATA is unavailable")
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")))
            .map(|p| p.join("clearra/tablebase"))
            .ok_or("tablebase: supply --directory when the data directory is unavailable")
    }
}

#[cfg(feature = "online-pc4-tablebase")]
#[path = "tablebase_host_execution.rs"]
mod host_execution;
#[cfg(feature = "online-pc4-tablebase")]
#[path = "tablebase_http_range.rs"]
mod http_range;
#[cfg(feature = "online-pc4-tablebase")]
#[path = "tablebase_local_execution.rs"]
mod local_execution;
#[cfg(feature = "online-pc4-tablebase")]
#[path = "tablebase_online_execution.rs"]
mod online_execution;
#[cfg(all(test, feature = "online-pc4-tablebase", feature = "wasm-cpu-runtime"))]
use local_execution::execute_local_at;
#[cfg(feature = "online-pc4-tablebase")]
pub(crate) use online_execution::execute;
fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
#[path = "tablebase_download_transport.rs"]
mod transport;
use transport::{curl_stream, discover};

fn install(
    root: &Path,
    revision: &str,
    files: &[Artifact],
    mut fetch: impl FnMut(&str, u64, &mut dyn FnMut(&[u8]) -> Result<()>) -> Result<()>,
) -> Result<Value> {
    let previous = active(root)?;
    if previous
        .as_ref()
        .is_some_and(|p| p["generation"]["revision"] == revision)
    {
        // An unchanged metadata pointer is not enough: verify full stored bytes
        // on explicit download/update, never while merely checking status.
        if verify_installed(root, previous.as_ref().unwrap(), files).is_ok() {
            let keep = previous.as_ref().and_then(|p| p["directory"].as_str());
            let cleanup_pending = clean_generations(root, keep).is_err();
            let mut value = status(root)?;
            value["cleanup_pending"] = cleanup_pending.into();
            return Ok(value);
        }
    }
    let keep = previous.as_ref().and_then(|p| p["directory"].as_str());
    clean_generations(root, keep)?;
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random)
        .map_err(|_| "tablebase: could not allocate a download transaction")?;
    let name = format!(
        "gen-{}",
        random
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    let staging = root.join(&name);
    fs::create_dir(&staging).map_err(|_| "tablebase: could not create download staging")?;
    let mut committed = false;
    let result = (|| {
        for artifact in files {
            let mut file = File::create_new(staging.join(artifact.path))
                .map_err(|_| "tablebase: could not create artifact file")?;
            let mut digest = Sha256::new();
            let mut length = 0_u64;
            fetch(
                &format!(
                    "https://huggingface.co/datasets/{REPOSITORY}/resolve/{revision}/{}",
                    artifact.path
                ),
                artifact.size,
                &mut |bytes| {
                    length += bytes.len() as u64;
                    if length > artifact.size {
                        return Err("tablebase: artifact exceeds its declared size");
                    }
                    file.write_all(bytes)
                        .map_err(|_| "tablebase: write failed; check free storage space")?;
                    digest.update(bytes);
                    Ok(())
                },
            )?;
            if length != artifact.size || format!("{:x}", digest.finalize()) != artifact.digest {
                return Err("tablebase: artifact integrity check failed");
            }
            file.sync_all()
                .map_err(|_| "tablebase: artifact flush failed")?;
        }
        let generation = format::qualify(&staging, revision, files)?;
        let pointer = json!({ "schema": "clearra.pc4.local-files.v1", "directory": name, "generation": generation });
        let pending = root.join("active.pending");
        remove_file_if_exists(&pending)?;
        let mut file =
            File::create_new(&pending).map_err(|_| "tablebase: could not stage activation")?;
        file.write_all(pointer.to_string().as_bytes())
            .map_err(|_| "tablebase: activation write failed")?;
        file.sync_all()
            .map_err(|_| "tablebase: activation flush failed")?;
        drop(file);
        fs::rename(pending, root.join("active.json"))
            .map_err(|_| "tablebase: activation failed; previous data preserved")?;
        committed = true;
        let cleanup_pending = clean_generations(root, Some(&name)).is_err();
        let mut result = status(root)?;
        result["cleanup_pending"] = cleanup_pending.into();
        Ok(result)
    })();
    if !committed {
        let _ = remove_generation(root, &name);
    }
    result
}
fn verify_installed(root: &Path, pointer: &Value, files: &[Artifact]) -> Result<()> {
    let name = pointer["directory"]
        .as_str()
        .ok_or("tablebase: invalid local pointer")?;
    for artifact in files {
        let path = root.join(name).join(artifact.path);
        reject_links(&path)?;
        let mut file = File::open(path).map_err(|_| "tablebase: local artifact missing")?;
        if file
            .metadata()
            .map_err(|_| "tablebase: local artifact missing")?
            .len()
            != artifact.size
        {
            return Err("tablebase: local size mismatch");
        }
        let mut digest = Sha256::new();
        let mut buffer = [0_u8; 65_536];
        loop {
            let n = file
                .read(&mut buffer)
                .map_err(|_| "tablebase: local artifact read failed")?;
            if n == 0 {
                break;
            }
            digest.update(&buffer[..n]);
        }
        if format!("{:x}", digest.finalize()) != artifact.digest {
            return Err("tablebase: local digest mismatch");
        }
    }
    Ok(())
}
fn generation_name(name: &str) -> bool {
    name.strip_prefix("gen-").is_some_and(|v| hex(v, 32))
}
fn active(root: &Path) -> Result<Option<Value>> {
    let path = root.join("active.json");
    reject_links(&path)?;
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("tablebase: cannot read local activation"),
    };
    if file
        .metadata()
        .map_err(|_| "tablebase: invalid local activation")?
        .len()
        > 131_072
    {
        return Err("tablebase: local activation is oversized");
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|_| "tablebase: cannot read local activation")?;
    let pointer: Value =
        serde_json::from_slice(&bytes).map_err(|_| "tablebase: invalid local activation")?;
    if pointer["schema"] != "clearra.pc4.local-files.v1"
        || !pointer["directory"].as_str().is_some_and(generation_name)
        || pointer["generation"]["repository"] != REPOSITORY
        || !pointer["generation"]["revision"]
            .as_str()
            .is_some_and(|s| hex(s, 40))
    {
        return Err("tablebase: unrecognized local activation; no files were removed");
    }
    Ok(Some(pointer))
}
fn status(root: &Path) -> Result<Value> {
    let Some(pointer) = active(root)? else {
        return Ok(json!({ "installed": false }));
    };
    let directory = root.join(pointer["directory"].as_str().unwrap());
    let mut size = 0;
    for name in FILES {
        let path = directory.join(name);
        reject_links(&path)?;
        size += fs::metadata(path)
            .map_err(|_| "tablebase: local artifact missing")?
            .len();
    }
    Ok(
        json!({ "installed": true, "profile": "jstris-180", "revision": pointer["generation"]["revision"], "stored_bytes": size }),
    )
}
fn reject_links(path: &Path) -> Result<()> {
    for parent in path.ancestors() {
        match fs::symlink_metadata(parent) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err("tablebase: linked storage paths are not allowed");
                }
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if metadata.file_attributes() & 0x400 != 0 {
                        return Err("tablebase: reparse storage paths are not allowed");
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("tablebase: storage path could not be checked"),
        }
    }
    Ok(())
}
fn remove_file_if_exists(path: &Path) -> Result<()> {
    reject_links(path)?;
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("tablebase: could not remove a managed file"),
    }
}
fn remove_generation(root: &Path, name: &str) -> Result<()> {
    if !generation_name(name) {
        return Err("tablebase: invalid managed generation name");
    }
    let directory = root.join(name);
    reject_links(&directory)?;
    // No recursive deletion: only the three exact owned files, then an empty
    // child directory. Unexpected files prevent removal instead of being lost.
    for file in FILES {
        remove_file_if_exists(&directory.join(file))?;
    }
    fs::remove_dir(directory)
        .map_err(|_| "tablebase: generation contains unexpected files or is in use")
}
fn clean_generations(root: &Path, keep: Option<&str>) -> Result<()> {
    for entry in fs::read_dir(root).map_err(|_| "tablebase: cannot list managed storage")? {
        let entry = entry.map_err(|_| "tablebase: cannot inspect managed generation")?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if generation_name(&name) && Some(name.as_str()) != keep {
            remove_generation(root, &name)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "tablebase_download_tests.rs"]
mod tests;
