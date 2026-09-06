#![cfg(not(target_arch = "wasm32"))]

// SRP rationale: this CTK3/P7 regression owner has one change reason: the
// observable staged minimum-cover execution contract changes across source,
// exact-proof, canonical-portfolio, cancellation, or watchdog boundaries.

use std::{
    env,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use clearra_app::AppCommand;
use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_executor::{WasmCpuSearchAdvance, WasmCpuSearchSession};
use clearra_coverage::{
    cover::exact_minimum_cover::{
        diagnostic_blocking_witness_shortcut, ExactMinimumCoverResidualAdmissionPolicy,
        ExactMinimumCoverSession, ExactMinimumCoverSessionAdvance,
        ExactMinimumCoverWitnessShortcutDiagnostics,
    },
    cover::exact_minimum_cover_portfolios::{
        ExactMinimumCoverEnumerationStop, ExactMinimumCoverPortfolioEnumerator,
        ExactMinimumCoverPortfolioPreparation,
    },
    pattern::pattern_bitset::PatternBitSet,
};
use clearra_problem::ProblemCompiler;
use clearra_wasm::WasmCommandRuntime;

#[path = "pc_minimals_ctk3_stage_probe/parallel_wave.rs"]
mod parallel_wave;

const SOURCE_COMMAND: &str = "clearra pc --lines 4 --board-mask 0x3c0f03c0f \
    --height 4 --pieces 6 --patterns P7 --hold empty --objective unique \
    --count unique --solution-probabilities --backend cpu --workers 1";
const KNOWN_MINIMUM_COVER_SOURCE_ROWS: [usize; 25] = [
    2, 6, 24, 37, 55, 57, 66, 69, 72, 85, 124, 148, 151, 159, 165, 167, 177, 181, 204, 206, 211,
    213, 221, 224, 227,
];
const EXPECTED_FIRST_CANONICAL_SOURCE_ROWS: [usize; 25] = [
    0, 6, 24, 37, 55, 57, 66, 69, 72, 85, 114, 124, 159, 160, 162, 165, 167, 177, 181, 206, 211,
    213, 221, 224, 227,
];
const EXPECTED_SECOND_CANONICAL_SOURCE_ROWS: [usize; 25] = [
    0, 6, 24, 37, 55, 57, 66, 69, 72, 85, 114, 124, 159, 160, 162, 165, 167, 177, 181, 206, 211,
    213, 221, 224, 229,
];
const EXPECTED_RAW_PROOF_SOURCE_ROWS: [usize; 25] = [
    8, 19, 24, 33, 38, 59, 66, 69, 75, 85, 124, 146, 151, 159, 162, 165, 167, 177, 181, 204, 206,
    213, 221, 224, 227,
];
const PROBE_CHILD_ENV: &str = "CLEARRA_CTK3_PORTFOLIO_PROBE_CHILD";
const PROBE_TIMEOUT_SECONDS_ENV: &str = "CLEARRA_CTK3_PORTFOLIO_PROBE_TIMEOUT_SECONDS";
const PROBE_STOP_AFTER_FIRST_SESSION_ADVANCE_ENV: &str =
    "CLEARRA_PROBE_STOP_AFTER_FIRST_SESSION_ADVANCE";
const PROBE_INCUMBENT_TARGET_ENV: &str = "CLEARRA_PROBE_INCUMBENT_TARGET";
const PROBE_SKIP_INCUMBENT_ENV: &str = "CLEARRA_PROBE_SKIP_INCUMBENT";
const PROBE_DRIVE_MINIMUM_SESSION_ENV: &str = "CLEARRA_PROBE_DRIVE_MINIMUM_SESSION";
const PROBE_RESIDUAL_ITERATION_BUDGET_ENV: &str = "CLEARRA_PROBE_RESIDUAL_ITERATION_BUDGET";
const PROBE_RECURSIVE_REFERENCE_ENV: &str = "CLEARRA_PROBE_RECURSIVE_REFERENCE";
const PROBE_RESIDUAL_AB_CANDIDATE_ENV: &str = "CLEARRA_PROBE_RESIDUAL_AB_CANDIDATE";
const DEFAULT_PROBE_TIMEOUT: Duration = Duration::from_secs(180);
const MAX_PROBE_TIMEOUT_SECONDS: u64 = 900;
const SOURCE_STAGE_TIMEOUT: Duration = Duration::from_secs(60);
const SOURCE_MAX_ADVANCES: u64 = 1_000_000;
const RESIDUAL_AB_VARIANT_TIMEOUT: Duration = Duration::from_secs(60);
const PORTFOLIO_STAGE_TIMEOUT: Duration = Duration::from_secs(20);
const PORTFOLIO_WORK_STEPS_PER_SLICE: u64 = 128;
const PORTFOLIO_MAX_WORK_STEPS_PER_STAGE: u64 = 100_000;

/// Guards the GUI's ordinary all-solutions Geometry path against replacing
/// the caller's cooperative budget with a tiny global cap. The CTK3/P7 source
/// search is intentionally used here because an eight-step cap inflated this
/// exact workload from tens of advances to roughly seventeen thousand ABI
/// round trips even though its Rust/WASM computation stayed sub-second.
#[test]
fn ctk3_source_search_consumes_the_caller_work_budget() {
    let request = WasmCommandRuntime::default()
        .compile_command_text(SOURCE_COMMAND)
        .expect("CTK3 source request");
    let AppCommand::Scenario(pc) = request.command() else {
        panic!("CTK3 source must compile as a PC scenario")
    };
    let problem = ProblemCompiler::compile_scenario_pc(pc.query()).expect("CTK3 source problem");
    let mut session = WasmCpuSearchSession::new(&problem).expect("CTK3 source session");
    let control = ExecutionControl::default();

    for advance in 1..=128_u64 {
        match session
            .advance(8_192, &control)
            .expect("CTK3 source advance")
        {
            WasmCpuSearchAdvance::Pending => {}
            WasmCpuSearchAdvance::Completed(result) => {
                assert_eq!(result.normalized_solution_coverages().len(), 246);
                eprintln!("CTK3 source completed in {advance} caller-budgeted advances");
                return;
            }
            WasmCpuSearchAdvance::Cancelled => panic!("CTK3 source unexpectedly cancelled"),
        }
    }

    panic!("CTK3 source did not complete within 128 caller-budgeted advances");
}

/// Exports only the source coverage matrix, never a minimum, incumbent, or
/// witness. This ignored diagnostic is a transport for future coverage-only
/// A/B runs; its hashes bind source row order and the corresponding canonical
/// candidate keys independently of any solver result.
#[test]
#[ignore = "explicit source-only CTK3 exact-cover matrix export"]
fn ctk3_export_exact_cover_diagnostic_matrix() {
    export_ctk3_exact_cover_diagnostic_matrix(
        "ctk3_export_exact_cover_diagnostic_matrix",
        SOURCE_COMMAND,
        "srs-plus",
        Some(246),
    );
}

/// A separate rule-bound source for Jstris comparisons. Do not reuse the SRS+
/// known witness or canonical row indices: this exporter never runs selection.
#[test]
#[ignore = "explicit source-only Jstris 180 CTK3 exact-cover matrix export"]
fn ctk3_export_jstris_180_exact_cover_diagnostic_matrix() {
    let source_command = format!("{SOURCE_COMMAND} --rule jstris-180");
    export_ctk3_exact_cover_diagnostic_matrix(
        "ctk3_export_jstris_180_exact_cover_diagnostic_matrix",
        &source_command,
        "jstris-180",
        None,
    );
}

fn export_ctk3_exact_cover_diagnostic_matrix(
    test_name: &'static str,
    source_command: &str,
    expected_rule: &str,
    expected_source_rows: Option<usize>,
) {
    const EXPORT_CHILD_ENV: &str = "CLEARRA_CTK3_MATRIX_EXPORT_CHILD";
    const EXPORT_DEADLINE: Duration = Duration::from_secs(30);
    if env::var(EXPORT_CHILD_ENV).ok().as_deref() != Some(test_name) {
        let executable = env::current_exe().expect("current matrix exporter executable");
        let mut command = Command::new(executable);
        command
            .args(["--exact", test_name, "--ignored", "--nocapture"])
            .env(EXPORT_CHILD_ENV, test_name)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = command.spawn().expect("spawn isolated matrix exporter");
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait().expect("poll matrix exporter") {
                assert!(status.success(), "matrix exporter failed with {status}");
                return;
            }
            if started.elapsed() >= EXPORT_DEADLINE {
                let _ = child.kill();
                let _ = child.wait();
                panic!("matrix exporter exceeded its 30-second process deadline");
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    let matrix =
        prepare_ctk3_diagnostic_matrix(source_command, expected_rule, expected_source_rows);
    write_ctk3_diagnostic_matrix(&matrix);
}

/// One source construction and identity check shared by immutable export and
/// same-process A/B. No minimum or witness is supplied with this input.
struct Ctk3DiagnosticMatrix {
    required: PatternBitSet,
    rows: Vec<PatternBitSet>,
    candidate_keys: Vec<String>,
    normalized_ordinals: Vec<usize>,
    matrix_identity: [u8; 32],
    artifact: serde_json::Value,
}

fn prepare_ctk3_diagnostic_matrix(
    source_command: &str,
    expected_rule: &str,
    expected_source_rows: Option<usize>,
) -> Ctk3DiagnosticMatrix {
    use clearra_core_domain::solution::normalized_tiling_solution::NormalizedTilingSolutionKey;
    use sha2::{Digest, Sha256};
    use std::collections::BTreeSet;

    const SOURCE_DEADLINE: Duration = Duration::from_secs(15);
    let started = Instant::now();
    let request = WasmCommandRuntime::default()
        .compile_command_text(source_command)
        .expect("matrix export source request");
    let AppCommand::Scenario(pc) = request.command() else {
        panic!("matrix export source must be a PC scenario")
    };
    let problem =
        ProblemCompiler::compile_scenario_pc(pc.query()).expect("matrix export source problem");
    assert_eq!(problem.rule_profile().rule().id().as_str(), expected_rule);
    let mut session = WasmCpuSearchSession::new(&problem).expect("matrix export source session");
    let control = ExecutionControl::default();
    let mut advances = 0_u64;
    let source = loop {
        assert!(
            advances < 128 && started.elapsed() < SOURCE_DEADLINE,
            "matrix export source exceeded 128 advances or 15 seconds"
        );
        advances += 1;
        match session
            .advance(8_192, &control)
            .expect("matrix export source advance")
        {
            WasmCpuSearchAdvance::Pending => {}
            WasmCpuSearchAdvance::Completed(result) => break result,
            WasmCpuSearchAdvance::Cancelled => panic!("matrix export source cancelled"),
        }
    };
    let pattern_count = source
        .field("coverage_pattern_count")
        .expect("matrix pattern count")
        .parse::<usize>()
        .expect("numeric matrix pattern count");
    let raw_rows = source.solution_coverages();
    let normalized_rows = source.normalized_solution_coverages();
    assert_eq!(pattern_count, 5_040);
    if let Some(expected_source_rows) = expected_source_rows {
        assert_eq!(raw_rows.len(), expected_source_rows);
    }
    assert!(
        !raw_rows.is_empty(),
        "diagnostic source has no candidate rows"
    );
    assert_eq!(normalized_rows.len(), raw_rows.len());
    let universe = problem
        .piece_source()
        .materialized_universe()
        .expect("matrix export materialized queue universe");
    assert_eq!(universe.pattern_count(), pattern_count);
    let queues = (0..pattern_count)
        .map(|pattern_ordinal| {
            universe
                .sequence_at(pattern_ordinal)
                .iter()
                .map(|piece| piece.as_ascii())
                .collect::<String>()
        })
        .collect::<Vec<_>>();
    assert_eq!(queues.iter().collect::<BTreeSet<_>>().len(), pattern_count);
    let word_count = pattern_count.div_ceil(u64::BITS as usize);
    let required_words = source.coverage_pattern_words();
    assert_eq!(required_words.len(), word_count);
    let last_mask = match pattern_count % u64::BITS as usize {
        0 => u64::MAX,
        remainder => (1_u64 << remainder) - 1,
    };
    assert_eq!(required_words[word_count - 1] & !last_mask, 0);
    let to_u64 = |value| u64::try_from(value).expect("diagnostic dimension fits u64");
    let word_hex = |words: &[u64]| {
        words
            .iter()
            .map(|word| format!("{word:016x}"))
            .collect::<Vec<_>>()
    };

    // Hash encoding: domain, row/pattern/word counts, required words, then
    // each raw row's words; every integer is an unsigned little-endian u64.
    let mut matrix_hash = Sha256::new();
    matrix_hash.update(b"clearra-exact-cover-diagnostic-matrix.v1\0");
    for dimension in [raw_rows.len(), pattern_count, word_count] {
        matrix_hash.update(to_u64(dimension).to_le_bytes());
    }
    for word in required_words {
        matrix_hash.update(word.to_le_bytes());
    }
    let mut covered_union = vec![0_u64; word_count];
    let mut normalized_ordinals = BTreeSet::new();
    let mut candidate_keys = BTreeSet::new();
    let mut exported_rows = Vec::with_capacity(raw_rows.len());
    let mut bindings = Vec::with_capacity(raw_rows.len());
    for (source_ordinal, coverage) in raw_rows.iter().enumerate() {
        assert_eq!(coverage.covered_patterns().pattern_count(), pattern_count);
        let key = NormalizedTilingSolutionKey::from_standard_board64_identity(coverage.identity())
            .to_string();
        assert!(
            candidate_keys.insert(key.clone()),
            "duplicate source candidate key"
        );
        let normalized_ordinal = normalized_rows
            .iter()
            .position(|normalized| normalized.solution_key() == key)
            .expect("raw candidate key must have normalized coverage");
        assert!(normalized_ordinals.insert(normalized_ordinal));
        assert_eq!(
            coverage.covered_patterns(),
            normalized_rows[normalized_ordinal].covered_patterns(),
            "candidate identity must bind identical raw/normalized coverage"
        );
        let words = (0..word_count)
            .map(|word| coverage.covered_patterns().word_at(word))
            .collect::<Vec<_>>();
        assert_eq!(words[word_count - 1] & !last_mask, 0);
        for (word_index, word) in words.iter().copied().enumerate() {
            assert_eq!(word & !required_words[word_index], 0);
            covered_union[word_index] |= word;
            matrix_hash.update(word.to_le_bytes());
        }
        exported_rows.push(serde_json::json!({
            "source_ordinal": source_ordinal,
            "normalized_ordinal": normalized_ordinal,
            "candidate_key": key,
            "masks_IOTSZJL_hex": word_hex(&diagnostic_colored_piece_masks(coverage.identity())),
            "coverage_words_hex": word_hex(&words),
        }));
        bindings.push((source_ordinal, normalized_ordinal, key));
    }
    assert_eq!(covered_union, required_words);
    assert_eq!(normalized_ordinals.len(), normalized_rows.len());
    let matrix_digest = matrix_hash.finalize();
    let matrix_sha256 = format!("{matrix_digest:x}");
    let mut binding_hash = Sha256::new();
    binding_hash.update(b"clearra-exact-cover-candidate-binding.v1\0");
    binding_hash.update(matrix_digest);
    for (source_ordinal, normalized_ordinal, key) in &bindings {
        for value in [*source_ordinal, *normalized_ordinal, key.len()] {
            binding_hash.update(to_u64(value).to_le_bytes());
        }
        binding_hash.update(key.as_bytes());
    }
    let candidate_binding_sha256 = format!("{:x}", binding_hash.finalize());
    let mut queue_hash = Sha256::new();
    queue_hash.update(b"clearra-exact-cover-queue-binding.v1\0");
    queue_hash.update(matrix_digest);
    for (ordinal, queue) in queues.iter().enumerate() {
        queue_hash.update(to_u64(ordinal).to_le_bytes());
        queue_hash.update(to_u64(queue.len()).to_le_bytes());
        queue_hash.update(queue.as_bytes());
    }
    let queue_binding_sha256 = format!("{:x}", queue_hash.finalize());
    let required = PatternBitSet::from_words(pattern_count, required_words.to_vec())
        .expect("diagnostic required coverage");
    let rows = raw_rows
        .iter()
        .map(|coverage| coverage.covered_patterns().clone())
        .collect();
    let (normalized_ordinals, candidate_keys) = bindings
        .into_iter()
        .map(|(_, normalized_ordinal, key)| (normalized_ordinal, key))
        .unzip();
    let artifact = serde_json::json!({
        "schema": "clearra-exact-cover-diagnostic-matrix.v1",
        "diagnostic_only": true,
        "source_command": source_command,
        "package_version": env!("CARGO_PKG_VERSION"),
        "source_advances": advances,
        "source_elapsed_ms": started.elapsed().as_millis(),
        "row_order": "source.solution_coverages; not normalized-key order",
        "row_count": raw_rows.len(),
        "pattern_count": pattern_count,
        "word_count": word_count,
        "word_encoding": "fixed-width lowercase hexadecimal u64; pattern bit i is LSB-first",
        "matrix_sha256": matrix_sha256,
        "matrix_hash_encoding": "domain-NUL,row_count,pattern_count,word_count,required_words,ordered_row_words; integers=u64-LE",
        "candidate_binding_sha256": candidate_binding_sha256,
        "queue_binding_sha256": queue_binding_sha256,
        "queue_hash_encoding": "domain-NUL,matrix_digest,(ordinal,queue_byte_length,queue_UTF8)*; integers=u64-LE",
        "candidate_hash_encoding": "domain-NUL,matrix_digest,(source_ordinal,normalized_ordinal,key_byte_length,key_UTF8)*; integers=u64-LE",
        "required_words_hex": word_hex(required_words),
        "queues": queues,
        "rows": exported_rows,
    });
    Ctk3DiagnosticMatrix {
        required,
        rows,
        candidate_keys,
        normalized_ordinals,
        matrix_identity: matrix_digest.into(),
        artifact,
    }
}

/// Presentation-only color unions. The domain adapter iterates the public
/// placement API, ORs every occurrence of each kind in IOTSZJL order, and does
/// not include the initial board. Exact keys still retain placement boundaries.
fn diagnostic_colored_piece_masks(
    identity: clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64TilingIdentity,
) -> [u64; 7] {
    use clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64ColoredTilingIdentity;

    StandardBoard64ColoredTilingIdentity::from_standard_board64_identity(identity).piece_masks()
}

#[test]
fn ctk3_diagnostic_piece_masks_union_repeated_kinds_in_iotszjl_order() {
    use clearra_core_domain::{
        piece::piece_kind::PieceKind,
        solution::normalized_tiling_solution::{
            NormalizedTilingSolutionKey, PiecePlacementMask, StandardBoard64TilingIdentity,
        },
    };

    let identity = StandardBoard64TilingIdentity::from_placements(
        1_u64 << 63,
        [
            PiecePlacementMask::new(PieceKind::L, 0x0f00_0000),
            PiecePlacementMask::new(PieceKind::I, 0x0000_000f),
            PiecePlacementMask::new(PieceKind::J, 0x00f0_0000),
            PiecePlacementMask::new(PieceKind::S, 0x0000_f000),
            PiecePlacementMask::new(PieceKind::I, 0xf000_0000),
            PiecePlacementMask::new(PieceKind::O, 0x0000_00f0),
            PiecePlacementMask::new(PieceKind::Z, 0x000f_0000),
            PiecePlacementMask::new(PieceKind::T, 0x0000_0f00),
        ],
    )
    .expect("nonoverlapping repeated-piece identity");
    assert_eq!(identity.placement_count(), 8);
    let key_before = NormalizedTilingSolutionKey::from_standard_board64_identity(identity);
    assert_eq!(
        diagnostic_colored_piece_masks(identity),
        [
            0xf000_000f,
            0x0000_00f0,
            0x0000_0f00,
            0x0000_f000,
            0x000f_0000,
            0x00f0_0000,
            0x0f00_0000
        ],
    );
    assert_eq!(
        NormalizedTilingSolutionKey::from_standard_board64_identity(identity),
        key_before,
    );
    assert_eq!(
        identity.placement_count(),
        8,
        "color export must not merge exact placements"
    );
}

#[test]
fn ctk3_diagnostic_piece_masks_leave_absent_kinds_and_initial_board_empty() {
    use clearra_core_domain::{
        piece::piece_kind::PieceKind,
        solution::normalized_tiling_solution::{PiecePlacementMask, StandardBoard64TilingIdentity},
    };

    let initial = 0xf000;
    let empty = StandardBoard64TilingIdentity::from_placements(initial, []).unwrap();
    assert_eq!(diagnostic_colored_piece_masks(empty), [0; 7]);
    let one = StandardBoard64TilingIdentity::from_placements(
        initial,
        [PiecePlacementMask::new(PieceKind::O, 0xf)],
    )
    .unwrap();
    assert_eq!(diagnostic_colored_piece_masks(one), [0, 0xf, 0, 0, 0, 0, 0]);
}

fn write_ctk3_diagnostic_matrix(matrix: &Ctk3DiagnosticMatrix) {
    use std::{fs, io::Write, path::Path};

    let matrix_sha256 = matrix.artifact["matrix_sha256"]
        .as_str()
        .expect("diagnostic matrix hash");
    let mut encoded =
        serde_json::to_vec_pretty(&matrix.artifact).expect("serialize diagnostic matrix");
    encoded.push(b'\n');

    // No user-selected output path, overwrite, or traversal through a parent
    // that resolves outside the repository. Existing artifacts are immutable.
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest_dir
        .parent()
        .and_then(Path::parent)
        .expect("matrix exporter repository root")
        .canonicalize()
        .expect("canonical matrix exporter repository root");
    let mut directory = workspace.clone();
    for component in ["_local", "exact-cover-fixtures"] {
        let requested = directory.join(component);
        match fs::create_dir(&requested) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => panic!("create diagnostic matrix directory: {error}"),
        }
        directory = requested
            .canonicalize()
            .expect("canonical diagnostic matrix directory");
        assert!(directory.is_dir() && directory.starts_with(&workspace));
    }
    let artifact_path = directory.join(format!("{matrix_sha256}.json"));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&artifact_path)
        .expect("create new diagnostic matrix; existing files are not overwritten");
    file.write_all(&encoded).expect("write diagnostic matrix");
    file.sync_all().expect("persist diagnostic matrix");
    eprintln!(
        "{}",
        serde_json::json!({
            "phase": "source_matrix_export",
            "path": artifact_path,
            "rows": matrix.rows.len(),
            "patterns": matrix.required.pattern_count(),
            "matrix_sha256": matrix_sha256,
            "candidate_binding_sha256": matrix.artifact["candidate_binding_sha256"],
            "queue_binding_sha256": matrix.artifact["queue_binding_sha256"],
            "elapsed_ms": matrix.artifact["source_elapsed_ms"],
        })
    );
}

/// Same binary and immutable Jstris source, fresh proof/selector in each arm.
/// Timings diagnose the algorithm only; they are not a GUI release verdict.
#[test]
#[ignore = "explicit Jstris 180 residual warm-seed first-canonical A/B probe"]
fn ctk3_jstris_180_residual_warm_seed_first_canonical_ab_probe() {
    run_jstris_first_canonical_ab(
        JstrisMinimumExperiment::WarmSeed,
        "ctk3_jstris_180_residual_warm_seed_first_canonical_ab_probe",
        "CLEARRA_CTK3_JSTRIS_WARM_AB_CHILD",
    );
}

#[test]
#[ignore = "explicit Jstris 180 cached pivot exhaustion first-canonical A/B probe"]
fn ctk3_jstris_180_cached_pivot_exhaustion_first_canonical_ab_probe() {
    run_jstris_first_canonical_ab(
        JstrisMinimumExperiment::CachedPivotExhaustion,
        "ctk3_jstris_180_cached_pivot_exhaustion_first_canonical_ab_probe",
        "CLEARRA_CTK3_JSTRIS_PIVOT_AB_CHILD",
    );
}

#[derive(Clone, Copy)]
enum JstrisMinimumExperiment {
    WarmSeed,
    CachedPivotExhaustion,
}

impl JstrisMinimumExperiment {
    fn phase(self, stage: &str) -> String {
        let name = match self {
            Self::WarmSeed => "warm_seed",
            Self::CachedPivotExhaustion => "cached_pivot_exhaustion",
        };
        format!("jstris_{name}_ab_{stage}")
    }

    fn switches(self, enabled: bool) -> (bool, bool) {
        match self {
            Self::WarmSeed => (enabled, false),
            Self::CachedPivotExhaustion => (false, enabled),
        }
    }
}

#[test]
fn ctk3_diagnostic_minimum_experiments_keep_warm_and_pivot_orthogonal() {
    assert_eq!(
        JstrisMinimumExperiment::WarmSeed.switches(false),
        (false, false),
    );
    assert_eq!(
        JstrisMinimumExperiment::WarmSeed.switches(true),
        (true, false),
    );
    assert_eq!(
        JstrisMinimumExperiment::CachedPivotExhaustion.switches(false),
        (false, false),
    );
    assert_eq!(
        JstrisMinimumExperiment::CachedPivotExhaustion.switches(true),
        (false, true),
    );
}

fn run_jstris_first_canonical_ab(
    experiment: JstrisMinimumExperiment,
    test_name: &str,
    child_env: &str,
) {
    use clearra_coverage::cover::exact_minimum_cover::{
        set_diagnostic_cached_pivot_exhaustion, set_diagnostic_residual_warm_seed,
    };

    const PROCESS_DEADLINE: Duration = Duration::from_secs(240);
    if env::var(child_env).ok().as_deref() != Some(test_name) {
        let mut command = Command::new(env::current_exe().expect("current Jstris A/B executable"));
        command
            .args(["--exact", test_name, "--ignored", "--nocapture"])
            .env(child_env, test_name)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = command.spawn().expect("spawn isolated Jstris A/B child");
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait().expect("poll Jstris A/B child") {
                assert!(
                    status.success(),
                    "Jstris A/B child did not complete: {status}"
                );
                return;
            }
            if started.elapsed() >= PROCESS_DEADLINE {
                let _ = child.kill();
                let _ = child.wait();
                panic!(
                    "Jstris A/B exceeded its 240-second fixture deadline; no completed comparison"
                );
            }
            thread::sleep(Duration::from_millis(50));
        }
    }

    struct ResetExperiments;
    impl Drop for ResetExperiments {
        fn drop(&mut self) {
            set_diagnostic_residual_warm_seed(false);
            set_diagnostic_cached_pivot_exhaustion(false);
        }
    }
    let _reset_experiments = ResetExperiments;
    set_diagnostic_residual_warm_seed(false);
    set_diagnostic_cached_pivot_exhaustion(false);
    let source_command = format!("{SOURCE_COMMAND} --rule jstris-180");
    let matrix = prepare_ctk3_diagnostic_matrix(&source_command, "jstris-180", None);
    let solver_input_started = Instant::now();
    let solver_input = normalized_ctk3_solver_input(&matrix);
    let solver_input_prepare_ms = solver_input_started.elapsed().as_millis();
    let workers = env::var("CLEARRA_CTK3_PARALLEL_WORKERS")
        .ok()
        .map(|value| {
            value
                .parse::<usize>()
                .expect("numeric Jstris A/B worker count")
        })
        .unwrap_or(11);
    assert!(
        (1..=64).contains(&workers),
        "Jstris A/B workers must be 1..64"
    );
    eprintln!(
        "{}",
        serde_json::json!({
            "phase": experiment.phase("source"),
            "source_command": source_command,
            "source_elapsed_ms": matrix.artifact["source_elapsed_ms"],
            "solver_input_prepare_ms": solver_input_prepare_ms,
            "matrix_sha256": matrix.artifact["matrix_sha256"],
            "candidate_binding_sha256": matrix.artifact["candidate_binding_sha256"],
            "queue_binding_sha256": matrix.artifact["queue_binding_sha256"],
            "solver_row_order": "normalized key ascending; candidate ID = solver ordinal + 1",
            "solver_row_binding_sha256": solver_input.binding_sha256,
            "rows": matrix.rows.len(),
            "patterns": matrix.required.pattern_count(),
            "workers": workers,
            "available_parallelism": thread::available_parallelism().map(|n| n.get()).ok(),
            "partitions_per_worker": 4,
            "idle_assistance": true,
            "arm_deadline_ms": 90_000,
            "native_probe_is_product_acceptance": false,
        })
    );
    let baseline = run_jstris_minimum_arm(&matrix, &solver_input, workers, experiment, false);
    let candidate = run_jstris_minimum_arm(&matrix, &solver_input, workers, experiment, true);
    assert_eq!(
        candidate.minimum, baseline.minimum,
        "exact minimum differs between arms"
    );
    assert_eq!(
        candidate.canonical_rows, baseline.canonical_rows,
        "first canonical portfolio differs between arms"
    );
    eprintln!(
        "{}",
        serde_json::json!({
            "phase": experiment.phase("comparison"),
            "complete": true,
            "optimal_cardinality": baseline.minimum,
            "same_first_canonical": true,
            "baseline_total_ms": baseline.total_ms,
            "candidate_total_ms": candidate.total_ms,
            "baseline_proof_ms": baseline.proof_ms,
            "candidate_proof_ms": candidate.proof_ms,
            "baseline_canonical_ms": baseline.canonical_ms,
            "candidate_canonical_ms": candidate.canonical_ms,
            "native_probe_is_product_acceptance": false,
        })
    );
}

struct Ctk3NormalizedSolverInput {
    rows: Vec<PatternBitSet>,
    source_ordinals: Vec<usize>,
    identity: [u8; 32],
    binding_sha256: String,
}

/// The raw matrix export remains unchanged. Only this first-canonical probe
/// follows the App's normalized-key order and its one-based candidate IDs.
fn normalized_ctk3_solver_input(matrix: &Ctk3DiagnosticMatrix) -> Ctk3NormalizedSolverInput {
    use sha2::{Digest, Sha256};

    let mut source_ordinals = (0..matrix.rows.len()).collect::<Vec<_>>();
    source_ordinals.sort_unstable_by_key(|&source| matrix.normalized_ordinals[source]);
    let mut binding = Sha256::new();
    binding.update(b"clearra-normalized-diagnostic-solver-rows.v1\0");
    binding.update(matrix.matrix_identity);
    for (normalized, &source) in source_ordinals.iter().enumerate() {
        assert_eq!(
            matrix.normalized_ordinals[source], normalized,
            "complete normalized permutation"
        );
        let key = &matrix.candidate_keys[source];
        if normalized > 0 {
            assert!(
                matrix.candidate_keys[source_ordinals[normalized - 1]] < *key,
                "App candidate IDs require strictly ascending normalized keys"
            );
        }
        for value in [normalized, source, key.len()] {
            binding.update(
                u64::try_from(value)
                    .expect("solver dimension fits u64")
                    .to_le_bytes(),
            );
        }
        binding.update(key.as_bytes());
    }
    let rows = source_ordinals
        .iter()
        .map(|&source| matrix.rows[source].clone())
        .collect();
    let digest = binding.finalize();
    Ctk3NormalizedSolverInput {
        rows,
        source_ordinals,
        identity: digest.into(),
        binding_sha256: format!("{digest:x}"),
    }
}

#[test]
fn ctk3_diagnostic_normalized_solver_order_preserves_raw_export_mapping() {
    let matrix = diagnostic_three_row_order_fixture();
    let original_artifact = matrix.artifact.clone();
    let input = normalized_ctk3_solver_input(&matrix);
    assert_eq!(input.source_ordinals, [1, 2, 0]);
    assert_eq!(
        input
            .rows
            .iter()
            .map(|row| row.word_at(0))
            .collect::<Vec<_>>(),
        [2, 4, 1]
    );
    assert_eq!(matrix.candidate_keys, ["z", "a", "m"]);
    assert_eq!(matrix.normalized_ordinals, [2, 0, 1]);
    assert_eq!(matrix.artifact, original_artifact);
    assert_jstris_portfolio(&matrix, &input, &[0, 1, 2]);
}

#[test]
fn ctk3_diagnostic_normalized_solver_order_rejects_forged_key_order() {
    let mut matrix = diagnostic_three_row_order_fixture();
    matrix.normalized_ordinals = vec![0, 1, 2];
    assert!(std::panic::catch_unwind(|| normalized_ctk3_solver_input(&matrix)).is_err());
}

fn diagnostic_three_row_order_fixture() -> Ctk3DiagnosticMatrix {
    Ctk3DiagnosticMatrix {
        required: PatternBitSet::from_words(3, vec![7]).unwrap(),
        rows: [1, 2, 4]
            .into_iter()
            .map(|word| PatternBitSet::from_words(3, vec![word]).unwrap())
            .collect(),
        candidate_keys: ["z", "a", "m"].into_iter().map(str::to_owned).collect(),
        normalized_ordinals: vec![2, 0, 1],
        matrix_identity: [0; 32],
        artifact: serde_json::json!({"row_order": "raw source fixture"}),
    }
}

struct JstrisMinimumArm {
    minimum: usize,
    canonical_rows: Vec<usize>,
    proof_ms: u128,
    canonical_ms: u128,
    total_ms: u128,
}

fn run_jstris_minimum_arm(
    matrix: &Ctk3DiagnosticMatrix,
    solver_input: &Ctk3NormalizedSolverInput,
    workers: usize,
    experiment: JstrisMinimumExperiment,
    enabled: bool,
) -> JstrisMinimumArm {
    use clearra_coverage::cover::exact_minimum_cover::{
        set_diagnostic_cached_pivot_exhaustion, set_diagnostic_residual_warm_seed,
    };
    use clearra_coverage::cover::{
        ExactMinimumCoverPortfolioPreparationAdvance, ExactMinimumCoverPortfolioPreparationSession,
    };
    use sha2::{Digest, Sha256};

    // Set before creating any workspace or child thread. The previous arm's
    // complete physical drain is guaranteed by the scoped wave scheduler.
    let (warm_seed, pivot_exhaustion) = experiment.switches(enabled);
    set_diagnostic_residual_warm_seed(warm_seed);
    set_diagnostic_cached_pivot_exhaustion(pivot_exhaustion);
    let started = Instant::now();
    let deadline = started + Duration::from_secs(90);
    let phase_identity = |phase: &[u8]| -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(b"clearra-jstris-warm-seed-ab.v1\0");
        hash.update(solver_input.identity);
        hash.update(phase);
        hash.finalize().into()
    };
    let mut proof =
        ExactMinimumCoverPortfolioPreparationSession::new(&matrix.required, &solver_input.rows)
            .expect("fresh Jstris exact minimum preparation");
    proof
        .enable_parallel(workers * 4, phase_identity(b"proof"))
        .unwrap();
    let mut last_query = None;
    let mut waves = 0usize;
    let (minimum, mut enumerator) = loop {
        assert!(
            Instant::now() < deadline,
            "Jstris experiment arm incomplete at proof deadline"
        );
        if let Some(query) = proof
            .parallel_query()
            .cloned()
            .filter(|query| Some(query.identity()) != last_query)
        {
            last_query = Some(query.identity());
            waves += 1;
            let wave_started = Instant::now();
            let limit = query.limit();
            let tasks = parallel_wave::run_until(
                query,
                parallel_wave::Owner::Proof(&mut proof),
                workers,
                true,
                deadline,
            );
            eprintln!(
                "{}",
                serde_json::json!({
                    "phase": experiment.phase("proof_wave"), "warm_seed": warm_seed,
                    "cached_pivot_exhaustion": pivot_exhaustion,
                    "wave": waves, "limit": limit, "tasks": tasks,
                    "elapsed_ms": wave_started.elapsed().as_millis(),
                })
            );
        }
        match proof
            .advance_with_memory_guard_and_control(128, &mut |_| Ok(()), &mut || {
                Instant::now() >= deadline
            })
            .expect("Jstris exact minimum advance")
        {
            ExactMinimumCoverPortfolioPreparationAdvance::Pending { .. } => {}
            ExactMinimumCoverPortfolioPreparationAdvance::Coverable {
                proof, enumerator, ..
            } => {
                assert!(proof.complete(), "minimum must be proved, not an incumbent");
                assert_eq!(proof.covered_patterns(), &matrix.required);
                assert_jstris_portfolio(matrix, solver_input, proof.row_indices());
                break (proof.row_indices().len(), enumerator);
            }
            other => {
                panic!("Jstris proof incomplete or cancelled, not a negative proof: {other:?}")
            }
        }
    };
    let proof_ms = started.elapsed().as_millis();
    enumerator
        .enable_parallel(workers * 4, phase_identity(b"canonical"))
        .unwrap();
    let canonical_started = Instant::now();
    let mut last_query = None;
    let mut waves = 0usize;
    let canonical_rows = loop {
        assert!(
            Instant::now() < deadline,
            "Jstris experiment arm incomplete at canonical deadline"
        );
        if let Some(query) = enumerator
            .parallel_query()
            .cloned()
            .filter(|query| Some(query.identity()) != last_query)
        {
            last_query = Some(query.identity());
            waves += 1;
            let wave_started = Instant::now();
            let limit = query.limit();
            let tasks = parallel_wave::run_until(
                query,
                parallel_wave::Owner::Canonical(&mut enumerator),
                workers,
                true,
                deadline,
            );
            eprintln!(
                "{}",
                serde_json::json!({
                    "phase": experiment.phase("canonical_wave"), "warm_seed": warm_seed,
                    "cached_pivot_exhaustion": pivot_exhaustion,
                    "wave": waves, "limit": limit, "tasks": tasks,
                    "elapsed_ms": wave_started.elapsed().as_millis(),
                })
            );
        }
        let page = enumerator
            .next_page_owned_with_memory_guard_and_control(1, 128, &mut |_| Ok(()), &mut || {
                Instant::now() >= deadline
            })
            .expect("Jstris exact first canonical advance");
        if let Some(first) = page.portfolios().first() {
            assert_eq!(first.row_indices().len(), minimum);
            assert_jstris_portfolio(matrix, solver_input, first.row_indices());
            break first.row_indices().to_vec();
        }
        assert_eq!(
            page.stop(),
            ExactMinimumCoverEnumerationStop::WorkBudgetExhausted,
            "Jstris canonical result incomplete or cancelled"
        );
    };
    assert!(
        Instant::now() < deadline,
        "Jstris arm completed outside its fixture deadline"
    );
    let result = JstrisMinimumArm {
        minimum,
        canonical_rows,
        proof_ms,
        canonical_ms: canonical_started.elapsed().as_millis(),
        total_ms: started.elapsed().as_millis(),
    };
    eprintln!(
        "{}",
        serde_json::json!({
            "phase": experiment.phase("arm"), "warm_seed": warm_seed, "complete": true,
            "cached_pivot_exhaustion": pivot_exhaustion,
            "optimal_cardinality": result.minimum,
            "canonical_candidate_ids": result.canonical_rows.iter().map(|&row| row + 1).collect::<Vec<_>>(),
            "source_rows": result.canonical_rows.iter().map(|&row| solver_input.source_ordinals[row]).collect::<Vec<_>>(),
            "normalized_ordinals": result.canonical_rows,
            "candidate_keys": result.canonical_rows.iter().map(|&row| &matrix.candidate_keys[solver_input.source_ordinals[row]]).collect::<Vec<_>>(),
            "proof_ms": result.proof_ms, "canonical_ms": result.canonical_ms, "total_ms": result.total_ms,
            "native_probe_is_product_acceptance": false,
        })
    );
    result
}

fn assert_jstris_portfolio(
    matrix: &Ctk3DiagnosticMatrix,
    solver_input: &Ctk3NormalizedSolverInput,
    selected: &[usize],
) {
    assert!(
        !selected.is_empty(),
        "nonempty required matrix needs a cover"
    );
    assert!(
        selected.windows(2).all(|pair| pair[0] < pair[1]),
        "canonical row order"
    );
    let mut union = vec![0u64; matrix.required.word_count()];
    for &index in selected {
        let &source = solver_input
            .source_ordinals
            .get(index)
            .expect("solver ordinal has original source");
        let row = matrix
            .rows
            .get(source)
            .expect("portfolio row belongs to original source");
        assert_eq!(
            row, &solver_input.rows[index],
            "solver permutation preserves source coverage"
        );
        for (word, covered) in union.iter_mut().enumerate() {
            *covered |= row.word_at(word);
        }
    }
    let covered = PatternBitSet::from_words(matrix.required.pattern_count(), union)
        .expect("selected source row union");
    assert_eq!(
        covered, matrix.required,
        "selected source rows cover exactly the required universe"
    );
}

/// Same-process release A/B for optional residual-dual admission. Keeping the
/// source matrix, executable, allocator state, and machine conditions shared
/// prevents a cold-link or cross-run timing difference from masquerading as a
/// solver improvement. A candidate timeout is evidence, not a test failure;
/// the all-admitted authority baseline must always terminate and prove 25.
#[test]
#[ignore = "explicit CTK3 residual-dual admission A/B probe"]
fn ctk3_residual_dual_admission_ab_probe() {
    let (required, rows) = ctk3_exact_cover_input();
    let candidate_name = env::var(PROBE_RESIDUAL_AB_CANDIDATE_ENV)
        .unwrap_or_else(|_| "skip-depth-14-plus".to_owned());
    if candidate_name == "cutoff-exp-cross" {
        let dense = ExactMinimumCoverResidualAdmissionPolicy {
            use_sparse_proposal_softmax: false,
            ..ExactMinimumCoverResidualAdmissionPolicy::default()
        };
        let sparse = ExactMinimumCoverResidualAdmissionPolicy::default();
        let variants = [
            ("dense-first", dense),
            ("cutoff-exp-first", sparse),
            ("cutoff-exp-second", sparse),
            ("dense-second", dense),
        ];
        let mut expected = None;
        for (name, policy) in variants {
            let observed = drive_residual_admission_variant(name, &required, &rows, policy)
                .expect("cross-ordered softmax A/B variant must terminate");
            assert_eq!(observed.len(), 25);
            if let Some(expected) = expected.as_ref() {
                assert_eq!(
                    &observed, expected,
                    "proposal-only softmax must preserve deterministic proof identity"
                );
            } else {
                expected = Some(observed);
            }
        }
        return;
    }

    let baseline = drive_residual_admission_variant(
        "all",
        &required,
        &rows,
        ExactMinimumCoverResidualAdmissionPolicy::default(),
    )
    .expect("all-admitted residual baseline must terminate");
    assert_eq!(baseline.len(), 25);

    let candidate_specs = match candidate_name.as_str() {
        "skip-depth-14-plus" => vec![(
            candidate_name,
            ExactMinimumCoverResidualAdmissionPolicy {
                maximum_search_depth: 13,
                ..ExactMinimumCoverResidualAdmissionPolicy::default()
            },
        )],
        "skip-gap-1" => vec![(
            candidate_name,
            ExactMinimumCoverResidualAdmissionPolicy {
                minimum_dual_gap: 2,
                ..ExactMinimumCoverResidualAdmissionPolicy::default()
            },
        )],
        "only-depth-10-through-13" => vec![(
            candidate_name,
            ExactMinimumCoverResidualAdmissionPolicy {
                minimum_search_depth: 10,
                maximum_search_depth: 13,
                ..ExactMinimumCoverResidualAdmissionPolicy::default()
            },
        )],
        "depth-through-13-gap-2-plus" => vec![(
            candidate_name,
            ExactMinimumCoverResidualAdmissionPolicy {
                minimum_dual_gap: 2,
                maximum_search_depth: 13,
                ..ExactMinimumCoverResidualAdmissionPolicy::default()
            },
        )],
        "caps-100-150" => vec![
            (
                "cap-100".to_owned(),
                ExactMinimumCoverResidualAdmissionPolicy {
                    maximum_iterations_per_attempt: 100,
                    ..ExactMinimumCoverResidualAdmissionPolicy::default()
                },
            ),
            (
                "cap-150".to_owned(),
                ExactMinimumCoverResidualAdmissionPolicy {
                    maximum_iterations_per_attempt: 150,
                    ..ExactMinimumCoverResidualAdmissionPolicy::default()
                },
            ),
        ],
        other => panic!("unknown residual A/B candidate: {other}"),
    };
    for (candidate_name, candidate_policy) in candidate_specs {
        if let Some(candidate) =
            drive_residual_admission_variant(&candidate_name, &required, &rows, candidate_policy)
        {
            assert_eq!(candidate.len(), 25);
            assert_eq!(
                candidate, baseline,
                "optional residual admission must preserve deterministic proof identity"
            );
        }
    }
}

/// Separates the positive witness shortcut from its exact fallback for the
/// actual first CTK3 canonical selector. This is intentionally ignored and
/// diagnostic-only; it carries no enumeration authority.
#[test]
#[ignore = "explicit CTK3 first-selector witness cursor probe"]
fn ctk3_first_selector_witness_cursor_probe() {
    let (required, rows) = ctk3_exact_cover_input();
    let selector_end = EXPECTED_RAW_PROOF_SOURCE_ROWS[0];
    let pattern_count = required.pattern_count() + 1;
    let word_count = pattern_count.div_ceil(u64::BITS as usize);
    let mut required_words = (0..word_count)
        .map(|word| required.word_at(word))
        .collect::<Vec<_>>();
    required_words[required.pattern_count() / u64::BITS as usize] |=
        1_u64 << (required.pattern_count() % u64::BITS as usize);
    let selector_required = PatternBitSet::from_words(pattern_count, required_words)
        .expect("CTK3 first-selector required set");
    let selector_rows = rows
        .iter()
        .enumerate()
        .map(|(row_index, row)| {
            let mut words = (0..word_count)
                .map(|word| row.word_at(word))
                .collect::<Vec<_>>();
            if row_index < selector_end {
                words[required.pattern_count() / u64::BITS as usize] |=
                    1_u64 << (required.pattern_count() % u64::BITS as usize);
            }
            PatternBitSet::from_words(pattern_count, words).expect("CTK3 first-selector row")
        })
        .collect::<Vec<_>>();

    let blocking_started = Instant::now();
    let blocking = diagnostic_blocking_witness_shortcut(
        &selector_required,
        &selector_rows,
        EXPECTED_RAW_PROOF_SOURCE_ROWS.len(),
        &EXPECTED_RAW_PROOF_SOURCE_ROWS,
    )
    .expect("blocking CTK3 positive shortcut");
    eprintln!(
        "{{\"phase\":\"first_selector_blocking_shortcut\",\"elapsed_ms\":{},\"outcome\":{:?}}}",
        blocking_started.elapsed().as_millis(),
        blocking,
    );

    let mut session = ExactMinimumCoverSession::diagnostic_at_most_with_witness(
        &selector_required,
        &selector_rows,
        EXPECTED_RAW_PROOF_SOURCE_ROWS.len(),
        &EXPECTED_RAW_PROOF_SOURCE_ROWS,
    )
    .expect("resumable CTK3 first-selector session");
    let started = Instant::now();
    let mut work_units = 0_u64;
    let mut advances = 0_u64;
    let mut previous = session.diagnostic_execution_state();
    let mut previous_coarse = (
        previous.phase,
        previous.witness_phase,
        previous.supporter_position,
    );
    let mut next_search_log = 10_000_u64;
    eprintln!(
        "{{\"phase\":\"first_selector_cursor_state\",\"work_units\":0,\"state\":{:?}}}",
        previous,
    );
    let outcome = loop {
        if work_units >= 120_000 || started.elapsed() >= Duration::from_secs(20) {
            break None;
        }
        advances += 1;
        let advance = session
            .advance(128)
            .expect("resumable CTK3 first-selector advance");
        let consumed = match &advance {
            ExactMinimumCoverSessionAdvance::Pending { visited_nodes }
            | ExactMinimumCoverSessionAdvance::Found { visited_nodes, .. }
            | ExactMinimumCoverSessionAdvance::ProvedNone { visited_nodes }
            | ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes } => *visited_nodes,
            ExactMinimumCoverSessionAdvance::Finished => 0,
        };
        work_units += consumed;
        let current = session.diagnostic_execution_state();
        let current_coarse = (
            current.phase,
            current.witness_phase,
            current.supporter_position,
        );
        if current_coarse != previous_coarse || current.search_nodes >= next_search_log {
            eprintln!(
                "{{\"phase\":\"first_selector_cursor_state\",\"elapsed_ms\":{},\"advances\":{},\"work_units\":{},\"state\":{:?}}}",
                started.elapsed().as_millis(),
                advances,
                work_units,
                current,
            );
            while current.search_nodes >= next_search_log {
                next_search_log = next_search_log.saturating_add(10_000);
            }
        }
        previous_coarse = current_coarse;
        previous = current;
        match advance {
            ExactMinimumCoverSessionAdvance::Pending { .. } => {}
            terminal => break Some(terminal),
        }
    };
    eprintln!(
        "{{\"phase\":\"first_selector_cursor_summary\",\"elapsed_ms\":{},\"advances\":{},\"work_units\":{},\"state\":{:?},\"outcome\":{:?}}}",
        started.elapsed().as_millis(),
        advances,
        work_units,
        previous,
        outcome,
    );
    if let ExactMinimumCoverWitnessShortcutDiagnostics::Found(blocking_rows) = blocking {
        assert_eq!(blocking_rows.len(), 25);
        assert!(blocking_rows[0] < selector_end);
    }
}

/// Compares the current second-coordinate range oracle with the bounded
/// adaptive alternative: prove the lex-smallest fixed row first, then prove
/// the remaining range only if that row is impossible. This is diagnostic
/// evidence only; portfolio authority continues to live in the coverage
/// crate.
#[test]
#[ignore = "explicit CTK3 second-coordinate negative-oracle A/B probe"]
fn ctk3_second_coordinate_negative_oracle_probe() {
    let (required, rows) = ctk3_exact_cover_input();
    let variants = [
        ("range-1-through-5", vec![0], 1, Some(6)),
        ("fixed-row-1", vec![0, 1], 2, None),
        ("fallback-range-2-through-5", vec![0], 2, Some(6)),
    ];
    for (name, prefix, start, selector_end) in variants {
        let (query_required, query_rows, slots) =
            ctk3_lex_query(&required, &rows, &prefix, start, selector_end);
        let mut session =
            ExactMinimumCoverSession::diagnostic_at_most(&query_required, &query_rows, slots)
                .expect("CTK3 negative oracle session");
        let started = Instant::now();
        let mut advances = 0_u64;
        let mut work_units = 0_u64;
        let mut max_advance = Duration::ZERO;
        let terminal = loop {
            if started.elapsed() >= Duration::from_secs(30) || work_units >= 200_000 {
                break "timeout";
            }
            advances += 1;
            let advance_started = Instant::now();
            let advance = session.advance(128).expect("CTK3 negative oracle advance");
            max_advance = max_advance.max(advance_started.elapsed());
            let consumed = match advance {
                ExactMinimumCoverSessionAdvance::Pending { visited_nodes } => visited_nodes,
                ExactMinimumCoverSessionAdvance::Found { visited_nodes, .. } => {
                    work_units += visited_nodes;
                    break "found";
                }
                ExactMinimumCoverSessionAdvance::ProvedNone { visited_nodes } => {
                    work_units += visited_nodes;
                    break "proved-none";
                }
                ExactMinimumCoverSessionAdvance::Cancelled { visited_nodes } => {
                    work_units += visited_nodes;
                    break "cancelled";
                }
                ExactMinimumCoverSessionAdvance::Finished => break "finished-without-decision",
            };
            work_units += consumed;
        };
        eprintln!(
            "{{\"phase\":\"second_coordinate_ab\",\"variant\":\"{}\",\"elapsed_ms\":{},\"advances\":{},\"work_units\":{},\"max_advance_us\":{},\"terminal\":\"{}\",\"state\":{:?}}}",
            name,
            started.elapsed().as_millis(),
            advances,
            work_units,
            max_advance.as_micros(),
            terminal,
            session.diagnostic_execution_state(),
        );
    }
}

fn ctk3_lex_query(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
    prefix: &[usize],
    start: usize,
    selector_end: Option<usize>,
) -> (PatternBitSet, Vec<PatternBitSet>, usize) {
    assert!(prefix.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(prefix.last().is_none_or(|row| *row < start));
    let mut covered = vec![0_u64; required.word_count()];
    for row in prefix.iter().copied() {
        for (word_index, covered_word) in covered.iter_mut().enumerate() {
            *covered_word |= rows[row].word_at(word_index);
        }
    }
    let pattern_count = required.pattern_count() + usize::from(selector_end.is_some());
    let word_count = pattern_count.div_ceil(u64::BITS as usize);
    let mut required_words = vec![0_u64; word_count];
    for word_index in 0..required.word_count() {
        required_words[word_index] = required.word_at(word_index) & !covered[word_index];
    }
    if selector_end.is_some() {
        required_words[required.pattern_count() / u64::BITS as usize] |=
            1_u64 << (required.pattern_count() % u64::BITS as usize);
    }
    let query_required = PatternBitSet::from_words(pattern_count, required_words)
        .expect("CTK3 lex query required set");
    let query_rows = rows
        .iter()
        .enumerate()
        .skip(start)
        .map(|(row_index, row)| {
            let mut words = vec![0_u64; word_count];
            for (word_index, word) in words.iter_mut().enumerate() {
                *word = row.word_at(word_index);
            }
            if selector_end.is_some_and(|end| row_index < end) {
                words[required.pattern_count() / u64::BITS as usize] |=
                    1_u64 << (required.pattern_count() % u64::BITS as usize);
            }
            PatternBitSet::from_words(pattern_count, words).expect("CTK3 lex query row")
        })
        .collect::<Vec<_>>();
    (
        query_required,
        query_rows,
        EXPECTED_FIRST_CANONICAL_SOURCE_ROWS.len() - prefix.len(),
    )
}

fn ctk3_exact_cover_input() -> (PatternBitSet, Vec<PatternBitSet>) {
    let request = WasmCommandRuntime::default()
        .compile_command_text(SOURCE_COMMAND)
        .expect("CTK3 A/B source request");
    let AppCommand::Scenario(pc) = request.command() else {
        panic!("CTK3 A/B source must compile as a PC scenario")
    };
    let problem = ProblemCompiler::compile_scenario_pc(pc.query()).expect("CTK3 A/B problem");
    let mut session = WasmCpuSearchSession::new(&problem).expect("CTK3 A/B source session");
    let control = ExecutionControl::default();
    let result = loop {
        match session
            .advance(8_192, &control)
            .expect("CTK3 A/B source advance")
        {
            WasmCpuSearchAdvance::Pending => {}
            WasmCpuSearchAdvance::Completed(result) => break result,
            WasmCpuSearchAdvance::Cancelled => panic!("CTK3 A/B source unexpectedly cancelled"),
        }
    };
    let pattern_count = result
        .field("coverage_pattern_count")
        .expect("CTK3 A/B pattern count")
        .parse::<usize>()
        .expect("numeric CTK3 A/B pattern count");
    let required =
        PatternBitSet::from_words(pattern_count, result.coverage_pattern_words().to_vec())
            .expect("CTK3 A/B required coverage");
    let rows = result
        .solution_coverages()
        .iter()
        .map(|coverage| coverage.covered_patterns().clone())
        .collect::<Vec<_>>();
    assert_eq!(pattern_count, 5_040);
    assert_eq!(rows.len(), 246);
    (required, rows)
}

fn drive_residual_admission_variant(
    name: &str,
    required: &PatternBitSet,
    rows: &[PatternBitSet],
    policy: ExactMinimumCoverResidualAdmissionPolicy,
) -> Option<Vec<usize>> {
    let mut session = ExactMinimumCoverSession::new(required, rows)
        .expect("CTK3 A/B minimum session construction");
    while session.diagnostic_incumbent_progress().is_none() {
        match session.advance(1).expect("CTK3 A/B preparation advance") {
            ExactMinimumCoverSessionAdvance::Pending { .. } => {}
            terminal => panic!("CTK3 A/B preparation terminated unexpectedly: {terminal:?}"),
        }
    }
    assert!(
        session.diagnostic_set_residual_admission(policy),
        "CTK3 A/B residual policy must attach before search"
    );
    assert!(session.diagnostic_reset_hot_cost());

    let started = Instant::now();
    let mut advances = 0_u64;
    let mut work_units = 0_u64;
    let mut maximum_advance = Duration::ZERO;
    let mut last_residual = session
        .diagnostic_residual_progress()
        .expect("CTK3 A/B initial residual diagnostics");
    let mut last_hot = session
        .diagnostic_hot_cost()
        .expect("CTK3 A/B initial hot-cost diagnostics");
    loop {
        if started.elapsed() >= RESIDUAL_AB_VARIANT_TIMEOUT {
            let residual = session
                .diagnostic_residual_progress()
                .expect("CTK3 A/B timeout residual diagnostics");
            let hot = session
                .diagnostic_hot_cost()
                .expect("CTK3 A/B timeout hot-cost diagnostics");
            eprintln!(
                "{{\"phase\":\"residual_admission_ab\",\"name\":\"{}\",\"terminal\":false,\"elapsed_ms\":{},\"advances\":{},\"work_units\":{},\"max_advance_us\":{},\"search_nodes\":{},\"proposal_attempts\":{},\"proposal_iterations\":{},\"certified_prunes\":{},\"remaining_proposal_iterations\":{},\"mirror_prox_ms\":{},\"softmax_ms\":{},\"gradient_ms\":{},\"softmax_cutoff_entries\":{},\"softmax_entries\":{},\"q_cutoff_incidences\":{},\"q_incidences\":{}}}",
                name,
                started.elapsed().as_millis(),
                advances,
                work_units,
                maximum_advance.as_micros(),
                residual.search_nodes,
                residual.proposal_attempts,
                residual.proposal_iterations,
                residual.certified_prunes,
                residual.remaining_proposal_iterations,
                hot.mirror_prox_nanoseconds / 1_000_000,
                (hot.softmax_p_nanoseconds
                    + hot.softmax_q_nanoseconds
                    + hot.softmax_middle_p_nanoseconds
                    + hot.softmax_middle_q_nanoseconds)
                    / 1_000_000,
                (hot.first_gradient_nanoseconds + hot.middle_gradient_nanoseconds) / 1_000_000,
                hot.softmax_p_cutoff_entries
                    + hot.softmax_q_cutoff_entries
                    + hot.softmax_middle_p_cutoff_entries
                    + hot.softmax_middle_q_cutoff_entries,
                hot.softmax_p_entries
                    + hot.softmax_q_entries
                    + hot.softmax_middle_p_entries
                    + hot.softmax_middle_q_entries,
                hot.softmax_q_cutoff_row_incidences
                    + hot.softmax_middle_q_cutoff_row_incidences,
                hot.softmax_q_row_incidences + hot.softmax_middle_q_row_incidences,
            );
            return None;
        }
        advances = advances.saturating_add(1);
        let advance_started = Instant::now();
        let outcome = session.advance(2_048).expect("CTK3 A/B exact advance");
        maximum_advance = maximum_advance.max(advance_started.elapsed());
        if let Some(residual) = session.diagnostic_residual_progress() {
            last_residual = residual;
        }
        if let Some(hot) = session.diagnostic_hot_cost() {
            last_hot = hot;
        }
        match outcome {
            ExactMinimumCoverSessionAdvance::Pending { visited_nodes } => {
                work_units = work_units.saturating_add(visited_nodes);
            }
            ExactMinimumCoverSessionAdvance::Found {
                result,
                visited_nodes,
            } => {
                work_units = work_units.saturating_add(visited_nodes);
                assert_eq!(result.row_indices().len(), 25);
                assert!(is_superset(result.covered_patterns(), required));
                eprintln!(
                    "{{\"phase\":\"residual_admission_ab\",\"name\":\"{}\",\"terminal\":true,\"elapsed_ms\":{},\"advances\":{},\"work_units\":{},\"max_advance_us\":{},\"cardinality\":{},\"row_indices\":{:?},\"search_nodes\":{},\"proposal_attempts\":{},\"proposal_iterations\":{},\"certified_prunes\":{},\"remaining_proposal_iterations\":{},\"attempts_by_gap\":{:?},\"iterations_by_gap\":{:?},\"prunes_by_gap\":{:?},\"attempts_by_depth\":{:?},\"iterations_by_depth\":{:?},\"prunes_by_depth\":{:?},\"prunes_by_checkpoint\":{:?},\"mirror_prox_ms\":{},\"softmax_ms\":{},\"gradient_ms\":{},\"softmax_cutoff_entries\":{},\"softmax_entries\":{},\"q_cutoff_incidences\":{},\"q_incidences\":{}}}",
                    name,
                    started.elapsed().as_millis(),
                    advances,
                    work_units,
                    maximum_advance.as_micros(),
                    result.row_indices().len(),
                    result.row_indices(),
                    last_residual.search_nodes,
                    last_residual.proposal_attempts,
                    last_residual.proposal_iterations,
                    last_residual.certified_prunes,
                    last_residual.remaining_proposal_iterations,
                    last_residual.proposal_attempts_by_dual_gap,
                    last_residual.proposal_iterations_by_dual_gap,
                    last_residual.certified_prunes_by_dual_gap,
                    last_residual.proposal_attempts_by_depth,
                    last_residual.proposal_iterations_by_depth,
                    last_residual.certified_prunes_by_depth,
                    last_residual.certified_prunes_by_checkpoint,
                    last_hot.mirror_prox_nanoseconds / 1_000_000,
                    (last_hot.softmax_p_nanoseconds
                        + last_hot.softmax_q_nanoseconds
                        + last_hot.softmax_middle_p_nanoseconds
                        + last_hot.softmax_middle_q_nanoseconds)
                        / 1_000_000,
                    (last_hot.first_gradient_nanoseconds
                        + last_hot.middle_gradient_nanoseconds)
                        / 1_000_000,
                    last_hot.softmax_p_cutoff_entries
                        + last_hot.softmax_q_cutoff_entries
                        + last_hot.softmax_middle_p_cutoff_entries
                        + last_hot.softmax_middle_q_cutoff_entries,
                    last_hot.softmax_p_entries
                        + last_hot.softmax_q_entries
                        + last_hot.softmax_middle_p_entries
                        + last_hot.softmax_middle_q_entries,
                    last_hot.softmax_q_cutoff_row_incidences
                        + last_hot.softmax_middle_q_cutoff_row_incidences,
                    last_hot.softmax_q_row_incidences
                        + last_hot.softmax_middle_q_row_incidences,
                );
                return Some(result.row_indices().to_vec());
            }
            terminal => panic!("CTK3 A/B exact proof terminated unexpectedly: {terminal:?}"),
        }
    }
}

/// Reproduces the CTK3 minimum-cover bottleneck without making every test run
/// pay for an intentionally expensive exact proof. Run with:
///
/// `cargo test -p clearra-wasm --test pc_minimals_ctk3_stage_probe -- --ignored --nocapture`
#[test]
#[ignore = "explicit CTK3 exact-minimum-cover performance probe"]
fn ctk3_minimum_cover_stage_probe() {
    if env::var_os(PROBE_CHILD_ENV).is_none() {
        supervise_ctk3_probe();
        return;
    }
    run_ctk3_probe_child();
}

fn supervise_ctk3_probe() {
    supervise_ctk3_named_probe("ctk3_minimum_cover_stage_probe");
}

fn supervise_ctk3_named_probe(test_name: &str) {
    let timeout = configured_probe_timeout();
    eprintln!(
        "{{\"phase\":\"supervisor\",\"timeout_ms\":{}}}",
        timeout.as_millis()
    );
    let executable = env::current_exe().expect("current CTK3 probe executable");
    let mut child = Command::new(executable)
        .arg("--exact")
        .arg(test_name)
        .arg("--ignored")
        .arg("--nocapture")
        .env(PROBE_CHILD_ENV, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn isolated CTK3 probe child");
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().expect("poll CTK3 probe child") {
            assert!(status.success(), "CTK3 probe child failed with {status}");
            return;
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            panic!(
                "CTK3 probe exceeded its {} ms process deadline",
                timeout.as_millis()
            );
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Explicit native predictor for the same portable partition/oracle contract
/// relayed by the browser ABI. The expected 25 is checked only AFTER the proof;
/// it is never passed to the solver as an incumbent or minimum authority.
#[test]
#[ignore = "explicit parallel CTK3 proof and first canonical performance probe"]
fn ctk3_parallel_minimum_cover_stage_probe() {
    use clearra_coverage::cover::{
        ExactMinimumCoverPortfolioPreparationAdvance, ExactMinimumCoverPortfolioPreparationSession,
    };
    if env::var_os(PROBE_CHILD_ENV).is_none() {
        supervise_ctk3_named_probe("ctk3_parallel_minimum_cover_stage_probe");
        return;
    }
    let workers = env::var("CLEARRA_CTK3_PARALLEL_WORKERS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (1..=64).contains(value))
        .unwrap_or(10);
    let partitions_per_worker = env::var("CLEARRA_CTK3_PARTITIONS_PER_WORKER")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (1..=64).contains(value))
        .unwrap_or(4);
    let assistance = env::var("CLEARRA_CTK3_IDLE_ASSIST").is_ok_and(|value| value == "1");
    eprintln!(
        "{{\"phase\":\"parallel_probe_configuration\",\"workers\":{workers},\"partitions_per_worker\":{partitions_per_worker},\"idle_assistance\":{assistance}}}"
    );
    let (required, rows) = ctk3_exact_cover_input();
    let mut proof = ExactMinimumCoverPortfolioPreparationSession::new(&required, &rows).unwrap();
    proof
        .enable_parallel(workers * partitions_per_worker, [1; 32])
        .unwrap();
    let proof_started = Instant::now();
    let mut proof_waves = 0;
    let mut proof_last_query = None;
    let mut enumerator = loop {
        assert!(
            proof_started.elapsed() < configured_probe_timeout(),
            "parallel proof fixture deadline"
        );
        if let Some(query) = proof
            .parallel_query()
            .cloned()
            .filter(|query| Some(query.identity()) != proof_last_query)
        {
            proof_last_query = Some(query.identity());
            proof_waves += 1;
            let limit = query.limit();
            let wave_started = Instant::now();
            let task_count = parallel_wave::run(
                query,
                parallel_wave::Owner::Proof(&mut proof),
                workers,
                assistance,
            );
            eprintln!("{{\"phase\":\"parallel_proof_query\",\"wave\":{proof_waves},\"limit\":{limit},\"tasks\":{task_count},\"elapsed_ms\":{}}}", wave_started.elapsed().as_millis());
        }
        match proof
            .advance_with_memory_guard_and_control(128, &mut |_| Ok(()), &mut || false)
            .unwrap()
        {
            ExactMinimumCoverPortfolioPreparationAdvance::Pending { .. } => {}
            ExactMinimumCoverPortfolioPreparationAdvance::Coverable {
                proof, enumerator, ..
            } => {
                assert_eq!(proof.row_indices().len(), 25);
                assert!(proof.complete());
                assert_eq!(proof.covered_patterns(), &required);
                eprintln!(
                    "{{\"phase\":\"parallel_proof_witness\",\"rows\":{:?}}}",
                    proof.row_indices()
                );
                break enumerator;
            }
            other => panic!("parallel proof did not establish exact minimum: {other:?}"),
        }
    };
    eprintln!("{{\"phase\":\"parallel_minimum_proof\",\"workers\":{workers},\"waves\":{proof_waves},\"elapsed_ms\":{},\"optimal_cardinality\":25}}", proof_started.elapsed().as_millis());
    enumerator
        .enable_parallel(workers * partitions_per_worker, [2; 32])
        .unwrap();
    let canonical_started = Instant::now();
    let mut canonical_waves = 0;
    let mut canonical_last_query = None;
    loop {
        assert!(
            canonical_started.elapsed() < configured_probe_timeout(),
            "parallel canonical fixture deadline"
        );
        if let Some(query) = enumerator
            .parallel_query()
            .cloned()
            .filter(|query| Some(query.identity()) != canonical_last_query)
        {
            canonical_last_query = Some(query.identity());
            canonical_waves += 1;
            let limit = query.limit();
            let wave_started = Instant::now();
            let task_count = parallel_wave::run(
                query,
                parallel_wave::Owner::Canonical(&mut enumerator),
                workers,
                assistance,
            );
            eprintln!("{{\"phase\":\"parallel_canonical_query\",\"wave\":{canonical_waves},\"limit\":{limit},\"tasks\":{task_count},\"elapsed_ms\":{}}}", wave_started.elapsed().as_millis());
        }
        let page = enumerator
            .next_page_owned_with_memory_guard_and_control(1, 128, &mut |_| Ok(()), &mut || false)
            .unwrap();
        if let Some(first) = page.portfolios().first() {
            assert_eq!(first.row_indices(), EXPECTED_FIRST_CANONICAL_SOURCE_ROWS);
            eprintln!("{{\"phase\":\"parallel_first_canonical\",\"workers\":{workers},\"waves\":{canonical_waves},\"elapsed_ms\":{},\"rows\":{:?}}}", canonical_started.elapsed().as_millis(), first.row_indices());
            break;
        }
        assert_eq!(
            page.stop(),
            ExactMinimumCoverEnumerationStop::WorkBudgetExhausted
        );
    }
}

fn configured_probe_timeout() -> Duration {
    env::var(PROBE_TIMEOUT_SECONDS_ENV)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|seconds| (1..=MAX_PROBE_TIMEOUT_SECONDS).contains(seconds))
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_PROBE_TIMEOUT)
}

fn run_ctk3_probe_child() {
    let source_started = Instant::now();
    let request = WasmCommandRuntime::default()
        .compile_command_text(SOURCE_COMMAND)
        .expect("CTK3 source request");
    let AppCommand::Scenario(pc) = request.command() else {
        panic!(
            "CTK3 source must compile as PC scenario: {:?}",
            request.command()
        )
    };
    let problem = ProblemCompiler::compile_scenario_pc(pc.query()).expect("CTK3 source problem");
    let mut session = WasmCpuSearchSession::new(&problem).expect("CTK3 source session");
    let control = ExecutionControl::default();
    let cancellation = control.cancellation.handle();
    let mut source_advances = 0_u64;
    let source = loop {
        if source_started.elapsed() >= SOURCE_STAGE_TIMEOUT
            || source_advances >= SOURCE_MAX_ADVANCES
        {
            cancellation.cancel();
            panic!(
                "CTK3 source generation exceeded its bounded stage budget: elapsed_ms={}, advances={source_advances}",
                source_started.elapsed().as_millis()
            );
        }
        source_advances += 1;
        match session
            .advance(8_192, &control)
            .expect("CTK3 source advance")
        {
            WasmCpuSearchAdvance::Pending => {}
            WasmCpuSearchAdvance::Completed(result) => break result,
            WasmCpuSearchAdvance::Cancelled => panic!("CTK3 source unexpectedly cancelled"),
        }
    };
    let source_elapsed = source_started.elapsed();
    let pattern_count = source
        .field("coverage_pattern_count")
        .expect("coverage pattern count")
        .parse::<usize>()
        .expect("numeric coverage pattern count");
    let required =
        PatternBitSet::from_words(pattern_count, source.coverage_pattern_words().to_vec())
            .expect("required coverage bitset");
    let rows = source
        .solution_coverages()
        .iter()
        .map(|coverage| coverage.covered_patterns().clone())
        .collect::<Vec<_>>();
    let mut known_witness_coverage = PatternBitSet::new(required.pattern_count());
    for source_row in KNOWN_MINIMUM_COVER_SOURCE_ROWS {
        known_witness_coverage
            .union_with(&rows[source_row])
            .expect("known CTK3 witness preserves the pattern universe");
    }
    assert_eq!(KNOWN_MINIMUM_COVER_SOURCE_ROWS.len(), 25);
    assert!(
        is_superset(&known_witness_coverage, &required),
        "known 25-row CTK3 witness must cover P7"
    );

    let session_prepare_started = Instant::now();
    let mut session = ExactMinimumCoverSession::new(&required, &rows)
        .expect("CTK3 resumable minimum-cover session preparation");
    eprintln!(
        "{{\"phase\":\"minimum_session_prepare\",\"elapsed_ms\":{}}}",
        session_prepare_started.elapsed().as_millis()
    );
    let first_advance_started = Instant::now();
    let first_advance = session
        .advance(1)
        .expect("first CTK3 resumable minimum-cover advance");
    eprintln!(
        "{{\"phase\":\"minimum_session_first_advance\",\"elapsed_ms\":{},\"outcome\":{:?}}}",
        first_advance_started.elapsed().as_millis(),
        first_advance,
    );
    let mut preparation_advances = 1_u64;
    let mut preparation_max_advance = first_advance_started.elapsed();
    while session.diagnostic_incumbent_progress().is_none() && preparation_advances < 32 {
        let advance_started = Instant::now();
        let outcome = session
            .advance(1)
            .expect("staged CTK3 minimum-cover preparation advance");
        let elapsed = advance_started.elapsed();
        preparation_max_advance = preparation_max_advance.max(elapsed);
        preparation_advances += 1;
        eprintln!(
            "{{\"phase\":\"minimum_session_preparation_advance\",\"advance\":{},\"elapsed_us\":{},\"outcome\":{:?}}}",
            preparation_advances,
            elapsed.as_micros(),
            outcome,
        );
    }
    eprintln!(
        "{{\"phase\":\"minimum_session_preparation_summary\",\"advances\":{},\"max_advance_us\":{},\"root_dual_lower_bound\":{:?}}}",
        preparation_advances,
        preparation_max_advance.as_micros(),
        session.diagnostic_root_dual_lower_bound(),
    );
    assert!(
        session.diagnostic_incumbent_progress().is_some(),
        "CTK3 session did not enter incumbent improvement after bounded preparation"
    );
    if let Some(iteration_budget) = env::var(PROBE_RESIDUAL_ITERATION_BUDGET_ENV)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
    {
        assert!(
            session.diagnostic_limit_residual_iterations(iteration_budget),
            "CTK3 session must retain a residual workspace after root certification"
        );
        eprintln!(
            "{{\"phase\":\"minimum_session_residual_budget\",\"requested_iterations\":{},\"effective_iterations\":{}}}",
            iteration_budget,
            session
                .diagnostic_residual_progress()
                .expect("residual diagnostics after budget control")
                .remaining_proposal_iterations,
        );
    }
    if env::var_os(PROBE_SKIP_INCUMBENT_ENV).is_some_and(|value| value == "1") {
        assert!(
            session.diagnostic_skip_incumbent_trials(),
            "CTK3 session must have an incumbent phase to skip"
        );
    }
    if let Some(target) = env::var(PROBE_INCUMBENT_TARGET_ENV)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
    {
        let trials_started = Instant::now();
        let mut maximum_advance = Duration::ZERO;
        let mut previous = session
            .diagnostic_incumbent_progress()
            .expect("CTK3 minimum session entered incumbent improvement");
        eprintln!(
            "{{\"phase\":\"minimum_session_incumbent\",\"trial\":{},\"cardinality\":{}}}",
            previous.0, previous.1,
        );
        while previous.0 < 20_000 && previous.1 > target {
            let advance_started = Instant::now();
            let outcome = session
                .advance(1)
                .expect("CTK3 randomized incumbent advance");
            maximum_advance = maximum_advance.max(advance_started.elapsed());
            let current = session
                .diagnostic_incumbent_progress()
                .expect("CTK3 minimum session remains observable");
            if current.1 != previous.1 {
                eprintln!(
                    "{{\"phase\":\"minimum_session_incumbent\",\"trial\":{},\"cardinality\":{},\"outcome\":{:?}}}",
                    current.0, current.1, outcome,
                );
            }
            previous = current;
        }
        eprintln!(
            "{{\"phase\":\"minimum_session_incumbent_summary\",\"target\":{},\"trials\":{},\"cardinality\":{},\"elapsed_ms\":{},\"max_advance_us\":{}}}",
            target,
            previous.0,
            previous.1,
            trials_started.elapsed().as_millis(),
            maximum_advance.as_micros(),
        );
    }
    let recursive_reference = if env::var_os(PROBE_RECURSIVE_REFERENCE_ENV)
        .is_some_and(|value| value == "1")
    {
        let snapshot_started = Instant::now();
        let mut incumbent_work = 0_u64;
        while session.diagnostic_incumbent_progress().is_some() {
            let outcome = session
                .advance(2_048)
                .expect("finish CTK3 incumbent before recursive A/B snapshot");
            let visited_nodes = match outcome {
                ExactMinimumCoverSessionAdvance::Pending { visited_nodes } => visited_nodes,
                other => panic!("incumbent snapshot preparation must remain pending: {other:?}"),
            };
            incumbent_work += visited_nodes;
        }
        eprintln!(
            "{{\"phase\":\"recursive_reference_snapshot\",\"incumbent_work\":{},\"elapsed_ms\":{}}}",
            incumbent_work,
            snapshot_started.elapsed().as_millis(),
        );

        let reference_started = Instant::now();
        let reference = session
            .diagnostic_recursive_reference()
            .expect("recursive reference execution")
            .expect("CTK3 session must expose a pristine exact-search snapshot");
        assert_eq!(reference.result.row_indices().len(), 25);
        eprintln!(
            "{{\"phase\":\"recursive_reference_summary\",\"elapsed_ms\":{},\"cardinality\":{},\"row_indices\":{:?},\"search_nodes\":{},\"proposal_attempts\":{},\"proposal_iterations\":{},\"certified_prunes\":{},\"remaining_proposal_iterations\":{},\"attempts_by_gap\":{:?},\"iterations_by_gap\":{:?},\"prunes_by_gap\":{:?},\"attempts_by_depth\":{:?},\"iterations_by_depth\":{:?},\"prunes_by_depth\":{:?}}}",
            reference_started.elapsed().as_millis(),
            reference.result.row_indices().len(),
            reference.result.row_indices(),
            reference.visited_nodes,
            reference.residual.proposal_attempts,
            reference.residual.proposal_iterations,
            reference.residual.certified_prunes,
            reference.residual.remaining_proposal_iterations,
            reference.residual.proposal_attempts_by_dual_gap,
            reference.residual.proposal_iterations_by_dual_gap,
            reference.residual.certified_prunes_by_dual_gap,
            reference.residual.proposal_attempts_by_depth,
            reference.residual.proposal_iterations_by_depth,
            reference.residual.certified_prunes_by_depth,
        );
        Some(reference)
    } else {
        None
    };

    if env::var_os(PROBE_DRIVE_MINIMUM_SESSION_ENV).is_some_and(|value| value == "1") {
        assert!(
            session.diagnostic_reset_hot_cost(),
            "CTK3 proof must expose diagnostic hot-cost state"
        );
        let proof_started = Instant::now();
        let mut advances = 0_u64;
        let mut consumed_work = 0_u64;
        let mut maximum_advance = Duration::ZERO;
        let mut last_residual = session
            .diagnostic_residual_progress()
            .expect("CTK3 residual diagnostics before exact proof");
        let mut last_hot_cost = session
            .diagnostic_hot_cost()
            .expect("CTK3 hot-cost diagnostics before exact proof");
        let mut reported_residual_exhaustion = last_residual.remaining_proposal_iterations == 0;
        loop {
            advances += 1;
            let advance_started = Instant::now();
            let outcome = session.advance(2_048).expect("drive CTK3 minimum session");
            maximum_advance = maximum_advance.max(advance_started.elapsed());
            if let Some(current) = session.diagnostic_residual_progress() {
                if !reported_residual_exhaustion
                    && last_residual.remaining_proposal_iterations != 0
                    && current.remaining_proposal_iterations == 0
                {
                    reported_residual_exhaustion = true;
                    eprintln!(
                        "{{\"phase\":\"minimum_session_residual_exhausted\",\"elapsed_ms\":{},\"advances\":{},\"work_units_before_current\":{},\"search_nodes\":{},\"proposal_attempts\":{},\"proposal_iterations\":{},\"certified_prunes\":{},\"attempts_by_gap\":{:?},\"iterations_by_gap\":{:?},\"prunes_by_gap\":{:?},\"attempts_by_depth\":{:?},\"iterations_by_depth\":{:?},\"prunes_by_depth\":{:?}}}",
                        proof_started.elapsed().as_millis(),
                        advances,
                        consumed_work,
                        current.search_nodes,
                        current.proposal_attempts,
                        current.proposal_iterations,
                        current.certified_prunes,
                        current.proposal_attempts_by_dual_gap,
                        current.proposal_iterations_by_dual_gap,
                        current.certified_prunes_by_dual_gap,
                        current.proposal_attempts_by_depth,
                        current.proposal_iterations_by_depth,
                        current.certified_prunes_by_depth,
                    );
                }
                last_residual = current;
            }
            if let Some(current) = session.diagnostic_hot_cost() {
                last_hot_cost = current;
            }
            match outcome {
                ExactMinimumCoverSessionAdvance::Pending { visited_nodes } => {
                    consumed_work += visited_nodes;
                }
                ExactMinimumCoverSessionAdvance::Found {
                    result,
                    visited_nodes,
                } => {
                    consumed_work += visited_nodes;
                    assert_eq!(result.row_indices().len(), 25);
                    if let Some(reference) = recursive_reference.as_ref() {
                        assert_eq!(
                            result.row_indices(),
                            reference.result.row_indices(),
                            "explicit and recursive proof snapshots must choose the same rows"
                        );
                    }
                    eprintln!(
                        "{{\"phase\":\"minimum_session_proof_summary\",\"elapsed_ms\":{},\"advances\":{},\"work_units\":{},\"max_advance_us\":{},\"cardinality\":{},\"search_nodes\":{},\"proposal_attempts\":{},\"proposal_iterations\":{},\"certified_prunes\":{},\"remaining_proposal_iterations\":{},\"attempts_by_gap\":{:?},\"iterations_by_gap\":{:?},\"prunes_by_gap\":{:?},\"attempts_by_depth\":{:?},\"iterations_by_depth\":{:?},\"prunes_by_depth\":{:?},\"prunes_by_checkpoint\":{:?}}}",
                        proof_started.elapsed().as_millis(),
                        advances,
                        consumed_work,
                        maximum_advance.as_micros(),
                        result.row_indices().len(),
                        last_residual.search_nodes,
                        last_residual.proposal_attempts,
                        last_residual.proposal_iterations,
                        last_residual.certified_prunes,
                        last_residual.remaining_proposal_iterations,
                        last_residual.proposal_attempts_by_dual_gap,
                        last_residual.proposal_iterations_by_dual_gap,
                        last_residual.certified_prunes_by_dual_gap,
                        last_residual.proposal_attempts_by_depth,
                        last_residual.proposal_iterations_by_depth,
                        last_residual.certified_prunes_by_depth,
                        last_residual.certified_prunes_by_checkpoint,
                    );
                    eprintln!(
                        "{{\"phase\":\"minimum_session_hot_cost\",\"memo_calls\":{},\"memo_ns\":{},\"rarest_calls\":{},\"rarest_ns\":{},\"top_gain_calls\":{},\"top_gain_ns\":{},\"root_certificate_calls\":{},\"root_certificate_ns\":{},\"packing_calls\":{},\"packing_ns\":{},\"residual_prepare_calls\":{},\"residual_prepare_ns\":{},\"mirror_prox_iterations\":{},\"mirror_prox_ns\":{},\"softmax_p_ns\":{},\"softmax_q_ns\":{},\"softmax_middle_p_ns\":{},\"softmax_middle_q_ns\":{},\"softmax_p_entries\":{},\"softmax_p_cutoff_entries\":{},\"softmax_q_entries\":{},\"softmax_q_cutoff_entries\":{},\"softmax_q_row_incidences\":{},\"softmax_q_cutoff_row_incidences\":{},\"softmax_middle_p_entries\":{},\"softmax_middle_p_cutoff_entries\":{},\"softmax_middle_q_entries\":{},\"softmax_middle_q_cutoff_entries\":{},\"softmax_middle_q_row_incidences\":{},\"softmax_middle_q_cutoff_row_incidences\":{},\"first_gradient_ns\":{},\"middle_gradient_ns\":{},\"log_update_ns\":{},\"averaging_ns\":{},\"exact_recert_calls\":{},\"exact_recert_ns\":{},\"branch_calls\":{},\"branch_ns\":{}}}",
                        last_hot_cost.memo_calls,
                        last_hot_cost.memo_nanoseconds,
                        last_hot_cost.rarest_support_calls,
                        last_hot_cost.rarest_support_nanoseconds,
                        last_hot_cost.top_gain_calls,
                        last_hot_cost.top_gain_nanoseconds,
                        last_hot_cost.root_certificate_calls,
                        last_hot_cost.root_certificate_nanoseconds,
                        last_hot_cost.packing_calls,
                        last_hot_cost.packing_nanoseconds,
                        last_hot_cost.residual_prepare_calls,
                        last_hot_cost.residual_prepare_nanoseconds,
                        last_hot_cost.mirror_prox_iterations,
                        last_hot_cost.mirror_prox_nanoseconds,
                        last_hot_cost.softmax_p_nanoseconds,
                        last_hot_cost.softmax_q_nanoseconds,
                        last_hot_cost.softmax_middle_p_nanoseconds,
                        last_hot_cost.softmax_middle_q_nanoseconds,
                        last_hot_cost.softmax_p_entries,
                        last_hot_cost.softmax_p_cutoff_entries,
                        last_hot_cost.softmax_q_entries,
                        last_hot_cost.softmax_q_cutoff_entries,
                        last_hot_cost.softmax_q_row_incidences,
                        last_hot_cost.softmax_q_cutoff_row_incidences,
                        last_hot_cost.softmax_middle_p_entries,
                        last_hot_cost.softmax_middle_p_cutoff_entries,
                        last_hot_cost.softmax_middle_q_entries,
                        last_hot_cost.softmax_middle_q_cutoff_entries,
                        last_hot_cost.softmax_middle_q_row_incidences,
                        last_hot_cost.softmax_middle_q_cutoff_row_incidences,
                        last_hot_cost.first_gradient_nanoseconds,
                        last_hot_cost.middle_gradient_nanoseconds,
                        last_hot_cost.log_update_nanoseconds,
                        last_hot_cost.averaging_nanoseconds,
                        last_hot_cost.exact_recertification_calls,
                        last_hot_cost.exact_recertification_nanoseconds,
                        last_hot_cost.branch_calls,
                        last_hot_cost.branch_nanoseconds,
                    );
                    break;
                }
                other => panic!("unexpected CTK3 minimum session terminal: {other:?}"),
            }
        }
    }
    drop(session);

    if env::var_os(PROBE_STOP_AFTER_FIRST_SESSION_ADVANCE_ENV).is_some_and(|value| value == "1") {
        return;
    }

    let source_memory = process_memory_snapshot();
    eprintln!(
        "{{\"phase\":\"source\",\"source_ms\":{},\"advances\":{},\"row_count\":{},\"pattern_count\":{},\"coverage_bitset_storage_bytes\":{},\"working_set_bytes\":{},\"peak_working_set_bytes\":{}}}",
        source_elapsed.as_millis(),
        source_advances,
        rows.len(),
        pattern_count,
        optional_u128_json(coverage_bitset_storage_bytes(&required, &rows)),
        optional_u64_json(source_memory.working_set_bytes),
        optional_u64_json(source_memory.peak_working_set_bytes),
    );

    let diagnostics_started = Instant::now();
    let mut support_signatures = Vec::with_capacity(pattern_count);
    let support_word_count = rows.len().div_ceil(u64::BITS as usize);
    let mut minimum_support = usize::MAX;
    let mut maximum_support = 0_usize;
    let mut total_support = 0_usize;
    for pattern in 0..pattern_count {
        let pattern_word = pattern / u64::BITS as usize;
        let pattern_bit = pattern % u64::BITS as usize;
        let mut signature = vec![0_u64; support_word_count];
        let mut support_count = 0_usize;
        for (row_index, row) in rows.iter().enumerate() {
            if row.word_at(pattern_word) & (1_u64 << pattern_bit) != 0 {
                signature[row_index / u64::BITS as usize] |=
                    1_u64 << (row_index % u64::BITS as usize);
                support_count += 1;
            }
        }
        minimum_support = minimum_support.min(support_count);
        maximum_support = maximum_support.max(support_count);
        total_support += support_count;
        support_signatures.push(signature);
    }
    support_signatures.sort_unstable();
    support_signatures.dedup();
    let greedy_cardinality = greedy_cover_cardinality(&required, &rows);
    let seeded_greedy_cardinality = (0..rows.len())
        .filter_map(|seed| greedy_cover_cardinality_with_seed(&required, &rows, seed))
        .min();
    let rarity_greedy_cardinality = rarity_greedy_cover_cardinality(&required, &rows);
    let dominance_started = Instant::now();
    let dominated_count = reference_dominated_row_count(&rows);
    let dominance_elapsed = dominance_started.elapsed();
    let packing_started = Instant::now();
    let nondominated_row_indices = reference_nondominated_row_indices(&rows);
    let nondominated_rows = nondominated_row_indices
        .iter()
        .map(|index| rows[*index].clone())
        .collect::<Vec<_>>();
    let minimal_supports = minimal_constraint_supports(&required, &nondominated_rows);
    if std::env::var_os("CLEARRA_PROBE_DUMP_SUPPORTS").is_some_and(|value| value == "1") {
        eprintln!(
            "ctk3-row-map:{}",
            nondominated_row_indices
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );
        for support in &minimal_supports {
            eprintln!(
                "ctk3-support:{}",
                support
                    .iter()
                    .map(|word| format!("{word:016x}"))
                    .collect::<Vec<_>>()
                    .join(",")
            );
        }
    }
    let support_word_count_after_dominance = nondominated_rows.len().div_ceil(u64::BITS as usize);
    let compatibility = compatibility_graph(&minimal_supports);
    let canonical_packing = greedy_disjoint_packing(
        &minimal_supports,
        &(0..minimal_supports.len()).collect::<Vec<_>>(),
    );
    let degree_order = compatibility_degree_order(&compatibility);
    let degree_packing = greedy_disjoint_packing(&minimal_supports, &degree_order);
    let dynamic_packing = dynamic_compatibility_packing(&compatibility, None);
    let seeded_dynamic_packing_indices = (0..minimal_supports.len())
        .map(|seed| dynamic_compatibility_packing_indices(&compatibility, Some(seed)))
        .max_by(|left, right| left.len().cmp(&right.len()).then_with(|| right.cmp(left)))
        .unwrap_or_default();
    let seeded_dynamic_packing = seeded_dynamic_packing_indices.len();
    let sum_over_packing_bound = sum_over_packing_bound(
        &minimal_supports,
        nondominated_rows.len(),
        &seeded_dynamic_packing_indices,
    );
    let exact_packing = std::env::var_os("CLEARRA_PROBE_EXACT_PACKING")
        .is_some_and(|value| value == "1")
        .then(|| {
            exact_compatibility_packing(
                &compatibility,
                seeded_dynamic_packing,
                Duration::from_secs(30),
            )
        });
    let randomized_incumbent = std::env::var_os("CLEARRA_PROBE_RANDOM_INCUMBENT")
        .is_some_and(|value| value == "1")
        .then(|| {
            let started = Instant::now();
            let (rows, repaired_rows) =
                randomized_compact_cover(&minimal_supports, nondominated_rows.len(), 20_000);
            (rows, repaired_rows, started.elapsed())
        });
    eprintln!(
        "{{\"phase\":\"diagnostics\",\"elapsed_ms\":{},\"greedy_cardinality\":{},\"seeded_greedy_cardinality\":{},\"rarity_greedy_cardinality\":{},\"unique_support_signatures\":{},\"minimum_support\":{},\"maximum_support\":{},\"average_support_milli\":{},\"dominance_ms\":{},\"dominated_rows\":{},\"nondominated_rows\":{},\"minimal_support_constraints\":{},\"support_words_after_dominance\":{},\"canonical_disjoint_packing\":{},\"degree_disjoint_packing\":{},\"dynamic_disjoint_packing\":{},\"seeded_dynamic_disjoint_packing\":{},\"sum_over_packing_bound\":{},\"packing_ms\":{}}}",
        diagnostics_started.elapsed().as_millis(),
        greedy_cardinality.map_or_else(|| "null".to_owned(), |value| value.to_string()),
        seeded_greedy_cardinality.map_or_else(|| "null".to_owned(), |value| value.to_string()),
        rarity_greedy_cardinality.map_or_else(|| "null".to_owned(), |value| value.to_string()),
        support_signatures.len(),
        minimum_support,
        maximum_support,
        total_support * 1_000 / pattern_count,
        dominance_elapsed.as_millis(),
        dominated_count,
        nondominated_rows.len(),
        minimal_supports.len(),
        support_word_count_after_dominance,
        canonical_packing,
        degree_packing,
        dynamic_packing,
        seeded_dynamic_packing,
        sum_over_packing_bound,
        packing_started.elapsed().as_millis(),
    );
    if let Some(exact_packing) = exact_packing {
        eprintln!(
            "{{\"phase\":\"exact_packing\",\"best\":{},\"complete\":{},\"nodes\":{},\"elapsed_ms\":{}}}",
            exact_packing.best,
            exact_packing.complete,
            exact_packing.nodes,
            exact_packing.elapsed.as_millis(),
        );
    }
    if let Some((rows, repaired_rows, elapsed)) = randomized_incumbent {
        eprintln!(
            "{{\"phase\":\"randomized_incumbent\",\"cardinality\":{},\"repaired_cardinality\":{},\"elapsed_ms\":{},\"rows\":{:?},\"repaired_rows\":{:?}}}",
            rows.len(),
            repaired_rows.len(),
            elapsed.as_millis(),
            rows,
            repaired_rows,
        );
    }
    if std::env::var_os("CLEARRA_PROBE_SKIP_COVER").is_some_and(|value| value == "1") {
        return;
    }

    // The public authority deliberately couples k* proof production to the
    // original-row enumerator construction, preventing a proof from one
    // matrix from being reused against another. This timer includes that small
    // immutable index construction, but excludes every canonical-portfolio
    // frontier decision measured below.
    let proof_started = Instant::now();
    let mut solver_guard_peak_bytes = 0_u128;
    let preparation = ExactMinimumCoverPortfolioEnumerator::prepare_with_memory_guard(
        &required,
        &rows,
        &mut |live_and_future_bytes| {
            solver_guard_peak_bytes = solver_guard_peak_bytes.max(live_and_future_bytes);
            Ok(())
        },
    )
    .expect("exact minimum-cover proof and portfolio preparation");
    let proof_elapsed = proof_started.elapsed();
    let (proof, mut portfolios) = match preparation {
        ExactMinimumCoverPortfolioPreparation::Coverable { proof, enumerator } => {
            (proof, enumerator)
        }
        ExactMinimumCoverPortfolioPreparation::Incomplete { proof } => panic!(
            "known CTK3 fixture became uncoverable: covered={}, required={}",
            proof.covered_patterns().count_ones(),
            required.count_ones()
        ),
    };
    assert!(proof.complete());
    assert_eq!(proof.row_indices().len(), 25, "known CTK3 optimum");
    assert_exact_cover(proof.row_indices(), &required, &rows);
    let proof_memory = process_memory_snapshot();
    eprintln!(
        "{{\"phase\":\"minimum_proof\",\"proof_and_enumerator_prepare_ms\":{},\"optimal_cardinality\":{},\"proof_rows\":{:?},\"proof_retained_bytes\":{},\"enumerator_retained_bytes\":{},\"solver_guard_peak_bytes\":{},\"working_set_bytes\":{},\"peak_working_set_bytes\":{}}}",
        proof_elapsed.as_millis(),
        proof.row_indices().len(),
        proof.row_indices(),
        optional_u128_json(proof.checked_retained_bytes()),
        optional_u128_json(portfolios.checked_retained_capacity_bytes()),
        solver_guard_peak_bytes,
        optional_u64_json(proof_memory.working_set_bytes),
        optional_u64_json(proof_memory.peak_working_set_bytes),
    );

    let first = measure_next_portfolio_stage("first_canonical", &mut portfolios);
    report_portfolio_stage(&first);
    let first_rows = first.row_indices.as_ref().unwrap_or_else(|| {
        panic!(
            "first CTK3 canonical portfolio was not materialized inside the {} ms stage budget: stop={:?}, elapsed_ms={}, work_steps={}",
            PORTFOLIO_STAGE_TIMEOUT.as_millis(),
            first.stop,
            first.elapsed.as_millis(),
            first.work_steps,
        )
    });
    assert_portfolio(first_rows, &required, &rows);
    assert_eq!(
        first_rows.as_slice(),
        EXPECTED_FIRST_CANONICAL_SOURCE_ROWS,
        "first CTK3 portfolio remains the original-row numeric-lex canonical identity"
    );
    assert!(
        first.elapsed < PORTFOLIO_STAGE_TIMEOUT,
        "first CTK3 canonical portfolio exceeded its {} ms stage budget: {} ms",
        PORTFOLIO_STAGE_TIMEOUT.as_millis(),
        first.elapsed.as_millis()
    );

    let second = measure_next_portfolio_stage("second_portfolio", &mut portfolios);
    report_portfolio_stage(&second);
    let second_rows = second.row_indices.as_ref().unwrap_or_else(|| {
        panic!(
            "second CTK3 portfolio was not materialized inside the {} ms stage budget: stop={:?}, elapsed_ms={}, work_steps={}",
            PORTFOLIO_STAGE_TIMEOUT.as_millis(),
            second.stop,
            second.elapsed.as_millis(),
            second.work_steps,
        )
    });
    assert_portfolio(second_rows, &required, &rows);
    assert!(
        first_rows.as_slice() < second_rows.as_slice(),
        "second CTK3 portfolio must follow the first in strict numeric lexicographic order"
    );
    assert_eq!(
        second_rows.as_slice(),
        EXPECTED_SECOND_CANONICAL_SOURCE_ROWS,
        "second CTK3 portfolio remains the next original-row numeric-lex identity"
    );
    assert!(
        second.elapsed < PORTFOLIO_STAGE_TIMEOUT,
        "second CTK3 portfolio exceeded its {} ms stage budget: {} ms",
        PORTFOLIO_STAGE_TIMEOUT.as_millis(),
        second.elapsed.as_millis()
    );
    let (dominance_elapsed, dominated_count) =
        if std::env::var_os("CLEARRA_PROBE_NAIVE_DOMINANCE").is_some_and(|value| value == "1") {
            let dominance_started = Instant::now();
            let dominated_count = reference_dominated_row_count(&rows);
            (Some(dominance_started.elapsed()), Some(dominated_count))
        } else {
            (None, None)
        };
    eprintln!(
        "{{\"phase\":\"complete\",\"source_ms\":{},\"row_count\":{},\"pattern_count\":{},\"dominance_ms\":{},\"dominated_rows\":{},\"proof_and_enumerator_prepare_ms\":{},\"optimal_cardinality\":{},\"first_materialized\":{},\"second_materialized\":{}}}",
        source_elapsed.as_millis(),
        rows.len(),
        pattern_count,
        dominance_elapsed.map_or_else(|| "null".to_owned(), |value| value.as_millis().to_string()),
        dominated_count.map_or_else(|| "null".to_owned(), |value| value.to_string()),
        proof_elapsed.as_millis(),
        proof.row_indices().len(),
        first.row_indices.is_some(),
        second.row_indices.is_some(),
    );
}

#[derive(Clone, Debug)]
struct PortfolioStageMeasurement {
    label: &'static str,
    elapsed: Duration,
    slices: u64,
    work_steps: u64,
    solver_cursor_work_steps: u64,
    candidate_combinations_tested: u64,
    impossible_prefix_subtrees_pruned: u64,
    stop: ExactMinimumCoverEnumerationStop,
    row_indices: Option<Vec<usize>>,
    known_alternative_count_decimal: String,
    total_alternative_count_decimal: Option<String>,
    enumeration_complete: bool,
    enumerator_retained_bytes: Option<u128>,
    process_memory: ProcessMemorySnapshot,
}

fn measure_next_portfolio_stage(
    label: &'static str,
    portfolios: &mut ExactMinimumCoverPortfolioEnumerator,
) -> PortfolioStageMeasurement {
    let started = Instant::now();
    let mut slices = 0_u64;
    let mut work_steps = 0_u64;
    let mut solver_cursor_work_steps = 0_u64;
    let mut candidate_combinations_tested = 0_u64;
    let mut impossible_prefix_subtrees_pruned = 0_u64;
    let mut known_alternative_count_decimal = portfolios.known_alternative_count_decimal();
    let mut total_alternative_count_decimal = None;
    let mut enumeration_complete = portfolios.enumeration_complete();
    let (stop, row_indices) = loop {
        if started.elapsed() >= PORTFOLIO_STAGE_TIMEOUT {
            break (ExactMinimumCoverEnumerationStop::Cancelled, None);
        }
        let remaining = PORTFOLIO_MAX_WORK_STEPS_PER_STAGE.saturating_sub(work_steps);
        if remaining == 0 {
            break (ExactMinimumCoverEnumerationStop::WorkBudgetExhausted, None);
        }
        let slice_budget = remaining.min(PORTFOLIO_WORK_STEPS_PER_SLICE);
        let page = portfolios
            .next_page_with_control(1, slice_budget, &mut || {
                started.elapsed() >= PORTFOLIO_STAGE_TIMEOUT
            })
            .expect("bounded exact portfolio page");
        slices = slices.checked_add(1).expect("portfolio slice count");
        work_steps = work_steps
            .checked_add(page.work_steps())
            .expect("portfolio work-step count");
        solver_cursor_work_steps = solver_cursor_work_steps
            .checked_add(page.solver_cursor_work_steps())
            .expect("portfolio solver-cursor work-step count");
        candidate_combinations_tested = candidate_combinations_tested
            .checked_add(page.candidate_combinations_tested())
            .expect("portfolio candidate count");
        impossible_prefix_subtrees_pruned = impossible_prefix_subtrees_pruned
            .checked_add(page.impossible_prefix_subtrees_pruned())
            .expect("portfolio prune count");
        known_alternative_count_decimal = page.known_alternative_count_decimal().to_owned();
        total_alternative_count_decimal = page
            .total_alternative_count_decimal()
            .map(ToOwned::to_owned);
        enumeration_complete = page.enumeration_complete();
        assert_eq!(
            page.work_steps(),
            page.solver_cursor_work_steps()
                + page.candidate_combinations_tested()
                + page.impossible_prefix_subtrees_pruned(),
            "every bounded frontier decision must have one progress classification"
        );
        if let Some(portfolio) = page.portfolios().first() {
            assert_eq!(page.portfolios().len(), 1);
            break (page.stop(), Some(portfolio.row_indices().to_vec()));
        }
        match page.stop() {
            ExactMinimumCoverEnumerationStop::WorkBudgetExhausted => {
                if slices == 1 || slices.is_multiple_of(64) {
                    eprintln!(
                        "{{\"phase\":\"portfolio_progress\",\"stage\":\"{label}\",\"elapsed_ms\":{},\"slices\":{},\"work_steps\":{},\"solver_cursor_work_steps\":{},\"candidate_combinations_tested\":{},\"impossible_prefix_subtrees_pruned\":{},\"known_alternative_count\":\"{}\"}}",
                        started.elapsed().as_millis(),
                        slices,
                        work_steps,
                        solver_cursor_work_steps,
                        candidate_combinations_tested,
                        impossible_prefix_subtrees_pruned,
                        known_alternative_count_decimal,
                    );
                }
            }
            stop @ (ExactMinimumCoverEnumerationStop::Cancelled
            | ExactMinimumCoverEnumerationStop::Sealed) => break (stop, None),
            ExactMinimumCoverEnumerationStop::PageFull => {
                panic!("page-full stop omitted its only requested portfolio")
            }
        }
    };
    PortfolioStageMeasurement {
        label,
        elapsed: started.elapsed(),
        slices,
        work_steps,
        solver_cursor_work_steps,
        candidate_combinations_tested,
        impossible_prefix_subtrees_pruned,
        stop,
        row_indices,
        known_alternative_count_decimal,
        total_alternative_count_decimal,
        enumeration_complete,
        enumerator_retained_bytes: portfolios.checked_retained_capacity_bytes(),
        process_memory: process_memory_snapshot(),
    }
}

fn report_portfolio_stage(measurement: &PortfolioStageMeasurement) {
    let row_indices = measurement
        .row_indices
        .as_ref()
        .map_or_else(|| "null".to_owned(), |rows| format!("{rows:?}"));
    let total = measurement
        .total_alternative_count_decimal
        .as_ref()
        .map_or_else(|| "null".to_owned(), |value| format!("\"{value}\""));
    eprintln!(
        "{{\"phase\":\"portfolio_stage\",\"stage\":\"{}\",\"elapsed_ms\":{},\"slices\":{},\"work_steps\":{},\"solver_cursor_work_steps\":{},\"candidate_combinations_tested\":{},\"impossible_prefix_subtrees_pruned\":{},\"stop\":\"{}\",\"materialized\":{},\"row_indices\":{},\"known_alternative_count\":\"{}\",\"total_alternative_count\":{},\"enumeration_complete\":{},\"enumerator_retained_bytes\":{},\"working_set_bytes\":{},\"peak_working_set_bytes\":{}}}",
        measurement.label,
        measurement.elapsed.as_millis(),
        measurement.slices,
        measurement.work_steps,
        measurement.solver_cursor_work_steps,
        measurement.candidate_combinations_tested,
        measurement.impossible_prefix_subtrees_pruned,
        enumeration_stop_name(measurement.stop),
        measurement.row_indices.is_some(),
        row_indices,
        measurement.known_alternative_count_decimal,
        total,
        measurement.enumeration_complete,
        optional_u128_json(measurement.enumerator_retained_bytes),
        optional_u64_json(measurement.process_memory.working_set_bytes),
        optional_u64_json(measurement.process_memory.peak_working_set_bytes),
    );
}

fn enumeration_stop_name(stop: ExactMinimumCoverEnumerationStop) -> &'static str {
    match stop {
        ExactMinimumCoverEnumerationStop::PageFull => "page-full",
        ExactMinimumCoverEnumerationStop::WorkBudgetExhausted => "work-budget-exhausted",
        ExactMinimumCoverEnumerationStop::Cancelled => "cancelled",
        ExactMinimumCoverEnumerationStop::Sealed => "sealed",
    }
}

fn assert_portfolio(row_indices: &[usize], required: &PatternBitSet, rows: &[PatternBitSet]) {
    assert_exact_cover(row_indices, required, rows);
    assert!(
        row_indices.windows(2).all(|pair| pair[0] < pair[1]),
        "portfolio row identities must be strictly increasing"
    );
}

fn assert_exact_cover(row_indices: &[usize], required: &PatternBitSet, rows: &[PatternBitSet]) {
    assert_eq!(
        row_indices.len(),
        25,
        "every verified CTK3 optimum has k*=25"
    );
    assert!(
        row_indices.iter().all(|row_index| *row_index < rows.len()),
        "portfolio row identity must belong to the original matrix"
    );
    let mut covered = PatternBitSet::new(required.pattern_count());
    for row_index in row_indices {
        covered
            .union_with(&rows[*row_index])
            .expect("portfolio preserves the CTK3 pattern universe");
    }
    assert!(
        is_superset(&covered, required),
        "emitted CTK3 portfolio must cover the exact required P7 universe"
    );
}

fn coverage_bitset_storage_bytes(required: &PatternBitSet, rows: &[PatternBitSet]) -> Option<u128> {
    rows.iter()
        .try_fold(required.checked_storage_retained_bytes()?, |total, row| {
            total.checked_add(row.checked_storage_retained_bytes()?)
        })
}

#[derive(Clone, Copy, Debug, Default)]
struct ProcessMemorySnapshot {
    working_set_bytes: Option<u64>,
    peak_working_set_bytes: Option<u64>,
}

#[cfg(target_os = "windows")]
fn process_memory_snapshot() -> ProcessMemorySnapshot {
    let output = Command::new("powershell.exe")
        .arg("-NoLogo")
        .arg("-NoProfile")
        .arg("-NonInteractive")
        .arg("-Command")
        .arg(
            "$p=Get-Process -Id ([int]$env:CLEARRA_PROBE_TARGET_PID); [Console]::Out.Write(('{0},{1}' -f $p.WorkingSet64,$p.PeakWorkingSet64))",
        )
        .env("CLEARRA_PROBE_TARGET_PID", std::process::id().to_string())
        .output()
        .ok();
    let Some(output) = output.filter(|output| output.status.success()) else {
        return ProcessMemorySnapshot::default();
    };
    let text = String::from_utf8(output.stdout).ok();
    let Some((working_set, peak_working_set)) =
        text.as_deref().and_then(|text| text.split_once(','))
    else {
        return ProcessMemorySnapshot::default();
    };
    ProcessMemorySnapshot {
        working_set_bytes: working_set.trim().parse().ok(),
        peak_working_set_bytes: peak_working_set.trim().parse().ok(),
    }
}

#[cfg(target_os = "linux")]
fn process_memory_snapshot() -> ProcessMemorySnapshot {
    let Some(status) = std::fs::read_to_string("/proc/self/status").ok() else {
        return ProcessMemorySnapshot::default();
    };
    ProcessMemorySnapshot {
        working_set_bytes: proc_status_kib(&status, "VmRSS:").and_then(|kib| kib.checked_mul(1024)),
        peak_working_set_bytes: proc_status_kib(&status, "VmHWM:")
            .and_then(|kib| kib.checked_mul(1024)),
    }
}

#[cfg(target_os = "linux")]
fn proc_status_kib(status: &str, field: &str) -> Option<u64> {
    status.lines().find_map(|line| {
        let value = line.strip_prefix(field)?.trim();
        value.split_ascii_whitespace().next()?.parse().ok()
    })
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn process_memory_snapshot() -> ProcessMemorySnapshot {
    ProcessMemorySnapshot::default()
}

fn optional_u64_json(value: Option<u64>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}

fn optional_u128_json(value: Option<u128>) -> String {
    value.map_or_else(|| "null".to_owned(), |value| value.to_string())
}

fn reference_dominated_row_count(rows: &[PatternBitSet]) -> usize {
    let mut dominated = vec![false; rows.len()];
    for left in 0..rows.len() {
        if dominated[left] {
            continue;
        }
        for right in 0..rows.len() {
            if left == right || dominated[left] {
                continue;
            }
            if is_superset(&rows[right], &rows[left]) {
                let equal = rows[right] == rows[left];
                if !equal || right < left {
                    dominated[left] = true;
                }
            }
        }
    }
    dominated.into_iter().filter(|value| *value).count()
}

fn reference_nondominated_row_indices(rows: &[PatternBitSet]) -> Vec<usize> {
    let mut dominated = vec![false; rows.len()];
    for left in 0..rows.len() {
        if dominated[left] {
            continue;
        }
        for right in 0..rows.len() {
            if left == right || dominated[left] {
                continue;
            }
            if is_superset(&rows[right], &rows[left]) {
                let equal = rows[right] == rows[left];
                if !equal || right < left {
                    dominated[left] = true;
                }
            }
        }
    }
    (0..rows.len()).filter(|index| !dominated[*index]).collect()
}

fn minimal_constraint_supports(required: &PatternBitSet, rows: &[PatternBitSet]) -> Vec<Vec<u64>> {
    let support_word_count = rows.len().div_ceil(u64::BITS as usize);
    let mut supports = Vec::with_capacity(required.count_ones() as usize);
    for pattern in 0..required.pattern_count() {
        let pattern_word = pattern / u64::BITS as usize;
        let pattern_bit = pattern % u64::BITS as usize;
        if required.word_at(pattern_word) & (1_u64 << pattern_bit) == 0 {
            continue;
        }
        let mut support = vec![0_u64; support_word_count];
        for (row_index, row) in rows.iter().enumerate() {
            if row.word_at(pattern_word) & (1_u64 << pattern_bit) != 0 {
                support[row_index / u64::BITS as usize] |=
                    1_u64 << (row_index % u64::BITS as usize);
            }
        }
        supports.push(support);
    }
    supports.sort_unstable();
    supports.dedup();
    supports.sort_unstable_by(|left, right| {
        support_size(left)
            .cmp(&support_size(right))
            .then_with(|| left.cmp(right))
    });
    let mut minimal = Vec::<Vec<u64>>::with_capacity(supports.len());
    'support: for support in supports {
        for retained in &minimal {
            if support_is_superset(&support, retained) {
                continue 'support;
            }
        }
        minimal.push(support);
    }
    minimal
}

fn support_size(support: &[u64]) -> usize {
    support.iter().map(|word| word.count_ones() as usize).sum()
}

fn support_is_superset(left: &[u64], right: &[u64]) -> bool {
    left.iter()
        .zip(right)
        .all(|(left, right)| left & right == *right)
}

fn supports_are_disjoint(left: &[u64], right: &[u64]) -> bool {
    left.iter()
        .zip(right)
        .all(|(left, right)| left & right == 0)
}

fn compatibility_graph(supports: &[Vec<u64>]) -> Vec<Vec<u64>> {
    let graph_word_count = supports.len().div_ceil(u64::BITS as usize);
    let mut graph = vec![vec![0_u64; graph_word_count]; supports.len()];
    for left in 0..supports.len() {
        for right in left + 1..supports.len() {
            if supports_are_disjoint(&supports[left], &supports[right]) {
                graph[left][right / u64::BITS as usize] |= 1_u64 << (right % u64::BITS as usize);
                graph[right][left / u64::BITS as usize] |= 1_u64 << (left % u64::BITS as usize);
            }
        }
    }
    graph
}

fn compatibility_degree_order(graph: &[Vec<u64>]) -> Vec<usize> {
    let mut order = (0..graph.len()).collect::<Vec<_>>();
    order.sort_unstable_by(|left, right| {
        graph[*right]
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum::<usize>()
            .cmp(
                &graph[*left]
                    .iter()
                    .map(|word| word.count_ones() as usize)
                    .sum::<usize>(),
            )
            .then_with(|| left.cmp(right))
    });
    order
}

fn greedy_disjoint_packing(supports: &[Vec<u64>], order: &[usize]) -> usize {
    let mut occupied = vec![0_u64; supports.first().map_or(0, Vec::len)];
    let mut size = 0_usize;
    for index in order {
        if !supports_are_disjoint(&occupied, &supports[*index]) {
            continue;
        }
        for (occupied, support) in occupied.iter_mut().zip(&supports[*index]) {
            *occupied |= *support;
        }
        size += 1;
    }
    size
}

fn dynamic_compatibility_packing(graph: &[Vec<u64>], seed: Option<usize>) -> usize {
    dynamic_compatibility_packing_indices(graph, seed).len()
}

fn dynamic_compatibility_packing_indices(graph: &[Vec<u64>], seed: Option<usize>) -> Vec<usize> {
    let word_count = graph.len().div_ceil(u64::BITS as usize);
    let mut candidates = vec![u64::MAX; word_count];
    if let Some(last) = candidates.last_mut() {
        let tail = graph.len() % u64::BITS as usize;
        if tail != 0 {
            *last = (1_u64 << tail) - 1;
        }
    }
    let mut packing = Vec::new();
    if let Some(seed) = seed {
        candidates.clone_from(&graph[seed]);
        packing.push(seed);
    }
    loop {
        let next = (0..graph.len())
            .filter(|index| {
                candidates[*index / u64::BITS as usize] & (1_u64 << (*index % u64::BITS as usize))
                    != 0
            })
            .max_by_key(|index| {
                graph[*index]
                    .iter()
                    .zip(&candidates)
                    .map(|(neighbors, candidates)| (neighbors & candidates).count_ones())
                    .sum::<u32>()
            });
        let Some(next) = next else {
            return packing;
        };
        packing.push(next);
        for (candidates, neighbors) in candidates.iter_mut().zip(&graph[next]) {
            *candidates &= *neighbors;
        }
    }
}

fn sum_over_packing_bound(supports: &[Vec<u64>], row_count: usize, packing: &[usize]) -> usize {
    let mut adjusted_degrees = vec![0_usize; row_count];
    for support in supports {
        for row in support_rows(support, row_count) {
            adjusted_degrees[row] += 1;
        }
    }

    let mut optimistically_covered_edges = 0_usize;
    for edge in packing {
        let rows = support_rows(&supports[*edge], row_count).collect::<Vec<_>>();
        let max_degree_row = rows
            .iter()
            .copied()
            .max_by_key(|row| adjusted_degrees[*row])
            .expect("packed support is non-empty");
        optimistically_covered_edges += adjusted_degrees[max_degree_row];
        for row in rows {
            adjusted_degrees[row] -= 1;
        }
        adjusted_degrees[max_degree_row] = 0;
    }

    adjusted_degrees.sort_unstable_by(|left, right| right.cmp(left));
    let additional = adjusted_degrees
        .into_iter()
        .take_while(|degree| {
            if optimistically_covered_edges >= supports.len() {
                false
            } else {
                optimistically_covered_edges += *degree;
                true
            }
        })
        .count();
    packing.len() + additional
}

fn support_rows(support: &[u64], row_count: usize) -> impl Iterator<Item = usize> + '_ {
    (0..row_count).filter(|row| {
        support[*row / u64::BITS as usize] & (1_u64 << (*row % u64::BITS as usize)) != 0
    })
}

struct ExactPackingProbeResult {
    best: usize,
    complete: bool,
    nodes: u64,
    elapsed: Duration,
}

fn exact_compatibility_packing(
    graph: &[Vec<u64>],
    incumbent: usize,
    time_limit: Duration,
) -> ExactPackingProbeResult {
    let started = Instant::now();
    let word_count = graph.len().div_ceil(u64::BITS as usize);
    let mut candidates = vec![u64::MAX; word_count];
    if let Some(last) = candidates.last_mut() {
        let tail = graph.len() % u64::BITS as usize;
        if tail != 0 {
            *last = (1_u64 << tail) - 1;
        }
    }
    let mut search = ExactCliqueProbe {
        graph,
        best: incumbent,
        nodes: 0,
        deadline: started + time_limit,
        timed_out: false,
    };
    search.expand(0, candidates);
    ExactPackingProbeResult {
        best: search.best,
        complete: !search.timed_out,
        nodes: search.nodes,
        elapsed: started.elapsed(),
    }
}

struct ExactCliqueProbe<'a> {
    graph: &'a [Vec<u64>],
    best: usize,
    nodes: u64,
    deadline: Instant,
    timed_out: bool,
}

impl ExactCliqueProbe<'_> {
    fn expand(&mut self, clique_size: usize, mut candidates: Vec<u64>) {
        self.nodes += 1;
        if self.nodes & 0x3fff == 0 && Instant::now() >= self.deadline {
            self.timed_out = true;
            return;
        }
        let (vertices, color_bounds) = greedy_color_order(self.graph, &candidates);
        for order_index in (0..vertices.len()).rev() {
            if self.timed_out || clique_size + color_bounds[order_index] <= self.best {
                return;
            }
            let vertex = vertices[order_index];
            if candidates[vertex / u64::BITS as usize] & (1_u64 << (vertex % u64::BITS as usize))
                == 0
            {
                continue;
            }
            let next_candidates = candidates
                .iter()
                .zip(&self.graph[vertex])
                .map(|(candidates, neighbors)| candidates & neighbors)
                .collect::<Vec<_>>();
            if next_candidates.iter().all(|word| *word == 0) {
                self.best = self.best.max(clique_size + 1);
            } else {
                self.expand(clique_size + 1, next_candidates);
            }
            candidates[vertex / u64::BITS as usize] &= !(1_u64 << (vertex % u64::BITS as usize));
        }
    }
}

fn greedy_color_order(graph: &[Vec<u64>], candidates: &[u64]) -> (Vec<usize>, Vec<usize>) {
    let candidate_count = candidates
        .iter()
        .map(|word| word.count_ones() as usize)
        .sum::<usize>();
    let mut vertices = Vec::with_capacity(candidate_count);
    let mut color_bounds = Vec::with_capacity(candidate_count);
    let mut uncolored = candidates.to_vec();
    let mut color = 0_usize;
    while uncolored.iter().any(|word| *word != 0) {
        color += 1;
        let mut available = uncolored.clone();
        while let Some(vertex) = first_set_bit(&available) {
            vertices.push(vertex);
            color_bounds.push(color);
            uncolored[vertex / u64::BITS as usize] &= !(1_u64 << (vertex % u64::BITS as usize));
            available[vertex / u64::BITS as usize] &= !(1_u64 << (vertex % u64::BITS as usize));
            for (available, neighbors) in available.iter_mut().zip(&graph[vertex]) {
                *available &= !neighbors;
            }
        }
    }
    (vertices, color_bounds)
}

fn first_set_bit(words: &[u64]) -> Option<usize> {
    words.iter().enumerate().find_map(|(word_index, word)| {
        (*word != 0).then(|| word_index * u64::BITS as usize + word.trailing_zeros() as usize)
    })
}

fn randomized_compact_cover(
    supports: &[Vec<u64>],
    row_count: usize,
    trial_count: usize,
) -> (Vec<usize>, Vec<usize>) {
    let constraint_word_count = supports.len().div_ceil(u64::BITS as usize);
    let mut row_coverages = vec![vec![0_u64; constraint_word_count]; row_count];
    for (constraint, support) in supports.iter().enumerate() {
        for (word_index, word) in support.iter().copied().enumerate() {
            let mut remaining = word;
            while remaining != 0 {
                let bit = remaining.trailing_zeros() as usize;
                let row = word_index * u64::BITS as usize + bit;
                row_coverages[row][constraint / u64::BITS as usize] |=
                    1_u64 << (constraint % u64::BITS as usize);
                remaining &= remaining - 1;
            }
        }
    }
    let mut target = vec![u64::MAX; constraint_word_count];
    if let Some(last) = target.last_mut() {
        let tail = supports.len() % u64::BITS as usize;
        if tail != 0 {
            *last = (1_u64 << tail) - 1;
        }
    }
    let mut best = (0..row_count).collect::<Vec<_>>();
    let mut random = 0x9e37_79b9_7f4a_7c15_u64;
    for trial in 0..trial_count {
        let mut covered = vec![0_u64; constraint_word_count];
        let mut selected = Vec::with_capacity(best.len());
        let mut selected_flags = vec![false; row_count];
        while !words_cover_probe(&covered, &target) {
            let mut gains = row_coverages
                .iter()
                .enumerate()
                .filter(|(row, _)| !selected_flags[*row])
                .map(|(row, coverage)| {
                    let gain = coverage
                        .iter()
                        .zip(&covered)
                        .zip(&target)
                        .map(|((coverage, covered), target)| {
                            (coverage & target & !covered).count_ones() as usize
                        })
                        .sum::<usize>();
                    (gain, row)
                })
                .filter(|(gain, _)| *gain != 0)
                .collect::<Vec<_>>();
            gains.sort_unstable_by(|left, right| {
                right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1))
            });
            if gains.is_empty() {
                break;
            }
            random ^= random << 13;
            random ^= random >> 7;
            random ^= random << 17;
            let restricted = (2 + trial % 7).min(gains.len());
            let choice = (random as usize) % restricted;
            let row = gains[choice].1;
            selected_flags[row] = true;
            selected.push(row);
            union_words_probe(&mut covered, &row_coverages[row]);
        }
        remove_redundant_cover_rows(&row_coverages, &target, &mut selected);
        if selected.len() < best.len() {
            best = selected;
        }
    }
    let unrepaired = best.clone();
    improve_cover_by_exchange(&row_coverages, &target, &mut best);
    (unrepaired, best)
}

fn remove_redundant_cover_rows(rows: &[Vec<u64>], target: &[u64], selected: &mut Vec<usize>) {
    let mut position = selected.len();
    while position != 0 {
        position -= 1;
        let mut covered = vec![0_u64; target.len()];
        for (other_position, row) in selected.iter().copied().enumerate() {
            if other_position != position {
                union_words_probe(&mut covered, &rows[row]);
            }
        }
        if words_cover_probe(&covered, target) {
            selected.remove(position);
        }
    }
}

fn improve_cover_by_exchange(rows: &[Vec<u64>], target: &[u64], selected: &mut Vec<usize>) {
    loop {
        let mut replacement = None;
        'pairs: for left in 0..selected.len() {
            for right in left + 1..selected.len() {
                let removed = [left, right];
                let (uncovered, retained) = uncovered_without(rows, target, selected, &removed);
                if let Some(row) = rows.iter().enumerate().find_map(|(row, coverage)| {
                    (!retained[row] && words_cover_masked_probe(coverage, &uncovered))
                        .then_some(row)
                }) {
                    replacement = Some((removed.to_vec(), vec![row]));
                    break 'pairs;
                }
            }
        }
        if replacement.is_none() {
            'triples: for first in 0..selected.len() {
                for second in first + 1..selected.len() {
                    for third in second + 1..selected.len() {
                        let removed = [first, second, third];
                        let (uncovered, retained) =
                            uncovered_without(rows, target, selected, &removed);
                        let Some(pivot) = first_set_bit(&uncovered) else {
                            replacement = Some((removed.to_vec(), Vec::new()));
                            break 'triples;
                        };
                        for left in 0..rows.len() {
                            if retained[left]
                                || rows[left][pivot / u64::BITS as usize]
                                    & (1_u64 << (pivot % u64::BITS as usize))
                                    == 0
                            {
                                continue;
                            }
                            let residual = uncovered
                                .iter()
                                .zip(&rows[left])
                                .map(|(uncovered, coverage)| uncovered & !coverage)
                                .collect::<Vec<_>>();
                            if residual.iter().all(|word| *word == 0) {
                                replacement = Some((removed.to_vec(), vec![left]));
                                break 'triples;
                            }
                            let residual_pivot = first_set_bit(&residual)
                                .expect("non-empty residual has one set bit");
                            for right in 0..rows.len() {
                                if right == left
                                    || retained[right]
                                    || rows[right][residual_pivot / u64::BITS as usize]
                                        & (1_u64 << (residual_pivot % u64::BITS as usize))
                                        == 0
                                {
                                    continue;
                                }
                                if words_cover_masked_probe_pair(
                                    &rows[left],
                                    &rows[right],
                                    &uncovered,
                                ) {
                                    replacement = Some((removed.to_vec(), vec![left, right]));
                                    break 'triples;
                                }
                            }
                        }
                    }
                }
            }
        }
        let Some((removed, additions)) = replacement else {
            return;
        };
        for position in removed.into_iter().rev() {
            selected.remove(position);
        }
        for row in additions {
            if !selected.contains(&row) {
                selected.push(row);
            }
        }
        selected.sort_unstable();
        remove_redundant_cover_rows(rows, target, selected);
    }
}

fn uncovered_without(
    rows: &[Vec<u64>],
    target: &[u64],
    selected: &[usize],
    removed_positions: &[usize],
) -> (Vec<u64>, Vec<bool>) {
    let mut covered = vec![0_u64; target.len()];
    let mut retained = vec![false; rows.len()];
    for (position, row) in selected.iter().copied().enumerate() {
        if removed_positions.contains(&position) {
            continue;
        }
        retained[row] = true;
        union_words_probe(&mut covered, &rows[row]);
    }
    let uncovered = covered
        .iter()
        .zip(target)
        .map(|(covered, target)| target & !covered)
        .collect();
    (uncovered, retained)
}

fn words_cover_masked_probe(coverage: &[u64], required: &[u64]) -> bool {
    coverage
        .iter()
        .zip(required)
        .all(|(coverage, required)| coverage & required == *required)
}

fn words_cover_masked_probe_pair(left: &[u64], right: &[u64], required: &[u64]) -> bool {
    left.iter()
        .zip(right)
        .zip(required)
        .all(|((left, right), required)| (left | right) & required == *required)
}

fn union_words_probe(target: &mut [u64], source: &[u64]) {
    for (target, source) in target.iter_mut().zip(source) {
        *target |= *source;
    }
}

fn words_cover_probe(covered: &[u64], target: &[u64]) -> bool {
    covered
        .iter()
        .zip(target)
        .all(|(covered, target)| covered & target == *target)
}

fn is_superset(left: &PatternBitSet, right: &PatternBitSet) -> bool {
    (0..left.word_count())
        .all(|word| left.word_at(word) & right.word_at(word) == right.word_at(word))
}

fn greedy_cover_cardinality(required: &PatternBitSet, rows: &[PatternBitSet]) -> Option<usize> {
    let mut covered = PatternBitSet::new(required.pattern_count());
    let mut selected = vec![false; rows.len()];
    let mut count = 0_usize;
    while !is_superset(&covered, required) {
        let next = rows
            .iter()
            .enumerate()
            .filter(|(row_index, _)| !selected[*row_index])
            .map(|(row_index, row)| {
                let gain = (0..required.word_count())
                    .map(|word| {
                        (row.word_at(word) & required.word_at(word) & !covered.word_at(word))
                            .count_ones() as usize
                    })
                    .sum::<usize>();
                (gain, row_index)
            })
            .max_by(|(left_gain, left_row), (right_gain, right_row)| {
                left_gain
                    .cmp(right_gain)
                    .then_with(|| right_row.cmp(left_row))
            })
            .filter(|(gain, _)| *gain != 0)
            .map(|(_, row_index)| row_index)?;
        selected[next] = true;
        count += 1;
        covered.union_with(&rows[next]).ok()?;
    }
    Some(count)
}

fn greedy_cover_cardinality_with_seed(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
    seed: usize,
) -> Option<usize> {
    let mut covered = PatternBitSet::new(required.pattern_count());
    covered.union_with(&rows[seed]).ok()?;
    let mut selected = vec![false; rows.len()];
    selected[seed] = true;
    let mut count = 1_usize;
    while !is_superset(&covered, required) {
        let next = rows
            .iter()
            .enumerate()
            .filter(|(row_index, _)| !selected[*row_index])
            .map(|(row_index, row)| {
                let gain = (0..required.word_count())
                    .map(|word| {
                        (row.word_at(word) & required.word_at(word) & !covered.word_at(word))
                            .count_ones() as usize
                    })
                    .sum::<usize>();
                (gain, row_index)
            })
            .max_by(|(left_gain, left_row), (right_gain, right_row)| {
                left_gain
                    .cmp(right_gain)
                    .then_with(|| right_row.cmp(left_row))
            })
            .filter(|(gain, _)| *gain != 0)
            .map(|(_, row_index)| row_index)?;
        selected[next] = true;
        count += 1;
        covered.union_with(&rows[next]).ok()?;
    }
    Some(count)
}

fn rarity_greedy_cover_cardinality(
    required: &PatternBitSet,
    rows: &[PatternBitSet],
) -> Option<usize> {
    const SCALE: u64 = 1 << 32;
    let support_counts = (0..required.pattern_count())
        .map(|pattern| {
            let word = pattern / u64::BITS as usize;
            let bit = pattern % u64::BITS as usize;
            rows.iter()
                .filter(|row| row.word_at(word) & (1_u64 << bit) != 0)
                .count()
        })
        .collect::<Vec<_>>();
    let mut covered = PatternBitSet::new(required.pattern_count());
    let mut selected = vec![false; rows.len()];
    let mut count = 0_usize;
    while !is_superset(&covered, required) {
        let next = rows
            .iter()
            .enumerate()
            .filter(|(row_index, _)| !selected[*row_index])
            .map(|(row_index, row)| {
                let mut score = 0_u64;
                let mut gain = 0_usize;
                for (pattern, support_count) in support_counts.iter().copied().enumerate() {
                    let word = pattern / u64::BITS as usize;
                    let bit = pattern % u64::BITS as usize;
                    if required.word_at(word)
                        & !covered.word_at(word)
                        & row.word_at(word)
                        & (1_u64 << bit)
                        == 0
                    {
                        continue;
                    }
                    gain += 1;
                    score += SCALE / support_count as u64;
                }
                (score, gain, row_index)
            })
            .max_by(|left, right| {
                left.0
                    .cmp(&right.0)
                    .then_with(|| left.1.cmp(&right.1))
                    .then_with(|| right.2.cmp(&left.2))
            })
            .filter(|(_, gain, _)| *gain != 0)
            .map(|(_, _, row_index)| row_index)?;
        selected[next] = true;
        count += 1;
        covered.union_with(&rows[next]).ok()?;
    }
    Some(count)
}
