//! Full P7P7 product CLI measurements and exact-artifact CI diagnostics.
//! Links only the manager's small dependencies, CTK3 and Core Domain, not App.
//! `inspect` and the unit tests never start a search. Run only an exact,
//! externally verified canonical CI/release CLI, under clearra-manage.

use clearra_core_domain::runtime_cpu_capacity::CpuCapacity;
use clearra_ctk3::{decode_ctk3_exact, Ctk3Color};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[path = "discord-p7p4-validate.rs"]
mod discord_p7p4;

const SOURCE: &str = "ctk3_w0kCoLsawgRDCgEBkPgHQBoAAIAEAOBvB0B6uw2QbjtJYqllLLVkbD7bvh8yLw";
const BASE: [u64; 4] = [0x0000_00c0_383f_3fc7, 0, 0, 0];
const TARGET: [u64; 4] = [0xfe3f_ff3f_c7c0_c038, 0x0000_000c_120f_c1fc, 0, 0];
const PATTERNS: u64 = 25_401_600;
const DRAWS: u64 = 14;
const SOLUTIONS: usize = 570;
const KEYS_SHA256: &str = "ec9ed9757ab5fbf725dbbecb1ef2c54741dca1687551d5acf6b6fef3f3684930";
const COVERAGE_SHA256: &str = "f8efc3f4e1a2947be585e7d04f3d1d03b7f3ed7a7e66a061e58cc6fd2b1abcb4";
const LOCAL_CONTROLS: &[&str] = &[
    "CLEARRA_EXTENDED_STANDARD_BAG_COVERAGE",
    "CLEARRA_EXTENDED_REACHABILITY_PREFILTER",
    "CLEARRA_EXTENDED_HARDDROP_PROOF",
    "CLEARRA_TRACE_MEMORY_ADMISSION",
    "CLEARRA_EXTENDED_FULL_DRAW_MULTISET",
    "CLEARRA_STANDARD_BAG_ABSORB",
];

fn main() {
    if let Err(error) = entry() {
        eprintln!("oracle_cli_bench: {error}");
        std::process::exit(1);
    }
}

fn entry() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let mode = args.next().ok_or_else(usage)?;
    if matches!(
        mode.as_str(),
        "prepare" | "abba" | "discord-prepare" | "discord-validate" | "receipts"
    ) {
        if args.next().is_some() {
            return Err("CI modes take only their declared diagnostic environment inputs".into());
        }
        return match mode.as_str() {
            "prepare" => prepare_diagnostic(),
            "abba" => run_abba(),
            "discord-prepare" => prepare_discord(),
            "discord-validate" => validate_discord(),
            "receipts" => collect_supervisor_receipts(),
            _ => unreachable!(),
        };
    }
    let mut cli = None;
    let mut expected_source = None;
    let mut expected_binary = None;
    let mut out = None;
    let mut input = None;
    let mut workers = vec![1, 2, 4, 8];
    let mut repeats = 1;
    while let Some(option) = args.next() {
        let value = args.next().ok_or_else(usage)?;
        match option.as_str() {
            "--cli" => cli = Some(PathBuf::from(value)),
            "--expected-source" => expected_source = Some(value),
            "--expected-binary-sha256" => expected_binary = Some(value.to_ascii_lowercase()),
            "--out-dir" => out = Some(PathBuf::from(value)),
            "--input" => input = Some(PathBuf::from(value)),
            "--workers" => {
                workers = value
                    .split(',')
                    .map(|value| {
                        value
                            .parse::<usize>()
                            .ok()
                            .filter(|value| *value > 0)
                            .ok_or_else(|| {
                                "--workers requires positive comma-separated integers".to_owned()
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if workers.iter().collect::<BTreeSet<_>>().len() != workers.len() {
                    return Err(
                        "duplicate worker counts are not a repetition policy; use --repeats".into(),
                    );
                }
            }
            "--repeats" => {
                repeats = value
                    .parse::<usize>()
                    .ok()
                    .filter(|n| *n > 0)
                    .ok_or("--repeats requires a positive integer")?
            }
            _ => return Err(usage()),
        }
    }
    let fixture = fixture()?;
    match mode.as_str() {
        "inspect" => {
            println!("{}", serde_json::to_string_pretty(&json!({
                "schema": "clearra.oracle-cli-plan.v1", "fixture": fixture,
                "cpu": cpu(),
                "commands": workers.iter().map(|n| json!({"workers": n, "argv": cli_args(*n)})).collect::<Vec<_>>(),
                "measurement": "Instant from child spawn through child exit; full CLI preparation, search, postprocess and JSON export included",
                "source_audit": source_audit(),
                "reference": reference(),
                "search_executed": false,
            })).map_err(|error| error.to_string())?);
            Ok(())
        }
        "analyze" => {
            if workers.len() != 1 {
                return Err("analyze requires exactly one --workers value".into());
            }
            let value: Value = serde_json::from_reader(BufReader::new(
                File::open(input.ok_or("analyze requires --input")?)
                    .map_err(|error| error.to_string())?,
            ))
            .map_err(|error| error.to_string())?;
            let report = analyze(
                &value,
                workers[0],
                &expected_source.ok_or("analyze requires --expected-source")?,
            );
            println!(
                "{}",
                serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
            );
            if report["accepted"] == true {
                Ok(())
            } else {
                Err("public-output verification failed".into())
            }
        }
        "run" => run(
            cli.ok_or("run requires --cli")?,
            &expected_source.ok_or("run requires --expected-source")?,
            &expected_binary.ok_or("run requires --expected-binary-sha256")?,
            out.ok_or("run requires --out-dir")?,
            &workers,
            repeats,
            fixture,
        ),
        _ => Err(usage()),
    }
}

fn usage() -> String {
    "usage: oracle-cli-bench prepare|abba|discord-prepare|discord-validate|receipts (CI environment) | inspect [--workers 1,2,4,8] | run --cli ABSOLUTE_PATH --expected-source FULL_COMMIT --expected-binary-sha256 HEX --out-dir NEW_DIRECTORY [--workers 1,2,4,8] [--repeats N] | analyze --input CLI_JSON --workers N --expected-source FULL_COMMIT".into()
}

fn cli_args(workers: usize) -> Vec<String> {
    [
        "--format",
        "json",
        "--include-solution-data",
        "build-probability",
        "--base-mask",
        &board_hex(BASE),
        "--target-mask",
        &board_hex(TARGET),
        "--height",
        "10",
        "--patterns",
        "P7P7",
        "--source-pieces",
        "14",
        "--hold",
        "empty",
        "--queue-knowledge",
        "oracle",
        "--aggregate",
        "buildability",
        "--result-mode",
        "all-solutions",
        "--rule",
        "srs-plus",
        "--no-mirror",
        "--backend",
        "cpu",
        "--workers",
        &workers.to_string(),
        "--use-all-cpu-threads",
        "--no-backend-fallback",
        "--no-legal-board",
        "--no-conditioned-reachability",
        "--no-build-dependency-dag",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}

fn board_hex(words: [u64; 4]) -> String {
    let full: String = words
        .iter()
        .rev()
        .map(|word| format!("{word:016x}"))
        .collect();
    let significant = full.trim_start_matches('0');
    format!(
        "0x{}",
        if significant.is_empty() {
            "0"
        } else {
            significant
        }
    )
}

fn fixture() -> Result<Value, String> {
    let document = decode_ctk3_exact(SOURCE).map_err(|error| format!("CTK3 decode: {error:?}"))?;
    if document.width != 10 || document.pages.len() != 1 {
        return Err("fixture width/page count changed".into());
    }
    let page = &document.pages[0];
    if page.height != 10 || page.operation.is_some() || page.garbage.is_some() {
        return Err("fixture page shape changed".into());
    }
    let mut base = [0u64; 4];
    let mut target = [0u64; 4];
    for (index, color) in page.cells.iter().enumerate() {
        if index >= 256 {
            return Err("fixture exceeds Board256".into());
        }
        let bit = 1u64 << (index % 64);
        match color {
            Ctk3Color::Empty => {}
            Ctk3Color::Gray => base[index / 64] |= bit,
            Ctk3Color::Piece(_) => target[index / 64] |= bit,
        }
    }
    let cells = |words: [u64; 4]| words.iter().map(|word| word.count_ones()).sum::<u32>();
    if base != BASE || target != TARGET || cells(base) != 22 || cells(target) != 56 {
        return Err("decoded fixture differs from pinned oracle_ctk3_bench.rs".into());
    }
    Ok(json!({
        "ctk3": SOURCE, "width": 10, "height": 10,
        "base_mask": board_hex(base), "base_cells": cells(base),
        "target_mask": board_hex(target), "target_cells": cells(target),
        "target_locks": DRAWS, "source_draws": DRAWS, "original_patterns": PATTERNS,
        "queue": "P7P7 => Standard7Bag", "initial_hold": "empty", "hold_enabled": true,
        "knowledge": "FullQueueOracle", "count_policy": "CountUnique", "rule": "SRS+",
        "aggregation": "Buildability", "colored_identity_filter": false, "horizontal_mirror": false,
        "projects_unplaced_lookahead": true, "projects_standard_bag_lookahead": true,
    }))
}

fn cpu() -> Value {
    let capacity = CpuCapacity::current();
    json!({
        "hard_limit": capacity.hard_limit(),
        "recommended_parallelism": capacity.recommended_parallelism(),
        "affinity_limit": capacity.affinity_limit(), "expected_vcpus": capacity.expected_vcpus(),
    })
}

fn reference() -> Value {
    json!({
        "source_commit": "91f558ed7ea1689ad62587aa08a5d7e5b5c7db15",
        "original_pattern_count": PATTERNS, "solution_count": SOLUTIONS,
        "keys_sha256": KEYS_SHA256,
        "key_encoding": "ordered-u64-le-byte-length-prefixed-utf8",
        "coverage_sha256": COVERAGE_SHA256,
        "coverage_encoding": "little-endian-u64-original-pattern-order",
    })
}

fn source_audit() -> Value {
    json!({
        "fixture": "crates/clearra-app/examples/oracle_ctk3_bench.rs",
        "cli_parser": "crates/clearra-cli-command/src/web_command_parser.rs::parse_build_probability_command",
        "bag_normalization": "crates/clearra-cli-command/src/web_command_request.rs::standard_7_bag_with_optional_leading_piece",
        "input_assembly": "crates/clearra-cli-command/src/web_build_probability_input.rs::to_query",
        "queue_and_locks": "P7P7 normalizes to Standard7Bag; explicit --source-pieces 14 keeps all original draws; target 56 cells means 14 locks",
        "unlimited_defaults": "max-patterns, max-candidates and max-nodes omitted; current product defaults are zero/unlimited; --max-patterns 0 is not accepted by the CLI parser",
        "transport_difference": "example ObjectivePolicy::All/retained_trace_limit=0; CLI ObjectivePolicy::Unique/retained_trace_limit=1. CountUnique and Buildability unchanged. Extended Build solver does not use this objective kind or retained trace limit as a search restriction; verify actual keys independently",
        "output": "--include-solution-data exposes ordered normalized keys in contract.artifacts.solution_keys, without requesting expensive per-solution probabilities",
        "coverage_limit": "public CLI JSON and solution-set artifacts do not expose coverage words. Verify complete/count/probability evidence, but do not claim a measured bitmap SHA match",
    })
}

fn bool_value(value: &Value) -> Option<bool> {
    value.as_bool().or_else(|| match value.as_str()? {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    })
}
fn u64_value(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}
fn f64_value(value: &Value) -> Option<f64> {
    value.as_f64().or_else(|| value.as_str()?.parse().ok())
}
fn key_hash(keys: &[String]) -> String {
    let mut digest = Sha256::new();
    for key in keys {
        digest.update((key.len() as u64).to_le_bytes());
        digest.update(key.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

fn analyze(value: &Value, workers: usize, expected_source: &str) -> Value {
    let summary = &value["summary"];
    let mut errors = Vec::<String>::new();
    let mut require = |condition: bool, message: &str| {
        if !condition {
            errors.push(message.into());
        }
    };
    require(
        value["runtime_identity"]["source_commit"].as_str() == Some(expected_source),
        "runtime source commit differs from verified canonical asset",
    );
    for key in [
        "count_complete",
        "probability_complete",
        "projects_unplaced_lookahead",
        "projects_standard_bag_lookahead",
    ] {
        require(
            bool_value(&summary[key]) == Some(true),
            &format!("{key} must be true"),
        );
    }
    for key in [
        "coverage_pattern_count",
        "total_possible_pattern_count",
        "materialized_pattern_count",
        "covered_pattern_count",
    ] {
        require(
            u64_value(&summary[key]) == Some(PATTERNS),
            &format!("{key} must equal {PATTERNS}"),
        );
    }
    require(
        u64_value(&summary["failed_pattern_count"]) == Some(0),
        "failed_pattern_count must equal zero",
    );
    require(
        u64_value(&summary["source_sequence_length"]) == Some(DRAWS),
        "source_sequence_length must remain fourteen draws",
    );
    require(
        u64_value(&summary["target_piece_count"]) == Some(DRAWS),
        "target_piece_count must remain fourteen locks",
    );
    require(
        u64_value(&summary["workers_requested"]) == Some(workers as u64),
        "workers_requested differs; no topology substitution allowed",
    );
    require(
        u64_value(&summary["workers_used"]) == Some(workers as u64),
        "workers_used differs; no topology substitution allowed",
    );
    require(
        bool_value(&summary["cpu_parallel_execution"]) == Some(workers > 1),
        "cpu_parallel_execution does not match requested worker count",
    );
    require(
        u64_value(&summary["unique_solution_count"]) == Some(SOLUTIONS as u64),
        "unique_solution_count must equal 570",
    );
    require(
        f64_value(&summary["coverage_probability"]) == Some(1.0),
        "coverage_probability must equal one",
    );
    require(
        value["contract"]["solution_data"]["status"].as_str() == Some("complete"),
        "exported solution-data status must be complete",
    );
    let keys: Option<Vec<String>> = value["contract"]["artifacts"]["solution_keys"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .map(|key| key.as_str().map(str::to_owned))
                .collect()
        });
    let hash = keys.as_ref().map(|keys| key_hash(keys));
    require(
        keys.as_ref().map(Vec::len) == Some(SOLUTIONS),
        "exactly 570 complete normalized keys must be exported",
    );
    require(
        hash.as_deref() == Some(KEYS_SHA256),
        "ordered normalized-key SHA256 differs from original fixture",
    );
    if let Some(keys) = &keys {
        require(
            keys.iter().collect::<BTreeSet<_>>().len() == keys.len(),
            "exported normalized keys contain duplicates",
        );
    }
    for key in ["truncated", "budget_exceeded", "backend_fallback_used"] {
        if !summary[key].is_null() {
            require(
                bool_value(&summary[key]) == Some(false),
                &format!("{key} is true or invalid"),
            );
        }
    }
    json!({
        "accepted": errors.is_empty(), "errors": errors,
        "runtime_identity": value["runtime_identity"],
        "workers": workers, "summary": summary,
        "resource_report": value["resource_report"],
        "solutions": {"key_count": keys.as_ref().map(Vec::len), "keys_sha256": hash,
            "reference_keys_match": hash.as_deref() == Some(KEYS_SHA256)},
        "coverage": {"observed_bitmap_sha256": Value::Null,
            "bitmap_hash_verified": false, "reference_bitmap_sha256": COVERAGE_SHA256,
            "complete_count_evidence_matches_full_reference": bool_value(&summary["count_complete"]) == Some(true)
                && bool_value(&summary["probability_complete"]) == Some(true)
                && u64_value(&summary["covered_pattern_count"]) == Some(PATTERNS)
                && u64_value(&summary["coverage_pattern_count"]) == Some(PATTERNS),
            "verification_limit": "coverage bitmap is not exposed by the public CLI; no computed bitmap hash is claimed"},
    })
}

fn file_hash(path: &Path) -> Result<String, String> {
    let mut input = BufReader::new(File::open(path).map_err(|error| error.to_string())?);
    let mut digest = Sha256::new();
    let mut bytes = [0u8; 65_536];
    loop {
        let count = input.read(&mut bytes).map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&bytes[..count]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn create_json(path: &Path, value: &Value) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::to_writer_pretty(&mut file, value).map_err(|error| error.to_string())?;
    file.write_all(b"\n").map_err(|error| error.to_string())
}

fn run(
    cli: PathBuf,
    expected_source: &str,
    expected_binary: &str,
    out: PathBuf,
    workers: &[usize],
    repeats: usize,
    fixture: Value,
) -> Result<(), String> {
    if expected_source.len() != 40 || !expected_source.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(
            "run requires the full canonical source commit, not a branch or abbreviated SHA".into(),
        );
    }
    if expected_binary.len() != 64 || !expected_binary.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("run requires the verified canonical CLI binary SHA256".into());
    }
    if !cli.is_absolute() {
        return Err("--cli must be an absolute path".into());
    }
    let binary = file_hash(&cli)?;
    if binary != expected_binary {
        return Err("CLI binary SHA256 does not match the supplied canonical asset digest; search not started".into());
    }
    let capacity = CpuCapacity::current();
    if workers
        .iter()
        .any(|workers| *workers > capacity.hard_limit())
    {
        return Err(format!(
            "requested topology exceeds CPU capacity {}; no worker reduction made",
            capacity.hard_limit()
        ));
    }
    for key in LOCAL_CONTROLS {
        if std::env::var_os(key).is_some() {
            return Err(format!("diagnostic control {key} is set; canonical benchmark requires an explicit clean environment"));
        }
    }
    fs::create_dir(&out)
        .map_err(|error| format!("new output directory {}: {error}", out.display()))?;
    let mut report = json!({
        "schema": "clearra.oracle-cli-bench.v1", "fixture": fixture,
        "cli": cli, "binary_sha256": binary, "expected_source_commit": expected_source,
        "started_unix_seconds": SystemTime::now().duration_since(UNIX_EPOCH).map_err(|error| error.to_string())?.as_secs(),
        "cpu": cpu(), "source_audit": source_audit(), "reference": reference(),
        "measurement": "fresh CLI child process wall time; includes prepare/search/postprocess/JSON export, excludes harness parse/hash validation",
        "workers": workers, "repeats": repeats,
    });
    create_json(&out.join("manifest.json"), &report)?;
    let mut results = Vec::new();
    for repeat in 1..=repeats {
        for &worker_count in workers {
            let stem = format!("workers-{worker_count:02}-run-{repeat:02}");
            let stdout_path = out.join(format!("{stem}.stdout.json"));
            let stderr_path = out.join(format!("{stem}.stderr.log"));
            let stdout = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&stdout_path)
                .map_err(|error| error.to_string())?;
            let stderr = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&stderr_path)
                .map_err(|error| error.to_string())?;
            let args = cli_args(worker_count);
            eprintln!("starting full P7P7 CLI workers={worker_count} repetition={repeat}");
            let start = Instant::now();
            let status = Command::new(&cli)
                .args(&args)
                .stdin(Stdio::null())
                .stdout(Stdio::from(stdout))
                .stderr(Stdio::from(stderr))
                .status();
            let elapsed = start.elapsed().as_secs_f64();
            let exit_code = status.as_ref().ok().and_then(|status| status.code());
            let parsed = File::open(&stdout_path)
                .map_err(|error| error.to_string())
                .and_then(|file| {
                    serde_json::from_reader::<_, Value>(BufReader::new(file))
                        .map_err(|error| error.to_string())
                });
            let mut result = match parsed {
                Ok(value) => analyze(&value, worker_count, expected_source),
                Err(error) => {
                    json!({"accepted": false, "errors": [format!("CLI JSON decode: {error}")]})
                }
            };
            result["cli_wall_seconds"] = json!(elapsed);
            result["exit_code"] = json!(exit_code);
            result["argv"] = json!(args);
            result["stdout_path"] = json!(stdout_path);
            result["stderr_path"] = json!(stderr_path);
            result["repeat"] = json!(repeat);
            if !status.as_ref().is_ok_and(|status| status.success()) {
                result["accepted"] = json!(false);
                result["process_error"] = json!(status
                    .err()
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| format!("CLI exit status {exit_code:?}")));
            }
            create_json(&out.join(format!("{stem}.result.json")), &result)?;
            let accepted = result["accepted"] == true;
            eprintln!("finished workers={worker_count} repetition={repeat} cli_wall_seconds={elapsed:.6} accepted={accepted}");
            results.push(result);
            if !accepted {
                report["runs"] = json!(results);
                report["accepted"] = json!(false);
                create_json(&out.join("report.json"), &report)?;
                return Err("CLI result incomplete or mismatched; further measurements stopped and raw output retained".into());
            }
        }
    }
    let baseline = results
        .iter()
        .filter(|run| run["workers"] == 1)
        .filter_map(|run| run["cli_wall_seconds"].as_f64())
        .collect::<Vec<_>>();
    let baseline_mean =
        (!baseline.is_empty()).then(|| baseline.iter().sum::<f64>() / baseline.len() as f64);
    let measurements = workers.iter().map(|&workers| {
        let times = results.iter().filter(|run| run["workers"] == workers).filter_map(|run| run["cli_wall_seconds"].as_f64()).collect::<Vec<_>>();
        let mean = times.iter().sum::<f64>() / times.len() as f64;
        json!({"workers": workers, "samples": times, "mean_seconds": mean,
            "speedup_vs_one_worker_mean": baseline_mean.map(|baseline| baseline / mean),
            "efficiency_vs_one_worker_mean": baseline_mean.map(|baseline| baseline / mean / workers as f64)})
    }).collect::<Vec<_>>();
    report["runs"] = json!(results);
    report["measurements"] = json!(measurements);
    report["accepted"] = json!(true);
    create_json(&out.join("report.json"), &report)?;
    println!(
        "{}",
        json!({"accepted": true, "report": out.join("report.json"), "measurements": measurements})
    );
    Ok(())
}

fn required_env(name: &str) -> Result<String, String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{name} is required"))
}

fn lowercase_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn positive_decimal(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('0')
        && value.bytes().all(|b| b.is_ascii_digit())
        && value.parse::<u64>().is_ok()
}

fn diagnostic_context() -> Result<Value, String> {
    let source = required_env("GITHUB_SHA")?;
    let repository = required_env("GITHUB_REPOSITORY")?;
    let run = required_env("GITHUB_RUN_ID")?;
    let attempt = required_env("GITHUB_RUN_ATTEMPT")?;
    if !lowercase_hex(&source, 40)
        || repository != "daejunnom/Clearra"
        || !positive_decimal(&run)
        || !positive_decimal(&attempt)
        || required_env("GITHUB_REF")? != "refs/heads/main"
        || required_env("GITHUB_EVENT_NAME")? != "workflow_dispatch"
        || required_env("CANDIDATE_SOURCE")? != source
    {
        return Err(
            "diagnostic must bind an exact candidate source to its main workflow dispatch".into(),
        );
    }
    Ok(
        json!({"repository":repository,"source_commit":source,"run_id":run,"run_attempt":attempt,
        "runner_os":required_env("RUNNER_OS")?,"runner_arch":required_env("RUNNER_ARCH")?,
        "runner_name":required_env("RUNNER_NAME")?}),
    )
}

fn diagnostic_root() -> Result<PathBuf, String> {
    let root = PathBuf::from(required_env("CLEARRA_DIAGNOSTIC_ROOT")?);
    if !root.is_absolute() {
        return Err("diagnostic root must be an admitted absolute path".into());
    }
    Ok(root)
}

fn diagnostic_cpu() -> Result<(Value, Vec<usize>), String> {
    if std::env::var_os("CLEARRA_EXPECTED_VCPUS").is_some() {
        return Err(
            "CPU capacity overrides are forbidden for this observed-runner diagnostic".into(),
        );
    }
    let capacity = CpuCapacity::current();
    Ok((cpu(), diagnostic_widths(capacity.hard_limit())))
}

fn diagnostic_widths(hard_limit: usize) -> Vec<usize> {
    [1, 2, hard_limit]
        .into_iter()
        .filter(|width| *width > 0 && *width <= hard_limit)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn read_json(path: &Path, maximum: u64) -> Result<Value, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > maximum {
        return Err(format!(
            "{} is not a bounded regular JSON file",
            path.display()
        ));
    }
    serde_json::from_reader(BufReader::new(
        File::open(path).map_err(|error| error.to_string())?,
    ))
    .map_err(|error| format!("{}: {error}", path.display()))
}

fn recorded_command(command: &mut Command, stdout: &Path, stderr: &Path) -> Result<bool, String> {
    let create = |path: &Path| {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|error| format!("{}: {error}", path.display()))
    };
    command
        .stdin(Stdio::null())
        .stdout(create(stdout)?)
        .stderr(create(stderr)?)
        .status()
        .map(|status| status.success())
        .map_err(|error| error.to_string())
}

fn github_file(endpoint: &str, path: &Path) -> Result<(), String> {
    // gh owns authenticated API transport and strips credentials on its storage
    // redirect. Never print response headers or enable its HTTP debug output.
    let mut command = Command::new("gh");
    command
        .args([
            "api",
            "--hostname",
            "github.com",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "X-GitHub-Api-Version: 2022-11-28",
            endpoint,
        ])
        .env_remove("GH_DEBUG");
    let stderr = path.with_file_name(format!(
        "{}.stderr.log",
        path.file_name()
            .ok_or("missing response filename")?
            .to_string_lossy()
    ));
    if !recorded_command(&mut command, path, &stderr)? {
        return Err(
            "exact GitHub artifact read failed; no retry or substitute input was attempted".into(),
        );
    }
    Ok(())
}

fn validate_linux_artifact(
    source: &str,
    run_id: &str,
    artifact_id: &str,
    run: &Value,
    artifact: &Value,
    jobs: &Value,
) -> Result<String, String> {
    if !lowercase_hex(source, 40) || !positive_decimal(run_id) || !positive_decimal(artifact_id) {
        return Err("canonical inputs require an exact source SHA and decimal IDs".into());
    }
    let exact_run_id = run_id.parse::<u64>().map_err(|error| error.to_string())?;
    let exact_artifact_id = artifact_id
        .parse::<u64>()
        .map_err(|error| error.to_string())?;
    let attempt = run["run_attempt"]
        .as_u64()
        .filter(|value| *value > 0)
        .ok_or("missing run attempt")?;
    if run["id"] != exact_run_id
        || run["head_sha"] != source
        || run["head_branch"] != "main"
        || run["event"] != "workflow_dispatch"
        || run["path"] != ".github/workflows/release-cli.yml"
        || run["repository"]["full_name"] != "daejunnom/Clearra"
        || run["head_repository"]["full_name"] != "daejunnom/Clearra"
        || !(run["status"] == "completed" && run["conclusion"] == "success"
            || run["status"] == "in_progress" && run["conclusion"].is_null())
    {
        return Err("input run is not the exact canonical main source/attempt".into());
    }
    let all_jobs = jobs["jobs"].as_array().ok_or("missing canonical jobs")?;
    if all_jobs.len() > 100 || jobs["total_count"].as_u64() != Some(all_jobs.len() as u64) {
        return Err("canonical jobs readback is not one complete page".into());
    }
    let linux = all_jobs
        .iter()
        .filter(|job| job["name"] == "linux-cli")
        .collect::<Vec<_>>();
    if linux.len() != 1
        || linux[0]["status"] != "completed"
        || linux[0]["conclusion"] != "success"
        || linux[0]["run_id"] != exact_run_id
        || linux[0]["run_attempt"] != attempt
        || linux[0]["head_sha"] != source
    {
        return Err("the exact Linux producer job has not succeeded".into());
    }
    let name = artifact["name"].as_str().ok_or("missing artifact name")?;
    let suffix = format!("-run-{run_id}-attempt-{attempt}");
    let version = name
        .strip_prefix("clearra-linux-cli-v")
        .and_then(|value| value.strip_suffix(&suffix))
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 80
                && value.as_bytes()[0].is_ascii_digit()
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b".+-".contains(&b))
        })
        .ok_or("artifact name does not bind its Linux producer run attempt")?;
    let digest = artifact["digest"]
        .as_str()
        .and_then(|value| value.strip_prefix("sha256:"));
    if artifact["id"] != exact_artifact_id || artifact["expired"] != false ||
        !artifact["size_in_bytes"].as_u64().is_some_and(|size| size > 0 && size <= 512 * 1024 * 1024) ||
        !digest.is_some_and(|digest| lowercase_hex(digest, 64)) ||
        artifact["workflow_run"]["id"] != exact_run_id || artifact["workflow_run"]["head_sha"] != source ||
        artifact["workflow_run"]["head_branch"] != "main" ||
        artifact["archive_download_url"] != format!("https://api.github.com/repos/daejunnom/Clearra/actions/artifacts/{artifact_id}/zip") {
        return Err("artifact metadata does not match the exact canonical source/run/ID/digest".into());
    }
    Ok(format!("Clearra-CLI-v{version}-linux-x86_64"))
}

fn prepare_input(root: &Path, arm: &str, context: &Value) -> Result<Value, String> {
    let prefix = arm.to_ascii_uppercase();
    let source = required_env(&format!("{prefix}_SOURCE"))?;
    let run_id = required_env(&format!("{prefix}_RUN"))?;
    let artifact_id = required_env(&format!("{prefix}_ARTIFACT_ID"))?;
    if !lowercase_hex(&source, 40) || !positive_decimal(&run_id) || !positive_decimal(&artifact_id)
    {
        return Err(format!(
            "{arm} inputs must be exact immutable source/run/artifact identities"
        ));
    }
    let directory = root.join(arm);
    fs::create_dir(&directory).map_err(|error| error.to_string())?;
    let base = "/repos/daejunnom/Clearra";
    github_file(
        &format!("{base}/actions/runs/{run_id}"),
        &directory.join("run.json"),
    )?;
    let run = read_json(&directory.join("run.json"), 8 * 1024 * 1024)?;
    let attempt = run["run_attempt"]
        .as_u64()
        .filter(|value| *value > 0)
        .ok_or("missing canonical attempt")?;
    github_file(
        &format!("{base}/actions/runs/{run_id}/attempts/{attempt}/jobs?per_page=100"),
        &directory.join("jobs.json"),
    )?;
    github_file(
        &format!("{base}/actions/artifacts/{artifact_id}"),
        &directory.join("artifact.json"),
    )?;
    let artifact = read_json(&directory.join("artifact.json"), 8 * 1024 * 1024)?;
    let jobs = read_json(&directory.join("jobs.json"), 8 * 1024 * 1024)?;
    let filename = validate_linux_artifact(&source, &run_id, &artifact_id, &run, &artifact, &jobs)?;
    let archive = directory.join("artifact.zip");
    github_file(
        &format!("{base}/actions/artifacts/{artifact_id}/zip"),
        &archive,
    )?;
    if fs::metadata(&archive)
        .map_err(|error| error.to_string())?
        .len()
        != artifact["size_in_bytes"].as_u64().unwrap()
        || format!("sha256:{}", file_hash(&archive)?) != artifact["digest"].as_str().unwrap()
    {
        return Err(
            "canonical artifact ZIP size/digest mismatch; no executable was started".into(),
        );
    }
    let listing = directory.join("archive-members.txt");
    if !recorded_command(
        Command::new("unzip").args(["-Z", "-1"]).arg(&archive),
        &listing,
        &directory.join("archive-list.stderr.log"),
    )? || fs::metadata(&listing)
        .map_err(|error| error.to_string())?
        .len()
        > 4096
        || fs::read_to_string(&listing).map_err(|error| error.to_string())?
            != format!("{filename}\n")
    {
        return Err("canonical artifact ZIP must contain exactly the one flat Linux binary".into());
    }
    let binary = directory.join(&filename);
    if !recorded_command(
        Command::new("unzip").arg("-p").arg(&archive).arg(&filename),
        &binary,
        &directory.join("archive-extract.stderr.log"),
    )? {
        return Err("canonical artifact ZIP extraction or CRC check failed".into());
    }
    let binary_metadata = fs::symlink_metadata(&binary).map_err(|error| error.to_string())?;
    if !binary_metadata.is_file()
        || binary_metadata.len() == 0
        || binary_metadata.len() > 512 * 1024 * 1024
    {
        return Err("canonical Linux executable is not a bounded regular file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
    }
    let binary_sha256 = file_hash(&binary)?;
    if !recorded_command(
        Command::new(&binary)
            .args(["--format", "json", "rules", "list"])
            .env_remove("GH_TOKEN")
            .env_remove("GITHUB_TOKEN"),
        &directory.join("runtime-probe.json"),
        &directory.join("runtime-probe.stderr.log"),
    )? {
        return Err("canonical executable runtime identity probe failed".into());
    }
    let probe = read_json(&directory.join("runtime-probe.json"), 8 * 1024 * 1024)?;
    for (field, expected) in [
        ("source_commit", source.as_str()),
        ("engine_build_id", source.as_str()),
        ("contract_schema_version", "clearra.search.contract.v2"),
        (
            "supply_semantics_id",
            "clearra.supply.projected-terminal-lookahead.v1",
        ),
        ("artifact_schema_version", "clearra.solution-data.v1"),
    ] {
        if probe["runtime_identity"][field] != expected {
            return Err(format!("runtime identity {field} mismatch"));
        }
    }
    if file_hash(&binary)? != binary_sha256 {
        return Err("Linux binary changed during its identity probe".into());
    }
    let receipt = json!({"schema":"clearra.canonical-cli-diagnostic-input.v1", "diagnostic_only":true,
        "diagnostic":context,"source_commit":source,"canonical_run_id":run_id,"canonical_run_attempt":attempt,
        "canonical_run_status":run["status"],"canonical_run_conclusion":run["conclusion"],
        "canonical_complete_success":run["status"] == "completed" && run["conclusion"] == "success",
        "authority":"diagnostic input only; successful Linux producer does not replace complete canonical acceptance",
        "artifact_id":artifact_id,"artifact_name":artifact["name"],"artifact_digest":artifact["digest"],
        "artifact_size_bytes":artifact["size_in_bytes"],"binary_path":binary,"binary_sha256":binary_sha256,
        "runtime_identity":probe["runtime_identity"]});
    create_json(&directory.join("receipt.json"), &receipt)?;
    Ok(receipt)
}

fn prepare_diagnostic() -> Result<(), String> {
    let context = diagnostic_context()?;
    let root = diagnostic_root()?;
    let (capacity, widths) = diagnostic_cpu()?;
    if required_env("BASELINE_SOURCE")? == required_env("CANDIDATE_SOURCE")? {
        return Err("ABBA requires distinct baseline and candidate source commits".into());
    }
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|error| error.to_string())?;
    if !head.status.success()
        || String::from_utf8_lossy(&head.stdout).trim() != required_env("CANDIDATE_SOURCE")?
    {
        return Err("adapter/harness checkout differs from the candidate source".into());
    }
    fs::create_dir_all(root.parent().ok_or("missing output parent")?)
        .map_err(|error| error.to_string())?;
    fs::create_dir(&root).map_err(|error| error.to_string())?;
    create_json(
        &root.join("diagnostic-plan.json"),
        &json!({"schema":"clearra.native-cli-diagnostic-plan.v1",
        "diagnostic_only":true,"diagnostic":context,"cpu":capacity,"workers":widths,
        "worker_policy":"distinct 1/2/N values within the observed CpuCapacity hard limit; no override or downsize",
        "baseline":{"source":required_env("BASELINE_SOURCE")?,"run":required_env("BASELINE_RUN")?,"artifact_id":required_env("BASELINE_ARTIFACT_ID")?},
        "candidate":{"source":required_env("CANDIDATE_SOURCE")?,"run":required_env("CANDIDATE_RUN")?,"artifact_id":required_env("CANDIDATE_ARTIFACT_ID")?},
        "p7p7_order":["baseline","candidate","candidate","baseline"],
        "scope":"same-runner diagnostic only; no publication, gate replacement or Cloud Run 8-vCPU performance claim"}),
    )?;
    for arm in ["baseline", "candidate"] {
        prepare_input(&root, arm, &context)?;
    }
    println!(
        "{}",
        json!({"prepared":true,"root":root,"cpu":capacity,"workers":widths})
    );
    Ok(())
}

fn verified_input(root: &Path, arm: &str, context: &Value) -> Result<Value, String> {
    let receipt = read_json(&root.join(arm).join("receipt.json"), 1024 * 1024)?;
    let prefix = arm.to_ascii_uppercase();
    if receipt["schema"] != "clearra.canonical-cli-diagnostic-input.v1"
        || receipt["diagnostic"] != *context
        || receipt["source_commit"] != required_env(&format!("{prefix}_SOURCE"))?
        || receipt["canonical_run_id"] != required_env(&format!("{prefix}_RUN"))?
        || receipt["artifact_id"] != required_env(&format!("{prefix}_ARTIFACT_ID"))?
        || receipt["diagnostic_only"] != true
    {
        return Err(
            "prepared input receipt does not bind this diagnostic run and exact input".into(),
        );
    }
    let binary = Path::new(
        receipt["binary_path"]
            .as_str()
            .ok_or("missing binary path")?,
    );
    if !binary.is_absolute()
        || binary.parent() != Some(root.join(arm).as_path())
        || file_hash(binary)?
            != receipt["binary_sha256"]
                .as_str()
                .ok_or("missing binary digest")?
    {
        return Err("prepared canonical binary path/digest changed".into());
    }
    Ok(receipt)
}

fn run_abba() -> Result<(), String> {
    let context = diagnostic_context()?;
    let root = diagnostic_root()?;
    let (capacity, widths) = diagnostic_cpu()?;
    let inputs = [
        verified_input(&root, "baseline", &context)?,
        verified_input(&root, "candidate", &context)?,
    ];
    let directory = root.join("p7p7");
    fs::create_dir(&directory).map_err(|error| error.to_string())?;
    create_json(
        &directory.join("plan.json"),
        &json!({"diagnostic":context,"diagnostic_only":true,
        "cpu":capacity,"workers":widths,"inputs":inputs,"order":["baseline","candidate","candidate","baseline"],"fixture":fixture()?}),
    )?;
    let mut runs = Vec::new();
    for &width in &widths {
        for (slot, arm) in [0usize, 1, 1, 0].into_iter().enumerate() {
            let label = ["baseline", "candidate"][arm];
            let input = &inputs[arm];
            if diagnostic_cpu()?.0 != capacity {
                return Err("runner CPU capacity changed during ABBA".into());
            }
            let output = directory.join(format!("workers-{width:02}-{}-{label}", slot + 1));
            let outcome = run(
                PathBuf::from(input["binary_path"].as_str().unwrap()),
                input["source_commit"].as_str().unwrap(),
                input["binary_sha256"].as_str().unwrap(),
                output.clone(),
                &[width],
                1,
                fixture()?,
            );
            let report = read_json(&output.join("report.json"), 32 * 1024 * 1024).ok();
            runs.push(json!({"arm":label,"workers":width,"abba_position":slot + 1,"report_path":output.join("report.json"),
                "accepted":outcome.is_ok(),"cli_wall_seconds":report.as_ref().and_then(|r| r["runs"][0]["cli_wall_seconds"].as_f64()),
                "error":outcome.as_ref().err()}));
            create_json(
                &directory.join(format!("completed-{:02}.json", runs.len())),
                &json!({"diagnostic":context,"diagnostic_only":true,"cpu":capacity,"runs":runs}),
            )?;
            if let Err(error) = outcome {
                create_json(
                    &directory.join("report.json"),
                    &json!({"diagnostic":context,"diagnostic_only":true,
                    "accepted":false,"cpu":capacity,"runs":runs,"error":error,"retry_performed":false}),
                )?;
                return Err(
                    "ABBA stopped at its first failed or incomplete sample; all outputs retained"
                        .into(),
                );
            }
        }
    }
    let mut means = BTreeMap::new();
    for arm in ["baseline", "candidate"] {
        for &width in &widths {
            let times = runs
                .iter()
                .filter(|r| r["arm"] == arm && r["workers"] == width)
                .filter_map(|r| r["cli_wall_seconds"].as_f64())
                .collect::<Vec<_>>();
            if times.len() != 2 {
                return Err("ABBA sample count mismatch".into());
            }
            means.insert((arm, width), times.iter().sum::<f64>() / 2.0);
        }
    }
    let comparisons = widths.iter().map(|&width| json!({"workers":width,
        "baseline_mean_seconds":means[&("baseline", width)],"candidate_mean_seconds":means[&("candidate", width)],
        "candidate_speedup_vs_baseline":means[&("baseline", width)] / means[&("candidate", width)],
        "baseline_speedup_vs_one_worker":means[&("baseline", 1)] / means[&("baseline", width)],
        "candidate_speedup_vs_one_worker":means[&("candidate", 1)] / means[&("candidate", width)]})).collect::<Vec<_>>();
    create_json(
        &directory.join("report.json"),
        &json!({"schema":"clearra.native-cli-abba.v1","diagnostic_only":true,
        "diagnostic":context,"accepted":true,"cpu":capacity,"runs":runs,"comparisons":comparisons,
        "scope":"one same-runner ABBA block per observed width; no statistical significance or Cloud Run topology claim"}),
    )?;
    println!("{}", json!({"accepted":true,"comparisons":comparisons}));
    Ok(())
}

fn prepare_discord() -> Result<(), String> {
    let context = diagnostic_context()?;
    let root = diagnostic_root()?;
    let (capacity, _) = diagnostic_cpu()?;
    let workers = CpuCapacity::current().hard_limit();
    let input = verified_input(&root, "candidate", &context)?;
    let directory = root.join("discord-p7p4");
    fs::create_dir(&directory).map_err(|error| error.to_string())?;
    create_json(
        &directory.join("plan.json"),
        &json!({"diagnostic":context,"diagnostic_only":true,
        "cpu":capacity,"workers":workers,"input":input,"deadline_ms":14 * 60_000,
        "request_mode":"path","capability_id":"pc.path","count_policy":"CountAll","target_lines":4,
        "stdout_stderr_limit_bytes":4 * 1024 * 1024,"artifact_limit_bytes":24 * 1024 * 1024,
        "scope":"actual product adapter at observed CI width; no Discord service/deployment/Cloud Run performance assertion"}),
    )?;
    let binary = input["binary_path"].as_str().unwrap();
    if binary.contains(['\r', '\n']) {
        return Err("GitHub output path must occupy one line".into());
    }
    let mut output = OpenOptions::new()
        .append(true)
        .open(required_env("GITHUB_OUTPUT")?)
        .map_err(|error| error.to_string())?;
    writeln!(output, "binary={binary}\nworkers={workers}").map_err(|error| error.to_string())?;
    Ok(())
}

fn validate_discord() -> Result<(), String> {
    let context = diagnostic_context()?;
    let root = diagnostic_root()?;
    let directory = root.join("discord-p7p4");
    let plan = read_json(&directory.join("plan.json"), 1024 * 1024)?;
    if plan["diagnostic"] != context
        || plan["diagnostic_only"] != true
        || plan["request_mode"] != "path"
        || plan["capability_id"] != "pc.path"
        || plan["count_policy"] != "CountAll"
        || plan["target_lines"] != 4
    {
        return Err("Discord diagnostic plan does not bind this run".into());
    }
    let workers = plan["workers"]
        .as_u64()
        .filter(|width| *width > 0)
        .ok_or("missing observed workers")?;
    let input = verified_input(&root, "candidate", &context)?;
    let output = directory.join("runner.json");
    let process_success = required_env("CLEARRA_DIAGNOSTIC_ADAPTER_EXIT")?
        .parse::<i32>()
        .map_err(|error| error.to_string())?
        == 0;
    let validation = discord_p7p4::validate(
        output.to_str().ok_or("non-UTF8 report path")?,
        input["source_commit"].as_str().unwrap(),
        workers,
    )
    .unwrap_or_else(|error| json!({"valid":false,"errors":[error]}));
    let accepted = process_success
        && validation["valid"] == true
        && validation["schema"] == "clearra.local-discord-p7p4-validation.v2"
        && validation["fixture"]["mode"] == plan["request_mode"];
    create_json(
        &directory.join("validation.json"),
        &json!({"diagnostic":context,"diagnostic_only":true,
        "accepted":accepted,"cpu":plan["cpu"],"workers":workers,"process_success":process_success,"validation":validation}),
    )?;
    if !accepted {
        return Err(
            "actual Discord adapter result failed its bound input/result-policy validation; no retry"
                .into(),
        );
    }
    println!(
        "{}",
        json!({"accepted":true,"discord_workers":workers,"cpu":plan["cpu"]})
    );
    Ok(())
}

fn collect_supervisor_receipts() -> Result<(), String> {
    let context = diagnostic_context()?;
    let root = diagnostic_root()?;
    if !root.is_dir() {
        return Ok(());
    }
    for (phase, log, producer, profile, timeout) in [
        (
            "prepare",
            root.with_file_name(format!(
                "{}-prepare.log",
                root.file_name().unwrap().to_string_lossy()
            )),
            "canonical-cli-input-diagnostic",
            "control",
            300,
        ),
        (
            "discord",
            root.join("discord-p7p4/runner.stderr.log"),
            "discord-p7p4-diagnostic",
            "benchmark-search",
            900,
        ),
        (
            "p7p7",
            root.join("p7p7-manager.log"),
            "oracle-cli-p7p7-diagnostic",
            "benchmark-search",
            7200,
        ),
    ] {
        if !log.exists() {
            continue;
        }
        let text = fs::read_to_string(&log).map_err(|error| error.to_string())?;
        let paths = text
            .lines()
            .filter_map(|line| line.strip_prefix("clearra_runtime_receipt="))
            .collect::<Vec<_>>();
        // Admission can fail before a runtime owner exists. Retain that log
        // without fabricating a receipt or retrying the rejected invocation.
        if paths.is_empty() {
            continue;
        }
        if paths.len() != 1 {
            return Err("supervisor log has ambiguous receipt identity".into());
        }
        let receipt = read_json(Path::new(paths[0]), 16 * 1024 * 1024)?;
        if receipt["schema_id"] != "clearra.runtime-receipt.v2"
            || receipt["outcome"]["producer"] != producer
            || receipt["outcome"]["profile"] != profile
            || receipt["outcome"]["timeout_seconds"] != timeout
            || receipt["outcome"]["automatic_retry"] != false
            || receipt["outcome"]["receipt"] != paths[0]
        {
            return Err(
                "supervisor receipt does not match the declared diagnostic invocation".into(),
            );
        }
        create_json(
            &root.join(format!("supervisor-{phase}.json")),
            &json!({
            "schema":"clearra.native-cli-diagnostic-supervision.v1","diagnostic_only":true,
            "diagnostic":context,"phase":phase,"receipt":receipt}),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagnostic_workflow_bash_blocks_parse_without_execution() {
        let workflow = include_str!("../../.github/workflows/native-cli-diagnostics.yml");
        let mut lines = workflow.lines().peekable();
        while let Some(line) = lines.next() {
            if line.trim() != "run: |" {
                continue;
            }
            let mut script = String::new();
            while lines
                .peek()
                .is_some_and(|line| line.starts_with("          ") || line.is_empty())
            {
                let line = lines.next().unwrap();
                script.push_str(line.strip_prefix("          ").unwrap_or(line));
                script.push('\n');
            }
            let mut shell = Command::new("bash")
                .arg("-n")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap();
            shell
                .stdin
                .take()
                .unwrap()
                .write_all(script.as_bytes())
                .unwrap();
            let result = shell.wait_with_output().unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        }
    }
    #[test]
    fn observed_cpu_widths_deduplicate_without_inventing_capacity() {
        assert_eq!(diagnostic_widths(1), vec![1]);
        assert_eq!(diagnostic_widths(2), vec![1, 2]);
        assert_eq!(diagnostic_widths(4), vec![1, 2, 4]);
        assert_eq!(diagnostic_widths(9), vec![1, 2, 9]);
    }
    fn artifact_fixture() -> (String, Value, Value, Value) {
        let source = "1".repeat(40);
        let run = json!({"id":100,"run_attempt":2,"head_sha":source,"head_branch":"main",
            "event":"workflow_dispatch","path":".github/workflows/release-cli.yml",
            "repository":{"full_name":"daejunnom/Clearra"},"head_repository":{"full_name":"daejunnom/Clearra"},
            "status":"completed","conclusion":"success"});
        let artifact = json!({"id":200,"name":"clearra-linux-cli-v0.9.1-run-100-attempt-2",
            "expired":false,"size_in_bytes":4096,"digest":format!("sha256:{}", "2".repeat(64)),
            "workflow_run":{"id":100,"head_sha":source,"head_branch":"main"},
            "archive_download_url":"https://api.github.com/repos/daejunnom/Clearra/actions/artifacts/200/zip"});
        let jobs = json!({"total_count":1,"jobs":[{"name":"linux-cli","run_id":100,"run_attempt":2,
            "head_sha":source,"status":"completed","conclusion":"success"}]});
        (source, run, artifact, jobs)
    }
    #[test]
    fn canonical_artifact_requires_exact_source_run_attempt_and_successful_linux_job() {
        let (source, run, artifact, jobs) = artifact_fixture();
        assert_eq!(
            validate_linux_artifact(&source, "100", "200", &run, &artifact, &jobs).unwrap(),
            "Clearra-CLI-v0.9.1-linux-x86_64"
        );
        for (field, bad) in [
            ("id", json!(101)),
            ("run_attempt", json!(1)),
            ("head_sha", json!("2".repeat(40))),
            ("event", json!("push")),
            ("path", json!(".github/workflows/candidate-preflight.yml")),
            ("head_branch", json!("candidate")),
            ("conclusion", json!("failure")),
        ] {
            let mut changed = run.clone();
            changed[field] = bad;
            assert!(
                validate_linux_artifact(&source, "100", "200", &changed, &artifact, &jobs).is_err(),
                "{field}"
            );
        }
        for (field, bad) in [
            ("id", json!(201)),
            ("name", json!("clearra-linux-cli-v0.9.1-run-100-attempt-1")),
            ("expired", json!(true)),
            ("size_in_bytes", json!(0)),
            ("digest", json!("sha256:bad")),
            (
                "archive_download_url",
                json!("https://untrusted.invalid/zip"),
            ),
        ] {
            let mut changed = artifact.clone();
            changed[field] = bad;
            assert!(
                validate_linux_artifact(&source, "100", "200", &run, &changed, &jobs).is_err(),
                "{field}"
            );
        }
        let mut changed = jobs.clone();
        changed["jobs"][0]["conclusion"] = json!("failure");
        assert!(validate_linux_artifact(&source, "100", "200", &run, &artifact, &changed).is_err());
        changed = jobs.clone();
        changed["total_count"] = json!(2);
        assert!(validate_linux_artifact(&source, "100", "200", &run, &artifact, &changed).is_err());
    }
    #[test]
    fn incomplete_canonical_run_is_only_a_diagnostic_input_after_linux_success() {
        let (source, mut run, artifact, mut jobs) = artifact_fixture();
        run["status"] = json!("in_progress");
        run["conclusion"] = Value::Null;
        assert!(validate_linux_artifact(&source, "100", "200", &run, &artifact, &jobs).is_ok());
        jobs["jobs"][0]["status"] = json!("in_progress");
        jobs["jobs"][0]["conclusion"] = Value::Null;
        assert!(validate_linux_artifact(&source, "100", "200", &run, &artifact, &jobs).is_err());
    }
    #[test]
    fn decoded_fixture_keeps_original_board_and_full_source() {
        let fixture = fixture().unwrap();
        assert_eq!(fixture["base_mask"], "0xc0383f3fc7");
        assert_eq!(fixture["target_mask"], "0xc120fc1fcfe3fff3fc7c0c038");
        assert_eq!(fixture["target_locks"], 14);
        assert_eq!(fixture["original_patterns"], 25_401_600);
    }
    #[test]
    fn argv_keeps_the_full_oracle_standard_bag_contract() {
        let args = cli_args(8);
        for pair in [
            ["--patterns", "P7P7"],
            ["--source-pieces", "14"],
            ["--hold", "empty"],
            ["--queue-knowledge", "oracle"],
            ["--workers", "8"],
        ] {
            assert!(args.windows(2).any(|window| window == pair));
        }
        assert!(!args.contains(&"--max-patterns".into()));
        assert!(!args.contains(&"--solution-probabilities".into()));
    }
    #[test]
    fn missing_public_evidence_cannot_pass_or_claim_a_bitmap_hash() {
        let report = analyze(&json!({"summary": {"count_complete": "true"}}), 8, "source");
        assert_eq!(report["accepted"], false);
        assert_eq!(report["coverage"]["bitmap_hash_verified"], false);
        assert!(report["coverage"]["observed_bitmap_sha256"].is_null());
    }
    #[test]
    fn key_hash_preserves_order_and_byte_length_not_character_count() {
        assert_ne!(
            key_hash(&["a".into(), "bc".into()]),
            key_hash(&["ab".into(), "c".into()])
        );
        assert_ne!(
            key_hash(&["a".into(), "b".into()]),
            key_hash(&["b".into(), "a".into()])
        );
        assert_eq!(bool_value(&json!("true")), Some(true));
        assert_eq!(u64_value(&json!("25401600")), Some(PATTERNS));
        assert_eq!(f64_value(&json!("1.0")), Some(1.0));
    }
}
