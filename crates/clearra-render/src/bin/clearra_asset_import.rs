use std::{
    collections::BTreeMap,
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use clearra_render::{
    rasterize_sanitized_svg, sanitize_svg, AssetImportLimits, AssetImportMetadata,
    AssetImportPipeline,
};
use tempfile::{Builder, TempDir};

const WORKER_COMMAND: &str = "__clearra_asset_import_worker";
const WORKER_TOKEN_ENV: &str = "CLEARRA_ASSET_IMPORT_WORKER_TOKEN";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("asset import failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    if arguments
        .first()
        .is_some_and(|value| value == WORKER_COMMAND)
    {
        return run_worker(&arguments[1..]);
    }
    run_isolated(&arguments)
}

fn run_isolated(arguments: &[String]) -> Result<(), String> {
    let command = arguments
        .first()
        .ok_or_else(|| "missing command: sanitize|rasterize|import".to_owned())?;
    if !matches!(command.as_str(), "sanitize" | "rasterize" | "import") {
        return Err(format!("unknown command:{command}"));
    }
    let options = parse_options(arguments[1..].to_vec())?;
    let (staging, worker_options, commit) = stage_output(command, &options)?;
    let token = worker_token();
    let mut child = Command::new(env::current_exe().map_err(|error| error.to_string())?)
        .arg(WORKER_COMMAND)
        .arg(command)
        .args(options_as_arguments(&worker_options))
        .env(WORKER_TOKEN_ENV, &token)
        .arg("--worker-token")
        .arg(&token)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("asset_import_worker_spawn_failed:{error}"))?;

    wait_for_worker(&mut child, AssetImportLimits::default().max_import_time_ms)?;
    commit.apply()?;
    drop(staging);
    Ok(())
}

fn run_worker(arguments: &[String]) -> Result<(), String> {
    let command = arguments
        .first()
        .ok_or_else(|| "asset_import_worker_command_missing".to_owned())?;
    let mut options = parse_options(arguments[1..].to_vec())?;
    let supplied_token = options
        .remove("worker-token")
        .ok_or_else(|| "asset_import_worker_token_missing".to_owned())?;
    let expected_token = env::var(WORKER_TOKEN_ENV)
        .map_err(|_| "asset_import_worker_environment_missing".to_owned())?;
    if supplied_token != expected_token {
        return Err("asset_import_worker_token_mismatch".to_owned());
    }

    let limits = AssetImportLimits::default();
    match command.as_str() {
        "sanitize" => {
            let input = read_utf8(required_path(&options, "input")?)?;
            let sanitized = sanitize_svg(&input, &limits)?;
            write_file(required_path(&options, "output")?, sanitized.as_bytes())
        }
        "rasterize" => {
            let input = read_utf8(required_path(&options, "input")?)?;
            let png = rasterize_sanitized_svg(&input, &limits)?;
            write_file(required_path(&options, "output")?, &png)
        }
        "import" => import_skin(&options, &limits),
        _ => Err(format!("asset_import_worker_command_forbidden:{command}")),
    }
}

fn wait_for_worker(child: &mut std::process::Child, timeout_ms: u64) -> Result<(), String> {
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("asset_import_worker_wait_failed:{error}"))?
        {
            if status.success() {
                return Ok(());
            }
            let mut stderr = String::new();
            if let Some(mut stream) = child.stderr.take() {
                let _ = stream.read_to_string(&mut stderr);
            }
            return Err(if stderr.trim().is_empty() {
                format!("asset_import_worker_failed:{status}")
            } else {
                stderr.trim().to_owned()
            });
        }
        if started.elapsed() >= Duration::from_millis(timeout_ms) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("svg_import_time_limit_exceeded".to_owned());
        }
        thread::sleep(Duration::from_millis(5));
    }
}

enum StagedCommit {
    File { staged: PathBuf, output: PathBuf },
    Directory { staged: PathBuf, output: PathBuf },
}

impl StagedCommit {
    fn apply(self) -> Result<(), String> {
        let (staged, output) = match self {
            Self::File { staged, output } | Self::Directory { staged, output } => (staged, output),
        };
        if output.exists() {
            return Err(format!(
                "asset_import_output_already_exists:{}",
                output.display()
            ));
        }
        fs::rename(&staged, &output).map_err(|error| {
            format!(
                "asset_import_atomic_commit_failed:{}:{}",
                output.display(),
                error
            )
        })
    }
}

fn stage_output(
    command: &str,
    options: &BTreeMap<String, String>,
) -> Result<(TempDir, BTreeMap<String, String>, StagedCommit), String> {
    let option_name = if command == "import" {
        "output-dir"
    } else {
        "output"
    };
    let output = required_path(options, option_name)?;
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let staging = Builder::new()
        .prefix(".clearra-asset-import-")
        .tempdir_in(parent)
        .map_err(|error| error.to_string())?;
    let staged = staging.path().join(if command == "import" {
        "bundle"
    } else {
        "artifact"
    });
    let mut worker_options = options.clone();
    worker_options.insert(
        option_name.to_owned(),
        staged.to_string_lossy().into_owned(),
    );
    let commit = if command == "import" {
        StagedCommit::Directory { staged, output }
    } else {
        StagedCommit::File { staged, output }
    };
    Ok((staging, worker_options, commit))
}

fn worker_token() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{}-{nanos}", std::process::id())
}

fn options_as_arguments(options: &BTreeMap<String, String>) -> Vec<String> {
    options
        .iter()
        .flat_map(|(key, value)| [format!("--{key}"), value.clone()])
        .collect()
}

fn import_skin(
    options: &BTreeMap<String, String>,
    limits: &AssetImportLimits,
) -> Result<(), String> {
    let input_path = required_path(options, "input")?;
    let output_dir = required_path(options, "output-dir")?;
    let source = fs::read(&input_path).map_err(|error| error.to_string())?;
    let metadata = AssetImportMetadata {
        source_label: option(options, "source-label", &input_path.to_string_lossy()),
        origin_kind: option(options, "origin-kind", "human-reviewed-svg"),
        skin_id: option(options, "skin-id", "default"),
        display_name: option(options, "display-name", "Clearra Skin"),
        license: option(options, "license", "MIT"),
        redistribution: option(options, "redistribution", "Permitted by declared license"),
        tile_width: parse_u32(options, "tile-width", 16)?,
        tile_height: parse_u32(options, "tile-height", 16)?,
    };
    let bundle = AssetImportPipeline::import_svg(&source, &metadata, limits)?;
    fs::create_dir_all(&output_dir).map_err(|error| error.to_string())?;
    write_file(output_dir.join("sanitized.svg"), bundle.sanitized_svg())?;
    write_file(output_dir.join("atlas.png"), bundle.atlas_png())?;
    write_file(output_dir.join("skin.json"), bundle.manifest_json())?;
    write_file(output_dir.join("provenance.json"), bundle.provenance_json())?;
    write_file(
        output_dir.join("import-report.json"),
        bundle.import_report_json(),
    )
}

fn parse_options(arguments: Vec<String>) -> Result<BTreeMap<String, String>, String> {
    if !arguments.len().is_multiple_of(2) {
        return Err("options must be --name value pairs".to_owned());
    }
    let mut options = BTreeMap::new();
    for pair in arguments.chunks_exact(2) {
        let key = pair[0]
            .strip_prefix("--")
            .ok_or_else(|| format!("invalid option: {}", pair[0]))?;
        if options.insert(key.to_owned(), pair[1].clone()).is_some() {
            return Err(format!("duplicate option:{key}"));
        }
    }
    Ok(options)
}

fn required_path(options: &BTreeMap<String, String>, key: &str) -> Result<PathBuf, String> {
    options
        .get(key)
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing option:--{key}"))
}

fn option(options: &BTreeMap<String, String>, key: &str, default: &str) -> String {
    options
        .get(key)
        .cloned()
        .unwrap_or_else(|| default.to_owned())
}

fn parse_u32(options: &BTreeMap<String, String>, key: &str, default: u32) -> Result<u32, String> {
    options
        .get(key)
        .map(|value| value.parse::<u32>().map_err(|_| format!("invalid --{key}")))
        .transpose()
        .map(|value| value.unwrap_or(default))
}

fn read_utf8(path: PathBuf) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| error.to_string())
}

fn write_file(path: PathBuf, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(path, bytes).map_err(|error| error.to_string())
}
