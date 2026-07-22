use super::*;

pub(super) const BACKEND_EQUIVALENCE_ALLOWED_DIFFERENCES: &[&str] = &[
    "backend_requested",
    "backend_selected",
    "gpu_trust_state",
    "memory_ticket_id",
    "fence_epoch",
    "elapsed_ms",
    "raw candidate order",
    "trace retention sample order",
];

pub(super) const TSAR_FIXTURE: &str =
    "tests/fixtures/external-pc/tsar_cannon_after_2bag_full_42.json";
pub(super) const PCO_SOLUTION_FUMEN: &str =
    "tests/fixtures/fumens/external-pc/pco_opener_full_63_solutions.fumen";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SourceSolutionSet {
    pub(super) unique_solution_count: usize,
    pub(super) hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SourceColoredFieldSet {
    pub(super) keys: BTreeSet<String>,
    pub(super) hash: String,
}

#[derive(Debug)]
pub(super) struct ExternalPcScenarioMaterial {
    pub(super) initial_board_mask: u64,
    pub(super) visible_height: u16,
    pub(super) hold_piece: Option<PieceKind>,
    pub(super) piece_window: usize,
    pub(super) exact_pieces: usize,
    pub(super) retained_trace_limit: usize,
    pub(super) rule: RuleProfile,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ExternalPcCase {
    PcoIHold,
    PcoIHoldMirror,
    TsarCannonFull42,
    TsarCannonFull42Mirror,
}

impl ExternalPcCase {
    pub(super) fn source_solution_set(self) -> Option<SourceSolutionSet> {
        match self {
            Self::PcoIHold | Self::PcoIHoldMirror | Self::TsarCannonFull42Mirror => None,
            Self::TsarCannonFull42 => Some(tsar_full_42_solution_set()),
        }
    }
}

#[derive(Debug)]
pub(super) struct BackendEvidence {
    pub(super) backend_requested: String,
    pub(super) backend_selected: String,
    pub(super) backend_fallback_used: bool,
    pub(super) backend_fallback_reason: String,
    pub(super) gpu_unavailable_reason: String,
    pub(super) gpu_failure_class: String,
    pub(super) gpu_failure_stage: String,
    pub(super) final_board_empty: bool,
    pub(super) unique_solution_count: usize,
    pub(super) normalized_unique_solution_count: usize,
    pub(super) actual_normalized_unique_solution_count: usize,
    pub(super) normalized_solution_set_hash: String,
    pub(super) actual_normalized_solution_set_hash: String,
    pub(super) normalized_solution_key_algorithm: String,
    pub(super) actual_solution_set_contract: String,
    pub(super) source_normalized_solution_set_hash: Option<String>,
    pub(super) source_normalized_unique_solution_count: Option<usize>,
    pub(super) coverage_probability: String,
    pub(super) count_complete: bool,
    pub(super) gpu_result_deterministic: bool,
    pub(super) gpu_result_cpu_confirmed: bool,
    pub(super) gpu_cpu_reference_match: bool,
    pub(super) gpu_assisted_buildup_reached: bool,
    pub(super) memory_leak_report_clean: bool,
    pub(super) normalized_solution_keys: BTreeSet<String>,
}

impl BackendEvidence {
    pub(super) fn from_result(
        source_solution_set: Option<SourceSolutionSet>,
        _requested_backend: RequestedSearchBackend,
        result: &CoreExecutionResult,
    ) -> Self {
        let final_board_empty = bool_field(result, "final_board_empty").unwrap_or_else(|| {
            result.solution_found() && result.field("completion_goal") == Some("clear-to-empty")
        });
        let unique_solution_count = usize_field(result, "unique_solution_count");
        let normalized_unique_solution_count =
            usize_field(result, "normalized_unique_solution_count");
        let actual_normalized_unique_solution_count =
            usize_field(result, "actual_normalized_unique_solution_count");
        Self {
            backend_requested: string_field(result, "backend_requested"),
            backend_selected: string_field(result, "backend_selected"),
            backend_fallback_used: bool_field(result, "backend_fallback_used").unwrap_or(false),
            backend_fallback_reason: string_field(result, "backend_fallback_reason"),
            gpu_unavailable_reason: string_field(result, "gpu_unavailable_reason"),
            gpu_failure_class: string_field(result, "gpu_failure_class"),
            gpu_failure_stage: string_field(result, "gpu_failure_stage"),
            final_board_empty,
            unique_solution_count,
            normalized_unique_solution_count,
            actual_normalized_unique_solution_count,
            normalized_solution_set_hash: string_field(result, "normalized_solution_set_hash"),
            actual_normalized_solution_set_hash: string_field(
                result,
                "actual_normalized_solution_set_hash",
            ),
            normalized_solution_key_algorithm: string_field(
                result,
                "normalized_solution_key_algorithm",
            ),
            actual_solution_set_contract: string_field(result, "actual_solution_set_contract"),
            source_normalized_solution_set_hash: source_solution_set
                .as_ref()
                .map(|source| source.hash.clone()),
            source_normalized_unique_solution_count: source_solution_set
                .as_ref()
                .map(|source| source.unique_solution_count),
            coverage_probability: string_field(result, "coverage_probability"),
            count_complete: bool_field(result, "count_complete").unwrap_or(false),
            gpu_result_deterministic: bool_field(result, "gpu_result_deterministic")
                .unwrap_or(false),
            gpu_result_cpu_confirmed: bool_field(result, "gpu_result_cpu_confirmed")
                .unwrap_or(false),
            gpu_cpu_reference_match: bool_field(result, "gpu_cpu_reference_match").unwrap_or(false),
            gpu_assisted_buildup_reached: bool_field(result, "gpu_assisted_buildup_reached")
                .unwrap_or(false),
            memory_leak_report_clean: bool_field(result, "memory_leak_report_clean")
                .unwrap_or(false),
            normalized_solution_keys: result.normalized_solution_keys().iter().cloned().collect(),
        }
    }
}

pub(super) fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root")
        .to_path_buf()
}

pub(super) fn fixture_text(path: &str) -> String {
    let full_path = workspace_root().join(path);
    fs::read_to_string(&full_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", full_path.display()))
}

pub(super) fn assert_fixture_marker(path: &str, marker: &str) {
    let text = fixture_text(path);
    assert!(
        text.contains(marker),
        "fixture {} missing marker {marker}",
        workspace_root().join(path).display()
    );
}

pub(super) fn materialized_external_pc_case(case: ExternalPcCase) -> ExternalPcScenarioMaterial {
    match case {
        ExternalPcCase::PcoIHold => ExternalPcScenarioMaterial {
            initial_board_mask: 0x0000_00e0_f87e_3f87,
            visible_height: 4,
            hold_piece: Some(PieceKind::I),
            piece_window: 4,
            exact_pieces: 4,
            retained_trace_limit: 1,
            rule: srs_plus(),
        },
        ExternalPcCase::PcoIHoldMirror => ExternalPcScenarioMaterial {
            initial_board_mask: 0x0000_00c1_f87f_1f87,
            visible_height: 4,
            hold_piece: Some(PieceKind::I),
            piece_window: 4,
            exact_pieces: 4,
            retained_trace_limit: 1,
            rule: srs_plus(),
        },
        ExternalPcCase::TsarCannonFull42 => ExternalPcScenarioMaterial {
            initial_board_mask: 0x0003_00c0_399e_3fdf,
            visible_height: 5,
            hold_piece: None,
            piece_window: 6,
            exact_pieces: 6,
            retained_trace_limit: 42,
            rule: srs_plus(),
        },
        ExternalPcCase::TsarCannonFull42Mirror => ExternalPcScenarioMaterial {
            initial_board_mask: 0x0000_0300_e67f_1fef,
            visible_height: 5,
            hold_piece: None,
            piece_window: 6,
            exact_pieces: 6,
            retained_trace_limit: 42,
            rule: srs_plus(),
        },
    }
}

pub(super) fn external_pc_scenario_query(
    case: ExternalPcCase,
    backend: RequestedSearchBackend,
    allow_backend_fallback: bool,
) -> PcScenarioQuery {
    let material = materialized_external_pc_case(case);
    let execution_policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(backend)
        .with_allow_backend_fallback(allow_backend_fallback)
        .with_max_candidates(5_000_000)
        .with_max_patterns(5_040)
        .with_workers(1);

    PcScenarioQuery::new(
        PcScenarioBoard::standard_10(material.visible_height, material.initial_board_mask),
        PcQueueInput::standard_7_bag(),
        PieceWindow::new(material.piece_window),
    )
    .with_exact_pieces(Some(material.exact_pieces))
    .with_min_remaining_queue(0)
    .with_hold_piece(material.hold_piece)
    .with_allow_hold(true)
    .with_rule(material.rule)
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_retained_trace_limit(material.retained_trace_limit)
    .with_execution_policy(execution_policy)
}

pub(super) fn bool_field(result: &CoreExecutionResult, key: &str) -> Option<bool> {
    result.bool_field(key)
}

pub(super) fn string_field(result: &CoreExecutionResult, key: &str) -> String {
    result
        .field(key)
        .unwrap_or_else(|| panic!("missing field {key}"))
        .to_owned()
}

pub(super) fn usize_field(result: &CoreExecutionResult, key: &str) -> usize {
    result
        .usize_field(key)
        .unwrap_or_else(|| panic!("missing usize field {key}"))
}

pub(super) fn tsar_full_42_solution_set() -> SourceSolutionSet {
    SourceSolutionSet {
        unique_solution_count: 42,
        hash: "cts1:4996a1501bbb8212".to_owned(),
    }
}

pub(super) fn pco_full_63_solution_set() -> SourceColoredFieldSet {
    let diagram = SourceFumenColoredFieldSet::decode(&fixture_text(PCO_SOLUTION_FUMEN))
        .expect("decode PCO 63-page colored-field solution fumen");
    assert_eq!(diagram.page_count(), 63);
    assert_eq!(diagram.initial_board_mask(), 0x0000_00e0_f87e_3f87);
    assert!(!diagram.operation_replay_available());
    SourceColoredFieldSet {
        keys: diagram.keys().clone(),
        hash: diagram.hash().to_owned(),
    }
}

pub(super) fn run_external_case(
    case: ExternalPcCase,
    backend: RequestedSearchBackend,
    allow_backend_fallback: bool,
) -> Result<BackendEvidence, String> {
    let query = external_pc_scenario_query(case, backend, allow_backend_fallback);
    let problem =
        ProblemCompiler::compile_scenario_pc(&query).map_err(|error| format!("{error:?}"))?;
    let result = PcService::execute(&problem).map_err(|error| format!("{error:?}"))?;
    Ok(BackendEvidence::from_result(
        case.source_solution_set(),
        backend,
        &result,
    ))
}

pub(super) fn assert_equivalent_result_contract(
    expected: &BackendEvidence,
    actual: &BackendEvidence,
) {
    assert!(
        BACKEND_EQUIVALENCE_ALLOWED_DIFFERENCES.contains(&"backend_requested"),
        "policy marker: backend_requested/backend_selected may differ"
    );
    assert_eq!(actual.final_board_empty, expected.final_board_empty);
    assert_eq!(actual.unique_solution_count, expected.unique_solution_count);
    assert_eq!(
        actual.normalized_unique_solution_count,
        expected.normalized_unique_solution_count
    );
    assert_eq!(
        actual.actual_normalized_unique_solution_count,
        expected.actual_normalized_unique_solution_count
    );
    assert_eq!(
        actual.normalized_solution_set_hash,
        expected.normalized_solution_set_hash
    );
    assert_eq!(
        actual.actual_normalized_solution_set_hash,
        expected.actual_normalized_solution_set_hash
    );
    assert_eq!(
        actual.normalized_solution_key_algorithm,
        expected.normalized_solution_key_algorithm
    );
    assert_eq!(
        actual.actual_solution_set_contract,
        expected.actual_solution_set_contract
    );
    assert_eq!(actual.coverage_probability, expected.coverage_probability);
    assert_eq!(actual.count_complete, expected.count_complete);
}

pub(super) fn assert_matches_source_solution_set(
    actual: &BackendEvidence,
    source: &SourceSolutionSet,
) {
    assert_eq!(
        actual.source_normalized_solution_set_hash.as_deref(),
        Some(source.hash.as_str())
    );
    assert_eq!(
        actual.source_normalized_unique_solution_count,
        Some(source.unique_solution_count)
    );
    assert_eq!(actual.actual_solution_set_contract, "normalized-tiling-set");
    assert_eq!(
        actual.normalized_solution_key_algorithm,
        "clearra-normalized-tiling-key-v1"
    );
    assert_eq!(
        actual.normalized_unique_solution_count,
        source.unique_solution_count
    );
    assert_eq!(
        actual.actual_normalized_unique_solution_count,
        source.unique_solution_count
    );
    assert_eq!(actual.normalized_solution_set_hash, source.hash);
    assert_eq!(actual.actual_normalized_solution_set_hash, source.hash);
}

pub(super) fn assert_matches_source_colored_field_set(
    actual: &BackendEvidence,
    source: &SourceColoredFieldSet,
) {
    let actual_keys = actual
        .normalized_solution_keys
        .iter()
        .map(|key| normalized_tiling_key_to_colored_field_key(key))
        .collect::<BTreeSet<_>>();
    let missing = source
        .keys
        .difference(&actual_keys)
        .take(10)
        .cloned()
        .collect::<Vec<_>>();
    let unexpected = actual_keys
        .difference(&source.keys)
        .take(10)
        .cloned()
        .collect::<Vec<_>>();

    assert_eq!(source.keys.len(), 63, "source hash={}", source.hash);
    assert_eq!(
        actual_keys, source.keys,
        "PCO colored-field set mismatch; missing={missing:?}, unexpected={unexpected:?}"
    );
}

fn normalized_tiling_key_to_colored_field_key(key: &str) -> String {
    let encoded = key
        .strip_prefix("ctk1|initial=")
        .unwrap_or_else(|| panic!("unexpected normalized key: {key}"));
    let (initial, placements) = encoded
        .split_once("|placements=")
        .unwrap_or_else(|| panic!("missing placements in normalized key: {key}"));
    let mut masks = [0u64; 7];
    for placement in placements
        .split(',')
        .filter(|placement| !placement.is_empty())
    {
        let (piece, mask) = placement
            .split_once(':')
            .unwrap_or_else(|| panic!("invalid placement: {placement}"));
        let piece =
            PieceKind::from_ascii(piece.chars().next().expect("piece id")).expect("standard piece");
        let index = PieceKind::STANDARD_TETROMINOES
            .iter()
            .position(|candidate| *candidate == piece)
            .expect("standard piece index");
        masks[index] |= u64::from_str_radix(mask, 16).expect("placement mask");
    }
    let colors = PieceKind::STANDARD_TETROMINOES
        .iter()
        .copied()
        .zip(masks)
        .filter(|(_, mask)| *mask != 0)
        .map(|(piece, mask)| format!("{}:{mask:016x}", piece.as_ascii()))
        .collect::<Vec<_>>()
        .join(",");
    format!("cfk1|initial={initial}|colors={colors}")
}

pub(super) fn assert_explicit_backend_fallback(
    evidence: &BackendEvidence,
    requested_backend: RequestedSearchBackend,
) {
    assert_eq!(evidence.backend_requested, requested_backend.as_str());
    assert_eq!(evidence.backend_selected, "cpu");
    assert!(evidence.backend_fallback_used);
    assert!(
        matches!(requested_backend, RequestedSearchBackend::Gpu),
        "expected an explicit GPU request"
    );
    assert_eq!(evidence.gpu_failure_class, "unavailable");
    assert_eq!(evidence.gpu_failure_stage, "capability-query");
    assert!(
        matches!(
            evidence.backend_fallback_reason.as_str(),
            "gpu_device_not_found" | "gpu_kernel_unavailable"
        ),
        "unexpected capability fallback reason: {}",
        evidence.backend_fallback_reason
    );
    assert!(!evidence.gpu_result_cpu_confirmed);
    assert!(!evidence.gpu_cpu_reference_match);
    assert!(!evidence.gpu_assisted_buildup_reached);
}

fn assert_hybrid_cpu_selection(evidence: &BackendEvidence) {
    assert_eq!(evidence.backend_requested, "hybrid");
    assert_eq!(evidence.backend_selected, "cpu");
    assert!(!evidence.backend_fallback_used);
    assert_eq!(evidence.backend_fallback_reason, "none");
    assert_eq!(evidence.gpu_failure_class, "none");
    assert_eq!(evidence.gpu_failure_stage, "none");
    assert!(matches!(
        evidence.gpu_unavailable_reason.as_str(),
        "gpu_backend_not_connected" | "gpu_device_not_found" | "gpu_kernel_unavailable"
    ));
}

pub(super) fn assert_connected_gpu_execution(
    evidence: &BackendEvidence,
    requested_backend: RequestedSearchBackend,
) {
    assert_eq!(evidence.backend_requested, requested_backend.as_str());
    assert_eq!(evidence.backend_selected, "gpu");
    assert!(!evidence.backend_fallback_used);
    assert_eq!(evidence.backend_fallback_reason, "none");
    assert!(evidence.gpu_result_deterministic);
    assert!(!evidence.gpu_result_cpu_confirmed);
    assert!(!evidence.gpu_cpu_reference_match);
}

pub(super) fn assert_gpu_execution_or_explicit_fallback(
    evidence: &BackendEvidence,
    requested_backend: RequestedSearchBackend,
) {
    if evidence.backend_selected == "gpu" {
        assert_connected_gpu_execution(evidence, requested_backend);
    } else if matches!(requested_backend, RequestedSearchBackend::Hybrid) {
        assert_hybrid_cpu_selection(evidence);
    } else {
        assert_explicit_backend_fallback(evidence, requested_backend);
    }
}

pub(super) fn assert_mirrored_solution_sets(
    original: &BackendEvidence,
    mirrored: &BackendEvidence,
    visible_height: u8,
) {
    let layout = Board64Layout::standard_10_by_lines(visible_height).expect("mirror layout");
    let expected = original
        .normalized_solution_keys
        .iter()
        .map(|key| mirror_normalized_solution_key(key, layout))
        .collect::<BTreeSet<_>>();
    let missing = expected
        .difference(&mirrored.normalized_solution_keys)
        .take(10)
        .cloned()
        .collect::<Vec<_>>();
    let unexpected = mirrored
        .normalized_solution_keys
        .difference(&expected)
        .take(10)
        .cloned()
        .collect::<Vec<_>>();

    assert_eq!(
        mirrored.normalized_solution_keys, expected,
        "mirrored solution set mismatch; missing={missing:?}, unexpected={unexpected:?}"
    );
    assert_eq!(
        original.actual_normalized_unique_solution_count,
        mirrored.actual_normalized_unique_solution_count
    );
    assert!(original.count_complete);
    assert!(mirrored.count_complete);
}

fn mirror_normalized_solution_key(key: &str, layout: Board64Layout) -> String {
    let encoded = key
        .strip_prefix("ctk1|initial=")
        .unwrap_or_else(|| panic!("unexpected normalized key: {key}"));
    let (initial, placements) = encoded
        .split_once("|placements=")
        .unwrap_or_else(|| panic!("missing placements in normalized key: {key}"));
    let initial = u64::from_str_radix(initial, 16).expect("initial mask");
    let placements = placements
        .split(',')
        .filter(|placement| !placement.is_empty())
        .map(|placement| {
            let (piece, mask) = placement
                .split_once(':')
                .unwrap_or_else(|| panic!("invalid placement: {placement}"));
            let piece = PieceKind::from_ascii(piece.chars().next().expect("piece id"))
                .expect("standard piece");
            let mask = u64::from_str_radix(mask, 16).expect("placement mask");
            PiecePlacementMask::new(
                mirror_piece(piece),
                MirrorTransform::mirror_mask(mask, layout),
            )
        });

    NormalizedTilingSolutionKey::from_placements(
        MirrorTransform::mirror_mask(initial, layout),
        placements,
    )
    .expect("mirrored normalized solution")
    .to_string()
}

fn mirror_piece(piece: PieceKind) -> PieceKind {
    match piece {
        PieceKind::S => PieceKind::Z,
        PieceKind::Z => PieceKind::S,
        PieceKind::J => PieceKind::L,
        PieceKind::L => PieceKind::J,
        PieceKind::I | PieceKind::O | PieceKind::T => piece,
    }
}
