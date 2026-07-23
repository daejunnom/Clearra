use clearra_core_domain::board::standard_pc_board::Board256Mask;
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_forward_search::{
    ForwardLineClearPolicy, ForwardPieceSource, ForwardSearchMode, ForwardSearchQuery,
    ForwardSpinCategory, ForwardSpinLineRequirement, ForwardSpinTarget,
};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_objectives::policy::score_objective_policy::{
    ScoreProfileSelection, SpinProfileSelection,
};
use clearra_pc_graph::request::{
    GpuDeviceSelection, PcCountPolicy, RequestedSearchBackend, WorkerPolicy,
};
use clearra_problem::BuildProbabilityAggregation;
use clearra_rules::profile::{
    builtin_rules::srs_plus,
    rule_profile::{RuleProfile, RuleProfileId},
};
use clearra_scoring::profile::SpinProfileId;
use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

use crate::{
    web_virtual_file::reject_native_path_semantics, WebBuildProbabilityInput, WebCommandError,
    WebCommandErrorCode, WebCommandRequest, WebPcScenarioInput, WebVirtualFileHandle,
};

#[derive(Clone, Debug, Default)]
pub struct WebCommandParser;

impl WebCommandParser {
    pub fn parse(command_text: &str) -> Result<WebCommandRequest, WebCommandError> {
        Self::parse_with_worker_limit(command_text, WorkerPolicy::hardware_worker_limit())
    }

    pub fn parse_with_worker_limit(
        command_text: &str,
        worker_hardware_limit: usize,
    ) -> Result<WebCommandRequest, WebCommandError> {
        reject_process_semantics(command_text)?;
        let tokens = tokenize(command_text)?;
        let mut cursor = 0usize;

        if tokens.get(cursor).map(String::as_str) == Some("clearra") {
            cursor += 1;
        }

        let command = tokens.get(cursor).ok_or_else(|| {
            WebCommandError::new(WebCommandErrorCode::EmptyCommand, "empty web command")
        })?;
        cursor += 1;

        match command.as_str() {
            "pc" => parse_pc_command(&tokens[cursor..], worker_hardware_limit.max(1)),
            "build-probability" => {
                parse_build_probability_command(&tokens[cursor..], worker_hardware_limit.max(1))
            }
            "damage" => {
                parse_forward_command(&tokens[cursor..], false, worker_hardware_limit.max(1))
            }
            "spin-finder" => {
                parse_forward_command(&tokens[cursor..], true, worker_hardware_limit.max(1))
            }
            "verify" => parse_verify_command(&tokens[cursor..]),
            _ => Err(WebCommandError::new(
                WebCommandErrorCode::UnsupportedCommand,
                format!("unsupported web command '{command}'"),
            )),
        }
    }
}

fn parse_forward_command(
    tokens: &[String],
    spin_finder: bool,
    worker_hardware_limit: usize,
) -> Result<WebCommandRequest, WebCommandError> {
    let mut board = Board256Mask::EMPTY;
    let mut height = 8_u8;
    let mut piece_source: Option<ForwardPieceSource> = None;
    let mut hold_enabled = true;
    let mut rule = RuleProfileId::SrsPlus;
    let mut spin_profile = SpinProfileId::TSpins;
    let mut initial_combo = None;
    let mut initial_back_to_back = None;
    let mut line_clear_policy = ForwardLineClearPolicy::Any;
    let mut minimum_damage = None;
    let mut target_lines = ForwardSpinLineRequirement::Any;
    let mut target_category = ForwardSpinCategory::Any;
    let mut workers = None;
    let mut use_all_logical_processors = false;
    let mut cursor = 0_usize;
    while cursor < tokens.len() {
        match tokens[cursor].as_str() {
            "--board-mask" => {
                let value = next_value(tokens, &mut cursor, "--board-mask")?;
                board = Board256Mask::from_words(parse_board_words(value, "--board-mask")?);
            }
            "--height" => {
                height = parse_positive(next_value(tokens, &mut cursor, "--height")?, "--height")?;
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
                if expression.sequence_len() > 8 {
                    return Err(WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        "spin-finder patterns may produce at most 8 pieces; use --queue for longer fixed sequences",
                    ));
                }
                set_forward_piece_source(
                    &mut piece_source,
                    ForwardPieceSource::pattern(expression),
                )?;
            }
            "--patterns" => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::UnsupportedCommand,
                    "damage search accepts only an exact --queue",
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
            "--spin-profile" => {
                let value = next_value(tokens, &mut cursor, "--spin-profile")?;
                spin_profile = SpinProfileId::parse(value).ok_or_else(|| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --spin-profile value '{value}'"),
                    )
                })?;
            }
            "--initial-combo" => {
                initial_combo = Some(parse_positive(
                    next_value(tokens, &mut cursor, "--initial-combo")?,
                    "--initial-combo",
                )?);
            }
            "--initial-b2b" => {
                let value = next_value(tokens, &mut cursor, "--initial-b2b")?;
                let parsed = value.parse::<u16>().map_err(|_| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --initial-b2b value '{value}'"),
                    )
                })?;
                initial_back_to_back = (parsed > 0).then_some(parsed - 1);
            }
            "--preserve-b2b" => {
                line_clear_policy = ForwardLineClearPolicy::PreserveBackToBack;
                cursor += 1;
            }
            "--minimum-damage" if !spin_finder => {
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
                            "spin-finder --lines must be any, 0..4, or 0+..4+",
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
    let piece_source = piece_source.ok_or_else(|| {
        WebCommandError::new(
            WebCommandErrorCode::MissingValue,
            if spin_finder {
                "spin-finder requires --queue or --patterns"
            } else {
                "damage search requires --queue"
            },
        )
    })?;
    let mode = if spin_finder {
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
    if let Some(workers) = workers {
        if workers > worker_hardware_limit {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!(
                    "--workers {workers} exceeds the hard limit of {worker_hardware_limit} logical processors"
                ),
            ));
        }
        let default_limit = WorkerPolicy::default_worker_limit_for_hardware(worker_hardware_limit);
        if workers > default_limit && !use_all_logical_processors {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!(
                    "--workers {workers} uses the reserved logical processor; pass --use-all-cpu-threads explicitly"
                ),
            ));
        }
    }
    let mut request =
        WebCommandRequest::forward(if spin_finder { "spin-finder" } else { "damage" }, query)
            .with_worker_hardware_limit(worker_hardware_limit)
            .with_use_all_logical_processors(use_all_logical_processors);
    if let Some(workers) = workers {
        request = request.with_workers(workers);
    }
    Ok(request)
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
    let mut use_all_logical_processors = false;
    let mut cpu_warmup = false;
    let mut include_horizontal_mirror = true;
    let mut aggregation = BuildProbabilityAggregation::Buildability;
    let mut spin_profile = None;
    let mut rule = srs_plus();
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
            "--use-all-cpu-threads" => {
                use_all_logical_processors = true;
                cursor += 1;
            }
            "--cpu-warmup" => {
                cpu_warmup = true;
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
                aggregation = match next_value(tokens, &mut cursor, "--aggregate")? {
                    "buildability" | "build" => BuildProbabilityAggregation::Buildability,
                    "spin" => {
                        BuildProbabilityAggregation::spin_search(SpinProfileSelection::TSpins)
                    }
                    value => {
                        return Err(WebCommandError::new(
                            WebCommandErrorCode::InvalidValue,
                            format!(
                                "unsupported build-probability aggregation '{value}'; expected buildability or spin"
                            ),
                        ));
                    }
                };
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
            "--rule" => {
                rule = parse_rule_profile(next_value(tokens, &mut cursor, "--rule")?)?;
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
    if let Some(profile) = spin_profile {
        if aggregation.requests_spin_coverage() {
            aggregation = BuildProbabilityAggregation::spin_search(profile);
        }
    }
    if !hold_enabled && hold_piece.is_some_and(|piece| piece.is_some()) {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "--no-hold cannot be combined with an occupied --hold slot",
        ));
    }
    if let Some(workers) = workers {
        let default_limit = WorkerPolicy::default_worker_limit_for_hardware(worker_hardware_limit);
        if workers > worker_hardware_limit {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!("--workers {workers} exceeds the hard limit of {worker_hardware_limit}"),
            ));
        }
        if workers > default_limit && !use_all_logical_processors {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                "using the reserved logical processor requires --use-all-cpu-threads",
            ));
        }
    }

    let mut input = WebBuildProbabilityInput::from_words(
        base_mask.ok_or_else(|| missing_build_probability_option("--base-mask"))?,
        target_mask.ok_or_else(|| missing_build_probability_option("--target-mask"))?,
        height.ok_or_else(|| missing_build_probability_option("--height"))?,
    )
    .with_hold_piece(hold_piece.unwrap_or(None))
    .with_allow_hold(hold_enabled)
    .with_horizontal_mirror_included(include_horizontal_mirror)
    .with_aggregation(aggregation);
    if let Some(source_piece_count) = source_piece_count {
        input = input.with_source_piece_count(source_piece_count);
    }
    let mut request = WebCommandRequest::build_probability(input)
        .with_rule(rule)
        .with_worker_hardware_limit(worker_hardware_limit)
        .with_hold_enabled(hold_enabled)
        .with_use_all_logical_processors(use_all_logical_processors)
        .with_cpu_warmup(cpu_warmup);
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

fn parse_pc_command(
    tokens: &[String],
    worker_hardware_limit: usize,
) -> Result<WebCommandRequest, WebCommandError> {
    let mut lines = 2u8;
    let mut backend = RequestedSearchBackend::Auto;
    let mut gpu_device = GpuDeviceSelection::Auto;
    let mut allow_backend_fallback = true;
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
    let mut score_requested = false;
    let mut score_profile = None;
    let mut spin_profile = None;
    let mut rule = srs_plus();
    let mut initial_b2b: Option<u32> = None;
    let mut retained_trace_limit = 1usize;
    let mut max_patterns: Option<usize> = None;
    let mut max_nodes: Option<usize> = None;
    let mut max_frontier_states: Option<usize> = None;
    let mut max_candidates: Option<usize> = None;
    let mut max_memory_mib: Option<u64> = None;
    let mut workers: Option<usize> = None;
    let mut use_all_logical_processors = false;
    let mut cpu_warmup = false;
    let mut gpu_warmup = false;
    let mut solution_probabilities = false;
    let mut virtual_files = Vec::new();
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
                allow_backend_fallback = matches!(backend, RequestedSearchBackend::Auto);
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
                score_requested = true;
            }
            "--rule" => {
                rule = parse_rule_profile(next_value(tokens, &mut cursor, "--rule")?)?;
            }
            "--initial-b2b" => {
                let value = next_value(tokens, &mut cursor, "--initial-b2b")?;
                initial_b2b = Some(value.parse::<u32>().map_err(|_| {
                    WebCommandError::new(
                        WebCommandErrorCode::InvalidValue,
                        format!("invalid --initial-b2b value '{value}'"),
                    )
                })?);
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
            "--solution-probabilities" => {
                solution_probabilities = true;
                cursor += 1;
            }
            "--allow-backend-fallback" => {
                allow_backend_fallback = true;
                cursor += 1;
            }
            "--no-backend-fallback" => {
                allow_backend_fallback = false;
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
            flag if flag.starts_with("--") => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::UnsupportedCommand,
                    format!("unsupported web command option '{flag}'"),
                ));
            }
            value => {
                return Err(WebCommandError::new(
                    WebCommandErrorCode::InvalidValue,
                    format!("unexpected web command token '{value}'"),
                ));
            }
        }
    }

    if !hold_enabled && hold_piece.is_some_and(|piece| piece.is_some()) {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "--no-hold cannot be combined with an occupied --hold slot",
        ));
    }
    if queue.is_some() && patterns.is_some() {
        return Err(WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            "--queue and --patterns are mutually exclusive",
        ));
    }
    if let Some(workers) = workers {
        let hardware_limit = worker_hardware_limit.max(1);
        let default_limit = WorkerPolicy::default_worker_limit_for_hardware(hardware_limit);
        if workers > hardware_limit {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!(
                    "--workers {workers} exceeds the hard limit of {hardware_limit} logical processors"
                ),
            ));
        }
        if workers > default_limit && !use_all_logical_processors {
            return Err(WebCommandError::new(
                WebCommandErrorCode::InvalidValue,
                format!(
                    "--workers {workers} uses the reserved logical processor; pass --use-all-cpu-threads explicitly"
                ),
            ));
        }
    }
    let mut objective = objective.unwrap_or_else(|| match count_policy {
        PcCountPolicy::CountAll => ObjectivePolicy::all(),
        PcCountPolicy::FirstSolution | PcCountPolicy::CountUnique => ObjectivePolicy::unique(),
    });
    if score_requested && !objective.score().requested() {
        objective = objective.with_score_summary();
    }
    if let Some(initial_b2b) = initial_b2b {
        objective = objective.with_initial_b2b(initial_b2b);
    }
    if let Some(profile) = score_profile {
        objective = objective.with_score_profile(profile);
    }
    if let Some(profile) = spin_profile {
        objective = objective.with_spin_profile(profile);
    }
    if objective.score().requested() {
        count_policy = PcCountPolicy::CountAll;
    }

    let mut request = WebCommandRequest::pc(lines, backend)
        .with_rule(rule)
        .with_worker_hardware_limit(worker_hardware_limit)
        .with_gpu_device(gpu_device)
        .with_allow_backend_fallback(allow_backend_fallback)
        .with_use_all_logical_processors(use_all_logical_processors)
        .with_cpu_warmup(cpu_warmup)
        .with_gpu_warmup(gpu_warmup)
        .with_solution_probabilities(solution_probabilities)
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
        let scenario = WebPcScenarioInput::new(board_mask, visible_height, piece_window)
            .with_hold_piece(hold_piece.unwrap_or(None))
            .with_allow_hold(hold_enabled)
            .with_count_policy(count_policy)
            .with_retained_trace_limit(retained_trace_limit);
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
    }
    for file in virtual_files {
        request = request.with_virtual_file(file);
    }
    Ok(request)
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

fn parse_board_words(value: &str, option: &str) -> Result<[u64; 4], WebCommandError> {
    let invalid = || {
        WebCommandError::new(
            WebCommandErrorCode::InvalidValue,
            format!("invalid {option} value '{value}'"),
        )
    };
    if let Some(hex) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
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

fn tokenize(command_text: &str) -> Result<Vec<String>, WebCommandError> {
    let tokens = command_text
        .split_whitespace()
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Err(WebCommandError::new(
            WebCommandErrorCode::EmptyCommand,
            "empty web command",
        ));
    }
    Ok(tokens)
}

fn reject_process_semantics(command_text: &str) -> Result<(), WebCommandError> {
    for marker in ["|", "&&", "`", "$(", ">", "<"] {
        if command_text.contains(marker) {
            return Err(WebCommandError::new(
                WebCommandErrorCode::ProcessSemantics,
                "web runtime does not accept shell or process control syntax",
            ));
        }
    }
    Ok(())
}
// SRP rationale: this module has one behavior-level change reason: parsing the complete public web command grammar into typed requests.
