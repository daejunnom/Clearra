use clearra_app::{
    BuildObjective, BuildProbabilityResultMode, BuildQueueKnowledge, BuildScoreProfile,
    FieldDocumentFormat, FieldDocumentTransformAppCommand, FieldDocumentTransformKind,
    FumenAppCommand, FumenTransformKind, ParityAppCommand, PcChanceIngressOrigin,
    PcFailedQueueIngressOrigin, PcMinimalsIngressOrigin, PcPathIngressOrigin, PcSaveIngressOrigin,
    PcScoreIngressOrigin, PcScoreMinimalsIngressOrigin, PcTilingIngressOrigin, RenderAppCommand,
    RenderArtifactFormat, RequestStructuralProfiles, SpinStructureProductMode,
    PC_SCORE_MAX_PATTERNS, PC_SCORE_MAX_PATTERN_BYTES, PC_SCORE_MAX_SOURCE_PIECES,
};
use clearra_core_domain::board::standard_pc_board::Board256Mask;
use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_forward_search::{
    ForwardLineClearPolicy, ForwardPieceSource, ForwardSearchMode, ForwardSearchQuery,
    ForwardSpinCategory, ForwardSpinLineRequirement, ForwardSpinTarget,
};
use clearra_fumen::SourceFumenColoredFieldSet;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_objectives::policy::score_objective_policy::{
    ScoreProfileSelection, SpinProfileSelection,
};
use clearra_pc_graph::request::{
    validate_pc_observation_objective, GpuDeviceSelection, PcCountPolicy, PcExecutionPolicy,
    PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow, RequestedSearchBackend,
    SupplyWindowSize, WorkerPolicy,
};
use clearra_problem::{
    BuildProbabilityAggregation, FinesseMetric, FinessePatternKnowledge, FinessePlacement,
    FinesseScoreRequest,
};
use clearra_rules::profile::{
    builtin_rules::srs_plus,
    rule_profile::{RuleProfile, RuleProfileId},
};
use clearra_scoring::profile::SpinProfileId;
use clearra_spin_structure_search::{
    MinimalityPolicy, PieceInventory, SpinLineRequirement, SpinStructureMode, SpinStructureQuery,
    StructureBoard,
};
use clearra_supply::{
    queue::{
        queue_parser::{parse_bag_aligned_pattern, parse_fixed_sequence, parse_observed_queue},
        queue_pattern_expression::QueuePatternExpression,
    },
    QueueObservationPolicy,
};

use crate::{
    ctk3_mask_input::parse_ctk3_board_mask, web_virtual_file::reject_native_path_semantics,
    WebBuildProbabilityInput, WebBuildV2Capability, WebBuildV2Input, WebCommandError,
    WebCommandErrorCode, WebCommandRequest, WebPcScenarioInput, WebSetupScoreInput,
    WebSetupScoreQueueInput, WebVirtualFileHandle,
};

#[derive(Clone, Debug, Default)]
pub struct WebCommandParser;

pub(crate) const PC_SCORE_MAX_ARGUMENT_TOKENS: usize = 64;
pub(crate) const PC_SCORE_MAX_ARGUMENT_BYTES: usize = 2_048;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum WebCompatibilityAuthority {
    /// Preserve the legacy public semantics without minting a typed product claim.
    #[default]
    PublicLegacyCompatibility,
    /// Preserve closed candidate provenance while the typed product remains internal.
    InternalTypedCandidate,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct BackendFallbackOverride {
    value: Option<bool>,
}

impl BackendFallbackOverride {
    fn record(&mut self, value: bool) -> Result<(), WebCommandError> {
        if self.value.is_some() {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                "backend fallback policy may be specified only once",
            ));
        }
        self.value = Some(value);
        Ok(())
    }

    fn resolve(self, backend: RequestedSearchBackend) -> bool {
        self.value
            .unwrap_or(matches!(backend, RequestedSearchBackend::Auto))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BuildAggregationKind {
    Buildability,
    Tiling,
    Spin,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SpinStructureRoute {
    Search,
    Cover,
    Guaranteed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct BuildAggregationOverride {
    value: Option<BuildAggregationKind>,
}

impl BuildAggregationOverride {
    fn record(&mut self, value: BuildAggregationKind) -> Result<(), WebCommandError> {
        if self.value.is_some() {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                "build-probability aggregation may be specified only once",
            ));
        }
        self.value = Some(value);
        Ok(())
    }

    fn resolve(self) -> BuildAggregationKind {
        self.value.unwrap_or(BuildAggregationKind::Buildability)
    }
}

impl WebCommandParser {
    pub fn parse(command_text: &str) -> Result<WebCommandRequest, WebCommandError> {
        Self::parse_with_worker_limit(command_text, WorkerPolicy::hardware_worker_limit())
    }

    pub fn parse_with_worker_limit(
        command_text: &str,
        worker_hardware_limit: usize,
    ) -> Result<WebCommandRequest, WebCommandError> {
        validate_raw_pc_score_command_text(
            command_text,
            WebCompatibilityAuthority::PublicLegacyCompatibility,
        )?;
        reject_process_semantics(command_text)?;
        let tokens = tokenize(command_text)?;
        Self::parse_tokens_with_worker_limit_and_authority(
            &tokens,
            worker_hardware_limit,
            WebCompatibilityAuthority::PublicLegacyCompatibility,
        )
    }

    pub fn parse_internal_typed_candidate(
        command_text: &str,
    ) -> Result<WebCommandRequest, WebCommandError> {
        Self::parse_internal_typed_candidate_with_worker_limit(
            command_text,
            WorkerPolicy::hardware_worker_limit(),
        )
    }

    pub fn parse_internal_typed_candidate_with_worker_limit(
        command_text: &str,
        worker_hardware_limit: usize,
    ) -> Result<WebCommandRequest, WebCommandError> {
        validate_raw_pc_score_command_text(
            command_text,
            WebCompatibilityAuthority::InternalTypedCandidate,
        )?;
        reject_process_semantics(command_text)?;
        let tokens = tokenize(command_text)?;
        Self::parse_tokens_with_worker_limit_and_authority(
            &tokens,
            worker_hardware_limit,
            WebCompatibilityAuthority::InternalTypedCandidate,
        )
    }

    pub fn parse_tokens(tokens: &[String]) -> Result<WebCommandRequest, WebCommandError> {
        Self::parse_tokens_with_worker_limit(tokens, WorkerPolicy::hardware_worker_limit())
    }

    pub fn parse_tokens_with_worker_limit(
        tokens: &[String],
        worker_hardware_limit: usize,
    ) -> Result<WebCommandRequest, WebCommandError> {
        Self::parse_tokens_with_worker_limit_and_authority(
            tokens,
            worker_hardware_limit,
            WebCompatibilityAuthority::PublicLegacyCompatibility,
        )
    }

    pub fn parse_tokens_internal_typed_candidate(
        tokens: &[String],
    ) -> Result<WebCommandRequest, WebCommandError> {
        Self::parse_tokens_internal_typed_candidate_with_worker_limit(
            tokens,
            WorkerPolicy::hardware_worker_limit(),
        )
    }

    pub fn parse_tokens_internal_typed_candidate_with_worker_limit(
        tokens: &[String],
        worker_hardware_limit: usize,
    ) -> Result<WebCommandRequest, WebCommandError> {
        Self::parse_tokens_with_worker_limit_and_authority(
            tokens,
            worker_hardware_limit,
            WebCompatibilityAuthority::InternalTypedCandidate,
        )
    }

    fn parse_tokens_with_worker_limit_and_authority(
        tokens: &[String],
        worker_hardware_limit: usize,
        compatibility_authority: WebCompatibilityAuthority,
    ) -> Result<WebCommandRequest, WebCommandError> {
        reject_nul_tokens(tokens)?;
        validate_pretranslation_pc_score_tokens(tokens, compatibility_authority)?;
        let translated =
            crate::sfinder_compat::translate_command_with_origin(tokens, compatibility_authority)?;
        let pc_chance_origin = translated.pc_chance_origin();
        let pc_score_origin = translated.pc_score_origin();
        let pc_save_origin = translated.pc_save_origin();
        let (tokens, request_structural_profiles) =
            extract_request_structural_profile_options(translated.tokens())?;
        let tokens = tokens.as_slice();
        let mut cursor = 0usize;

        if tokens.get(cursor).map(String::as_str) == Some("clearra") {
            cursor += 1;
        }

        let command = tokens.get(cursor).ok_or_else(|| {
            WebCommandError::new(WebCommandErrorCode::EmptyCommand, "empty CLI command")
        })?;
        cursor += 1;
        let request = match command.as_str() {
            "pc" => match tokens.get(cursor).map(String::as_str) {
                Some("allspin-sol") => parse_pc_allspin_command(
                    &tokens[cursor + 1..],
                    worker_hardware_limit.max(1),
                    PcAllSpinCommandKind::ExactQueue,
                ),
                Some("allspin-pres-chance") => parse_pc_allspin_command(
                    &tokens[cursor + 1..],
                    worker_hardware_limit.max(1),
                    PcAllSpinCommandKind::PatternChance,
                ),
                Some("chance") => parse_pc_chance_command(
                    &tokens[cursor + 1..],
                    worker_hardware_limit.max(1),
                    pc_chance_origin.unwrap_or(PcChanceIngressOrigin::CanonicalPcChance),
                ),
                Some("minimals") => {
                    parse_pc_minimals_command(&tokens[cursor + 1..], worker_hardware_limit.max(1))
                }
                Some("path") => {
                    parse_pc_path_command(&tokens[cursor + 1..], worker_hardware_limit.max(1))
                }
                Some("score") => parse_pc_score_command(
                    &tokens[cursor + 1..],
                    worker_hardware_limit.max(1),
                    pc_score_origin.unwrap_or(PcScoreIngressOrigin::CanonicalPcScore),
                ),
                Some("score-finder") => parse_pc_score_command(
                    &tokens[cursor + 1..],
                    worker_hardware_limit.max(1),
                    PcScoreIngressOrigin::CanonicalPcScoreFinder,
                ),
                Some("score-minimals") => parse_pc_score_minimals_command(
                    &tokens[cursor + 1..],
                    worker_hardware_limit.max(1),
                ),
                Some("saves") => parse_pc_save_command(
                    &tokens[cursor + 1..],
                    worker_hardware_limit.max(1),
                    pc_save_origin.unwrap_or(PcSaveIngressOrigin::CanonicalPcSaves),
                ),
                Some("best-save") => parse_pc_save_command(
                    &tokens[cursor + 1..],
                    worker_hardware_limit.max(1),
                    pc_save_origin.unwrap_or(PcSaveIngressOrigin::CanonicalPcBestSave),
                ),
                Some("tiling") => {
                    parse_pc_tiling_command(&tokens[cursor + 1..], worker_hardware_limit.max(1))
                }
                Some("failed-queue") => parse_pc_failed_queue_command(
                    &tokens[cursor + 1..],
                    worker_hardware_limit.max(1),
                    PcFailedQueueIngressOrigin::CanonicalFailedQueue,
                ),
                Some("failed_queue") => Err(WebCommandError::new(
                    WebCommandErrorCode::UnsupportedCommand,
                    "pc failed_queue is not an authorized canonical spelling",
                )),
                _ => parse_pc_command(&tokens[cursor..], worker_hardware_limit.max(1), false),
            },
            "failed-queue" => {
                parse_pc_command(&tokens[cursor..], worker_hardware_limit.max(1), true)
            }
            "failed_queue"
                if compatibility_authority == WebCompatibilityAuthority::InternalTypedCandidate =>
            {
                parse_pc_failed_queue_command(
                    &tokens[cursor..],
                    worker_hardware_limit.max(1),
                    PcFailedQueueIngressOrigin::CompatibilityFailedQueueUnderscore,
                )
            }
            "failed_queue" => {
                parse_pc_command(&tokens[cursor..], worker_hardware_limit.max(1), true)
            }
            "percent" => parse_percent_command(&tokens[cursor..], worker_hardware_limit.max(1)),
            "build-probability" => {
                parse_build_probability_command(&tokens[cursor..], worker_hardware_limit.max(1))
            }
            "build" => parse_build_v2_command(&tokens[cursor..], worker_hardware_limit.max(1)),
            "finesse" => parse_finesse_command(&tokens[cursor..], worker_hardware_limit.max(1)),
            "setup-finder" => {
                parse_setup_command(&tokens[cursor..], worker_hardware_limit.max(1), None)
            }
            "setup" => match tokens.get(cursor).map(String::as_str) {
                Some("joint") => parse_setup_command(
                    &tokens[cursor + 1..],
                    worker_hardware_limit.max(1),
                    Some(clearra_problem::SetupCandidatePriority::All),
                ),
                Some("build") => parse_setup_command(
                    &tokens[cursor + 1..],
                    worker_hardware_limit.max(1),
                    Some(clearra_problem::SetupCandidatePriority::BuildProbabilityFirst),
                ),
                Some("pc") => parse_setup_command(
                    &tokens[cursor + 1..],
                    worker_hardware_limit.max(1),
                    Some(clearra_problem::SetupCandidatePriority::PcProbabilityFirst),
                ),
                Some("score") => {
                    parse_setup_score_command(&tokens[cursor + 1..], worker_hardware_limit.max(1))
                }
                _ => parse_setup_command(&tokens[cursor..], worker_hardware_limit.max(1), None),
            },
            "damage" => parse_forward_command(
                &tokens[cursor..],
                false,
                false,
                worker_hardware_limit.max(1),
            ),
            "spin-finder" => {
                parse_forward_command(&tokens[cursor..], true, false, worker_hardware_limit.max(1))
            }
            "ren" => {
                parse_forward_command(&tokens[cursor..], false, true, worker_hardware_limit.max(1))
            }
            "spin-structure" => match tokens.get(cursor).map(String::as_str) {
                Some("search") => parse_spin_structure_command(
                    &tokens[cursor + 1..],
                    worker_hardware_limit.max(1),
                    SpinStructureRoute::Search,
                ),
                Some("cover") => parse_spin_structure_command(
                    &tokens[cursor + 1..],
                    worker_hardware_limit.max(1),
                    SpinStructureRoute::Cover,
                ),
                Some("guaranteed") => parse_spin_structure_command(
                    &tokens[cursor + 1..],
                    worker_hardware_limit.max(1),
                    SpinStructureRoute::Guaranteed,
                ),
                _ => parse_spin_structure_command(
                    &tokens[cursor..],
                    worker_hardware_limit.max(1),
                    SpinStructureRoute::Search,
                ),
            },
            "verify" => parse_verify_command(&tokens[cursor..]),
            "utility" => parse_utility_command(&tokens[cursor..]),
            _ => Err(WebCommandError::new(
                WebCommandErrorCode::UnsupportedCommand,
                format!("unsupported CLI command '{command}'"),
            )),
        }?;
        Ok(request.with_request_structural_profiles(request_structural_profiles))
    }
}

fn extract_request_structural_profile_options(
    tokens: &[String],
) -> Result<(Vec<String>, RequestStructuralProfiles), WebCommandError> {
    let mut forwarded = Vec::with_capacity(tokens.len());
    let mut board = None::<String>;
    let mut piece = None::<String>;
    let mut bag = None::<String>;
    let mut cursor = 0usize;
    while cursor < tokens.len() {
        let option = tokens[cursor].as_str();
        let slot = match option {
            "--board-profile" => Some((&mut board, "board")),
            "--piece-profile" => Some((&mut piece, "piece")),
            "--bag-profile" => Some((&mut bag, "bag")),
            _ => None,
        };
        if let Some((slot, kind)) = slot {
            if slot.is_some() {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("request {kind} profile may be specified only once"),
                ));
            }
            let value = tokens.get(cursor + 1).ok_or_else(|| {
                WebCommandError::new(
                    WebCommandErrorCode::MissingValue,
                    format!("{option} requires a canonical profile id"),
                )
            })?;
            *slot = Some(value.clone());
            cursor += 2;
            continue;
        }
        forwarded.push(tokens[cursor].clone());
        cursor += 1;
    }
    let board = board.as_deref().unwrap_or("standard-10");
    let piece = piece.as_deref().unwrap_or("standard-tetrominoes");
    let bag = bag.as_deref().unwrap_or("standard-7-bag");
    let profiles =
        RequestStructuralProfiles::parse_canonical(board, piece, bag).map_err(|error| {
            WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!("invalid request profile selection: {error}"),
            )
        })?;
    Ok((forwarded, profiles))
}

fn parse_pc_save_command(
    tokens: &[String],
    worker_hardware_limit: usize,
    origin: PcSaveIngressOrigin,
) -> Result<WebCommandRequest, WebCommandError> {
    for token in tokens {
        if matches!(
            token.as_str(),
            "--queue"
                | "--objective"
                | "--count"
                | "--score"
                | "--tiling-only"
                | "--solution-probabilities"
                | "--queue-knowledge"
                | "--max-memory-mib"
                | "--visible-seven"
                | "--tablebase"
                | "--tb"
                | "--precompute-build-dependencies"
                | "--build-dependency-dag"
        ) {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!("pc saves does not accept an explicit {token} override"),
            ));
        }
    }
    let mut forwarded = Vec::with_capacity(tokens.len() + 2);
    forwarded.extend_from_slice(tokens);
    forwarded.extend(["--objective".to_owned(), "all".to_owned()]);
    parse_pc_command(&forwarded, worker_hardware_limit, false).map(|request| {
        request
            .with_count_policy(PcCountPolicy::CountAll)
            .with_pc_save_product_capability(origin)
    })
}

fn parse_pc_tiling_command(
    tokens: &[String],
    worker_hardware_limit: usize,
) -> Result<WebCommandRequest, WebCommandError> {
    validate_pc_tiling_options(tokens)?;
    // The canonical subcommand is the sole authority. Objective lowering is
    // applied after generic field parsing so legacy `--tiling-only` and
    // `--objective tiling` requests cannot acquire this product contract.
    parse_pc_command(tokens, worker_hardware_limit, false).map(|request| {
        request
            .with_count_policy(PcCountPolicy::CountUnique)
            .with_objective(ObjectivePolicy::tiling())
            .with_pc_tiling_product_capability(PcTilingIngressOrigin::CanonicalPcTiling)
    })
}

fn validate_pc_tiling_options(tokens: &[String]) -> Result<(), WebCommandError> {
    let mut seen = Vec::<&'static str>::new();
    let mut cursor = 0usize;
    while cursor < tokens.len() {
        let option = tokens[cursor].as_str();
        let (slot, takes_value) = match option {
            "--lines" => ("lines", true),
            "--queue" => ("queue", true),
            "--patterns" => ("patterns", true),
            "--board-mask" => ("board-mask", true),
            "--height" => ("height", true),
            "--pieces" => ("pieces", true),
            "--hold" => ("hold", true),
            "--no-hold" => ("hold", false),
            "--source-pieces" => ("source-pieces", true),
            "--backend" => ("backend", true),
            "--gpu-device" => ("gpu-device", true),
            "--workers" => ("workers", true),
            "--auto-workers" => ("auto-workers", true),
            "--use-all-cpu-threads" => ("use-all-cpu-threads", false),
            "--cpu-warmup" => ("cpu-warmup", false),
            "--gpu-warmup" => ("gpu-warmup", false),
            "--max-patterns" => ("max-patterns", true),
            "--max-nodes" => ("max-nodes", true),
            "--max-frontier-states" => ("max-frontier-states", true),
            "--max-candidates" => ("max-candidates", true),
            "--max-memory-mib" => ("max-memory-mib", true),
            "--allow-backend-fallback" => ("backend-fallback", false),
            "--no-backend-fallback" => ("backend-fallback", false),
            flag if flag.starts_with("--") => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("pc tiling does not accept {flag}"),
                ));
            }
            value => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("unexpected pc tiling token '{value}'"),
                ));
            }
        };
        if seen.contains(&slot) {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!("pc tiling option {option} duplicates the {slot} selection"),
            ));
        }
        seen.push(slot);
        if takes_value {
            let _ = next_value(tokens, &mut cursor, option)?;
        } else {
            cursor += 1;
        }
    }
    if seen.contains(&"workers") && seen.contains(&"auto-workers") {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "pc tiling --workers and --auto-workers are mutually exclusive",
        ));
    }
    Ok(())
}

fn parse_pc_chance_command(
    tokens: &[String],
    worker_hardware_limit: usize,
    origin: PcChanceIngressOrigin,
) -> Result<WebCommandRequest, WebCommandError> {
    parse_pc_command(tokens, worker_hardware_limit, false)
        .map(|request| request.with_pc_chance_product_capability(origin))
}

fn parse_pc_minimals_command(
    tokens: &[String],
    worker_hardware_limit: usize,
) -> Result<WebCommandRequest, WebCommandError> {
    for token in tokens {
        if matches!(
            token.as_str(),
            "--objective"
                | "--count"
                | "--score"
                | "--tiling-only"
                | "--max-memory-mib"
                | "--tablebase"
                | "--precompute-build-dependencies"
        ) {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!("pc minimals does not accept an explicit {token} override"),
            ));
        }
    }
    let mut forwarded = Vec::with_capacity(tokens.len() + 2);
    forwarded.extend_from_slice(tokens);
    forwarded.extend(["--objective".to_owned(), "minimum-cover".to_owned()]);
    parse_pc_command(&forwarded, worker_hardware_limit, false).map(|request| {
        request
            .with_count_policy(PcCountPolicy::CountUnique)
            .with_pc_minimals_product_capability(PcMinimalsIngressOrigin::CanonicalPcMinimals)
    })
}

fn parse_pc_path_command(
    tokens: &[String],
    worker_hardware_limit: usize,
) -> Result<WebCommandRequest, WebCommandError> {
    for token in tokens {
        if matches!(
            token.as_str(),
            "--objective"
                | "--count"
                | "--score"
                | "--tiling-only"
                | "--solution-probabilities"
                | "--queue-knowledge"
                | "--max-memory-mib"
                | "--tablebase"
                | "--tb"
                | "--precompute-build-dependencies"
                | "--build-dependency-dag"
        ) {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!("pc path does not accept an explicit {token} override"),
            ));
        }
    }
    let mut forwarded = Vec::with_capacity(tokens.len() + 2);
    forwarded.extend_from_slice(tokens);
    forwarded.extend(["--objective".to_owned(), "all".to_owned()]);
    parse_pc_command(&forwarded, worker_hardware_limit, false).map(|request| {
        request
            .with_count_policy(PcCountPolicy::CountAll)
            .with_pc_path_product_capability(PcPathIngressOrigin::CanonicalPcPath)
    })
}

fn parse_pc_failed_queue_command(
    tokens: &[String],
    worker_hardware_limit: usize,
    origin: PcFailedQueueIngressOrigin,
) -> Result<WebCommandRequest, WebCommandError> {
    parse_pc_command(tokens, worker_hardware_limit, true)
        .map(|request| request.with_pc_failed_queue_product_capability(origin))
}

fn parse_pc_score_command(
    tokens: &[String],
    worker_hardware_limit: usize,
    origin: PcScoreIngressOrigin,
) -> Result<WebCommandRequest, WebCommandError> {
    validate_pc_score_arguments(tokens)?;
    validate_pc_score_options(tokens)?;
    if origin.is_score_finder() {
        validate_pc_score_finder_options(tokens)?;
    }
    let mut forwarded = Vec::with_capacity(tokens.len() + 10);
    forwarded.extend_from_slice(tokens);
    forwarded.extend([
        "--objective".to_owned(),
        "all".to_owned(),
        "--score".to_owned(),
        "--backend".to_owned(),
        "cpu".to_owned(),
        "--no-backend-fallback".to_owned(),
        "--max-patterns".to_owned(),
        PC_SCORE_MAX_PATTERNS.to_string(),
    ]);
    if matches!(
        origin,
        PcScoreIngressOrigin::CompatibilityScore | PcScoreIngressOrigin::CanonicalPcScoreFinder
    ) {
        forwarded.extend(["--score-profile".to_owned(), "jstris-ultra".to_owned()]);
    }
    if origin.is_score_finder() {
        forwarded.extend(["--spin-profile".to_owned(), "t-spins".to_owned()]);
    }
    parse_pc_command(&forwarded, worker_hardware_limit, false)
        .map(|request| request.with_pc_score_product_capability(origin))
}

fn validate_pc_score_finder_options(tokens: &[String]) -> Result<(), WebCommandError> {
    let mut fixed_queue = false;
    let mut cursor = 0usize;
    while cursor < tokens.len() {
        match tokens[cursor].as_str() {
            "--patterns" | "--pattern" | "-p" => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    "pc score-finder requires one exact fixed queue",
                ));
            }
            "--score-profile" | "--spin-profile" => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    "pc score-finder owns its fixed jstris-ultra and t-spins profiles",
                ));
            }
            "--queue" => {
                if fixed_queue {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "pc score-finder queue may be specified only once",
                    ));
                }
                fixed_queue = true;
                let _ = next_value(tokens, &mut cursor, "--queue")?;
            }
            _ => cursor += 1,
        }
    }
    if !fixed_queue {
        return Err(WebCommandError::new(
            WebCommandErrorCode::MissingValue,
            "pc score-finder requires one exact fixed queue",
        ));
    }
    Ok(())
}

fn parse_pc_score_minimals_command(
    tokens: &[String],
    worker_hardware_limit: usize,
) -> Result<WebCommandRequest, WebCommandError> {
    validate_pc_score_arguments(tokens)?;
    validate_pc_score_options(tokens)?;
    let mut forwarded = Vec::with_capacity(tokens.len() + 10);
    forwarded.extend_from_slice(tokens);
    forwarded.extend([
        "--objective".to_owned(),
        "minimum-cover".to_owned(),
        "--score".to_owned(),
        "--backend".to_owned(),
        "cpu".to_owned(),
        "--no-backend-fallback".to_owned(),
        "--max-patterns".to_owned(),
        PC_SCORE_MAX_PATTERNS.to_string(),
    ]);
    parse_pc_command(&forwarded, worker_hardware_limit, false).map(|request| {
        request
            .with_count_policy(PcCountPolicy::CountAll)
            .with_pc_score_minimals_product_capability(
                PcScoreMinimalsIngressOrigin::CanonicalPcScoreMinimals,
            )
    })
}

fn validate_pc_score_options(tokens: &[String]) -> Result<(), WebCommandError> {
    let mut score_profile = false;
    let mut spin_profile = false;
    let mut initial_b2b = false;
    let mut cursor = 0usize;
    while cursor < tokens.len() {
        let option = tokens[cursor].as_str();
        match option {
            "--objective"
            | "--count"
            | "--score"
            | "--tiling-only"
            | "--preserve-b2b"
            | "--solution-probabilities" => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("pc score does not accept an explicit {option} override"),
                ));
            }
            "--backend"
            | "--gpu-device"
            | "--gpu-warmup"
            | "--allow-backend-fallback"
            | "--no-backend-fallback"
            | "--tablebase"
            | "--tb"
            | "--no-tablebase"
            | "--no-tb"
            | "--build-dependency-dag"
            | "--no-build-dependency-dag"
            | "--retained-traces"
            | "--max-patterns"
            | "--max-nodes"
            | "--max-frontier-states"
            | "--max-candidates"
            | "--max-memory-mib" => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("pc score does not accept an explicit {option} execution override"),
                ));
            }
            "--workers" | "--auto-workers" => {
                let _ = next_value(tokens, &mut cursor, option)?;
            }
            "--use-all-cpu-threads" | "--cpu-warmup" => cursor += 1,
            "--score-profile" | "--spin-profile" | "--initial-b2b" => {
                let seen = match option {
                    "--score-profile" => &mut score_profile,
                    "--spin-profile" => &mut spin_profile,
                    "--initial-b2b" => &mut initial_b2b,
                    _ => unreachable!(),
                };
                if *seen {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("pc score {option} may be specified only once"),
                    ));
                }
                *seen = true;
                let _ = next_value(tokens, &mut cursor, option)?;
            }
            _ => cursor += 1,
        }
    }
    Ok(())
}

fn validate_raw_pc_score_command_text(
    command_text: &str,
    compatibility_authority: WebCompatibilityAuthority,
) -> Result<(), WebCommandError> {
    let mut tokens = command_text.split_whitespace();
    let mut command = tokens.next();
    let mut prefix_bytes = 0usize;
    if command == Some("clearra") {
        prefix_bytes = "clearra ".len();
        command = tokens.next();
    }
    let score_prefix_bytes = match command {
        Some("pc") => match tokens.next() {
            Some("score") => Some(prefix_bytes + "pc score".len()),
            Some("score-minimals") => Some(prefix_bytes + "pc score-minimals".len()),
            Some("score-finder") => Some(prefix_bytes + "pc score-finder".len()),
            _ => None,
        },
        Some(command)
            if compatibility_authority == WebCompatibilityAuthority::InternalTypedCandidate
                && command.eq_ignore_ascii_case("score") =>
        {
            Some(prefix_bytes + "score".len())
        }
        Some(command)
            if compatibility_authority == WebCompatibilityAuthority::InternalTypedCandidate
                && command.eq_ignore_ascii_case("sfinder")
                && tokens
                    .next()
                    .is_some_and(|command| command.eq_ignore_ascii_case("score")) =>
        {
            Some(prefix_bytes + "sfinder score".len())
        }
        _ => None,
    };
    let raw_limit = score_prefix_bytes.map(|prefix_bytes| {
        prefix_bytes + PC_SCORE_MAX_ARGUMENT_BYTES + PC_SCORE_MAX_ARGUMENT_TOKENS
    });
    if raw_limit.is_some_and(|limit| command_text.len() > limit) {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!(
                "pc score command text exceeds the {}-byte ingress limit",
                raw_limit.expect("score prefix produced a raw limit")
            ),
        ));
    }
    Ok(())
}

pub(crate) fn validate_pretranslation_pc_score_tokens(
    tokens: &[String],
    compatibility_authority: WebCompatibilityAuthority,
) -> Result<(), WebCommandError> {
    let mut command_index = usize::from(tokens.first().map(String::as_str) == Some("clearra"));
    let Some(command) = tokens.get(command_index).map(String::as_str) else {
        return Ok(());
    };
    command_index += 1;
    let argument_index = if command == "pc"
        && matches!(
            tokens.get(command_index).map(String::as_str),
            Some("score" | "score-minimals" | "score-finder")
        ) {
        Some(command_index + 1)
    } else if compatibility_authority == WebCompatibilityAuthority::InternalTypedCandidate
        && command.eq_ignore_ascii_case("score")
    {
        Some(command_index)
    } else if compatibility_authority == WebCompatibilityAuthority::InternalTypedCandidate
        && command.eq_ignore_ascii_case("sfinder")
        && tokens
            .get(command_index)
            .is_some_and(|command| command.eq_ignore_ascii_case("score"))
    {
        Some(command_index + 1)
    } else {
        None
    };
    argument_index.map_or(Ok(()), |index| {
        validate_pc_score_arguments(&tokens[index..])
    })
}

fn validate_pc_score_arguments(arguments: &[String]) -> Result<(), WebCommandError> {
    if arguments.len() > PC_SCORE_MAX_ARGUMENT_TOKENS {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!("pc score accepts at most {PC_SCORE_MAX_ARGUMENT_TOKENS} argument tokens"),
        ));
    }
    let argument_bytes = arguments
        .iter()
        .try_fold(0usize, |total, argument| total.checked_add(argument.len()));
    if argument_bytes.is_none_or(|bytes| bytes > PC_SCORE_MAX_ARGUMENT_BYTES) {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!("pc score accepts at most {PC_SCORE_MAX_ARGUMENT_BYTES} argument bytes"),
        ));
    }

    for (index, option) in arguments.iter().enumerate() {
        let Some(value) = arguments
            .get(index + 1)
            .filter(|value| !value.starts_with("--"))
        else {
            continue;
        };
        match option.as_str() {
            "--patterns" | "--pattern" => {
                if value.len() > PC_SCORE_MAX_PATTERN_BYTES {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!(
                            "pc score --patterns accepts at most {PC_SCORE_MAX_PATTERN_BYTES} UTF-8 bytes"
                        ),
                    ));
                }
                if value.contains(';') {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "pc score --patterns accepts one factorized expression without alternatives",
                    ));
                }
            }
            "--queue" => {
                if value.len() > PC_SCORE_MAX_SOURCE_PIECES {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!(
                            "pc score --queue accepts at most {PC_SCORE_MAX_SOURCE_PIECES} source pieces"
                        ),
                    ));
                }
            }
            "--source-pieces"
                if value
                    .parse::<usize>()
                    .is_ok_and(|pieces| pieces > PC_SCORE_MAX_SOURCE_PIECES) =>
            {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("pc score accepts at most {PC_SCORE_MAX_SOURCE_PIECES} source pieces"),
                ));
            }
            _ => {}
        }
    }
    Ok(())
}

fn parse_finesse_command(
    tokens: &[String],
    worker_hardware_limit: usize,
) -> Result<WebCommandRequest, WebCommandError> {
    let (mode, arguments) = tokens.split_first().ok_or_else(|| {
        WebCommandError::new(
            WebCommandErrorCode::MissingValue,
            "finesse requires search or score",
        )
    })?;
    match mode.as_str() {
        "search" => {
            let mut forwarded = arguments.to_vec();
            forwarded.extend([
                "--finesse".to_owned(),
                "inputs".to_owned(),
                "--no-mirror".to_owned(),
            ]);
            parse_build_probability_command(&forwarded, worker_hardware_limit)
        }
        "score" => {
            let mut forwarded = Vec::with_capacity(arguments.len() + 6);
            let mut placements = None;
            let mut saw_initial = false;
            let mut cursor = 0usize;
            while cursor < arguments.len() {
                match arguments[cursor].as_str() {
                    "--placements" => {
                        if placements.is_some() {
                            return Err(WebCommandError::new(
                                WebCommandErrorCode::InvalidValue,
                                "finesse score --placements may be specified only once",
                            ));
                        }
                        let value = next_value(arguments, &mut cursor, "--placements")?;
                        placements = Some(parse_finesse_placements(value)?);
                    }
                    "--initial-mask" => {
                        if saw_initial {
                            return Err(WebCommandError::new(
                                WebCommandErrorCode::InvalidValue,
                                "finesse score --initial-mask may be specified only once",
                            ));
                        }
                        saw_initial = true;
                        let value = next_value(arguments, &mut cursor, "--initial-mask")?;
                        forwarded.push("--base-mask".to_owned());
                        forwarded.push(value.to_owned());
                    }
                    "--base-mask" | "--target-mask" | "--finesse" => {
                        return Err(WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            format!("finesse score does not accept {}", arguments[cursor]),
                        ));
                    }
                    _ => {
                        forwarded.push(arguments[cursor].clone());
                        cursor += 1;
                    }
                }
            }
            if !saw_initial {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::MissingValue,
                    "finesse score requires --initial-mask",
                ));
            }
            let placements = placements.ok_or_else(|| {
                WebCommandError::new(
                    WebCommandErrorCode::MissingValue,
                    "finesse score requires --placements",
                )
            })?;
            forwarded.extend([
                "--target-mask".to_owned(),
                "0".to_owned(),
                "--finesse".to_owned(),
                "inputs".to_owned(),
                "--no-mirror".to_owned(),
            ]);
            parse_build_probability_command(&forwarded, worker_hardware_limit)
                .map(|request| request.with_finesse_score(placements))
        }
        value => Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!("invalid finesse mode '{value}'; expected search or score"),
        )),
    }
}

fn parse_finesse_placements(value: &str) -> Result<FinesseScoreRequest, WebCommandError> {
    let mut placements = Vec::new();
    // `|` remains reserved for rejected process-control syntax at every
    // boundary, so multi-placement score requests use an ordinary comma.
    for (index, placement) in value.split(',').enumerate() {
        if index >= FinesseScoreRequest::MAX_PLACEMENTS {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!(
                    "finesse score accepts at most {} placements",
                    FinesseScoreRequest::MAX_PLACEMENTS
                ),
            ));
        }
        let parts = placement.split(':').collect::<Vec<_>>();
        if parts.len() != 4 {
            return Err(invalid_finesse_placement(index));
        }
        let piece = match parts[0].to_ascii_uppercase().as_str() {
            "I" => PieceKind::I,
            "O" => PieceKind::O,
            "T" => PieceKind::T,
            "S" => PieceKind::S,
            "Z" => PieceKind::Z,
            "J" => PieceKind::J,
            "L" => PieceKind::L,
            _ => return Err(invalid_finesse_placement(index)),
        };
        let rotation = match parts[1].to_ascii_lowercase().as_str() {
            "spawn" | "north" | "0" => RotationState::Zero,
            "right" | "east" | "1" => RotationState::Right,
            "reverse" | "south" | "2" => RotationState::Two,
            "left" | "west" | "3" => RotationState::Left,
            _ => return Err(invalid_finesse_placement(index)),
        };
        let x = parts[2]
            .parse::<i16>()
            .map_err(|_| invalid_finesse_placement(index))?;
        let y = parts[3]
            .parse::<i16>()
            .map_err(|_| invalid_finesse_placement(index))?;
        placements.push(FinessePlacement::new(piece, rotation, x, y));
    }
    FinesseScoreRequest::new(placements).ok_or_else(|| {
        WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "finesse score requires at least one placement",
        )
    })
}

fn invalid_finesse_placement(index: usize) -> WebCommandError {
    WebCommandError::new(
        WebCommandErrorCode::InvalidValue,
        format!(
            "invalid finesse placement {}; expected PIECE:rotation:x:y",
            index + 1
        ),
    )
}

fn parse_spin_structure_command(
    tokens: &[String],
    worker_hardware_limit: usize,
    route: SpinStructureRoute,
) -> Result<WebCommandRequest, WebCommandError> {
    let mut board = StructureBoard::EMPTY;
    let mut board_option = None;
    let mut height = 8_u8;
    let mut height_was_explicit = false;
    let mut canonical_minimum_height = None;
    let mut inventory = None;
    let mut mode = SpinStructureMode::TSpins;
    let mut line_requirement = SpinLineRequirement::AtLeast(1);
    let mut fill_bottom = 0_u8;
    let mut fill_top = 5_u8;
    let mut fill_top_was_explicit = false;
    let mut rule_profile = RuleProfileId::SrsPlus;
    let mut max_placements = None;
    let mut minimality = MinimalityPolicy::SubsetMinimal;
    let mut workers = None;
    let mut automatic_worker_limit = None;
    let mut use_all_logical_processors = false;
    let mut max_patterns = SpinStructureProductMode::default_max_patterns();
    let mut max_patterns_seen = false;
    let mut objective_seen = false;
    let mut final_piece = PieceKind::T;
    let mut final_piece_seen = false;
    let mut dependency_report = false;
    let mut dependency_report_seen = false;
    let mut cursor = 0_usize;

    while cursor < tokens.len() {
        match tokens[cursor].as_str() {
            "--board-mask" => {
                set_spin_structure_board_option(&mut board_option, "--board-mask")?;
                let value = next_value(tokens, &mut cursor, "--board-mask")?;
                board = StructureBoard::from_words(parse_board_words(value, "--board-mask")?);
            }
            "--board-mask-v1" => {
                set_spin_structure_board_option(&mut board_option, "--board-mask-v1")?;
                let value = next_value(tokens, &mut cursor, "--board-mask-v1")?;
                let mask = parse_ctk3_board_mask(value, "--board-mask-v1")?;
                board = StructureBoard::from_words(mask.words());
                canonical_minimum_height = Some(mask.visible_height().max(4));
            }
            "--height" => {
                height = parse_positive(next_value(tokens, &mut cursor, "--height")?, "--height")?;
                height_was_explicit = true;
            }
            "--pieces" | "--inventory" => {
                if inventory.is_some() {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "spin-structure piece inventory may be specified only once",
                    ));
                }
                let option = tokens[cursor].clone();
                let value = next_value(tokens, &mut cursor, &option)?;
                inventory = Some(PieceInventory::parse(value).map_err(|error| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid spin-structure inventory: {error}"),
                    )
                })?);
            }
            "--spin-profile" | "--profile" => {
                let option = tokens[cursor].clone();
                let value = next_value(tokens, &mut cursor, &option)?;
                mode = SpinStructureMode::parse(value).ok_or_else(|| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid {option} value '{value}'"),
                    )
                })?;
            }
            "--lines" => {
                let value = next_value(tokens, &mut cursor, "--lines")?;
                line_requirement = SpinLineRequirement::parse(value).ok_or_else(|| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --lines value '{value}'"),
                    )
                })?;
                if line_requirement == SpinLineRequirement::AtLeast(0) {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "--lines 0+ is not a meaningful spin-structure requirement",
                    ));
                }
            }
            "--fill-bottom" => {
                let value = next_value(tokens, &mut cursor, "--fill-bottom")?;
                fill_bottom = parse_u8_allow_zero(value, "--fill-bottom")?;
            }
            "--fill-top" => {
                fill_top =
                    parse_positive(next_value(tokens, &mut cursor, "--fill-top")?, "--fill-top")?;
                fill_top_was_explicit = true;
            }
            "--rule" => {
                rule_profile = parse_rule_profile(next_value(tokens, &mut cursor, "--rule")?)?.id();
            }
            "--max-placements" => {
                max_placements = Some(parse_positive(
                    next_value(tokens, &mut cursor, "--max-placements")?,
                    "--max-placements",
                )?);
            }
            "--minimality" => {
                let value = next_value(tokens, &mut cursor, "--minimality")?;
                minimality = MinimalityPolicy::parse(value).ok_or_else(|| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --minimality value '{value}'"),
                    )
                })?;
            }
            "--objective" => {
                if route != SpinStructureRoute::Cover {
                    return Err(spin_structure_route_option_error(route, "--objective"));
                }
                if objective_seen {
                    return Err(repeated_spin_structure_option("--objective"));
                }
                let value = next_value(tokens, &mut cursor, "--objective")?;
                if !matches!(value, "min-cover" | "minimum-cover") {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!(
                            "invalid spin-structure cover objective '{value}'; expected min-cover"
                        ),
                    ));
                }
                objective_seen = true;
            }
            "--max-patterns" => {
                if route == SpinStructureRoute::Search {
                    return Err(spin_structure_route_option_error(route, "--max-patterns"));
                }
                if max_patterns_seen {
                    return Err(repeated_spin_structure_option("--max-patterns"));
                }
                let value = next_value(tokens, &mut cursor, "--max-patterns")?;
                max_patterns = parse_nonnegative_usize(value, "--max-patterns")?;
                if !(1..=SpinStructureProductMode::default_max_patterns()).contains(&max_patterns) {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!(
                            "spin-structure --max-patterns must be between 1 and {}",
                            SpinStructureProductMode::default_max_patterns()
                        ),
                    ));
                }
                max_patterns_seen = true;
            }
            "--final-piece" => {
                if route != SpinStructureRoute::Guaranteed {
                    return Err(spin_structure_route_option_error(route, "--final-piece"));
                }
                if final_piece_seen {
                    return Err(repeated_spin_structure_option("--final-piece"));
                }
                let value = next_value(tokens, &mut cursor, "--final-piece")?;
                let mut chars = value.chars();
                let piece = chars.next().ok_or_else(|| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "--final-piece requires one standard tetromino",
                    )
                })?;
                if chars.next().is_some() {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!(
                            "invalid --final-piece value '{value}'; expected one of I,O,T,S,Z,J,L"
                        ),
                    ));
                }
                final_piece = PieceKind::from_ascii(piece.to_ascii_uppercase()).map_err(|_| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!(
                            "invalid --final-piece value '{value}'; expected one of I,O,T,S,Z,J,L"
                        ),
                    )
                })?;
                final_piece_seen = true;
            }
            "--dependency-report" | "--no-dependency-report" => {
                if route != SpinStructureRoute::Guaranteed {
                    return Err(spin_structure_route_option_error(
                        route,
                        tokens[cursor].as_str(),
                    ));
                }
                if dependency_report_seen {
                    return Err(repeated_spin_structure_option(
                        "--dependency-report/--no-dependency-report",
                    ));
                }
                dependency_report = tokens[cursor] == "--dependency-report";
                dependency_report_seen = true;
                cursor += 1;
            }
            "--workers" => {
                workers = Some(parse_positive(
                    next_value(tokens, &mut cursor, "--workers")?,
                    "--workers",
                )?);
            }
            "--auto-workers" => {
                automatic_worker_limit = Some(parse_positive(
                    next_value(tokens, &mut cursor, "--auto-workers")?,
                    "--auto-workers",
                )?);
            }
            "--use-all-cpu-threads" | "--use-all-logical-processors" => {
                use_all_logical_processors = true;
                cursor += 1;
            }
            flag if flag.starts_with("--") => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::UnsupportedCommand,
                    format!("unsupported spin-structure option '{flag}'"),
                ));
            }
            value => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("unexpected spin-structure token '{value}'"),
                ));
            }
        }
    }

    if let Some(minimum_height) = canonical_minimum_height {
        if height_was_explicit && height < minimum_height {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!(
                    "--height {height} is below the --board-mask-v1 minimum height {minimum_height}"
                ),
            ));
        }
        height = height.max(minimum_height);
    }
    if !fill_top_was_explicit {
        fill_top = fill_top.min(height);
    }
    let inventory = inventory.ok_or_else(|| {
        WebCommandError::new(
            WebCommandErrorCode::MissingValue,
            "spin-structure requires --pieces",
        )
    })?;
    if route == SpinStructureRoute::Guaranteed {
        if inventory.count(final_piece) == 0 {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!(
                    "spin-structure guaranteed --final-piece {} is absent from the supplied inventory",
                    final_piece.as_ascii()
                ),
            ));
        }
        if mode.t_only() && final_piece != PieceKind::T {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                "T-spin profiles require --final-piece T",
            ));
        }
    }
    if max_placements.is_some_and(|limit| u16::from(limit) > inventory.total()) {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "--max-placements cannot exceed the supplied piece inventory",
        ));
    }
    let mut query = SpinStructureQuery::new(inventory, mode);
    query.initial_board = board;
    query.height = height;
    query.line_requirement = line_requirement;
    query.fill_bottom = fill_bottom;
    query.fill_top = fill_top;
    query.rule_profile = rule_profile;
    query.max_placements = max_placements;
    query.minimality = minimality;
    query.validate().map_err(|error| {
        WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!("invalid spin-structure request: {error}"),
        )
    })?;

    validate_worker_options(
        workers,
        automatic_worker_limit,
        use_all_logical_processors,
        worker_hardware_limit,
    )?;
    let product_mode = match route {
        SpinStructureRoute::Search => SpinStructureProductMode::Search,
        SpinStructureRoute::Cover => SpinStructureProductMode::Cover { max_patterns },
        SpinStructureRoute::Guaranteed => SpinStructureProductMode::Guaranteed {
            final_piece,
            max_patterns,
            dependency_report,
        },
    };
    let mut request = WebCommandRequest::spin_structure(query)
        .with_spin_structure_product_mode(product_mode)
        .with_worker_hardware_limit(worker_hardware_limit)
        .with_use_all_logical_processors(use_all_logical_processors);
    if let Some(workers) = workers {
        request = request.with_workers(workers);
    } else if let Some(workers) = automatic_worker_limit {
        request = request.with_automatic_worker_limit(workers);
    }
    Ok(request)
}

fn repeated_spin_structure_option(option: &str) -> WebCommandError {
    WebCommandError::new(
        WebCommandErrorCode::InvalidValue,
        format!("spin-structure option {option} may be specified only once"),
    )
}

fn spin_structure_route_option_error(route: SpinStructureRoute, option: &str) -> WebCommandError {
    let route = match route {
        SpinStructureRoute::Search => "search",
        SpinStructureRoute::Cover => "cover",
        SpinStructureRoute::Guaranteed => "guaranteed",
    };
    WebCommandError::new(
        WebCommandErrorCode::InvalidValue,
        format!("spin-structure {route} does not accept {option}"),
    )
}

fn set_spin_structure_board_option(
    target: &mut Option<&'static str>,
    option: &'static str,
) -> Result<(), WebCommandError> {
    if let Some(previous) = target.replace(option) {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!("spin-structure cannot combine {previous} with {option}"),
        ));
    }
    Ok(())
}

fn parse_u8_allow_zero(value: &str, option: &str) -> Result<u8, WebCommandError> {
    value.parse::<u8>().map_err(|_| {
        WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!("invalid {option} value '{value}'"),
        )
    })
}

fn parse_percent_command(
    tokens: &[String],
    worker_hardware_limit: usize,
) -> Result<WebCommandRequest, WebCommandError> {
    enum QueueMode {
        Observed,
        BagAligned,
        Fixed,
    }

    let mut queue_text = String::new();
    let mut mode = QueueMode::Observed;
    let mut minimum_len = None;
    let mut max_patterns = 0;
    let mut failed_pattern_limit = 100;
    let mut cursor = 0;
    while cursor < tokens.len() {
        match tokens[cursor].as_str() {
            "--queue" | "-q" => {
                let option = tokens[cursor].clone();
                queue_text = next_value(tokens, &mut cursor, &option)?.to_owned();
            }
            "--observed" => {
                mode = QueueMode::Observed;
                cursor += 1;
            }
            "--bag-aligned" | "--bag" => {
                mode = QueueMode::BagAligned;
                cursor += 1;
            }
            "--fixed" => {
                mode = QueueMode::Fixed;
                cursor += 1;
            }
            "--min-len" | "--minimum-len" => {
                let option = tokens[cursor].clone();
                minimum_len = Some(parse_nonnegative_usize(
                    next_value(tokens, &mut cursor, &option)?,
                    &option,
                )?);
            }
            "--max-patterns" => {
                max_patterns = parse_nonnegative_usize(
                    next_value(tokens, &mut cursor, "--max-patterns")?,
                    "--max-patterns",
                )?;
            }
            "--failed-count" | "--failed-pattern-limit" => {
                let option = tokens[cursor].clone();
                failed_pattern_limit =
                    parse_nonnegative_usize(next_value(tokens, &mut cursor, &option)?, &option)?;
            }
            value if !value.starts_with('-') && queue_text.is_empty() => {
                queue_text = value.to_owned();
                cursor += 1;
            }
            flag => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("unsupported percent option '{flag}'"),
                ));
            }
        }
    }

    let (queue, observed_queue) = match mode {
        QueueMode::Observed => (
            PcQueueInput::observed(parse_observed_queue(&queue_text).map_err(|_| {
                WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    "invalid observed percent queue",
                )
            })?),
            true,
        ),
        QueueMode::BagAligned => (
            PcQueueInput::bag_aligned_pattern(parse_bag_aligned_pattern(&queue_text).map_err(
                |_| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "invalid bag-aligned percent pattern",
                    )
                },
            )?),
            false,
        ),
        QueueMode::Fixed => (
            PcQueueInput::fixed_sequence(parse_fixed_sequence(&queue_text).map_err(|_| {
                WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    "invalid fixed percent sequence",
                )
            })?),
            false,
        ),
    };
    let minimum_len = minimum_len.unwrap_or(queue.len()).max(1);
    let supply_window_len = if observed_queue {
        minimum_len.max(queue.len())
    } else {
        minimum_len
    };
    let query = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(1, 0x3f0),
        queue,
        PieceWindow::new(minimum_len),
    )
    .with_exact_pieces(Some(1))
    .with_supply_window_size(SupplyWindowSize::new(supply_window_len))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_retained_trace_limit(0)
    .with_execution_policy(
        PcExecutionPolicy::mvp_default()
            .with_worker_hardware_limit(worker_hardware_limit)
            .with_max_patterns(max_patterns),
    );

    Ok(WebCommandRequest::percent(query, failed_pattern_limit))
}

fn parse_setup_score_command(
    tokens: &[String],
    worker_hardware_limit: usize,
) -> Result<WebCommandRequest, WebCommandError> {
    let mut document_format = None;
    let mut document = None;
    let mut setup_queue = None;
    let mut setup_patterns = None;
    let mut solution_queue = None;
    let mut solution_patterns = None;
    let mut clear_height = 4_u8;
    let mut clear_seen = false;
    let mut hold_enabled = true;
    let mut hold_seen = false;
    let mut score_profile = ScoreProfileSelection::Tetrio;
    let mut score_profile_seen = false;
    let mut initial_b2b = 0_u32;
    let mut initial_b2b_seen = false;
    let mut rule = srs_plus();
    let mut rule_seen = false;
    let mut max_patterns = None;
    let mut workers = None;
    let mut automatic_worker_limit = None;
    let mut use_all_logical_processors = false;
    let mut use_all_seen = false;
    let mut backend_seen = false;
    let mut no_fallback_seen = false;
    let mut cursor = 0_usize;
    while cursor < tokens.len() {
        let option = tokens[cursor].as_str();
        match option {
            "--document-format" => {
                if document_format.is_some() {
                    return Err(repeated_setup_score_option(option));
                }
                let value = next_value(tokens, &mut cursor, option)?;
                document_format = Some(FieldDocumentFormat::parse(value).map_err(|_| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!(
                            "invalid Setup score document format '{value}'; expected ctk3 or fumen"
                        ),
                    )
                })?);
            }
            "--document" => set_unique_string(
                &mut document,
                next_value(tokens, &mut cursor, option)?,
                "Setup score repeats --document",
            )?,
            "--setup-queue" => set_unique_string(
                &mut setup_queue,
                next_value(tokens, &mut cursor, option)?,
                "Setup score repeats --setup-queue",
            )?,
            "--setup-patterns" => set_unique_string(
                &mut setup_patterns,
                next_value(tokens, &mut cursor, option)?,
                "Setup score repeats --setup-patterns",
            )?,
            "--solution-queue" => set_unique_string(
                &mut solution_queue,
                next_value(tokens, &mut cursor, option)?,
                "Setup score repeats --solution-queue",
            )?,
            "--solution-patterns" => set_unique_string(
                &mut solution_patterns,
                next_value(tokens, &mut cursor, option)?,
                "Setup score repeats --solution-patterns",
            )?,
            "--clear" | "--clear-height" => {
                if clear_seen {
                    return Err(repeated_setup_score_option("--clear"));
                }
                let value = next_value(tokens, &mut cursor, option)?;
                clear_height = value
                    .parse::<u8>()
                    .ok()
                    .filter(|value| (1..=6).contains(value))
                    .ok_or_else(|| {
                        WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            "Setup score --clear must be an integer in 1..=6",
                        )
                    })?;
                clear_seen = true;
            }
            "--hold" => {
                if hold_seen {
                    return Err(repeated_setup_score_option("--hold/--no-hold"));
                }
                hold_enabled = true;
                hold_seen = true;
                cursor += 1;
            }
            "--no-hold" => {
                if hold_seen {
                    return Err(repeated_setup_score_option("--hold/--no-hold"));
                }
                hold_enabled = false;
                hold_seen = true;
                cursor += 1;
            }
            "--score-profile" => {
                if score_profile_seen {
                    return Err(repeated_setup_score_option(option));
                }
                let value = next_value(tokens, &mut cursor, option)?;
                score_profile = ScoreProfileSelection::parse(value).ok_or_else(|| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!(
                            "invalid Setup score profile '{value}'; expected tetrio, guideline, or jstris-ultra"
                        ),
                    )
                })?;
                score_profile_seen = true;
            }
            "--initial-b2b" => {
                if initial_b2b_seen {
                    return Err(repeated_setup_score_option(option));
                }
                initial_b2b = next_value(tokens, &mut cursor, option)?
                    .parse::<u32>()
                    .map_err(|_| {
                        WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            "Setup score --initial-b2b must be an unsigned integer",
                        )
                    })?;
                initial_b2b_seen = true;
            }
            "--rule" => {
                if rule_seen {
                    return Err(repeated_setup_score_option(option));
                }
                rule = parse_rule_profile(next_value(tokens, &mut cursor, option)?)?;
                rule_seen = true;
            }
            "--max-patterns" => {
                if max_patterns.is_some() {
                    return Err(repeated_setup_score_option(option));
                }
                max_patterns = Some(parse_positive(
                    next_value(tokens, &mut cursor, option)?,
                    option,
                )?);
            }
            "--workers" => {
                if workers.is_some() {
                    return Err(repeated_setup_score_option(option));
                }
                workers = Some(parse_positive(
                    next_value(tokens, &mut cursor, option)?,
                    option,
                )?);
            }
            "--auto-workers" | "--automatic-worker-limit" => {
                if automatic_worker_limit.is_some() {
                    return Err(repeated_setup_score_option("--auto-workers"));
                }
                automatic_worker_limit = Some(parse_positive(
                    next_value(tokens, &mut cursor, option)?,
                    option,
                )?);
            }
            "--use-all-cpu-threads" | "--use-all-logical-processors" => {
                if use_all_seen {
                    return Err(repeated_setup_score_option("--use-all-logical-processors"));
                }
                use_all_logical_processors = true;
                use_all_seen = true;
                cursor += 1;
            }
            "--backend" => {
                if backend_seen {
                    return Err(repeated_setup_score_option(option));
                }
                let value = next_value(tokens, &mut cursor, option)?;
                if RequestedSearchBackend::parse(value) != Some(RequestedSearchBackend::Cpu) {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "Setup score is CPU-only",
                    ));
                }
                backend_seen = true;
            }
            "--no-backend-fallback" => {
                if no_fallback_seen {
                    return Err(repeated_setup_score_option(option));
                }
                no_fallback_seen = true;
                cursor += 1;
            }
            "--allow-backend-fallback" => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    "Setup score does not accept backend fallback",
                ));
            }
            "--max-memory"
            | "--max-memory-mib"
            | "--gpu-device"
            | "--gpu-warmup"
            | "--cpu-warmup"
            | "--max-nodes"
            | "--max-frontier-states"
            | "--max-candidates"
            | "--queue"
            | "--patterns"
            | "--objective"
            | "--queue-knowledge"
            | "--source-pieces" => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("Setup score does not accept {option}"),
                ));
            }
            flag if flag.starts_with("--") => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::UnsupportedCommand,
                    format!("unsupported Setup score option '{flag}'"),
                ));
            }
            value => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("unexpected Setup score token '{value}'"),
                ));
            }
        }
    }
    let setup_source = match (setup_queue, setup_patterns) {
        (Some(queue), None) => WebSetupScoreQueueInput::queue(queue),
        (None, Some(patterns)) => WebSetupScoreQueueInput::patterns(patterns),
        (Some(_), Some(_)) => {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                "Setup score requires exactly one of --setup-queue or --setup-patterns",
            ));
        }
        (None, None) => {
            return Err(WebCommandError::new(
                WebCommandErrorCode::MissingValue,
                "Setup score requires exactly one of --setup-queue or --setup-patterns",
            ));
        }
    };
    let solution_source = match (solution_queue, solution_patterns) {
        (Some(queue), None) => WebSetupScoreQueueInput::queue(queue),
        (None, Some(patterns)) => WebSetupScoreQueueInput::patterns(patterns),
        (Some(_), Some(_)) => {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                "Setup score requires exactly one of --solution-queue or --solution-patterns",
            ));
        }
        (None, None) => {
            return Err(WebCommandError::new(
                WebCommandErrorCode::MissingValue,
                "Setup score requires exactly one of --solution-queue or --solution-patterns",
            ));
        }
    };
    validate_worker_options(
        workers,
        automatic_worker_limit,
        use_all_logical_processors,
        worker_hardware_limit,
    )?;
    let input = WebSetupScoreInput::new(
        document_format.ok_or_else(|| {
            WebCommandError::new(
                WebCommandErrorCode::MissingValue,
                "Setup score requires --document-format",
            )
        })?,
        document.ok_or_else(|| {
            WebCommandError::new(
                WebCommandErrorCode::MissingValue,
                "Setup score requires --document",
            )
        })?,
        setup_source,
        solution_source,
    )
    .with_clear_height(clear_height)
    .with_setup_hold_enabled(hold_enabled)
    .with_score_profile(score_profile)
    .with_initial_b2b(initial_b2b);
    let mut request = WebCommandRequest::setup_score(input)
        .with_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_rule(rule)
        .with_worker_hardware_limit(worker_hardware_limit)
        .with_runtime_webgpu_available(false)
        .with_hold_enabled(hold_enabled)
        .with_use_all_logical_processors(use_all_logical_processors);
    if let Some(limit) = max_patterns {
        request = request.with_max_patterns(limit);
    }
    if let Some(workers) = workers {
        request = request.with_workers(workers);
    } else if let Some(limit) = automatic_worker_limit {
        request = request.with_automatic_worker_limit(limit);
    }
    Ok(request)
}

fn repeated_setup_score_option(option: &str) -> WebCommandError {
    WebCommandError::new(
        WebCommandErrorCode::InvalidValue,
        format!("Setup score option {option} may be specified only once"),
    )
}

fn parse_setup_command(
    tokens: &[String],
    worker_hardware_limit: usize,
    canonical_priority: Option<clearra_problem::SetupCandidatePriority>,
) -> Result<WebCommandRequest, WebCommandError> {
    let mut remaining = "IOTSZJL".to_owned();
    let mut allow_post_cycle_borrow = false;
    let mut candidate_priority =
        canonical_priority.unwrap_or(clearra_problem::SetupCandidatePriority::All);
    let mut length_preference = clearra_problem::SetupLengthPreference::Auto;
    let mut max_setup_pieces = 9_u8;
    let mut explicit_search_mode = None;
    let mut queue_observation_policy = QueueObservationPolicy::default();
    let mut queue_based = None;
    let mut next_cycle_remaining = None;
    let mut rule = srs_plus();
    let mut path_detail_setup_id = None;
    let mut path_detail_condition_id = None;
    let mut workers = None;
    let mut automatic_worker_limit = None;
    let mut use_all_logical_processors = false;
    let mut tablebase_requested = false;
    let mut cursor = 0_usize;
    while cursor < tokens.len() {
        match tokens[cursor].as_str() {
            "--remaining" | "--queue" => {
                let option = tokens[cursor].clone();
                remaining = next_value(tokens, &mut cursor, &option)?.to_owned();
            }
            "--allow-post-cycle-borrow" => {
                allow_post_cycle_borrow = true;
                cursor += 1;
            }
            "--qb" => {
                queue_based = Some(next_value(tokens, &mut cursor, "--qb")?.to_owned());
            }
            "--next-cycle-remaining" => {
                next_cycle_remaining =
                    Some(next_value(tokens, &mut cursor, "--next-cycle-remaining")?.to_owned());
            }
            "--mode" => {
                let value = next_value(tokens, &mut cursor, "--mode")?;
                let Some(mode) = clearra_problem::SetupSearchMode::from_keyword(value) else {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid setup mode '{value}'; expected oracle or qb"),
                    ));
                };
                explicit_search_mode = Some(mode);
            }
            "--queue-knowledge" => {
                let value = next_value(tokens, &mut cursor, "--queue-knowledge")?;
                queue_observation_policy =
                    QueueObservationPolicy::from_keyword(value).ok_or_else(|| {
                        WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            format!(
                                "invalid queue knowledge '{value}'; expected oracle or visible-7"
                            ),
                        )
                    })?;
            }
            "--priority" => {
                let value = next_value(tokens, &mut cursor, "--priority")?;
                let Some(priority) = clearra_problem::SetupCandidatePriority::from_keyword(value)
                else {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid setup priority '{value}'; expected all, build, or pc"),
                    ));
                };
                if canonical_priority.is_some_and(|expected| expected != priority) {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!(
                            "canonical setup family fixes --priority to '{}'",
                            candidate_priority.keyword()
                        ),
                    ));
                }
                candidate_priority = priority;
            }
            "--setup-length" => {
                let value = next_value(tokens, &mut cursor, "--setup-length")?;
                let Some(preference) = clearra_problem::SetupLengthPreference::from_keyword(value)
                else {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!(
                            "invalid setup length preference '{value}'; expected auto, longer, or shorter"
                        ),
                    ));
                };
                length_preference = preference;
            }
            "--max-setup-pieces" => {
                let value = next_value(tokens, &mut cursor, "--max-setup-pieces")?;
                max_setup_pieces = value
                    .parse::<u8>()
                    .ok()
                    .filter(|count| (1..=10).contains(count))
                    .ok_or_else(|| {
                        WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            format!(
                                "invalid maximum setup piece count '{value}'; expected 1 through 10"
                            ),
                        )
                    })?;
            }
            "--paths-for" => {
                path_detail_setup_id =
                    Some(next_value(tokens, &mut cursor, "--paths-for")?.to_owned());
            }
            "--condition" => {
                path_detail_condition_id =
                    Some(next_value(tokens, &mut cursor, "--condition")?.to_owned());
            }
            "--rule" => {
                rule = parse_rule_profile(next_value(tokens, &mut cursor, "--rule")?)?;
            }
            "--workers" => {
                workers = Some(parse_positive(
                    next_value(tokens, &mut cursor, "--workers")?,
                    "--workers",
                )?);
            }
            "--auto-workers" => {
                automatic_worker_limit = Some(parse_positive(
                    next_value(tokens, &mut cursor, "--auto-workers")?,
                    "--auto-workers",
                )?);
            }
            "--use-all-cpu-threads" => {
                use_all_logical_processors = true;
                cursor += 1;
            }
            "--tablebase" | "--tb" => {
                tablebase_requested = true;
                cursor += 1;
            }
            "--no-tablebase" | "--no-tb" => {
                tablebase_requested = false;
                cursor += 1;
            }
            flag if flag.starts_with("--") => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::UnsupportedCommand,
                    format!("unsupported setup option '{flag}'"),
                ));
            }
            value => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("unexpected setup token '{value}'"),
                ));
            }
        }
    }
    let pieces =
        clearra_supply::queue::queue_parser::parse_piece_sequence(&remaining.to_ascii_uppercase())
            .map_err(|error| {
                WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("invalid setup residue: {error:?}"),
                )
            })?;
    let queue_based_pieces = queue_based
        .map(|value| {
            clearra_supply::queue::queue_parser::parse_piece_sequence(&value.to_ascii_uppercase())
                .map_err(|error| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid observed QB pieces: {error:?}"),
                    )
                })
        })
        .transpose()?;
    let next_cycle_remaining_pieces = next_cycle_remaining
        .map(|value| {
            clearra_supply::queue::queue_parser::parse_piece_sequence(&value.to_ascii_uppercase())
                .map_err(|error| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid next-cycle remaining pieces: {error:?}"),
                    )
                })
        })
        .transpose()?;
    let search_mode = match (explicit_search_mode, queue_based_pieces.is_some()) {
        (Some(clearra_problem::SetupSearchMode::ShapeOracle), true) => {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                "setup mode oracle cannot be combined with --qb",
            ));
        }
        (Some(clearra_problem::SetupSearchMode::QueueBased), false) => {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                "setup mode qb requires --qb observed pieces",
            ));
        }
        (Some(mode), _) => mode,
        (None, true) => clearra_problem::SetupSearchMode::QueueBased,
        (None, false) => clearra_problem::SetupSearchMode::ShapeOracle,
    };
    let cycle =
        clearra_problem::query::cycle_for_remaining_count(pieces.len()).ok_or_else(|| {
            WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                "setup residue must contain between one and seven pieces",
            )
        })?;
    if allow_post_cycle_borrow && cycle != 7 {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "--allow-post-cycle-borrow requires exactly three remaining pieces",
        ));
    }
    if let Some(next_cycle_pieces) = next_cycle_remaining_pieces.as_ref() {
        let expected = setup_next_cycle_remaining_count(cycle);
        if next_cycle_pieces.len() != expected {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!(
                    "--next-cycle-remaining requires {expected} pieces for setup cycle {cycle}"
                ),
            ));
        }
    }
    validate_worker_options(
        workers,
        automatic_worker_limit,
        use_all_logical_processors,
        worker_hardware_limit,
    )?;
    let mut request = WebCommandRequest::setup(pieces, allow_post_cycle_borrow)
        .with_rule(rule)
        .with_setup_candidate_priority(candidate_priority)
        .with_setup_length_preference(length_preference)
        .with_setup_max_pieces(max_setup_pieces)
        .with_setup_search_mode(search_mode)
        .with_queue_observation_policy(queue_observation_policy)
        .with_worker_hardware_limit(worker_hardware_limit)
        .with_use_all_logical_processors(use_all_logical_processors)
        .with_tablebase_requested(tablebase_requested);
    if let Some(pieces) = queue_based_pieces {
        request = request.with_setup_queue_based_pieces(pieces);
    }
    if let Some(pieces) = next_cycle_remaining_pieces {
        request = request.with_setup_next_cycle_remaining_pieces(pieces);
    }
    match (path_detail_setup_id, path_detail_condition_id) {
        (Some(setup_id), Some(condition_id)) => {
            if canonical_priority.is_some() {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    "canonical setup ranked families do not accept path-detail options",
                ));
            }
            let detail = clearra_problem::SetupPathDetail::from_setup_id(&setup_id, condition_id)
                .ok_or_else(|| {
                WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    "invalid setup path detail request",
                )
            })?;
            request = request.with_setup_path_detail(detail);
        }
        (Some(_), None) => {
            return Err(WebCommandError::new(
                WebCommandErrorCode::MissingValue,
                "--condition is required with --paths-for",
            ));
        }
        (None, Some(_)) => {
            return Err(WebCommandError::new(
                WebCommandErrorCode::MissingValue,
                "--paths-for is required with --condition",
            ));
        }
        (None, None) => {}
    }
    if let Some(workers) = workers {
        request = request.with_workers(workers);
    } else if let Some(workers) = automatic_worker_limit {
        request = request.with_automatic_worker_limit(workers);
    }
    Ok(request)
}

fn setup_next_cycle_remaining_count(cycle: u8) -> usize {
    // Keep malformed text commands out of AppRequest; the compiler enforces
    // this same cycle-derived terminal inventory invariant defensively.
    match cycle {
        1 => 4,
        2 => 1,
        3 => 5,
        4 => 2,
        5 => 6,
        6 => 3,
        7 => 7,
        _ => 0,
    }
}

fn parse_forward_command(
    tokens: &[String],
    spin_finder: bool,
    ren: bool,
    worker_hardware_limit: usize,
) -> Result<WebCommandRequest, WebCommandError> {
    let mut board = Board256Mask::EMPTY;
    let mut height = 8_u8;
    let mut board_option = None;
    let mut height_was_explicit = false;
    let mut canonical_minimum_height = None;
    let mut piece_source: Option<ForwardPieceSource> = None;
    let mut hold_enabled = true;
    let mut rule = RuleProfileId::SrsPlus;
    let mut spin_profile = if ren {
        SpinProfileId::Disabled
    } else if spin_finder {
        SpinProfileId::TSpins
    } else {
        SpinProfileId::AllMiniPlus
    };
    let mut initial_combo = None;
    let mut initial_back_to_back = None;
    let mut line_clear_policy = ForwardLineClearPolicy::Any;
    let mut minimum_damage = None;
    let mut target_lines = ForwardSpinLineRequirement::Any;
    let mut target_category = ForwardSpinCategory::Any;
    let mut workers = None;
    let mut automatic_worker_limit = None;
    let mut use_all_logical_processors = false;
    let mut cursor = 0_usize;
    while cursor < tokens.len() {
        match tokens[cursor].as_str() {
            "--board-mask" => {
                set_forward_board_option(&mut board_option, "--board-mask")?;
                let value = next_value(tokens, &mut cursor, "--board-mask")?;
                board = Board256Mask::from_words(parse_board_words(value, "--board-mask")?);
            }
            "--board-mask-v1" => {
                set_forward_board_option(&mut board_option, "--board-mask-v1")?;
                let value = next_value(tokens, &mut cursor, "--board-mask-v1")?;
                let mask = parse_ctk3_board_mask(value, "--board-mask-v1")?;
                board = Board256Mask::from_words(mask.words());
                canonical_minimum_height = Some(mask.visible_height().max(8));
            }
            "--height" => {
                height = parse_positive(next_value(tokens, &mut cursor, "--height")?, "--height")?;
                height_was_explicit = true;
            }
            "--queue" => {
                let value = next_value(tokens, &mut cursor, "--queue")?;
                let pieces = clearra_supply::queue::queue_parser::parse_piece_sequence(value)
                    .map_err(|error| {
                        WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            format!("invalid forward queue: {error:?}"),
                        )
                    })?;
                set_forward_piece_source(
                    &mut piece_source,
                    ForwardPieceSource::fixed_queue(pieces),
                )?;
            }
            "--patterns" if spin_finder => {
                let value = next_value(tokens, &mut cursor, "--patterns")?;
                let expression =
                    QueuePatternExpression::parse(value, 5_764_801).map_err(|error| {
                        WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            format!("invalid spin-finder pattern: {error}"),
                        )
                    })?;
                set_forward_piece_source(
                    &mut piece_source,
                    ForwardPieceSource::pattern(expression),
                )?;
            }
            "--patterns" => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::UnsupportedCommand,
                    if ren {
                        "REN search accepts only an exact --queue"
                    } else {
                        "damage search accepts only an exact --queue"
                    },
                ));
            }
            "--hold" => {
                hold_enabled = true;
                cursor += 1;
            }
            "--no-hold" => {
                hold_enabled = false;
                cursor += 1;
            }
            "--rule" => {
                rule = parse_rule_profile(next_value(tokens, &mut cursor, "--rule")?)?.id();
            }
            "--spin-profile" if !ren => {
                let value = next_value(tokens, &mut cursor, "--spin-profile")?;
                spin_profile = SpinProfileId::parse(value).ok_or_else(|| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --spin-profile value '{value}'"),
                    )
                })?;
            }
            "--initial-combo" if !ren => {
                let value = next_value(tokens, &mut cursor, "--initial-combo")?;
                let parsed = value.parse::<u16>().map_err(|_| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --initial-combo value '{value}'"),
                    )
                })?;
                initial_combo = (parsed > 0).then_some(parsed);
            }
            "--initial-b2b" if !ren => {
                let value = next_value(tokens, &mut cursor, "--initial-b2b")?;
                let parsed = value.parse::<u16>().map_err(|_| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --initial-b2b value '{value}'"),
                    )
                })?;
                initial_back_to_back = parsed.checked_sub(1);
            }
            "--preserve-b2b" if !ren => {
                line_clear_policy = ForwardLineClearPolicy::PreserveBackToBack;
                cursor += 1;
            }
            "--minimum-damage" if !spin_finder && !ren => {
                let value = next_value(tokens, &mut cursor, "--minimum-damage")?;
                minimum_damage = Some(value.parse::<u32>().map_err(|_| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --minimum-damage value '{value}'"),
                    )
                })?);
            }
            "--lines" if spin_finder => {
                let value = next_value(tokens, &mut cursor, "--lines")?;
                target_lines = if value.eq_ignore_ascii_case("any") {
                    ForwardSpinLineRequirement::Any
                } else {
                    let (line_text, at_least) = value
                        .strip_suffix('+')
                        .map_or((value, false), |minimum| (minimum, true));
                    let lines = line_text.parse::<u8>().map_err(|_| {
                        WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            format!("invalid --lines value '{value}'"),
                        )
                    })?;
                    if lines > 4 {
                        return Err(WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            "spin-finder --lines must be any, 0..4, or 1+..4+",
                        ));
                    }
                    if at_least && lines == 0 {
                        return Err(WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            "spin-finder --lines at-least form must be 1+ through 4+",
                        ));
                    }
                    if at_least {
                        ForwardSpinLineRequirement::AtLeast(lines)
                    } else {
                        ForwardSpinLineRequirement::Exact(lines)
                    }
                };
            }
            "--spin-category" if spin_finder => {
                let value = next_value(tokens, &mut cursor, "--spin-category")?;
                target_category = match value.to_ascii_lowercase().as_str() {
                    "any" => ForwardSpinCategory::Any,
                    "t" | "t-piece" => ForwardSpinCategory::T,
                    "other" | "non-t" => ForwardSpinCategory::Other,
                    _ => {
                        return Err(WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            format!("invalid --spin-category value '{value}'"),
                        ))
                    }
                };
            }
            "--workers" => {
                workers = Some(parse_positive(
                    next_value(tokens, &mut cursor, "--workers")?,
                    "--workers",
                )?);
            }
            "--auto-workers" => {
                automatic_worker_limit = Some(parse_positive(
                    next_value(tokens, &mut cursor, "--auto-workers")?,
                    "--auto-workers",
                )?);
            }
            "--use-all-cpu-threads" => {
                use_all_logical_processors = true;
                cursor += 1;
            }
            flag if flag.starts_with("--") => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::UnsupportedCommand,
                    format!("unsupported forward-search option '{flag}'"),
                ));
            }
            value => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("unexpected forward-search token '{value}'"),
                ));
            }
        }
    }
    if let Some(minimum_height) = canonical_minimum_height {
        if height_was_explicit && height < minimum_height {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!(
                    "--height {height} is below the --board-mask-v1 minimum height {minimum_height}"
                ),
            ));
        }
        height = height.max(minimum_height);
    }
    if !(1..=24).contains(&height) {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "forward-search --height must be between 1 and 24",
        ));
    }
    let piece_source = piece_source.ok_or_else(|| {
        WebCommandError::new(
            WebCommandErrorCode::MissingValue,
            if spin_finder {
                "spin-finder requires --queue or --patterns"
            } else if ren {
                "REN search requires --queue"
            } else {
                "damage search requires --queue"
            },
        )
    })?;
    if ren && piece_source.sequence_len() > clearra_forward_search::MAX_REN_QUEUE_PIECES {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "REN search accepts at most 22 queue pieces",
        ));
    }
    if spin_finder && spin_profile == SpinProfileId::Disabled {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "spin-finder requires an enabled --spin-profile",
        ));
    }
    if spin_finder
        && target_category == ForwardSpinCategory::Other
        && !spin_profile.recognizes_non_t_immobile_spins()
    {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "--spin-category other requires an all-spin or all-mini spin profile",
        ));
    }
    let mode = if ren {
        ForwardSearchMode::MaximumRen
    } else if spin_finder {
        ForwardSearchMode::SpinFinder(ForwardSpinTarget::with_line_requirement(
            target_lines,
            target_category,
        ))
    } else if let Some(minimum_damage) = minimum_damage {
        ForwardSearchMode::DamageAtLeast(minimum_damage)
    } else {
        ForwardSearchMode::MaximumDamage
    };
    let query = ForwardSearchQuery::new_with_source(
        board,
        height,
        piece_source,
        hold_enabled,
        rule,
        spin_profile,
        initial_combo,
        initial_back_to_back,
        mode,
    )
    .with_line_clear_policy(line_clear_policy);
    validate_worker_options(
        workers,
        automatic_worker_limit,
        use_all_logical_processors,
        worker_hardware_limit,
    )?;
    let command_kind = if ren {
        "ren"
    } else if spin_finder {
        "spin-finder"
    } else {
        "damage"
    };
    let mut request = WebCommandRequest::forward(command_kind, query)
        .with_worker_hardware_limit(worker_hardware_limit)
        .with_use_all_logical_processors(use_all_logical_processors);
    if let Some(workers) = workers {
        request = request.with_workers(workers);
    } else if let Some(workers) = automatic_worker_limit {
        request = request.with_automatic_worker_limit(workers);
    }
    Ok(request)
}

fn set_forward_board_option(
    target: &mut Option<&'static str>,
    option: &'static str,
) -> Result<(), WebCommandError> {
    if let Some(previous) = target.replace(option) {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!("forward search cannot combine {previous} with {option}"),
        ));
    }
    Ok(())
}

fn set_forward_piece_source(
    target: &mut Option<ForwardPieceSource>,
    source: ForwardPieceSource,
) -> Result<(), WebCommandError> {
    if target.replace(source).is_some() {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "forward search accepts exactly one of --queue or --patterns",
        ));
    }
    Ok(())
}

fn parse_build_v2_command(
    tokens: &[String],
    worker_hardware_limit: usize,
) -> Result<WebCommandRequest, WebCommandError> {
    let subcommand = tokens.first().ok_or_else(|| {
        WebCommandError::new(
            WebCommandErrorCode::MissingValue,
            "build requires a canonical subcommand",
        )
    })?;
    let (capability, options) = match subcommand.as_str() {
        "cover" => (WebBuildV2Capability::Cover, &tokens[1..]),
        "setup" => (WebBuildV2Capability::Setup, &tokens[1..]),
        "congruent" => (WebBuildV2Capability::Congruent, &tokens[1..]),
        "congruent-cover" => (WebBuildV2Capability::CongruentCover, &tokens[1..]),
        "setup-cover" => (WebBuildV2Capability::SetupCover, &tokens[1..]),
        "setup-cover-percent" => (WebBuildV2Capability::SetupCoverPercent, &tokens[1..]),
        "setup-cover-score" => (WebBuildV2Capability::SetupCoverScore, &tokens[1..]),
        "evaluate" => {
            let evaluation = tokens.get(1).ok_or_else(|| {
                WebCommandError::new(
                    WebCommandErrorCode::MissingValue,
                    "build evaluate requires a canonical subcommand",
                )
            })?;
            let capability = match evaluation.as_str() {
                "cover" => WebBuildV2Capability::EvaluateCover,
                "minimals" => WebBuildV2Capability::EvaluateMinimals,
                "score" => WebBuildV2Capability::EvaluateScore,
                "b2b-cover" => WebBuildV2Capability::EvaluateB2bCover,
                "cover-percent" => WebBuildV2Capability::EvaluateCoverPercent,
                value => {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::UnsupportedCommand,
                        format!("unsupported build evaluate subcommand '{value}'"),
                    ));
                }
            };
            (capability, &tokens[2..])
        }
        value => {
            return Err(WebCommandError::new(
                WebCommandErrorCode::UnsupportedCommand,
                format!("unsupported build subcommand '{value}'"),
            ));
        }
    };

    let mut base_words = None;
    let mut target_words = None;
    let mut visible_height = None;
    let mut target_format = None;
    let mut target_document = None;
    let mut solution_format = None;
    let mut solution_document = None;
    let mut queue = None;
    let mut patterns = None;
    let mut hold_piece = None;
    let mut hold_enabled = true;
    let mut hold_option_seen = false;
    let mut source_piece_count = None;
    let mut queue_knowledge = None;
    let mut objective = None;
    let mut score_profile = None;
    let mut initial_b2b = None;
    let mut rule = srs_plus();
    let mut rule_seen = false;
    let mut max_patterns = None;
    let mut max_nodes = None;
    let mut max_frontier_states = None;
    let mut max_candidates = None;
    let mut workers = None;
    let mut automatic_worker_limit = None;
    let mut use_all_logical_processors = false;
    let mut use_all_seen = false;
    let mut cpu_warmup = false;
    let mut cpu_warmup_seen = false;
    let mut backend_seen = false;
    let mut no_backend_fallback_seen = false;
    let mut cursor = 0usize;

    while cursor < options.len() {
        let option = options[cursor].as_str();
        match option {
            "--base-mask" => {
                let value = parse_board_words(
                    next_value(options, &mut cursor, "--base-mask")?,
                    "--base-mask",
                )?;
                set_build_v2_option(&mut base_words, value, "--base-mask")?;
            }
            "--target-mask" => {
                let value = parse_board_words(
                    next_value(options, &mut cursor, "--target-mask")?,
                    "--target-mask",
                )?;
                set_build_v2_option(&mut target_words, value, "--target-mask")?;
            }
            "--height" => {
                let value =
                    parse_positive(next_value(options, &mut cursor, "--height")?, "--height")?;
                set_build_v2_option(&mut visible_height, value, "--height")?;
            }
            "--target-format" => {
                let value = next_value(options, &mut cursor, "--target-format")?;
                let format = FieldDocumentFormat::parse(value).map_err(|_| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --target-format value '{value}'; expected ctk3 or fumen"),
                    )
                })?;
                set_build_v2_option(&mut target_format, format, "--target-format")?;
            }
            "--target-document" => {
                let value = next_value(options, &mut cursor, "--target-document")?.to_owned();
                set_build_v2_option(&mut target_document, value, "--target-document")?;
            }
            "--solution-format" => {
                let value = next_value(options, &mut cursor, "--solution-format")?;
                let format = FieldDocumentFormat::parse(value).map_err(|_| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!(
                            "invalid --solution-format value '{value}'; expected ctk3 or fumen"
                        ),
                    )
                })?;
                set_build_v2_option(&mut solution_format, format, "--solution-format")?;
            }
            "--solution-document" => {
                let value = next_value(options, &mut cursor, "--solution-document")?.to_owned();
                set_build_v2_option(&mut solution_document, value, "--solution-document")?;
            }
            "--queue" => {
                let value = next_value(options, &mut cursor, "--queue")?.to_owned();
                set_build_v2_option(&mut queue, value, "--queue")?;
            }
            "--patterns" | "--pattern" => {
                let value = next_value(options, &mut cursor, option)?.to_owned();
                set_build_v2_option(&mut patterns, value, "--patterns")?;
            }
            "--hold" => {
                if hold_option_seen {
                    return Err(repeated_build_v2_option("--hold/--no-hold"));
                }
                hold_piece = parse_hold_piece(next_value(options, &mut cursor, "--hold")?)?;
                hold_enabled = true;
                hold_option_seen = true;
            }
            "--no-hold" => {
                if hold_option_seen {
                    return Err(repeated_build_v2_option("--hold/--no-hold"));
                }
                hold_enabled = false;
                hold_option_seen = true;
                cursor += 1;
            }
            "--source-pieces" => {
                let value = parse_positive(
                    next_value(options, &mut cursor, "--source-pieces")?,
                    "--source-pieces",
                )?;
                set_build_v2_option(&mut source_piece_count, value, "--source-pieces")?;
            }
            "--queue-knowledge" => {
                let value = next_value(options, &mut cursor, "--queue-knowledge")?;
                let parsed = BuildQueueKnowledge::parse(value).ok_or_else(|| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!(
                            "invalid --queue-knowledge value '{value}'; expected oracle or visible-7"
                        ),
                    )
                })?;
                set_build_v2_option(&mut queue_knowledge, parsed, "--queue-knowledge")?;
            }
            "--objective" => {
                let value = next_value(options, &mut cursor, "--objective")?;
                let parsed = BuildObjective::parse(value).ok_or_else(|| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!(
                            "invalid --objective value '{value}'; expected all, unique, min-cover, max-probability-minimum, or max-score-cover"
                        ),
                    )
                })?;
                set_build_v2_option(&mut objective, parsed, "--objective")?;
            }
            "--score-profile" => {
                let value = next_value(options, &mut cursor, "--score-profile")?;
                let parsed = BuildScoreProfile::parse(value).ok_or_else(|| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!(
                            "invalid --score-profile value '{value}'; expected tetrio, guideline, or jstris-ultra"
                        ),
                    )
                })?;
                set_build_v2_option(&mut score_profile, parsed, "--score-profile")?;
            }
            "--initial-b2b" => {
                let value = next_value(options, &mut cursor, "--initial-b2b")?;
                let parsed = value.parse::<u16>().map_err(|_| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --initial-b2b value '{value}'; expected 0..65535"),
                    )
                })?;
                set_build_v2_option(&mut initial_b2b, parsed, "--initial-b2b")?;
            }
            "--rule" => {
                if rule_seen {
                    return Err(repeated_build_v2_option("--rule"));
                }
                rule = parse_rule_profile(next_value(options, &mut cursor, "--rule")?)?;
                rule_seen = true;
            }
            "--max-patterns" => {
                let value = parse_positive(
                    next_value(options, &mut cursor, "--max-patterns")?,
                    "--max-patterns",
                )?;
                set_build_v2_option(&mut max_patterns, value, "--max-patterns")?;
            }
            "--max-nodes" => {
                let value = parse_positive(
                    next_value(options, &mut cursor, "--max-nodes")?,
                    "--max-nodes",
                )?;
                set_build_v2_option(&mut max_nodes, value, "--max-nodes")?;
            }
            "--max-frontier-states" => {
                let value = parse_positive(
                    next_value(options, &mut cursor, "--max-frontier-states")?,
                    "--max-frontier-states",
                )?;
                set_build_v2_option(&mut max_frontier_states, value, "--max-frontier-states")?;
            }
            "--max-candidates" => {
                let value = parse_positive(
                    next_value(options, &mut cursor, "--max-candidates")?,
                    "--max-candidates",
                )?;
                set_build_v2_option(&mut max_candidates, value, "--max-candidates")?;
            }
            "--workers" => {
                let value =
                    parse_positive(next_value(options, &mut cursor, "--workers")?, "--workers")?;
                set_build_v2_option(&mut workers, value, "--workers")?;
            }
            "--auto-workers" | "--automatic-worker-limit" => {
                let value = parse_positive(next_value(options, &mut cursor, option)?, option)?;
                set_build_v2_option(&mut automatic_worker_limit, value, "--auto-workers")?;
            }
            "--use-all-cpu-threads" | "--use-all-logical-processors" => {
                if use_all_seen {
                    return Err(repeated_build_v2_option("--use-all-cpu-threads"));
                }
                use_all_logical_processors = true;
                use_all_seen = true;
                cursor += 1;
            }
            "--cpu-warmup" => {
                if cpu_warmup_seen {
                    return Err(repeated_build_v2_option("--cpu-warmup"));
                }
                cpu_warmup = true;
                cpu_warmup_seen = true;
                cursor += 1;
            }
            "--backend" => {
                if backend_seen {
                    return Err(repeated_build_v2_option("--backend"));
                }
                let value = next_value(options, &mut cursor, "--backend")?;
                let backend = RequestedSearchBackend::parse(value).ok_or_else(|| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --backend value '{value}'"),
                    )
                })?;
                if backend != RequestedSearchBackend::Cpu {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "Build v2 is CPU-only",
                    ));
                }
                backend_seen = true;
            }
            "--no-backend-fallback" => {
                if no_backend_fallback_seen {
                    return Err(repeated_build_v2_option("--no-backend-fallback"));
                }
                no_backend_fallback_seen = true;
                cursor += 1;
            }
            "--allow-backend-fallback" => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    "Build v2 is CPU-only and does not accept backend fallback",
                ));
            }
            "--max-memory-mib" | "--max-memory" => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    "Build v2 does not accept max-memory-mib until governed request and response memory authority exists",
                ));
            }
            flag if flag.starts_with("--") => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::UnsupportedCommand,
                    format!("unsupported {} option '{flag}'", capability.capability_id()),
                ));
            }
            value => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("unexpected {} token '{value}'", capability.capability_id()),
                ));
            }
        }
    }

    match (&queue, &patterns) {
        (Some(_), Some(_)) => {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                "Build v2 accepts exactly one of --queue or --patterns",
            ));
        }
        (None, None) => {
            return Err(WebCommandError::new(
                WebCommandErrorCode::MissingValue,
                "Build v2 requires exactly one of --queue or --patterns",
            ));
        }
        _ => {}
    }
    validate_worker_options(
        workers,
        automatic_worker_limit,
        use_all_logical_processors,
        worker_hardware_limit,
    )?;

    let objective = objective.unwrap_or_else(|| capability.default_objective());
    if !capability.supports_objective(objective) {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!(
                "{} does not accept objective '{}'",
                capability.capability_id(),
                objective.as_str()
            ),
        ));
    }
    if !capability.score_capable() && (score_profile.is_some() || initial_b2b.is_some()) {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!(
                "{} does not accept score-profile or initial-b2b options",
                capability.capability_id()
            ),
        ));
    }

    let mut input = if capability == WebBuildV2Capability::Cover {
        if target_format.is_some()
            || target_document.is_some()
            || solution_format.is_some()
            || solution_document.is_some()
        {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                "build.cover accepts base/target masks, not a target or solution document",
            ));
        }
        let input = WebBuildV2Input::cover(
            base_words.ok_or_else(|| missing_build_v2_option(capability, "--base-mask"))?,
            target_words.ok_or_else(|| missing_build_v2_option(capability, "--target-mask"))?,
            visible_height.ok_or_else(|| missing_build_v2_option(capability, "--height"))?,
            objective,
        )?;
        match source_piece_count {
            Some(count) => input.with_source_piece_count(count)?,
            None => input,
        }
    } else if capability.uses_target_document() {
        if base_words.is_some()
            || target_words.is_some()
            || visible_height.is_some()
            || source_piece_count.is_some()
            || solution_format.is_some()
            || solution_document.is_some()
        {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!(
                    "{} accepts only a nominal target document",
                    capability.capability_id()
                ),
            ));
        }
        WebBuildV2Input::target_document(
            capability,
            target_format.ok_or_else(|| missing_build_v2_option(capability, "--target-format"))?,
            target_document
                .as_deref()
                .ok_or_else(|| missing_build_v2_option(capability, "--target-document"))?,
            objective,
        )?
    } else {
        if base_words.is_some()
            || target_words.is_some()
            || visible_height.is_some()
            || source_piece_count.is_some()
            || target_format.is_some()
            || target_document.is_some()
        {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!(
                    "{} accepts only a nominal supplied-solution document",
                    capability.capability_id()
                ),
            ));
        }
        WebBuildV2Input::solution_document(
            capability,
            solution_format
                .ok_or_else(|| missing_build_v2_option(capability, "--solution-format"))?,
            solution_document
                .as_deref()
                .ok_or_else(|| missing_build_v2_option(capability, "--solution-document"))?,
            objective,
        )?
    };

    input = input
        .with_queue_knowledge(queue_knowledge.unwrap_or_default())
        .with_hold_piece(hold_piece)
        .with_allow_hold(hold_enabled);
    if capability.score_capable() {
        input = input
            .with_score_options(score_profile.unwrap_or_default(), initial_b2b.unwrap_or(0))?;
    }

    let mut request = WebCommandRequest::build_v2(input)
        .with_backend(RequestedSearchBackend::Cpu)
        .with_allow_backend_fallback(false)
        .with_rule(rule)
        .with_worker_hardware_limit(worker_hardware_limit)
        .with_runtime_webgpu_available(false)
        .with_hold_enabled(hold_enabled)
        .with_use_all_logical_processors(use_all_logical_processors)
        .with_cpu_warmup(cpu_warmup);
    if let Some(queue) = queue {
        request = request.with_queue(queue);
    }
    if let Some(patterns) = patterns {
        request = request.with_patterns(patterns);
    }
    if let Some(limit) = max_patterns {
        request = request.with_max_patterns(limit);
    }
    if let Some(limit) = max_nodes {
        request = request.with_max_nodes(limit);
    }
    if let Some(limit) = max_frontier_states {
        request = request.with_max_frontier_states(limit);
    }
    if let Some(limit) = max_candidates {
        request = request.with_max_candidates(limit);
    }
    if let Some(workers) = workers {
        request = request.with_workers(workers);
    } else if let Some(workers) = automatic_worker_limit {
        request = request.with_automatic_worker_limit(workers);
    }
    Ok(request)
}

fn set_build_v2_option<T>(
    slot: &mut Option<T>,
    value: T,
    option: &str,
) -> Result<(), WebCommandError> {
    if slot.replace(value).is_some() {
        Err(repeated_build_v2_option(option))
    } else {
        Ok(())
    }
}

fn repeated_build_v2_option(option: &str) -> WebCommandError {
    WebCommandError::new(
        WebCommandErrorCode::InvalidValue,
        format!("Build v2 option {option} may be specified only once"),
    )
}

fn missing_build_v2_option(capability: WebBuildV2Capability, option: &str) -> WebCommandError {
    WebCommandError::new(
        WebCommandErrorCode::MissingValue,
        format!("{} requires {option}", capability.capability_id()),
    )
}

fn parse_build_probability_command(
    tokens: &[String],
    worker_hardware_limit: usize,
) -> Result<WebCommandRequest, WebCommandError> {
    let mut base_mask = None;
    let mut target_mask = None;
    let mut height = None;
    let mut queue = None;
    let mut patterns = None;
    let mut hold_piece = None;
    let mut hold_enabled = true;
    let mut source_piece_count = None;
    let mut max_patterns = None;
    let mut max_candidates = None;
    let mut max_memory_mib = None;
    let mut workers = None;
    let mut automatic_worker_limit = None;
    let mut use_all_logical_processors = false;
    let mut cpu_warmup = false;
    let mut backend = RequestedSearchBackend::Cpu;
    let mut backend_fallback = BackendFallbackOverride::default();
    let mut include_horizontal_mirror = true;
    let mut aggregation = BuildAggregationOverride::default();
    let mut result_mode: Option<BuildProbabilityResultMode> = None;
    let mut score_profile = ScoreProfileSelection::Tetrio;
    let mut score_profile_requested = false;
    let mut initial_b2b = 0_u16;
    let mut initial_b2b_requested = false;
    let mut failed_pattern_limit = 100_usize;
    let mut failed_pattern_limit_requested = false;
    let mut spin_profile = None;
    let mut preserve_back_to_back = false;
    let mut precompute_build_dependencies = false;
    let mut build_dependency_option_requested = false;
    let mut solution_probabilities = false;
    let mut queue_knowledge = None;
    let mut finesse_metric = FinesseMetric::Off;
    let mut finesse_option_requested = false;
    let mut finesse_pattern_knowledge = FinessePatternKnowledge::Both;
    let mut pattern_knowledge_option_requested = false;
    let mut rule = srs_plus();
    let mut rule_requested = false;
    let mut cursor = 0usize;

    while cursor < tokens.len() {
        match tokens[cursor].as_str() {
            "--base-mask" => {
                let value = next_value(tokens, &mut cursor, "--base-mask")?;
                base_mask = Some(parse_board_words(value, "--base-mask")?);
            }
            "--target-mask" => {
                let value = next_value(tokens, &mut cursor, "--target-mask")?;
                target_mask = Some(parse_board_words(value, "--target-mask")?);
            }
            "--height" => {
                let value = next_value(tokens, &mut cursor, "--height")?;
                height = Some(parse_positive(value, "--height")?);
            }
            "--queue" => {
                queue = Some(next_value(tokens, &mut cursor, "--queue")?.to_owned());
            }
            "--patterns" | "--pattern" => {
                let option = tokens[cursor].clone();
                patterns = Some(next_value(tokens, &mut cursor, &option)?.to_owned());
            }
            "--hold" => {
                hold_piece = Some(parse_hold_piece(next_value(
                    tokens,
                    &mut cursor,
                    "--hold",
                )?)?);
                hold_enabled = true;
            }
            "--no-hold" => {
                hold_enabled = false;
                cursor += 1;
            }
            "--source-pieces" => {
                source_piece_count = Some(parse_positive(
                    next_value(tokens, &mut cursor, "--source-pieces")?,
                    "--source-pieces",
                )?);
            }
            "--max-patterns" => {
                max_patterns = Some(parse_positive(
                    next_value(tokens, &mut cursor, "--max-patterns")?,
                    "--max-patterns",
                )?);
            }
            "--max-candidates" => {
                max_candidates = Some(parse_positive(
                    next_value(tokens, &mut cursor, "--max-candidates")?,
                    "--max-candidates",
                )?);
            }
            "--max-memory-mib" => {
                max_memory_mib = Some(parse_positive(
                    next_value(tokens, &mut cursor, "--max-memory-mib")?,
                    "--max-memory-mib",
                )?);
            }
            "--workers" => {
                workers = Some(parse_positive(
                    next_value(tokens, &mut cursor, "--workers")?,
                    "--workers",
                )?);
            }
            "--auto-workers" => {
                automatic_worker_limit = Some(parse_positive(
                    next_value(tokens, &mut cursor, "--auto-workers")?,
                    "--auto-workers",
                )?);
            }
            "--use-all-cpu-threads" => {
                use_all_logical_processors = true;
                cursor += 1;
            }
            "--cpu-warmup" => {
                cpu_warmup = true;
                cursor += 1;
            }
            "--backend" => {
                let value = next_value(tokens, &mut cursor, "--backend")?;
                backend = RequestedSearchBackend::parse(value).ok_or_else(|| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --backend value '{value}'"),
                    )
                })?;
            }
            "--allow-backend-fallback" => {
                backend_fallback.record(true)?;
                cursor += 1;
            }
            "--no-backend-fallback" => {
                backend_fallback.record(false)?;
                cursor += 1;
            }
            "--include-mirror" => {
                include_horizontal_mirror = true;
                cursor += 1;
            }
            "--no-mirror" => {
                include_horizontal_mirror = false;
                cursor += 1;
            }
            "--aggregate" => {
                let value = match next_value(tokens, &mut cursor, "--aggregate")? {
                    "buildability" | "build" => BuildAggregationKind::Buildability,
                    "tiling" => BuildAggregationKind::Tiling,
                    "spin" => BuildAggregationKind::Spin,
                    value => {
                        return Err(WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            format!(
                                "unsupported build-probability aggregation '{value}'; expected buildability, tiling, or spin"
                            ),
                        ));
                    }
                };
                aggregation.record(value)?;
            }
            "--tiling-only" => {
                aggregation.record(BuildAggregationKind::Tiling)?;
                cursor += 1;
            }
            "--result-mode" => {
                let value = next_value(tokens, &mut cursor, "--result-mode")?;
                let parsed = match value {
                    "all" | "all-solutions" => BuildProbabilityResultMode::AllSolutions,
                    "paths" | "complete-replay-paths" => {
                        BuildProbabilityResultMode::CompleteReplayPaths
                    }
                    "score" | "field-average-score" => {
                        BuildProbabilityResultMode::FieldAverageScore
                    }
                    "fixed-score" | "fixed-queue-maximum-score" => {
                        BuildProbabilityResultMode::FixedQueueMaximumScore
                    }
                    "score-minimals" | "highest-score-minimum-set" => {
                        BuildProbabilityResultMode::HighestScoreMinimumSet
                    }
                    "failed" | "failed-queue" | "failed-queues" => {
                        BuildProbabilityResultMode::FailedQueues
                    }
                    _ => {
                        return Err(WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            format!(
                                "unsupported build result mode '{value}'; expected all-solutions, complete-replay-paths, minimum score products, field-average-score, or failed-queues"
                            ),
                        ));
                    }
                };
                if result_mode.replace(parsed).is_some() {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "build result mode may be specified only once",
                    ));
                }
            }
            "--paths" => {
                if result_mode
                    .replace(BuildProbabilityResultMode::CompleteReplayPaths)
                    .is_some()
                {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "build result mode may be specified only once",
                    ));
                }
                cursor += 1;
            }
            "--score" => {
                if result_mode
                    .replace(BuildProbabilityResultMode::FieldAverageScore)
                    .is_some()
                {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "build result mode may be specified only once",
                    ));
                }
                cursor += 1;
            }
            "--score-profile" => {
                let value = next_value(tokens, &mut cursor, "--score-profile")?;
                if score_profile_requested {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "--score-profile may be specified only once",
                    ));
                }
                score_profile = ScoreProfileSelection::parse(value).ok_or_else(|| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --score-profile value '{value}'"),
                    )
                })?;
                score_profile_requested = true;
            }
            "--initial-b2b" => {
                let value = next_value(tokens, &mut cursor, "--initial-b2b")?;
                if initial_b2b_requested {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "--initial-b2b may be specified only once",
                    ));
                }
                initial_b2b = value.parse::<u16>().map_err(|_| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --initial-b2b value '{value}'"),
                    )
                })?;
                initial_b2b_requested = true;
            }
            "--failed-count" | "--failed-pattern-limit" => {
                if failed_pattern_limit_requested {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "--failed-count may be specified only once",
                    ));
                }
                failed_pattern_limit = parse_positive(
                    next_value(tokens, &mut cursor, "--failed-count")?,
                    "--failed-count",
                )?;
                failed_pattern_limit_requested = true;
            }
            "--spin-profile" => {
                let value = next_value(tokens, &mut cursor, "--spin-profile")?;
                spin_profile = Some(SpinProfileSelection::parse(value).ok_or_else(|| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --spin-profile value '{value}'"),
                    )
                })?);
            }
            "--preserve-b2b" => {
                preserve_back_to_back = true;
                cursor += 1;
            }
            "--build-dependency-dag" => {
                precompute_build_dependencies = true;
                build_dependency_option_requested = true;
                cursor += 1;
            }
            "--no-build-dependency-dag" => {
                precompute_build_dependencies = false;
                build_dependency_option_requested = true;
                cursor += 1;
            }
            "--solution-probabilities" => {
                if solution_probabilities {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "build-probability solution probabilities may be requested only once",
                    ));
                }
                solution_probabilities = true;
                cursor += 1;
            }
            "--queue-knowledge" => {
                let value = next_value(tokens, &mut cursor, "--queue-knowledge")?;
                let policy = match value {
                    "oracle" => QueueObservationPolicy::FullQueueOracle,
                    "visible-7" => QueueObservationPolicy::VisibleSeven,
                    _ => {
                        return Err(WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            format!(
                                "unsupported --queue-knowledge value '{value}'; expected oracle or visible-7"
                            ),
                        ));
                    }
                };
                if queue_knowledge.replace(policy).is_some() {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "build-probability queue knowledge may be specified only once",
                    ));
                }
            }
            "--finesse" => {
                let value = next_value(tokens, &mut cursor, "--finesse")?;
                finesse_metric = FinesseMetric::parse(value).ok_or_else(|| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("unsupported --finesse value '{value}'; expected off or inputs"),
                    )
                })?;
                finesse_option_requested = true;
            }
            "--pattern-knowledge" => {
                let value = next_value(tokens, &mut cursor, "--pattern-knowledge")?;
                finesse_pattern_knowledge =
                    FinessePatternKnowledge::parse(value).ok_or_else(|| {
                        WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            format!(
                                "unsupported --pattern-knowledge value '{value}'; expected both, oracle, or visible-7"
                            ),
                        )
                    })?;
                pattern_knowledge_option_requested = true;
            }
            "--rule" => {
                rule = parse_rule_profile(next_value(tokens, &mut cursor, "--rule")?)?;
                rule_requested = true;
            }
            flag if flag.starts_with("--") => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::UnsupportedCommand,
                    format!("unsupported build-probability option '{flag}'"),
                ));
            }
            value => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("unexpected build-probability token '{value}'"),
                ));
            }
        }
    }

    if queue.is_some() && patterns.is_some() {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "--queue and --patterns are mutually exclusive",
        ));
    }
    let aggregation_kind = aggregation.resolve();
    let tiling_only = aggregation_kind == BuildAggregationKind::Tiling;
    let result_mode = result_mode.unwrap_or_default();
    let queue_knowledge = queue_knowledge.unwrap_or_default();
    if tiling_only && queue_knowledge == QueueObservationPolicy::VisibleSeven {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "visible-7 queue knowledge is unavailable with tiling-only Build semantics",
        ));
    }
    if tiling_only
        && (spin_profile.is_some()
            || preserve_back_to_back
            || build_dependency_option_requested
            || solution_probabilities
            || finesse_option_requested
            || pattern_knowledge_option_requested
            || rule_requested)
    {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "tiling aggregation cannot be combined with rule, spin, B2B-preservation, BuildUp dependency, per-solution probability, or finesse options",
        ));
    }
    if result_mode != BuildProbabilityResultMode::AllSolutions
        && (aggregation_kind != BuildAggregationKind::Buildability
            || finesse_metric.requested()
            || preserve_back_to_back)
    {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "The selected Build result mode is incompatible with this engine aggregation: non-all result modes require buildability without finesse or B2B-preservation",
        ));
    }
    if (score_profile_requested || initial_b2b_requested)
        && !matches!(
            result_mode,
            BuildProbabilityResultMode::FieldAverageScore
                | BuildProbabilityResultMode::FixedQueueMaximumScore
                | BuildProbabilityResultMode::HighestScoreMinimumSet
        )
    {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "--score-profile and --initial-b2b require a Build score result mode",
        ));
    }
    if failed_pattern_limit_requested && result_mode != BuildProbabilityResultMode::FailedQueues {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "--failed-count requires --result-mode failed-queues",
        ));
    }
    if result_mode == BuildProbabilityResultMode::CompleteReplayPaths
        && height.is_some_and(|height| height > 6)
    {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "complete Build replay paths currently require --height 1..6",
        ));
    }
    if spin_profile.is_some()
        && aggregation_kind != BuildAggregationKind::Spin
        && !preserve_back_to_back
    {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "--spin-profile requires --aggregate spin or --preserve-b2b",
        ));
    }
    if pattern_knowledge_option_requested && !finesse_metric.requested() {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "--pattern-knowledge requires --finesse inputs",
        ));
    }
    let aggregation = match aggregation_kind {
        BuildAggregationKind::Buildability => BuildProbabilityAggregation::Buildability,
        BuildAggregationKind::Tiling => BuildProbabilityAggregation::TilingOnly,
        BuildAggregationKind::Spin => BuildProbabilityAggregation::spin_search(
            spin_profile.unwrap_or(SpinProfileSelection::TSpins),
        ),
    };
    let constraint_profile = spin_profile.unwrap_or(SpinProfileSelection::TSpins);
    if !hold_enabled && hold_piece.is_some_and(|piece| piece.is_some()) {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "--no-hold cannot be combined with an occupied --hold slot",
        ));
    }
    validate_worker_options(
        workers,
        automatic_worker_limit,
        use_all_logical_processors,
        worker_hardware_limit,
    )?;

    let allow_backend_fallback = backend_fallback.resolve(backend);
    let mut input = WebBuildProbabilityInput::from_words(
        base_mask.ok_or_else(|| missing_build_probability_option("--base-mask"))?,
        target_mask.ok_or_else(|| missing_build_probability_option("--target-mask"))?,
        height.ok_or_else(|| missing_build_probability_option("--height"))?,
    )
    .with_hold_piece(hold_piece.unwrap_or(None))
    .with_allow_hold(hold_enabled)
    .with_horizontal_mirror_included(include_horizontal_mirror)
    .with_aggregation(aggregation)
    .with_result_mode(result_mode)
    .with_failed_pattern_limit(failed_pattern_limit)
    .with_finesse(finesse_metric, finesse_pattern_knowledge);
    if let Some(source_piece_count) = source_piece_count {
        input = input.with_source_piece_count(source_piece_count);
    }
    let mut request = WebCommandRequest::build_probability(input)
        .with_backend(backend)
        .with_allow_backend_fallback(allow_backend_fallback)
        .with_rule(rule)
        .with_worker_hardware_limit(worker_hardware_limit)
        .with_hold_enabled(hold_enabled)
        .with_use_all_logical_processors(use_all_logical_processors)
        .with_cpu_warmup(cpu_warmup)
        .with_precompute_build_dependencies(precompute_build_dependencies)
        .with_solution_probabilities(solution_probabilities)
        .with_queue_observation_policy(queue_knowledge);
    if matches!(
        result_mode,
        BuildProbabilityResultMode::CompleteReplayPaths
            | BuildProbabilityResultMode::FieldAverageScore
            | BuildProbabilityResultMode::FixedQueueMaximumScore
            | BuildProbabilityResultMode::HighestScoreMinimumSet
    ) {
        request = request.with_objective(
            ObjectivePolicy::unique()
                .with_score_summary()
                .with_score_profile(score_profile)
                .with_initial_b2b(u32::from(initial_b2b)),
        );
    } else if result_mode == BuildProbabilityResultMode::FailedQueues {
        request = request.with_objective(ObjectivePolicy::unique());
    } else if tiling_only {
        request = request.with_objective(ObjectivePolicy::tiling());
    } else if preserve_back_to_back {
        request = request.with_objective(
            ObjectivePolicy::unique().with_back_to_back_preservation(constraint_profile),
        );
    }
    if let Some(queue) = queue {
        request = request.with_queue(queue);
    }
    if let Some(patterns) = patterns {
        request = request.with_patterns(patterns);
    }
    if let Some(source_piece_count) = source_piece_count {
        request = request.with_source_piece_count(source_piece_count);
    }
    if let Some(max_patterns) = max_patterns {
        request = request.with_max_patterns(max_patterns);
    }
    if let Some(max_candidates) = max_candidates {
        request = request.with_max_candidates(max_candidates);
    }
    if let Some(max_memory_mib) = max_memory_mib {
        request = request.with_max_memory_mib(max_memory_mib);
    }
    if let Some(workers) = workers {
        request = request.with_workers(workers);
    } else if let Some(workers) = automatic_worker_limit {
        request = request.with_automatic_worker_limit(workers);
    }
    Ok(request)
}

fn parse_verify_command(tokens: &[String]) -> Result<WebCommandRequest, WebCommandError> {
    match tokens {
        [] => Ok(WebCommandRequest::verify(None)),
        [scope] if matches!(scope.as_str(), "pc" | "setup" | "cover" | "build" | "kicks") => {
            Ok(WebCommandRequest::verify(Some(scope.clone())))
        }
        [scope] => Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!("invalid verify scope '{scope}'"),
        )),
        _ => Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "verify accepts at most one scope",
        )),
    }
}

fn parse_utility_command(tokens: &[String]) -> Result<WebCommandRequest, WebCommandError> {
    let subcommand = match tokens.first().map(String::as_str) {
        Some(
            value @ ("sequence"
            | "sequence-dependencies"
            | "parity"
            | "fumen"
            | "render"
            | "to-gray"
            | "mirror"),
        ) => value,
        _ => {
            return Err(WebCommandError::new(
                WebCommandErrorCode::UnsupportedCommand,
                "utility requires sequence, sequence-dependencies, parity, fumen, render, to-gray, or mirror",
            ));
        }
    };
    match subcommand {
        "sequence" | "sequence-dependencies" => {
            parse_operation_document_utility(subcommand, &tokens[1..])
        }
        "parity" => parse_parity_utility(&tokens[1..]),
        "fumen" => parse_fumen_utility(&tokens[1..]),
        "render" => parse_render_utility(&tokens[1..]),
        "to-gray" | "mirror" => parse_field_document_transform_utility(subcommand, &tokens[1..]),
        _ => unreachable!("subcommand was closed above"),
    }
}

fn parse_operation_document_utility(
    subcommand: &str,
    tokens: &[String],
) -> Result<WebCommandRequest, WebCommandError> {
    let mut document = None::<String>;
    let mut rule_profile = None::<String>;
    let mut kick_profile = None::<String>;
    let mut timeout_seconds = None::<u16>;
    let mut cursor = 0_usize;
    while cursor < tokens.len() {
        let option = tokens[cursor].as_str();
        let target = match option {
            "--document" => &mut document,
            "--rule-profile" => &mut rule_profile,
            "--kick-profile" => &mut kick_profile,
            "--timeout-seconds" => {
                if timeout_seconds.is_some() {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("utility {subcommand} repeats --timeout-seconds"),
                    ));
                }
                let value = next_value(tokens, &mut cursor, option)?;
                timeout_seconds = Some(value.parse::<u16>().map_err(|_| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!(
                            "utility {subcommand} timeout-seconds must be an integer in 1..=900"
                        ),
                    )
                })?);
                continue;
            }
            flag if flag.starts_with("--") => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("utility {subcommand} does not accept {flag}"),
                ));
            }
            value => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("unexpected utility {subcommand} token '{value}'"),
                ));
            }
        };
        if target.is_some() {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!("utility {subcommand} repeats {option}"),
            ));
        }
        *target = Some(next_value(tokens, &mut cursor, option)?.to_owned());
    }
    let document = document.ok_or_else(|| {
        WebCommandError::new(
            WebCommandErrorCode::MissingValue,
            format!("utility {subcommand} requires --document <CTK3_OR_FUMEN>"),
        )
    })?;
    if subcommand == "sequence" {
        crate::operation_sequence_request_from_document(
            &document,
            rule_profile.as_deref(),
            kick_profile.as_deref(),
            timeout_seconds,
        )
    } else {
        crate::sequence_dependencies_request_from_document(
            &document,
            rule_profile.as_deref(),
            kick_profile.as_deref(),
            timeout_seconds,
        )
    }
}

fn parse_parity_utility(tokens: &[String]) -> Result<WebCommandRequest, WebCommandError> {
    let (format, document) = parse_single_typed_document_options("parity", tokens, &[])?;
    let command = ParityAppCommand::new(format, document)
        .map_err(|error| invalid_utility("parity", error))?;
    Ok(WebCommandRequest::parity(command))
}

fn parse_field_document_transform_utility(
    subcommand: &str,
    tokens: &[String],
) -> Result<WebCommandRequest, WebCommandError> {
    let (format, document) = parse_single_typed_document_options(subcommand, tokens, &[])?;
    let transform = FieldDocumentTransformKind::parse(subcommand)
        .map_err(|error| invalid_utility(subcommand, error))?;
    let command = FieldDocumentTransformAppCommand::new(transform, format, document)
        .map_err(|error| invalid_utility(subcommand, error))?;
    Ok(WebCommandRequest::field_document_transform(command))
}

fn parse_fumen_utility(tokens: &[String]) -> Result<WebCommandRequest, WebCommandError> {
    let transform = tokens.first().ok_or_else(|| {
        WebCommandError::new(
            WebCommandErrorCode::MissingValue,
            "utility fumen requires a closed transform",
        )
    })?;
    let transform =
        FumenTransformKind::parse(transform).map_err(|error| invalid_utility("fumen", error))?;
    let mut format = None;
    let mut documents = Vec::new();
    let mut page_number = None;
    let mut page_shift = None;
    let mut comments = Vec::new();
    let mut cursor = 1_usize;
    while cursor < tokens.len() {
        let option = tokens[cursor].as_str();
        match option {
            "--format" => set_unique_string(
                &mut format,
                next_value(tokens, &mut cursor, option)?,
                "utility fumen repeats --format",
            )?,
            "--document" => documents.push(next_value(tokens, &mut cursor, option)?.to_owned()),
            "--page" => {
                if page_number.is_some() {
                    return Err(invalid_web("utility fumen repeats --page"));
                }
                page_number = Some(parse_usize(
                    next_value(tokens, &mut cursor, option)?,
                    "utility fumen page must be a positive integer",
                )?);
            }
            "--offset" => {
                if page_shift.is_some() {
                    return Err(invalid_web("utility fumen repeats --offset"));
                }
                page_shift = Some(
                    next_value(tokens, &mut cursor, option)?
                        .parse::<isize>()
                        .map_err(|_| {
                            invalid_web("utility fumen offset must be a signed integer")
                        })?,
                );
            }
            "--comment" => comments.push(next_value(tokens, &mut cursor, option)?.to_owned()),
            flag if flag.starts_with("--") => {
                return Err(invalid_web(format!("utility fumen does not accept {flag}")))
            }
            value => {
                return Err(invalid_web(format!(
                    "unexpected utility fumen token '{value}'"
                )))
            }
        }
    }
    let format = required_format(format, "fumen")?;
    let command = FumenAppCommand::new(
        format,
        transform,
        documents,
        page_number,
        page_shift,
        comments,
    )
    .map_err(|error| invalid_utility("fumen", error))?;
    Ok(WebCommandRequest::fumen(command))
}

fn parse_render_utility(tokens: &[String]) -> Result<WebCommandRequest, WebCommandError> {
    let mut format = None;
    let mut document = None;
    let mut artifact_format = None;
    let mut page_number = None;
    let mut cursor = 0_usize;
    while cursor < tokens.len() {
        let option = tokens[cursor].as_str();
        match option {
            "--format" => set_unique_string(
                &mut format,
                next_value(tokens, &mut cursor, option)?,
                "utility render repeats --format",
            )?,
            "--document" => set_unique_string(
                &mut document,
                next_value(tokens, &mut cursor, option)?,
                "utility render repeats --document",
            )?,
            "--artifact-format" => set_unique_string(
                &mut artifact_format,
                next_value(tokens, &mut cursor, option)?,
                "utility render repeats --artifact-format",
            )?,
            "--page" => {
                if page_number.is_some() {
                    return Err(invalid_web("utility render repeats --page"));
                }
                page_number = Some(parse_usize(
                    next_value(tokens, &mut cursor, option)?,
                    "utility render page must be a positive integer",
                )?);
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_web(format!(
                    "utility render does not accept {flag}"
                )))
            }
            value => {
                return Err(invalid_web(format!(
                    "unexpected utility render token '{value}'"
                )))
            }
        }
    }
    let format = required_format(format, "render")?;
    let document = document.ok_or_else(|| {
        WebCommandError::new(
            WebCommandErrorCode::MissingValue,
            "utility render requires --document",
        )
    })?;
    let artifact_format =
        RenderArtifactFormat::parse(artifact_format.as_deref().ok_or_else(|| {
            WebCommandError::new(
                WebCommandErrorCode::MissingValue,
                "utility render requires --artifact-format png|gif",
            )
        })?)
        .map_err(|error| invalid_utility("render", error))?;
    let command = RenderAppCommand::new(format, document, artifact_format, page_number)
        .map_err(|error| invalid_utility("render", error))?;
    Ok(WebCommandRequest::render(command))
}

fn parse_single_typed_document_options(
    command: &str,
    tokens: &[String],
    _extra: &[&str],
) -> Result<(FieldDocumentFormat, String), WebCommandError> {
    let mut format = None;
    let mut document = None;
    let mut cursor = 0_usize;
    while cursor < tokens.len() {
        let option = tokens[cursor].as_str();
        match option {
            "--format" => set_unique_string(
                &mut format,
                next_value(tokens, &mut cursor, option)?,
                &format!("utility {command} repeats --format"),
            )?,
            "--document" => set_unique_string(
                &mut document,
                next_value(tokens, &mut cursor, option)?,
                &format!("utility {command} repeats --document"),
            )?,
            flag if flag.starts_with("--") => {
                return Err(invalid_web(format!(
                    "utility {command} does not accept {flag}"
                )))
            }
            value => {
                return Err(invalid_web(format!(
                    "unexpected utility {command} token '{value}'"
                )))
            }
        }
    }
    Ok((
        required_format(format, command)?,
        document.ok_or_else(|| {
            WebCommandError::new(
                WebCommandErrorCode::MissingValue,
                format!("utility {command} requires --document"),
            )
        })?,
    ))
}

fn required_format(
    format: Option<String>,
    command: &str,
) -> Result<FieldDocumentFormat, WebCommandError> {
    let format = format.ok_or_else(|| {
        WebCommandError::new(
            WebCommandErrorCode::MissingValue,
            format!("utility {command} requires --format ctk3|fumen"),
        )
    })?;
    FieldDocumentFormat::parse(&format).map_err(|error| invalid_utility(command, error))
}

fn set_unique_string(
    target: &mut Option<String>,
    value: &str,
    repeated_message: &str,
) -> Result<(), WebCommandError> {
    if target.is_some() {
        return Err(invalid_web(repeated_message));
    }
    *target = Some(value.to_owned());
    Ok(())
}

fn parse_usize(value: &str, message: &str) -> Result<usize, WebCommandError> {
    value.parse::<usize>().map_err(|_| invalid_web(message))
}

fn invalid_utility(command: &str, error: impl core::fmt::Display) -> WebCommandError {
    invalid_web(format!("invalid utility {command} request: {error}"))
}

fn invalid_web(message: impl Into<String>) -> WebCommandError {
    WebCommandError::new(WebCommandErrorCode::InvalidValue, message)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PcAllSpinCommandKind {
    ExactQueue,
    PatternChance,
}

impl PcAllSpinCommandKind {
    const fn name(self) -> &'static str {
        match self {
            Self::ExactQueue => "allspin-sol",
            Self::PatternChance => "allspin-pres-chance",
        }
    }

    const fn product_capability_contract(self) -> clearra_app::ProductCapabilityContract {
        match self {
            Self::ExactQueue => clearra_app::ProductCapabilityContract::PcAllSpinSolution,
            Self::PatternChance => {
                clearra_app::ProductCapabilityContract::PcAllSpinPreservationChance
            }
        }
    }
}

fn parse_pc_allspin_command(
    tokens: &[String],
    worker_hardware_limit: usize,
    kind: PcAllSpinCommandKind,
) -> Result<WebCommandRequest, WebCommandError> {
    let mut seen = Vec::new();
    let mut required_input_supplied = false;
    let mut spin_profile = None;
    let mut board_mask_supplied = false;
    let mut visible_height = None;
    let mut pieces_supplied = false;
    let mut lines = None;
    let mut cursor = 0_usize;

    while cursor < tokens.len() {
        let option = tokens[cursor].as_str();
        match option {
            "--queue" if kind == PcAllSpinCommandKind::ExactQueue => {
                next_unique_pc_allspin_value(
                    tokens,
                    &mut cursor,
                    &mut seen,
                    "required-input",
                    option,
                )?;
                required_input_supplied = true;
            }
            "--patterns" | "--pattern" if kind == PcAllSpinCommandKind::PatternChance => {
                next_unique_pc_allspin_value(
                    tokens,
                    &mut cursor,
                    &mut seen,
                    "required-input",
                    option,
                )?;
                required_input_supplied = true;
            }
            "--queue" | "--patterns" | "--pattern" => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!(
                        "pc {} does not accept {option}; exact solution search requires --queue and preservation chance requires --patterns",
                        kind.name()
                    ),
                ));
            }
            "--spin-profile" => {
                let value = next_unique_pc_allspin_value(
                    tokens,
                    &mut cursor,
                    &mut seen,
                    "spin-profile",
                    option,
                )?;
                spin_profile = Some(parse_canonical_pc_allspin_profile(value).ok_or_else(|| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!(
                            "invalid --spin-profile value '{value}'; pc All-Spin requires one explicit canonical six-profile value"
                        ),
                    )
                })?);
            }
            "--board-mask" => {
                next_unique_pc_allspin_value(
                    tokens,
                    &mut cursor,
                    &mut seen,
                    "initial-board-mask",
                    option,
                )?;
                board_mask_supplied = true;
            }
            "--height" => {
                let value = next_unique_pc_allspin_value(
                    tokens,
                    &mut cursor,
                    &mut seen,
                    "initial-height",
                    option,
                )?;
                let height = parse_positive::<u16>(value, option)?;
                if !(1..=6).contains(&height) {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "pc All-Spin initial --height must be in 1..=6 for a 10-column u64 board",
                    ));
                }
                visible_height = Some(height);
            }
            "--pieces" => {
                let value = next_unique_pc_allspin_value(
                    tokens,
                    &mut cursor,
                    &mut seen,
                    "initial-pieces",
                    option,
                )?;
                parse_positive::<usize>(value, option)?;
                pieces_supplied = true;
            }
            "--lines" => {
                let value =
                    next_unique_pc_allspin_value(tokens, &mut cursor, &mut seen, "lines", option)?;
                lines = Some(parse_positive::<u8>(value, option)?);
            }
            "--rule"
            | "--backend"
            | "--gpu-device"
            | "--workers"
            | "--auto-workers"
            | "--max-patterns"
            | "--max-nodes"
            | "--max-frontier-states"
            | "--max-candidates"
            | "--max-memory-mib" => {
                let canonical = match option {
                    "--rule" => "rule",
                    "--backend" => "backend",
                    "--gpu-device" => "gpu-device",
                    "--workers" => "workers",
                    "--auto-workers" => "auto-workers",
                    "--max-patterns" => "max-patterns",
                    "--max-nodes" => "max-nodes",
                    "--max-frontier-states" => "max-frontier-states",
                    "--max-candidates" => "max-candidates",
                    "--max-memory-mib" => "max-memory-mib",
                    _ => unreachable!("matched PC All-Spin value option"),
                };
                next_unique_pc_allspin_value(tokens, &mut cursor, &mut seen, canonical, option)?;
            }
            "--no-hold" | "--use-all-cpu-threads" | "--cpu-warmup" | "--gpu-warmup" => {
                let canonical = match option {
                    "--no-hold" => "hold-policy",
                    "--use-all-cpu-threads" => "logical-processors",
                    "--cpu-warmup" => "cpu-warmup",
                    "--gpu-warmup" => "gpu-warmup",
                    _ => unreachable!("matched PC All-Spin flag"),
                };
                record_pc_allspin_option(&mut seen, canonical, option)?;
                cursor += 1;
            }
            "--tablebase" | "--tb" | "--no-tablebase" | "--no-tb" => {
                record_pc_allspin_option(&mut seen, "tablebase-policy", option)?;
                cursor += 1;
            }
            "--build-dependency-dag" | "--no-build-dependency-dag" => {
                record_pc_allspin_option(&mut seen, "dependency-dag-policy", option)?;
                cursor += 1;
            }
            "--allow-backend-fallback" | "--no-backend-fallback" => {
                record_pc_allspin_option(&mut seen, "backend-fallback-policy", option)?;
                cursor += 1;
            }
            "--hold"
            | "--source-pieces"
            | "--count"
            | "--objective"
            | "--tiling-only"
            | "--score"
            | "--score-profile"
            | "--initial-b2b"
            | "--retained-traces"
            | "--solution-probabilities"
            | "--queue-knowledge"
            | "--preserve-b2b"
            | "--solution-fumen"
            | "--input"
            | "--file"
            | "--fixture"
            | "--output" => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("pc {} does not accept {option}", kind.name()),
                ));
            }
            flag if flag.starts_with("--") => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::UnsupportedCommand,
                    format!("unsupported pc {} option '{flag}'", kind.name()),
                ));
            }
            value => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("unexpected pc {} token '{value}'", kind.name()),
                ));
            }
        }
    }

    if !required_input_supplied {
        let option = match kind {
            PcAllSpinCommandKind::ExactQueue => "--queue",
            PcAllSpinCommandKind::PatternChance => "--patterns",
        };
        return Err(WebCommandError::new(
            WebCommandErrorCode::MissingValue,
            format!("pc {} requires exactly one {option}", kind.name()),
        ));
    }
    let spin_profile = spin_profile.ok_or_else(|| {
        WebCommandError::new(
            WebCommandErrorCode::MissingValue,
            format!(
                "pc {} requires exactly one explicit --spin-profile",
                kind.name()
            ),
        )
    })?;

    let scenario_option_count = usize::from(board_mask_supplied)
        + usize::from(visible_height.is_some())
        + usize::from(pieces_supplied);
    if !matches!(scenario_option_count, 0 | 3) {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "pc All-Spin initial field requires --board-mask, --height, and --pieces together",
        ));
    }
    if scenario_option_count == 3 {
        if let Some(lines) = lines {
            if Some(u16::from(lines)) != visible_height {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    "pc All-Spin --lines must equal initial-field --height when both are supplied",
                ));
            }
        }
    } else if lines.is_some_and(|lines| !matches!(lines, 2 | 4 | 6)) {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "opening PC All-Spin --lines must be 2, 4, or 6",
        ));
    }

    // The public forms intentionally cannot request preservation themselves.
    // Inject the fixed existential B2B constraint only after every public
    // option has passed the dedicated fail-closed preflight.
    let mut forwarded = tokens.to_vec();
    if scenario_option_count == 3 && lines.is_none() {
        forwarded.push("--lines".to_owned());
        forwarded.push(
            visible_height
                .expect("complete scenario trio requires a parsed height")
                .to_string(),
        );
    }
    forwarded.push("--preserve-b2b".to_owned());
    parse_pc_command(&forwarded, worker_hardware_limit, false).map(|request| {
        request.with_pc_allspin_product_capability(kind.product_capability_contract(), spin_profile)
    })
}

fn parse_canonical_pc_allspin_profile(value: &str) -> Option<SpinProfileSelection> {
    match value {
        "t-spins" => Some(SpinProfileSelection::TSpins),
        "t-spins-plus" => Some(SpinProfileSelection::TSpinsPlus),
        "all-spin" => Some(SpinProfileSelection::AllSpin),
        "all-spin-plus" => Some(SpinProfileSelection::AllSpinPlus),
        "all-mini" => Some(SpinProfileSelection::AllMini),
        "all-mini-plus" => Some(SpinProfileSelection::AllMiniPlus),
        _ => None,
    }
}

fn next_unique_pc_allspin_value<'a>(
    tokens: &'a [String],
    cursor: &mut usize,
    seen: &mut Vec<&'static str>,
    canonical: &'static str,
    option: &str,
) -> Result<&'a str, WebCommandError> {
    record_pc_allspin_option(seen, canonical, option)?;
    next_value(tokens, cursor, option)
}

fn record_pc_allspin_option(
    seen: &mut Vec<&'static str>,
    canonical: &'static str,
    option: &str,
) -> Result<(), WebCommandError> {
    if seen.contains(&canonical) {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!("pc All-Spin option {option} duplicates an existing {canonical} selection"),
        ));
    }
    seen.push(canonical);
    Ok(())
}

fn parse_pc_command(
    tokens: &[String],
    worker_hardware_limit: usize,
    failed_queue_requested: bool,
) -> Result<WebCommandRequest, WebCommandError> {
    let mut lines = 4u8;
    let mut backend = RequestedSearchBackend::Auto;
    let mut gpu_device = GpuDeviceSelection::Auto;
    let mut backend_fallback = BackendFallbackOverride::default();
    let mut queue: Option<String> = None;
    let mut patterns: Option<String> = None;
    let mut board_mask: Option<u64> = None;
    let mut visible_height: Option<u16> = None;
    let mut piece_window: Option<usize> = None;
    let mut hold_piece: Option<Option<PieceKind>> = None;
    let mut hold_enabled = true;
    let mut source_piece_count: Option<usize> = None;
    let mut count_policy = PcCountPolicy::CountUnique;
    let mut objective: Option<ObjectivePolicy> = None;
    let mut tiling_only_flag = false;
    let mut score_requested = false;
    let mut score_profile = None;
    let mut spin_profile = None;
    let mut preserve_back_to_back = false;
    let mut rule = srs_plus();
    let mut rule_requested = false;
    let mut initial_b2b: Option<u32> = None;
    let mut retained_trace_limit = 1usize;
    let mut max_patterns: Option<usize> = None;
    let mut max_nodes: Option<usize> = None;
    let mut max_frontier_states: Option<usize> = None;
    let mut max_candidates: Option<usize> = None;
    let mut max_memory_mib: Option<u64> = None;
    let mut workers: Option<usize> = None;
    let mut automatic_worker_limit: Option<usize> = None;
    let mut use_all_logical_processors = false;
    let mut cpu_warmup = false;
    let mut gpu_warmup = false;
    let mut tablebase_requested = false;
    let mut precompute_build_dependencies = false;
    let mut solution_probabilities = false;
    let mut queue_observation_policy = QueueObservationPolicy::default();
    let mut virtual_files = Vec::new();
    let mut allowed_colored_solution_identities = None;
    let mut failed_pattern_limit = usize::MAX;
    let mut cursor = 0usize;

    while cursor < tokens.len() {
        match tokens[cursor].as_str() {
            "--lines" => {
                let value = next_value(tokens, &mut cursor, "--lines")?;
                lines = value.parse::<u8>().map_err(|_| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --lines value '{value}'"),
                    )
                })?;
            }
            "--backend" => {
                let value = next_value(tokens, &mut cursor, "--backend")?;
                backend = RequestedSearchBackend::parse(value).ok_or_else(|| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --backend value '{value}'"),
                    )
                })?;
            }
            "--gpu-device" => {
                let value = next_value(tokens, &mut cursor, "--gpu-device")?;
                gpu_device = GpuDeviceSelection::parse(value).ok_or_else(|| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --gpu-device value '{value}'"),
                    )
                })?;
            }
            "--queue" => {
                let value = next_value(tokens, &mut cursor, "--queue")?;
                queue = Some(value.to_owned());
            }
            "--patterns" | "--pattern" => {
                let option = tokens[cursor].clone();
                let value = next_value(tokens, &mut cursor, option.as_str())?;
                patterns = Some(value.to_owned());
            }
            "--board-mask" => {
                let value = next_value(tokens, &mut cursor, "--board-mask")?;
                board_mask = Some(parse_u64(value, "--board-mask")?);
            }
            "--height" => {
                let value = next_value(tokens, &mut cursor, "--height")?;
                visible_height = Some(parse_positive(value, "--height")?);
            }
            "--pieces" => {
                let value = next_value(tokens, &mut cursor, "--pieces")?;
                piece_window = Some(parse_positive(value, "--pieces")?);
            }
            "--hold" => {
                let value = next_value(tokens, &mut cursor, "--hold")?;
                hold_piece = Some(parse_hold_piece(value)?);
                hold_enabled = true;
            }
            "--no-hold" => {
                hold_enabled = false;
                cursor += 1;
            }
            "--source-pieces" => {
                let value = next_value(tokens, &mut cursor, "--source-pieces")?;
                source_piece_count = Some(parse_positive(value, "--source-pieces")?);
            }
            "--count" => {
                count_policy = match next_value(tokens, &mut cursor, "--count")? {
                    "all" | "count-all" => PcCountPolicy::CountAll,
                    "unique" | "count-unique" => PcCountPolicy::CountUnique,
                    value => {
                        return Err(WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            format!("invalid --count value '{value}'"),
                        ))
                    }
                };
            }
            "--objective" => {
                let value = next_value(tokens, &mut cursor, "--objective")?;
                objective = Some(parse_objective(value)?);
            }
            "--tiling-only" => {
                tiling_only_flag = true;
                cursor += 1;
            }
            "--score" => {
                score_requested = true;
                cursor += 1;
            }
            "--score-profile" => {
                let value = next_value(tokens, &mut cursor, "--score-profile")?;
                score_profile = Some(ScoreProfileSelection::parse(value).ok_or_else(|| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --score-profile value '{value}'"),
                    )
                })?);
                score_requested = true;
            }
            "--spin-profile" => {
                let value = next_value(tokens, &mut cursor, "--spin-profile")?;
                spin_profile = Some(SpinProfileSelection::parse(value).ok_or_else(|| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --spin-profile value '{value}'"),
                    )
                })?);
            }
            "--preserve-b2b" => {
                preserve_back_to_back = true;
                cursor += 1;
            }
            "--rule" => {
                rule = parse_rule_profile(next_value(tokens, &mut cursor, "--rule")?)?;
                rule_requested = true;
            }
            "--initial-b2b" => {
                let value = next_value(tokens, &mut cursor, "--initial-b2b")?;
                initial_b2b = Some(u32::from(value.parse::<u16>().map_err(|_| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --initial-b2b value '{value}'"),
                    )
                })?));
            }
            "--retained-traces" => {
                let value = next_value(tokens, &mut cursor, "--retained-traces")?;
                retained_trace_limit = parse_positive(value, "--retained-traces")?;
            }
            "--max-patterns" => {
                let value = next_value(tokens, &mut cursor, "--max-patterns")?;
                max_patterns = Some(parse_positive(value, "--max-patterns")?);
            }
            "--max-nodes" => {
                let value = next_value(tokens, &mut cursor, "--max-nodes")?;
                max_nodes = Some(parse_positive(value, "--max-nodes")?);
            }
            "--max-frontier-states" => {
                let value = next_value(tokens, &mut cursor, "--max-frontier-states")?;
                max_frontier_states = Some(parse_positive(value, "--max-frontier-states")?);
            }
            "--max-candidates" => {
                let value = next_value(tokens, &mut cursor, "--max-candidates")?;
                max_candidates = Some(parse_positive(value, "--max-candidates")?);
            }
            "--max-memory-mib" => {
                let value = next_value(tokens, &mut cursor, "--max-memory-mib")?;
                max_memory_mib = Some(parse_positive(value, "--max-memory-mib")?);
            }
            "--workers" => {
                let value = next_value(tokens, &mut cursor, "--workers")?;
                workers = Some(parse_positive(value, "--workers")?);
            }
            "--auto-workers" => {
                let value = next_value(tokens, &mut cursor, "--auto-workers")?;
                automatic_worker_limit = Some(parse_positive(value, "--auto-workers")?);
            }
            "--use-all-cpu-threads" => {
                use_all_logical_processors = true;
                cursor += 1;
            }
            "--cpu-warmup" => {
                cpu_warmup = true;
                cursor += 1;
            }
            "--gpu-warmup" => {
                gpu_warmup = true;
                cursor += 1;
            }
            "--tablebase" | "--tb" => {
                tablebase_requested = true;
                cursor += 1;
            }
            "--no-tablebase" | "--no-tb" => {
                tablebase_requested = false;
                cursor += 1;
            }
            "--build-dependency-dag" => {
                precompute_build_dependencies = true;
                cursor += 1;
            }
            "--no-build-dependency-dag" => {
                precompute_build_dependencies = false;
                cursor += 1;
            }
            "--solution-probabilities" => {
                solution_probabilities = true;
                cursor += 1;
            }
            "--queue-knowledge" => {
                let value = next_value(tokens, &mut cursor, "--queue-knowledge")?;
                queue_observation_policy =
                    QueueObservationPolicy::from_keyword(value).ok_or_else(|| {
                        WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            format!(
                                "invalid queue knowledge '{value}'; expected oracle or visible-7"
                            ),
                        )
                    })?;
            }
            "--allow-backend-fallback" => {
                backend_fallback.record(true)?;
                cursor += 1;
            }
            "--no-backend-fallback" => {
                backend_fallback.record(false)?;
                cursor += 1;
            }
            "--input" | "--file" | "--fixture" => {
                let option = tokens[cursor].clone();
                let value = next_value(tokens, &mut cursor, option.as_str())?;
                let handle =
                    WebVirtualFileHandle::new("browser-input", value, "application/json", 0)?;
                virtual_files.push(handle);
            }
            "--output" => {
                let value = next_value(tokens, &mut cursor, "--output")?;
                reject_native_path_semantics(value)?;
            }
            "--solution-fumen" => {
                if allowed_colored_solution_identities.is_some() {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "solution identity input may be specified only once",
                    ));
                }
                let value = next_value(tokens, &mut cursor, "--solution-fumen")?;
                let solutions = SourceFumenColoredFieldSet::decode(value).map_err(|error| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid supplied solution Fumen: {error:?}"),
                    )
                })?;
                allowed_colored_solution_identities = Some(solutions.identities().to_vec());
            }
            "--failed-count" | "--limit" if failed_queue_requested => {
                let option = tokens[cursor].clone();
                let value = next_value(tokens, &mut cursor, option.as_str())?;
                failed_pattern_limit = value.parse::<usize>().map_err(|_| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid {option} value '{value}'"),
                    )
                })?;
            }
            flag if flag.starts_with("--") => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::UnsupportedCommand,
                    format!("unsupported CLI command option '{flag}'"),
                ));
            }
            value => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("unexpected CLI command token '{value}'"),
                ));
            }
        }
    }

    if queue.is_some() && patterns.is_some() {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "--queue and --patterns are mutually exclusive",
        ));
    }
    validate_worker_options(
        workers,
        automatic_worker_limit,
        use_all_logical_processors,
        worker_hardware_limit,
    )?;
    if tiling_only_flag
        && objective.is_some_and(|policy| {
            policy.kind() != clearra_core_domain::objective::objective_kind::ObjectiveKind::Tiling
        })
    {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "--tiling-only conflicts with a non-tiling --objective",
        ));
    }
    if failed_queue_requested {
        let incompatible = [
            (tiling_only_flag, "--tiling-only"),
            (objective.is_some(), "--objective"),
            (score_requested, "--score"),
            (score_profile.is_some(), "--score-profile"),
            (initial_b2b.is_some(), "--initial-b2b"),
            (solution_probabilities, "--solution-probabilities"),
            (
                spin_profile.is_some() && !preserve_back_to_back,
                "--spin-profile without --preserve-b2b",
            ),
        ];
        if let Some((_, option)) = incompatible.into_iter().find(|(enabled, _)| *enabled) {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!("{option} is not available with failed-queue search"),
            ));
        }
        count_policy = PcCountPolicy::CountAll;
    }
    if !failed_queue_requested
        && spin_profile.is_some()
        && !score_requested
        && !preserve_back_to_back
    {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "--spin-profile requires --score or --preserve-b2b",
        ));
    }
    if !failed_queue_requested && initial_b2b.is_some() && !score_requested {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "--initial-b2b requires --score",
        ));
    }
    let mut objective = if failed_queue_requested {
        ObjectivePolicy::all()
    } else {
        objective.unwrap_or_else(|| match count_policy {
            PcCountPolicy::CountAll => ObjectivePolicy::all(),
            PcCountPolicy::FirstSolution | PcCountPolicy::CountUnique => ObjectivePolicy::unique(),
        })
    };
    if tiling_only_flag {
        objective = ObjectivePolicy::tiling();
    }
    let tiling_only =
        objective.kind() == clearra_core_domain::objective::objective_kind::ObjectiveKind::Tiling;
    if tiling_only {
        let incompatible = [
            (rule_requested, "--rule"),
            (score_requested, "--score"),
            (score_profile.is_some(), "--score-profile"),
            (spin_profile.is_some(), "--spin-profile"),
            (preserve_back_to_back, "--preserve-b2b"),
            (initial_b2b.is_some(), "--initial-b2b"),
            (tablebase_requested, "--tablebase"),
            (precompute_build_dependencies, "--build-dependency-dag"),
            (solution_probabilities, "--solution-probabilities"),
            (
                queue_observation_policy.requires_observation_policy(),
                "--queue-knowledge visible-7",
            ),
        ];
        if let Some((_, option)) = incompatible.into_iter().find(|(enabled, _)| *enabled) {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!("{option} is not available with tiling-only search"),
            ));
        }
        count_policy = PcCountPolicy::CountUnique;
    }
    if !hold_enabled && hold_piece.is_some_and(|piece| piece.is_some()) {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "--no-hold cannot be combined with an occupied --hold slot",
        ));
    }
    if score_requested && !objective.score().requested() {
        objective = objective.with_score_summary();
    }
    if let Some(initial_b2b) = initial_b2b {
        objective = objective.with_initial_b2b(initial_b2b);
    }
    if let Some(profile) = score_profile {
        objective = objective.with_score_profile(profile);
    }
    if let Some(profile) = spin_profile.filter(|_| objective.score().requested()) {
        objective = objective.with_spin_profile(profile);
    }
    if preserve_back_to_back {
        objective = objective
            .with_back_to_back_preservation(spin_profile.unwrap_or(SpinProfileSelection::TSpins));
    }
    validate_pc_observation_objective(queue_observation_policy, objective.kind()).map_err(
        |error| {
            WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!("{}: {}", error.code(), error.message()),
            )
        },
    )?;
    if objective.score().requested() {
        count_policy = PcCountPolicy::CountAll;
    }

    let allow_backend_fallback = backend_fallback.resolve(backend);
    let mut request = WebCommandRequest::pc(lines, backend)
        .with_rule(rule)
        .with_worker_hardware_limit(worker_hardware_limit)
        .with_gpu_device(gpu_device)
        .with_allow_backend_fallback(allow_backend_fallback)
        .with_use_all_logical_processors(use_all_logical_processors)
        .with_cpu_warmup(cpu_warmup)
        .with_gpu_warmup(gpu_warmup)
        .with_tablebase_requested(tablebase_requested)
        .with_precompute_build_dependencies(precompute_build_dependencies)
        .with_solution_probabilities(solution_probabilities)
        .with_queue_observation_policy(queue_observation_policy)
        .with_hold_enabled(hold_enabled)
        .with_count_policy(count_policy)
        .with_objective(objective);
    if let Some(source_piece_count) = source_piece_count {
        request = request.with_source_piece_count(source_piece_count);
    }
    if let Some(queue) = queue {
        request = request.with_queue(queue);
    }
    if let Some(patterns) = patterns {
        request = request.with_patterns(patterns);
    }
    let scenario_requested = board_mask.is_some()
        || visible_height.is_some()
        || piece_window.is_some()
        || hold_piece.is_some();
    if scenario_requested {
        let board_mask = board_mask.ok_or_else(|| missing_scenario_option("--board-mask"))?;
        let visible_height = visible_height.ok_or_else(|| missing_scenario_option("--height"))?;
        let piece_window = piece_window.ok_or_else(|| missing_scenario_option("--pieces"))?;
        let mut scenario = WebPcScenarioInput::new(board_mask, visible_height, piece_window)
            .with_hold_piece(hold_piece.unwrap_or(None))
            .with_allow_hold(hold_enabled)
            .with_count_policy(count_policy)
            .with_retained_trace_limit(retained_trace_limit);
        if let Some(identities) = allowed_colored_solution_identities {
            scenario = scenario.with_allowed_colored_solution_identities(identities);
        }
        let scenario = if let Some(source_piece_count) = source_piece_count {
            scenario.with_source_piece_count(source_piece_count)
        } else {
            scenario
        };
        request = request.with_scenario(scenario);
    }
    if let Some(max_patterns) = max_patterns {
        request = request.with_max_patterns(max_patterns);
    }
    if let Some(max_nodes) = max_nodes {
        request = request.with_max_nodes(max_nodes);
    }
    if let Some(max_frontier_states) = max_frontier_states {
        request = request.with_max_frontier_states(max_frontier_states);
    }
    if let Some(max_candidates) = max_candidates {
        request = request.with_max_candidates(max_candidates);
    }
    if let Some(max_memory_mib) = max_memory_mib {
        request = request.with_max_memory_mib(max_memory_mib);
    }
    if let Some(workers) = workers {
        request = request.with_workers(workers);
    } else if let Some(workers) = automatic_worker_limit {
        request = request.with_automatic_worker_limit(workers);
    }
    for file in virtual_files {
        request = request.with_virtual_file(file);
    }
    if failed_queue_requested {
        request = request.with_failed_queue_mode(failed_pattern_limit);
    }
    Ok(request)
}

fn validate_worker_options(
    workers: Option<usize>,
    automatic_worker_limit: Option<usize>,
    use_all_logical_processors: bool,
    worker_hardware_limit: usize,
) -> Result<(), WebCommandError> {
    if workers.is_some() && automatic_worker_limit.is_some() {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "--workers and --auto-workers are mutually exclusive",
        ));
    }
    let Some((option, workers)) = workers
        .map(|workers| ("--workers", workers))
        .or_else(|| automatic_worker_limit.map(|workers| ("--auto-workers", workers)))
    else {
        return Ok(());
    };
    let hardware_limit = worker_hardware_limit.max(1);
    if workers > hardware_limit {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!(
                "{option} {workers} exceeds the hard limit of {hardware_limit} logical processors"
            ),
        ));
    }
    let default_limit = WorkerPolicy::default_worker_limit_for_hardware(hardware_limit);
    if workers > default_limit && !use_all_logical_processors {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!(
                "{option} {workers} uses the reserved logical processor; pass --use-all-cpu-threads explicitly"
            ),
        ));
    }
    Ok(())
}

fn parse_rule_profile(value: &str) -> Result<RuleProfile, WebCommandError> {
    let normalized = value.trim().to_ascii_lowercase().replace('_', "-");
    RuleProfileId::parse(&normalized)
        .map(RuleProfile::new)
        .ok_or_else(|| {
            WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!("invalid --rule value '{value}'"),
            )
        })
}

fn parse_objective(value: &str) -> Result<ObjectivePolicy, WebCommandError> {
    match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
        "all" => Ok(ObjectivePolicy::all()),
        "unique" => Ok(ObjectivePolicy::unique()),
        "minimum-cover" | "min-cover" => Ok(ObjectivePolicy::minimum_cover()),
        "tiling" => Ok(ObjectivePolicy::tiling()),
        _ => Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!("invalid --objective value '{value}'"),
        )),
    }
}

fn parse_u64(value: &str, option: &str) -> Result<u64, WebCommandError> {
    let parsed = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"));
    let result = match parsed {
        Some(hex) => u64::from_str_radix(hex, 16),
        None => value.parse::<u64>(),
    };
    result.map_err(|_| {
        WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!("invalid {option} value '{value}'"),
        )
    })
}

fn parse_nonnegative_usize(value: &str, option: &str) -> Result<usize, WebCommandError> {
    value.parse::<usize>().map_err(|_| {
        WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!("invalid {option} value '{value}'"),
        )
    })
}

fn parse_board_words(value: &str, option: &str) -> Result<[u64; 4], WebCommandError> {
    let invalid = || {
        WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!("invalid {option} value '{value}'"),
        )
    };
    let prefixed_hex = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"));
    // Discord's canonical 10 x 24 field contract is a fixed-width 240-bit
    // hexadecimal value without a prefix. Keep shorter unprefixed values on
    // the established decimal path so existing CLI commands remain stable.
    let canonical_field_hex =
        (value.len() == 60 && value.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(value);
    if let Some(hex) = prefixed_hex.or(canonical_field_hex) {
        if hex.is_empty() || hex.len() > 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(invalid());
        }
        let mut words = [0_u64; 4];
        for (index, chunk_end) in (0..hex.len()).rev().step_by(16).enumerate() {
            let begin = chunk_end.saturating_sub(15);
            words[index] =
                u64::from_str_radix(&hex[begin..=chunk_end], 16).map_err(|_| invalid())?;
        }
        return Ok(words);
    }

    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid());
    }
    let mut words = [0_u64; 4];
    for digit in value.bytes().map(|byte| u64::from(byte - b'0')) {
        let mut carry = digit as u128;
        for word in &mut words {
            let next = u128::from(*word) * 10 + carry;
            *word = next as u64;
            carry = next >> 64;
        }
        if carry != 0 {
            return Err(invalid());
        }
    }
    Ok(words)
}

fn parse_positive<T>(value: &str, option: &str) -> Result<T, WebCommandError>
where
    T: std::str::FromStr + PartialEq + Default,
{
    let parsed = value.parse::<T>().map_err(|_| {
        WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!("invalid {option} value '{value}'"),
        )
    })?;
    if parsed == T::default() {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!("{option} must be positive"),
        ));
    }
    Ok(parsed)
}

fn parse_hold_piece(value: &str) -> Result<Option<PieceKind>, WebCommandError> {
    if matches!(value.to_ascii_lowercase().as_str(), "empty" | "none") {
        return Ok(None);
    }
    let mut characters = value.chars();
    let piece = characters
        .next()
        .ok_or_else(|| missing_scenario_option("--hold"))?;
    if characters.next().is_some() {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!("invalid --hold value '{value}'"),
        ));
    }
    PieceKind::from_ascii(piece).map(Some).map_err(|_| {
        WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!("invalid --hold value '{value}'"),
        )
    })
}

fn missing_scenario_option(option: &str) -> WebCommandError {
    WebCommandError::new(
        WebCommandErrorCode::MissingValue,
        format!("scenario PC requires {option}"),
    )
}

fn missing_build_probability_option(option: &str) -> WebCommandError {
    WebCommandError::new(
        WebCommandErrorCode::MissingValue,
        format!("build-probability requires {option}"),
    )
}

fn next_value<'a>(
    tokens: &'a [String],
    cursor: &mut usize,
    option: &str,
) -> Result<&'a str, WebCommandError> {
    let value_index = *cursor + 1;
    let value = tokens.get(value_index).ok_or_else(|| {
        WebCommandError::new(
            WebCommandErrorCode::MissingValue,
            format!("missing value for {option}"),
        )
    })?;
    if value.starts_with("--") {
        return Err(WebCommandError::new(
            WebCommandErrorCode::MissingValue,
            format!("missing value for {option}"),
        ));
    }
    *cursor += 2;
    Ok(value)
}

fn reject_nul_tokens(tokens: &[String]) -> Result<(), WebCommandError> {
    if tokens.iter().any(|token| token.contains('\0')) {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "CLI command values must not contain NUL",
        ));
    }
    Ok(())
}

fn tokenize(command_text: &str) -> Result<Vec<String>, WebCommandError> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut token_started = false;
    let mut quoted = false;
    let mut escaped = false;
    for character in command_text.chars() {
        if character == '\0' {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                "CLI command values must not contain NUL",
            ));
        }
        if quoted {
            if escaped {
                if !matches!(character, '"' | '\\') {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "CLI command quoted values only escape quote or backslash",
                    ));
                }
                token.push(character);
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quoted = false;
            } else {
                token.push(character);
            }
            continue;
        }
        if character == '"' {
            quoted = true;
            token_started = true;
        } else if character.is_whitespace() {
            if token_started {
                tokens.push(core::mem::take(&mut token));
                token_started = false;
            }
        } else {
            token.push(character);
            token_started = true;
        }
    }
    if quoted || escaped {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "CLI command contains an unterminated quoted value",
        ));
    }
    if token_started {
        tokens.push(token);
    }
    if tokens.is_empty() {
        return Err(WebCommandError::new(
            WebCommandErrorCode::EmptyCommand,
            "empty CLI command",
        ));
    }
    Ok(tokens)
}

fn reject_process_semantics(command_text: &str) -> Result<(), WebCommandError> {
    let mut quoted = false;
    let mut escaped = false;
    let mut token = String::new();
    let mut token_started = false;
    let mut current_is_pattern_value = false;
    let mut next_is_pattern_value = false;
    let mut characters = command_text.chars().peekable();
    while let Some(character) = characters.next() {
        if quoted {
            if escaped {
                token.push(character);
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                quoted = false;
            } else {
                token.push(character);
            }
            continue;
        }
        if character == '"' {
            if !token_started {
                token_started = true;
                current_is_pattern_value = next_is_pattern_value;
                next_is_pattern_value = false;
            }
            quoted = true;
            continue;
        }

        if matches!(character, '\r' | '\n') {
            return Err(WebCommandError::new(
                WebCommandErrorCode::ProcessSemantics,
                "web runtime does not accept shell or process control syntax",
            ));
        }
        if character.is_whitespace() {
            if token_started {
                next_is_pattern_value = is_pattern_expression_option(&token);
                token.clear();
                token_started = false;
                current_is_pattern_value = false;
            }
            continue;
        }

        if !token_started {
            token_started = true;
            current_is_pattern_value = next_is_pattern_value;
            next_is_pattern_value = false;
        }
        // A semicolon is part of Clearra's queue-pattern grammar when it joins
        // two alternatives in the value of a pattern option. It is not process
        // control there: this parser owns the text and never invokes a shell.
        // Require both adjacent alternative fragments to stay in the same raw
        // token so `--patterns P7; clearra ...` remains fail-closed.
        let pattern_alternative_separator = character == ';'
            && current_is_pattern_value
            && !token.is_empty()
            && characters
                .peek()
                .is_some_and(|next| !next.is_whitespace() && *next != '"');
        let process_semantics =
            matches!(character, '|' | '&' | ';' | '`' | '>' | '<' | '\r' | '\n')
                || (character == '$' && characters.peek() == Some(&'('));
        if process_semantics && !pattern_alternative_separator {
            return Err(WebCommandError::new(
                WebCommandErrorCode::ProcessSemantics,
                "web runtime does not accept shell or process control syntax",
            ));
        }
        token.push(character);
    }
    Ok(())
}

fn is_pattern_expression_option(token: &str) -> bool {
    matches!(
        token,
        "--patterns" | "--pattern" | "-p" | "--setup-patterns" | "--solution-patterns"
    )
}
// SRP rationale: this module has one behavior-level change reason: parsing the complete public CLI command grammar into typed requests.
