//! A local evidence producer, not a user-visible backend selector.
//!
//! The C reference and Rust CPU consume the same compiled PC request, with
//! external accelerators disabled. Do not link this target into a release.
//! `--family-output` is for untimed diagnosis only. Verify its absolute path
//! with `clearra-manage storage verify` before starting the supervised run.

use std::fs::OpenOptions;
use std::io::{self, BufWriter, Write};
use std::path::{Component, Path, PathBuf};
use std::{env, thread, time::Instant};

use clearra_core_domain::{
    execution_cancellation::ExecutionControl,
    pc::pc_target::PcTarget,
    piece::piece_kind::PieceKind::{I, O},
};
use clearra_core_executor::{CoreExecutionResult, PcService, WasmCpuSearchBackend};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcCountPolicy, PcExecutionPolicy, PcHoldPolicy, PcQueueInput,
    PcScenarioBoard, PcScenarioQuery, PcSolutionProbabilityPolicy, PieceWindow,
    RequestedSearchBackend, SupplyWindowSize,
};
use clearra_problem::{ProblemCompileError, ProblemCompiler, SearchProblem};
use clearra_rules::profile::builtin_rules::srs_plus;
use clearra_supply::queue::fixed_sequence::FixedSequence;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const FAMILY_SCHEMA: &str = "clearra.native-rust-family.v1";
const MAX_FAMILY_ROWS: usize = 1_000_000;
const MAX_FAMILY_KEY_BYTES: usize = 1024;
const MAX_FAMILY_BYTES: usize = 128 * 1024 * 1024;

#[derive(Clone, Copy)]
enum Arm {
    NativeC,
    RustCpu,
}

impl Arm {
    fn parse(arg: &str) -> Result<Self, String> {
        match arg {
            "native-c" => Ok(Self::NativeC),
            "rust-cpu" => Ok(Self::RustCpu),
            _ => Err("arm must be native-c or rust-cpu".into()),
        }
    }

    fn execute(
        self,
        problem: &SearchProblem,
        control: &ExecutionControl,
    ) -> Result<CoreExecutionResult, String> {
        match self {
            // PcService is the independent native-C packing and BuildUp
            // reference; do not route A through a switchable product default.
            Self::NativeC => PcService::execute_with_control(problem, control)
                .map_err(|error| format!("{error:?}")),
            Self::RustCpu => WasmCpuSearchBackend::execute_with_control(problem, control)
                .map_err(|error| format!("{error:?}")),
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::NativeC => "native-c",
            Self::RustCpu => "rust-cpu",
        }
    }
}

fn compile_case(case: &str, workers: usize) -> Result<SearchProblem, String> {
    let policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_workers(workers)
        .with_worker_hardware_limit(workers)
        .with_use_all_logical_processors(true)
        .with_cpu_warmup(false)
        .with_exact_legal_board_enabled(false)
        .with_conditioned_reachability_enabled(false)
        .with_tablebase_requested(false);
    match case {
        "fixed-2l" => ProblemCompiler::compile_opening_pc(
            &OpeningPcSearchQuery::new(PcTarget::two_lines())
                .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                    I, I, O, O, O,
                ])))
                .with_hold_policy(PcHoldPolicy::Disabled)
                .with_rule(srs_plus())
                .with_objective(ObjectivePolicy::all())
                .with_execution_policy(policy),
        ),
        "existing-p7" => compile_standard_bag_case(0x3_c0f0_3c0f, 6, policy),
        "full-p7p4" => compile_standard_bag_case(0, 10, policy),
        _ => return Err("case must be fixed-2l, existing-p7, or full-p7p4".into()),
    }
    .map_err(|error| format!("compile PC request: {error:?}"))
}

fn compile_standard_bag_case(
    initial_mask: u64,
    placements: usize,
    policy: PcExecutionPolicy,
) -> Result<SearchProblem, ProblemCompileError> {
    // Matches the historical CLI --board-mask/--height/--pieces route.
    // Bare P7/P7P4 are standard bag descriptors, with one lookahead for
    // empty hold and the product scenario's single retained witness.
    ProblemCompiler::compile_scenario_pc(
        &PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, initial_mask),
            PcQueueInput::standard_7_bag(),
            PieceWindow::new(placements),
        )
        .with_exact_pieces(Some(placements))
        .with_min_remaining_queue(0)
        .with_allow_hold(true)
        .with_rule(srs_plus())
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit)
        .with_supply_window_size(SupplyWindowSize::new(placements + 1))
        .with_retained_trace_limit(1)
        .with_execution_policy(policy),
    )
}

fn normalized_key_sequence_sha256(result: &CoreExecutionResult) -> Option<String> {
    let keys = result.normalized_solution_keys();
    // This is optional evidence. If a backend retains compact identities
    // only, its authoritative normalized-set hash is still reported as such.
    if keys.is_empty() || result.usize_field("normalized_unique_solution_count") != Some(keys.len())
    {
        return None;
    }
    let mut hash = Sha256::new();
    for key in keys {
        hash.update((key.len() as u64).to_le_bytes());
        hash.update(key.as_bytes());
    }
    Some(format!("{:x}", hash.finalize()))
}

fn coverage_sha256(result: &CoreExecutionResult) -> Option<String> {
    let words = result.coverage_pattern_words();
    if words.is_empty() {
        return None;
    }
    let mut hash = Sha256::new();
    for word in words {
        hash.update(word.to_le_bytes());
    }
    Some(format!("{:x}", hash.finalize()))
}

#[cfg(target_os = "linux")]
fn process_rss_peak_bytes() -> Option<u64> {
    // All solver workers are threads of this process. VmHWM is a kernel
    // process high-water mark; it is not the supervisor's sampled tree RSS.
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let mut fields = status
        .lines()
        .find_map(|line| line.strip_prefix("VmHWM:"))?
        .split_ascii_whitespace();
    let kibibytes = fields.next()?.parse::<u64>().ok()?;
    (fields.next() == Some("kB") && fields.next().is_none())
        .then_some(kibibytes)?
        .checked_mul(1024)
}

#[cfg(not(target_os = "linux"))]
fn process_rss_peak_bytes() -> Option<u64> {
    None
}

fn report(
    case: &str,
    arm: Arm,
    workers: usize,
    available_workers: usize,
    solve_ns: u128,
    result: &CoreExecutionResult,
) -> Value {
    let ordered_key_hash = normalized_key_sequence_sha256(result);
    let coverage_hash = coverage_sha256(result);
    json!({
        "schema": "clearra.native-rust-cpu-abba.sample.v1",
        "compiled_source_commit": option_env!("CLEARRA_SOURCE_COMMIT"),
        "case": case,
        "arm": arm.label(),
        "requested_workers": workers,
        "available_parallelism": available_workers,
        "cpu_warmup": false,
        "solver_elapsed_ns": solve_ns.to_string(),
        "linux_process_rss_hwm_bytes": process_rss_peak_bytes(),
        "semantic": {
            "count_complete": result.bool_field("count_complete"),
            "probability_complete": result.bool_field("probability_complete"),
            "resource_truncated": result.bool_field("resource_truncated"),
            "solution_count": result.usize_field("normalized_unique_solution_count"),
            "solution_set_hash": result.field("normalized_solution_set_hash"),
            "ordered_key_sha256": ordered_key_hash,
            "pattern_count": result.usize_field("coverage_pattern_count"),
            "covered_pattern_count": result.usize_field("covered_pattern_count"),
            "coverage_sha256": coverage_hash,
            "build_variant_count": result.field("build_variant_count"),
        },
        "backend": {
            "selected": result.field("backend_selected"),
            "packing": result.field("candidate_backend"),
            "packing_execution_source": result.field("packing_execution_source"),
            "buildup": result.field("buildup_backend"),
            "buildup_execution_source": result.field("buildup_execution_source"),
            "fallback": result.bool_field("backend_fallback_used"),
            "reported_workers": result.usize_field("workers_used"),
            "reported_active_workers": result.usize_field("parallel_active_workers"),
            "native_geometry_dag_workers": result.usize_field("native_cpu_geometry_dag_workers"),
            "native_candidate_workers": result.usize_field("native_cpu_candidate_workers"),
            "native_buildup_workers": result.usize_field("native_cpu_buildup_workers"),
            "parallel": result.bool_field("cpu_parallel_execution"),
            "decision_reason": result.field("cpu_parallel_decision_reason"),
        },
        "work": {
            "geometry_searched_nodes": result.usize_field("searched_nodes"),
            "packing_candidates": result.usize_field("packing_candidate_count"),
            "build_order_nodes": result.usize_field("total_build_order_nodes"),
        },
    })
}

struct FamilyOutput {
    path: PathBuf,
    source_commit: &'static str,
}

impl FamilyOutput {
    fn prepare(path: &Path) -> Result<Self, String> {
        let source_commit = option_env!("CLEARRA_SOURCE_COMMIT")
            .filter(|commit| is_lower_hex(commit, 40))
            .ok_or("family capture requires a compiled source commit")?;
        if !path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
        {
            return Err("family output requires a verified absolute path".into());
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| valid_family_filename(name))
            .ok_or("family output requires a simple .ndjson filename")?;
        let repo = env::current_dir()
            .and_then(|dir| dir.canonicalize())
            .map_err(|error| format!("resolve repository root: {error}"))?;
        if !repo.join("config/clearra-management.v1.json").is_file() {
            return Err("run family capture from the Clearra repository root".into());
        }
        let root = repo.join("_local/artifacts/benchmarks");
        if root.canonicalize().ok().as_ref() != Some(&root) {
            return Err("family capture requires an existing managed benchmark root".into());
        }
        let parent = path
            .parent()
            .ok_or("family output requires a parent directory")?
            .canonicalize()
            .map_err(|error| format!("resolve family output directory: {error}"))?;
        if !parent.is_dir() || !parent.starts_with(root) {
            return Err("family output must stay within the managed benchmark root".into());
        }
        let path = parent.join(name);
        match path.symlink_metadata() {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("inspect family output path: {error}")),
            Ok(_) => return Err("family output file must not already exist".into()),
        }
        Ok(Self {
            path,
            source_commit,
        })
    }

    fn write(
        self,
        case: &str,
        arm: Arm,
        workers: usize,
        available_workers: usize,
        result: &CoreExecutionResult,
    ) -> Result<(), String> {
        if result.bool_field("count_complete") != Some(true)
            || result.bool_field("resource_truncated") != Some(false)
            || result.bool_field("backend_fallback_used") != Some(false)
        {
            return Err(
                "family capture requires complete, untruncated results without fallback".into(),
            );
        }
        let expected_backend = match arm {
            Arm::NativeC => "cpu",
            Arm::RustCpu => "wasm-cpu",
        };
        let backend = result
            .field("backend_selected")
            .filter(|&backend| backend == expected_backend)
            .ok_or("family capture backend does not match the requested arm")?;
        let reported_workers = result
            .usize_field("workers_used")
            .filter(|width| (1..=workers).contains(width))
            .ok_or("family capture lacks a valid reported worker width")?;
        let keys = result.normalized_solution_keys();
        if keys.is_empty()
            || result.usize_field("normalized_unique_solution_count") != Some(keys.len())
        {
            return Err("family capture requires the entire materialized key family".into());
        }
        let set_hash = result
            .field("normalized_solution_set_hash")
            .filter(|hash| {
                hash.strip_prefix("cts1:")
                    .is_some_and(|hex| is_lower_hex(hex, 16))
            })
            .ok_or("family capture lacks a valid normalized solution set hash")?;
        let keys = sorted_family_keys(keys)?;
        let header = json!({
            "kind": "header",
            "schema": FAMILY_SCHEMA,
            "source_commit": self.source_commit,
            "arm": arm.label(),
            "case": case,
            "requested_workers": workers,
            "available_parallelism": available_workers,
            "backend_selected": backend,
            "reported_workers": reported_workers,
            "count_complete": true,
            "resource_truncated": false,
        });
        write_family_file(&self.path, &header, &keys, set_hash)
            .map_err(|error| format!("write family capture: {error}"))
    }
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_family_filename(name: &str) -> bool {
    name.strip_suffix(".ndjson").is_some_and(|stem| {
        !stem.is_empty()
            && stem
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    })
}

fn sorted_family_keys(keys: &[String]) -> Result<Vec<&str>, String> {
    if keys.len() > MAX_FAMILY_ROWS {
        return Err("family capture row limit exceeded".into());
    }
    if keys.iter().any(|key| {
        key.is_empty() || key.len() > MAX_FAMILY_KEY_BYTES || key.chars().any(char::is_control)
    }) {
        return Err("family capture contains an invalid normalized key".into());
    }
    let mut sorted: Vec<&str> = keys.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    if sorted.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err("family capture contains duplicate normalized keys".into());
    }
    Ok(sorted)
}

struct BoundedOutput<W> {
    inner: W,
    remaining: usize,
}

impl<W: Write> Write for BoundedOutput<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.len() > self.remaining {
            return Err(io::Error::other("family capture exceeds byte limit"));
        }
        let written = self.inner.write(buf)?;
        self.remaining -= written;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn write_family_file(path: &Path, header: &Value, keys: &[&str], set_hash: &str) -> io::Result<()> {
    // The solver may run for minutes between preflight and opening the file.
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("missing output directory"))?;
    if parent.canonicalize()? != parent {
        return Err(io::Error::other(
            "family output directory changed after preflight",
        ));
    }
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    let mut output = BoundedOutput {
        inner: BufWriter::new(file),
        remaining: MAX_FAMILY_BYTES,
    };
    write_family_records(&mut output, header, keys, set_hash)?;
    output.flush()?;
    output.inner.get_ref().sync_all()
}

fn write_family_records(
    mut output: impl Write,
    header: &Value,
    keys: &[&str],
    set_hash: &str,
) -> io::Result<()> {
    serde_json::to_writer(&mut output, header)?;
    output.write_all(b"\n")?;
    let mut hash = Sha256::new();
    for &key in keys {
        output.write_all(b"{\"kind\":\"key\",\"key\":")?;
        serde_json::to_writer(&mut output, key)?;
        output.write_all(b"}\n")?;
        // Keys are UTF-8 byte ordered. Length prefixes avoid concatenation
        // ambiguity and agree with the sequence digest's framing.
        hash.update((key.len() as u64).to_le_bytes());
        hash.update(key.as_bytes());
    }
    serde_json::to_writer(
        &mut output,
        &json!({
            "kind": "footer",
            "count": keys.len(),
            "keys_sha256": format!("{:x}", hash.finalize()),
            "solution_set_hash": set_hash,
        }),
    )?;
    output.write_all(b"\n")
}

fn main() -> Result<(), String> {
    if env::var("CLEARRA_RUNTIME_SUPERVISED").as_deref() != Ok("1")
        || env::var("CLEARRA_RUNTIME_PROFILE").as_deref() != Ok("benchmark-search")
    {
        return Err(
            "run this example under clearra-manage runtime run --profile benchmark-search".into(),
        );
    }
    let args: Vec<_> = env::args().skip(1).collect();
    let (arm, case, workers, family_output, memory_diagnostic) = match args.as_slice() {
        [arm, case, workers] => (arm, case, workers, None, false),
        [arm, case, workers, option, path] if option == "--family-output" => {
            let output = FamilyOutput::prepare(Path::new(path))?;
            (arm, case, workers, Some(output), false)
        }
        [arm, case, workers, option] if option == "--memory-diagnostic" => {
            (arm, case, workers, None, true)
        }
        _ => {
            return Err(
                "expected <arm> <case> <workers> [--family-output <absolute .ndjson path> | --memory-diagnostic]".into(),
            )
        }
    };
    let arm = Arm::parse(arm)?;
    let available_workers = thread::available_parallelism().map_or(1, |available| available.get());
    let workers = workers
        .parse::<usize>()
        .ok()
        .filter(|&workers| (1..=available_workers).contains(&workers))
        .ok_or_else(|| format!("workers must be 1..={available_workers}"))?;
    if family_output.is_some() && workers > 256 {
        return Err("family capture supports at most 256 requested workers".into());
    }
    let problem = compile_case(case, workers)?;
    let control = ExecutionControl::default();
    let started = Instant::now();
    #[cfg(all(feature = "local-search-ab", target_os = "linux"))]
    let (result, memory_observation) = if memory_diagnostic {
        if !matches!(arm, Arm::RustCpu) || workers < 2 || case != "full-p7p4" {
            return Err("memory diagnostic requires parallel rust-cpu full-p7p4".into());
        }
        let (outcome, observation) =
            clearra_core_executor::backend::observe_native_pc_memory(|| {
                arm.execute(&problem, &control)
            })
            .map_err(|error| format!("native memory observer: {error}"))?;
        let result = outcome?;
        if result.bool_field("cpu_parallel_execution") != Some(true)
            || result.usize_field("parallel_active_workers") != Some(workers)
        {
            return Err(
                "memory diagnostic did not exercise the requested native parallel width".into(),
            );
        }
        (result, Some(observation))
    } else {
        (arm.execute(&problem, &control)?, None)
    };
    #[cfg(not(all(feature = "local-search-ab", target_os = "linux")))]
    if memory_diagnostic {
        return Err("memory diagnostic requires Linux and the local-search-ab feature".into());
    }
    #[cfg(not(all(feature = "local-search-ab", target_os = "linux")))]
    let result = arm.execute(&problem, &control)?;
    let solve_ns = started.elapsed().as_nanos();
    let sample = report(case, arm, workers, available_workers, solve_ns, &result);
    #[cfg(all(feature = "local-search-ab", target_os = "linux"))]
    let sample = if let Some(observation) = memory_observation {
        let mut sample = sample;
        sample["memory_diagnostic"] = observation;
        sample["memory_diagnostic"]["timing_comparable_to_abba"] = json!(false);
        sample["memory_diagnostic"]["result_retained_bytes_after_solver"] =
            json!(result.checked_resource_retained_bytes());
        sample
    } else {
        sample
    };
    if let Some(output) = family_output {
        output.write(case, arm, workers, available_workers, &result)?;
    }
    println!("{sample}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_filename_disallows_alternate_streams_and_hidden_paths() {
        assert!(valid_family_filename("native-c_full-p7p4.ndjson"));
        assert!(!valid_family_filename(".env.ndjson"));
        assert!(!valid_family_filename("data:stream.ndjson"));
        assert!(!valid_family_filename("capture.json"));
    }

    #[test]
    fn capture_keys_are_byte_sorted_and_unique() {
        let keys = ["z", "β", "aa", "α", "a"].map(String::from);
        assert_eq!(
            sorted_family_keys(&keys).unwrap(),
            ["a", "aa", "z", "α", "β"]
        );
        assert!(sorted_family_keys(&["key".to_owned(), "key".to_owned()]).is_err());
        assert!(sorted_family_keys(&["two\nlines".to_owned()]).is_err());
        assert!(sorted_family_keys(&["x".repeat(MAX_FAMILY_KEY_BYTES + 1)]).is_err());
    }

    #[test]
    fn capture_footer_is_final_lf_terminated_and_hashes_framed_utf8() {
        let mut out = Vec::new();
        let header = json!({"kind": "header"});
        write_family_records(&mut out, &header, &["ab", "é"], "cts1:0123456789abcdef").unwrap();
        let output = String::from_utf8(out).unwrap();
        assert!(output.ends_with('\n'));
        let lines: Vec<Value> = output
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let expected = Sha256::digest(b"\x02\0\0\0\0\0\0\0ab\x02\0\0\0\0\0\0\0\xc3\xa9");
        assert_eq!(
            lines,
            [
                header,
                json!({"kind": "key", "key": "ab"}),
                json!({"kind": "key", "key": "é"}),
                json!({
                    "kind": "footer",
                    "count": 2,
                    "keys_sha256": format!("{expected:x}"),
                    "solution_set_hash": "cts1:0123456789abcdef",
                }),
            ]
        );
    }

    #[test]
    fn family_write_rejects_overflow_without_writing_excess_bytes() {
        let mut limited = BoundedOutput {
            inner: Vec::new(),
            remaining: 2,
        };
        limited.write_all(b"ab").unwrap();
        assert!(limited.write_all(b"c").is_err());
        assert_eq!(limited.inner, b"ab");
    }
}
