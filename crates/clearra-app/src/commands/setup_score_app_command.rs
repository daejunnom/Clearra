use clearra_coverage::pattern::pattern_id::PatternId;
use clearra_host_contract::{
    ProductResultPayload, ProductResultPayloadContent, SetupScoreCandidatePayload,
    SetupScoreRankingPayload,
};
use clearra_objectives::policy::{
    objective_policy::ObjectivePolicy, score_objective_policy::ScoreProfileSelection,
};
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    RequestedSearchBackend, SupplyWindowSize,
};
use clearra_problem::{
    BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityQuery,
    BuildSolutionProbabilityPolicy,
};
use clearra_rules::profile::rule_profile::RuleProfile;
use sha2::{Digest, Sha256};

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    build_solution_probability_result::build_v2_facade::{
        BuildColoredTargetSetV1, BuildObjective, BuildSetupV1Request,
    },
    commands::execution_error_response::core_execution_error_response,
    pc_score_summary_result::{
        PcScoreCompiledAuthority, PcScoreCompiledAuthorityError, PcScoreIngressOrigin,
    },
    render::{AppMessage, AppRenderModel, AppResultKind},
    SetupScoreDocumentV1, PC_SCORE_MAX_PATTERNS,
};

pub const SETUP_SCORE_PROBLEM_CONTRACT: &str = "setup-document-score.v1";
pub const SETUP_SCORE_INPUT_CONTRACT: &str = "setup-score-document.v1";
pub const SETUP_SCORE_RESULT_CONTRACT: &str = "setup-score-ranking.v1";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetupScoreAppCommandError {
    ClearHeightInvalid,
    DocumentOutsideClearHeight,
    CompletedBoardInvalid,
    ContinuationPieceCountInvalid,
    CoverageExecutionPolicyInvalid,
    CoverageQueryInvalid,
    CoverageRequestInvalid,
}

#[derive(Clone, Debug, PartialEq)]
struct SetupScoreCandidateRequest {
    candidate_id: String,
    completed_board_mask: u64,
    coverage: BuildSetupV1Request,
    continuation: PcScenarioQuery,
}

/// Nominal App request for the three-phase `setup.score` product.
///
/// The command owns both actual producer inputs. It never accepts caller-
/// supplied coverage rows or score scalars, and attack is absent from its
/// reduction and public DTO.
#[derive(Clone, Debug, PartialEq)]
pub struct SetupScoreAppCommand {
    document_format: crate::FieldDocumentFormat,
    document_hash: String,
    source_page_count: usize,
    rule: RuleProfile,
    score_profile: ScoreProfileSelection,
    initial_b2b: u32,
    candidates: Vec<SetupScoreCandidateRequest>,
}

impl SetupScoreAppCommand {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        document: SetupScoreDocumentV1,
        setup_queue: PcQueueInput,
        setup_standard_bag_len: Option<usize>,
        solution_queue: PcQueueInput,
        solution_standard_bag_len: Option<usize>,
        clear_height: u8,
        setup_hold_enabled: bool,
        score_profile: ScoreProfileSelection,
        initial_b2b: u32,
        rule: RuleProfile,
        coverage_execution_policy: PcExecutionPolicy,
    ) -> Result<Self, SetupScoreAppCommandError> {
        if !(1..=6).contains(&clear_height) {
            return Err(SetupScoreAppCommandError::ClearHeightInvalid);
        }
        if document.visible_height() > clear_height {
            return Err(SetupScoreAppCommandError::DocumentOutsideClearHeight);
        }
        if coverage_execution_policy.requested_backend() != RequestedSearchBackend::Cpu
            || coverage_execution_policy.allow_backend_fallback()
            || coverage_execution_policy.max_memory_mib().is_some()
        {
            return Err(SetupScoreAppCommandError::CoverageExecutionPolicyInvalid);
        }
        let continuation_execution_policy = score_execution_policy(&coverage_execution_policy);

        let document_format = document.format();
        let document_hash = document.document_hash().to_owned();
        let source_page_count = document.source_page_count();
        let mut candidates = Vec::with_capacity(document.candidates().len());
        for candidate in document.candidates() {
            let completed_board_mask = candidate.completed_board_mask();
            let visible_cell_count = usize::from(clear_height) * 10;
            let visible_mask = if visible_cell_count == 64 {
                u64::MAX
            } else {
                (1_u64 << visible_cell_count) - 1
            };
            if completed_board_mask & !visible_mask != 0 {
                return Err(SetupScoreAppCommandError::DocumentOutsideClearHeight);
            }

            let field = BuildProbabilityField::from_words_preserving_height(
                clear_height,
                [candidate.initial_board_mask(), 0, 0, 0],
                [candidate.target_cells_mask(), 0, 0, 0],
            )
            .map_err(|_| SetupScoreAppCommandError::CoverageQueryInvalid)?
            .with_horizontal_mirror_included(false);
            let setup_piece_count = field.target_piece_count();
            if setup_piece_count != document.setup_piece_count() {
                return Err(SetupScoreAppCommandError::CoverageQueryInvalid);
            }
            let compact_base = field
                .compact_base_mask()
                .ok_or(SetupScoreAppCommandError::CoverageQueryInvalid)?;
            let mut setup_query = PcScenarioQuery::new(
                PcScenarioBoard::standard_10(u16::from(clear_height), compact_base),
                setup_queue.clone(),
                PieceWindow::new(setup_piece_count),
            )
            .with_rule(rule)
            .with_exact_pieces(Some(setup_piece_count))
            .with_min_remaining_queue(0)
            .with_allow_hold(setup_hold_enabled)
            .with_count_policy(PcCountPolicy::CountUnique)
            .with_objective(ObjectivePolicy::unique())
            .with_retained_trace_limit(1)
            .with_execution_policy(coverage_execution_policy.clone());
            if let Some(length) = setup_standard_bag_len {
                setup_query =
                    setup_query
                        .with_supply_window_size(SupplyWindowSize::new(length.min(
                            setup_piece_count.saturating_add(usize::from(setup_hold_enabled)),
                        )));
            }
            let coverage_query = BuildProbabilityQuery::new(setup_query, field)
                .with_aggregation(BuildProbabilityAggregation::Buildability)
                .with_solution_probability_policy(BuildSolutionProbabilityPolicy::Include);
            let candidate_document_hash =
                candidate_document_hash(document_hash.as_str(), candidate.candidate_id());
            let target = BuildColoredTargetSetV1::new(
                clear_height,
                1,
                candidate_document_hash,
                [candidate.identity()],
            )
            .map_err(|_| SetupScoreAppCommandError::CoverageRequestInvalid)?;
            let coverage = BuildSetupV1Request::new(coverage_query, target, BuildObjective::Unique)
                .map_err(|_| SetupScoreAppCommandError::CoverageRequestInvalid)?;

            let normalized_board =
                PcScenarioBoard::standard_10(u16::from(clear_height), completed_board_mask)
                    .after_initial_line_clear();
            let occupied = normalized_board.occupied_mask().count_ones() as usize;
            let empty_cells = visible_cell_count
                .checked_sub(occupied)
                .ok_or(SetupScoreAppCommandError::CompletedBoardInvalid)?;
            if empty_cells == 0 || empty_cells % 4 != 0 {
                return Err(SetupScoreAppCommandError::ContinuationPieceCountInvalid);
            }
            let continuation_piece_count = empty_cells / 4;
            if continuation_piece_count > 15 {
                return Err(SetupScoreAppCommandError::ContinuationPieceCountInvalid);
            }
            let mut continuation = PcScenarioQuery::new(
                normalized_board,
                solution_queue.clone(),
                PieceWindow::new(continuation_piece_count),
            )
            .with_rule(rule)
            .with_exact_pieces(Some(continuation_piece_count))
            .with_min_remaining_queue(0)
            .with_allow_hold(false)
            .with_count_policy(PcCountPolicy::CountAll)
            .with_objective(
                ObjectivePolicy::all()
                    .with_score_profile(score_profile)
                    .with_initial_b2b(initial_b2b),
            )
            .with_retained_trace_limit(1)
            .with_execution_policy(continuation_execution_policy.clone());
            if let Some(length) = solution_standard_bag_len {
                continuation = continuation.with_supply_window_size(SupplyWindowSize::new(
                    length.min(continuation_piece_count),
                ));
            }
            candidates.push(SetupScoreCandidateRequest {
                candidate_id: candidate.candidate_id().to_owned(),
                completed_board_mask,
                coverage,
                continuation,
            });
        }
        Ok(Self {
            document_format,
            document_hash,
            source_page_count,
            rule,
            score_profile,
            initial_b2b,
            candidates,
        })
    }

    pub const fn document_format(&self) -> crate::FieldDocumentFormat {
        self.document_format
    }

    pub fn document_hash(&self) -> &str {
        &self.document_hash
    }

    pub const fn source_page_count(&self) -> usize {
        self.source_page_count
    }

    pub const fn score_profile(&self) -> ScoreProfileSelection {
        self.score_profile
    }

    pub const fn rule(&self) -> RuleProfile {
        self.rule
    }

    pub const fn initial_b2b(&self) -> u32 {
        self.initial_b2b
    }

    /// Conservative upper bound for every Setup-score owner which remains
    /// live beside one exact PC-score authority during direct App execution.
    pub(crate) fn checked_direct_external_retained_upper_bound_bytes(&self) -> Option<u128> {
        let mut bytes = (self.document_hash.capacity() as u128).checked_add(
            (self.candidates.capacity() as u128)
                .checked_mul(core::mem::size_of::<SetupScoreCandidateRequest>() as u128)?,
        )?;
        let mut transient = (self.candidates.len() as u128)
            .checked_mul(core::mem::size_of::<ReducedCandidate>() as u128)?;
        for candidate in &self.candidates {
            bytes = bytes
                .checked_add(candidate.candidate_id.capacity() as u128)?
                .checked_add(candidate.coverage.checked_retained_capacity_bytes()?)?
                .checked_add(candidate.continuation.checked_retained_capacity_bytes()?)?;
            let pattern_count = candidate.coverage.setup_score_max_patterns();
            let row_bytes =
                clearra_coverage::pattern::pattern_bitset::PatternBitSet::checked_all_projection(
                    pattern_count,
                )?
                .storage_retained_bytes;
            let weight_bytes =
                (pattern_count as u128).checked_mul(core::mem::size_of::<f64>() as u128)?;
            transient = transient
                .checked_add(candidate.candidate_id.capacity() as u128)?
                .checked_add(row_bytes)?
                .checked_add(weight_bytes)?
                .checked_add(256)?;
        }
        bytes.checked_add(transient)
    }
}

#[derive(Clone, Debug)]
struct ReducedCandidate {
    candidate_id: String,
    completed_board_mask: u64,
    setup_covered_pattern_count: usize,
    setup_covered_probability: String,
    setup_row: clearra_coverage::pattern::pattern_bitset::PatternBitSet,
    continuation_probability: String,
    expected_score_bits: u64,
    expected_score: String,
}

impl RunnableAppCommand for SetupScoreAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        match execute(self, context) {
            Ok(payload) => AppResponse::success(AppRenderModel::Verify(AppMessage::new(
                AppResultKind::Setup,
                Vec::new(),
            )))
            .with_public_product_result(payload, None),
            Err(SetupScoreRunError::Core(error)) => core_execution_error_response(error),
            Err(SetupScoreRunError::Rejected(detail)) => AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(AppErrorCode::ExecutionFailed, detail),
            ),
        }
    }
}

enum SetupScoreRunError {
    Core(clearra_core_executor::CoreExecutionError),
    Rejected(String),
}

fn execute(
    command: SetupScoreAppCommand,
    context: &AppExecutionContext<'_>,
) -> Result<ProductResultPayload, SetupScoreRunError> {
    let mut reduced = Vec::with_capacity(command.candidates.len());
    let mut canonical_weights = None;
    let mut pattern_universe_identity = None;
    for candidate in command.candidates.iter().cloned() {
        let coverage = candidate
            .coverage
            .execute_setup_score_coverage(
                context.services().core_executor(),
                context.execution_control(),
            )
            .map_err(|error| {
                SetupScoreRunError::Rejected(format!(
                    "setup score Build coverage rejected: {error:?}"
                ))
            })?;
        if let Some(weights) = &canonical_weights {
            if weights != coverage.weights() {
                return Err(SetupScoreRunError::Rejected(
                    "setup score Build pattern weights differ between candidates".to_owned(),
                ));
            }
        } else {
            canonical_weights = Some(coverage.weights().clone());
        }
        if let Some(identity) = &pattern_universe_identity {
            if identity != coverage.pattern_universe_identity() {
                return Err(SetupScoreRunError::Rejected(
                    "setup score Build pattern universe differs between candidates".to_owned(),
                ));
            }
        } else {
            pattern_universe_identity = Some(coverage.pattern_universe_identity().to_owned());
        }

        let authority = match PcScoreCompiledAuthority::compile_scenario(
            candidate.continuation,
            PcScoreIngressOrigin::CanonicalPcScore,
        ) {
            Ok(authority) => authority,
            Err(PcScoreCompiledAuthorityError::ResourceAdmission(report)) => {
                return Err(SetupScoreRunError::Core(
                    clearra_core_executor::CoreExecutionError::resource_incomplete(
                        "execution-admission",
                        0,
                        *report,
                    ),
                ));
            }
            Err(PcScoreCompiledAuthorityError::ProblemCompile(error)) => {
                return Err(SetupScoreRunError::Rejected(format!(
                    "setup score continuation compile failed: {error:?}"
                )));
            }
            Err(PcScoreCompiledAuthorityError::Contract(error)) => {
                return Err(SetupScoreRunError::Rejected(format!(
                    "setup score continuation contract rejected: {error}"
                )));
            }
        };
        let (_, score_evidence) = context
            .services()
            .core_executor()
            .execute_pc_score_with_control(
                &authority,
                context.pc_score_external_retained_context_bytes(),
                context.execution_control(),
            )
            .map_err(SetupScoreRunError::Core)?;
        let score = score_evidence.report();
        reduced.push(ReducedCandidate {
            candidate_id: candidate.candidate_id,
            completed_board_mask: candidate.completed_board_mask,
            setup_covered_pattern_count: coverage.row().count_ones() as usize,
            setup_covered_probability: coverage.covered_probability().to_owned(),
            setup_row: coverage.row().clone(),
            continuation_probability: score.covered_probability().to_owned(),
            expected_score_bits: score.unconditional_expected_score_bits(),
            expected_score: score.unconditional_expected_score().to_owned(),
        });
    }
    let weights = canonical_weights.ok_or_else(|| {
        SetupScoreRunError::Rejected("setup score candidate set is empty".to_owned())
    })?;
    let average_priority_score = (0..weights.len()).try_fold(0.0_f64, |total, index| {
        let pattern = PatternId::new(index);
        let best = reduced
            .iter()
            .filter(|candidate| candidate.setup_row.contains(pattern))
            .map(|candidate| f64::from_bits(candidate.expected_score_bits))
            .reduce(f64::max)
            .unwrap_or(0.0);
        let weight = weights.weight(pattern).ok_or_else(|| {
            SetupScoreRunError::Rejected("setup score pattern weight is missing".to_owned())
        })?;
        Ok::<_, SetupScoreRunError>(total + weight.get() * best)
    })?;
    if !average_priority_score.is_finite() || average_priority_score < 0.0 {
        return Err(SetupScoreRunError::Rejected(
            "setup score reduction produced an invalid average".to_owned(),
        ));
    }
    reduced.sort_by(|left, right| {
        f64::from_bits(right.expected_score_bits)
            .total_cmp(&f64::from_bits(left.expected_score_bits))
            .then_with(|| left.candidate_id.cmp(&right.candidate_id))
    });
    let evaluation_identity_sha256 = evaluation_identity(
        &command,
        pattern_universe_identity.as_deref().ok_or_else(|| {
            SetupScoreRunError::Rejected(
                "setup score pattern universe identity is missing".to_owned(),
            )
        })?,
        &reduced,
    );
    let candidates = reduced
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            SetupScoreCandidatePayload::try_new(
                (index + 1).to_string(),
                candidate.candidate_id.as_str(),
                format!("0x{:016x}", candidate.completed_board_mask),
                candidate.setup_covered_pattern_count.to_string(),
                candidate.setup_covered_probability.as_str(),
                candidate.continuation_probability.as_str(),
                candidate.expected_score.as_str(),
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            SetupScoreRunError::Rejected(format!("setup score Host candidate rejected: {error:?}"))
        })?;
    let payload = SetupScoreRankingPayload::try_new(
        SETUP_SCORE_RESULT_CONTRACT,
        command.document_hash.as_str(),
        evaluation_identity_sha256,
        command.document_format.as_str(),
        command.rule.id().as_str(),
        command.score_profile.as_str(),
        command.initial_b2b.to_string(),
        "unconditional-expected-score-descending-then-canonical-candidate-id",
        command.source_page_count.to_string(),
        candidates.len().to_string(),
        weights.len().to_string(),
        average_priority_score.to_string(),
        true,
        candidates,
    )
    .map_err(|error| {
        SetupScoreRunError::Rejected(format!("setup score Host payload rejected: {error:?}"))
    })?;
    Ok(ProductResultPayload::new(
        "setup.score",
        SETUP_SCORE_RESULT_CONTRACT,
        ProductResultPayloadContent::SetupScoreRanking(payload),
    ))
}

fn score_execution_policy(coverage_policy: &PcExecutionPolicy) -> PcExecutionPolicy {
    let policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_max_patterns(PC_SCORE_MAX_PATTERNS);

    #[cfg(target_family = "wasm")]
    {
        // Browser workers own separate WASM module instances. A Setup-score
        // continuation therefore remains a one-worker child; the host-side
        // coordinator owns any wider coverage policy.
        let _ = coverage_policy;
        policy.with_workers(1)
    }

    #[cfg(not(target_family = "wasm"))]
    {
        let use_all = coverage_policy.use_all_logical_processors();
        let effective_workers = coverage_policy.workers();
        // `with_worker_policy` deliberately clears an Auto ceiling. Rebuild a
        // hardware ceiling which preserves the already-clamped effective
        // width while retaining Auto versus Fixed and the n-1/use-all rule.
        let effective_hardware_limit = if use_all {
            effective_workers
        } else {
            effective_workers.saturating_add(1)
        };
        policy
            .with_worker_policy(coverage_policy.worker_policy())
            .with_worker_hardware_limit(effective_hardware_limit)
            .with_use_all_logical_processors(use_all)
            .with_cpu_warmup(coverage_policy.cpu_warmup())
    }
}

fn candidate_document_hash(document_hash: &str, candidate_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"clearra.setup-score-candidate-document.v1\0");
    hash_text(&mut hasher, document_hash);
    hash_text(&mut hasher, candidate_id);
    format!("{:x}", hasher.finalize())
}

fn evaluation_identity(
    command: &SetupScoreAppCommand,
    pattern_universe_identity: &str,
    candidates: &[ReducedCandidate],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"clearra.setup-score-ranking.v1\0");
    for value in [
        command.document_hash.as_str(),
        pattern_universe_identity,
        command.document_format.as_str(),
        command.rule.id().as_str(),
        command.score_profile.as_str(),
    ] {
        hash_text(&mut hasher, value);
    }
    hasher.update(command.initial_b2b.to_be_bytes());
    hasher.update((candidates.len() as u128).to_be_bytes());
    for candidate in candidates {
        hash_text(&mut hasher, candidate.candidate_id.as_str());
        hasher.update(candidate.completed_board_mask.to_be_bytes());
        hasher.update(candidate.expected_score_bits.to_be_bytes());
        hasher.update((candidate.setup_covered_pattern_count as u128).to_be_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn hash_text(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u128).to_be_bytes());
    hasher.update(value.as_bytes());
}

#[cfg(test)]
mod tests {
    use clearra_ctk3::{encode_ctk3, Ctk3Color, Ctk3Document, Ctk3Page, Ctk3Piece};
    #[cfg(target_family = "wasm")]
    use clearra_pc_graph::request::WorkerPolicy;
    use clearra_rules::profile::rule_profile::RuleProfileId;
    use clearra_supply::queue::queue_parser;

    use super::*;

    fn setup_score_document() -> SetupScoreDocumentV1 {
        let mut cells = vec![Ctk3Color::Empty; 20];
        cells[0..4].fill(Ctk3Color::Piece(Ctk3Piece::I));
        let source = encode_ctk3(&Ctk3Document::new(10, vec![Ctk3Page::new(2, cells)]))
            .expect("Setup-score CTK3 fixture");
        SetupScoreDocumentV1::decode(crate::FieldDocumentFormat::Ctk3, &source)
            .expect("canonical Setup-score document")
    }

    fn setup_score_command(policy: PcExecutionPolicy) -> SetupScoreAppCommand {
        SetupScoreAppCommand::new(
            setup_score_document(),
            PcQueueInput::fixed_sequence(
                queue_parser::parse_fixed_sequence("I").expect("Setup queue"),
            ),
            None,
            PcQueueInput::fixed_sequence(
                queue_parser::parse_fixed_sequence("OTSJ").expect("continuation queue"),
            ),
            None,
            2,
            false,
            ScoreProfileSelection::Tetrio,
            0,
            RuleProfile::new(RuleProfileId::SrsPlus),
            policy,
        )
        .expect("valid Setup-score command")
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    fn setup_score_continuation_inherits_native_cpu_parallel_controls_only() {
        let baseline = PcExecutionPolicy::mvp_default();
        let policies = [
            baseline
                .clone()
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_workers(3)
                .with_worker_hardware_limit(4)
                .with_cpu_warmup(true)
                .with_allow_backend_fallback(false),
            baseline
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_automatic_worker_limit(2)
                .with_worker_hardware_limit(8)
                .with_use_all_logical_processors(true)
                .with_allow_backend_fallback(false),
        ];

        for coverage_policy in policies {
            let expected_worker_policy = coverage_policy.worker_policy();
            let expected_workers = coverage_policy.workers();
            let expected_use_all = coverage_policy.use_all_logical_processors();
            let expected_cpu_warmup = coverage_policy.cpu_warmup();
            let command = setup_score_command(coverage_policy);
            let continuation = command
                .candidates
                .first()
                .expect("Setup-score candidate")
                .continuation
                .execution_policy();

            assert_eq!(continuation.worker_policy(), expected_worker_policy);
            assert_eq!(continuation.workers(), expected_workers);
            assert_eq!(continuation.use_all_logical_processors(), expected_use_all);
            assert_eq!(continuation.cpu_warmup(), expected_cpu_warmup);
            assert_eq!(
                continuation.requested_backend(),
                RequestedSearchBackend::Cpu
            );
            assert!(!continuation.allow_backend_fallback());
            assert!(!continuation.gpu_warmup());
            assert!(!continuation.tablebase_requested());
            assert!(!continuation.precompute_build_dependencies());
            assert_eq!(continuation.max_memory_mib(), None);
            assert_eq!(continuation.max_patterns(), PC_SCORE_MAX_PATTERNS);
        }
    }

    #[test]
    #[cfg(target_family = "wasm")]
    fn setup_score_continuation_keeps_one_worker_per_wasm_instance() {
        let command = setup_score_command(
            PcExecutionPolicy::mvp_default()
                .with_requested_backend(RequestedSearchBackend::Cpu)
                .with_workers(4)
                .with_worker_hardware_limit(4)
                .with_use_all_logical_processors(true)
                .with_allow_backend_fallback(false),
        );
        let continuation = command
            .candidates
            .first()
            .expect("Setup-score candidate")
            .continuation
            .execution_policy();
        assert_eq!(continuation.worker_policy(), WorkerPolicy::Fixed(1));
        assert_eq!(continuation.workers(), 1);
    }
}
