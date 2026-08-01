use super::{
    execution_backend_aliases::resolve_cpu_execution_aliases,
    has_help,
    parse_option_value::{
        option_value, parse_u32_option, parse_u8_option, parse_usize_option, unknown_option,
    },
    CliHelpTopic, CliParseError, ParsedCliCommand, PcArgs,
};
use clearra_supply::queue::queue_observation_policy::QueueObservationPolicy;

pub(crate) fn parse_pc(args: &[String]) -> Result<ParsedCliCommand, CliParseError> {
    if has_help(args) {
        return Ok(ParsedCliCommand::Help(CliHelpTopic::Pc));
    }
    parse_pc_args(args).map(ParsedCliCommand::Pc)
}

pub(crate) fn parse_pc_args(args: &[String]) -> Result<PcArgs, CliParseError> {
    let mut lines = PcArgs::default().lines();
    let mut queue = String::new();
    let mut fixed_queue = false;
    let mut hold_enabled = true;
    let mut objective: Option<String> = None;
    let mut tiling_only = false;
    let mut hold_option: Option<bool> = None;
    let mut score_requested = false;
    let mut score_profile = None;
    let mut spin_profile = None;
    let mut initial_b2b = None;
    let mut rule = None;
    let mut kick_profile_json = None;
    let mut backend = None;
    let mut workers = None;
    let mut use_all_logical_processors = None;
    let mut cpu_warmup = None;
    let mut gpu_warmup = None;
    let mut tablebase_requested = None;
    let mut precompute_build_dependencies = None;
    let mut cpu_threads = None;
    let mut no_gpu = false;
    let mut deterministic = None;
    let mut max_frontier_states = None;
    let mut max_candidates = None;
    let mut max_patterns = None;
    let mut max_memory_mib = None;
    let mut gpu_device = None;
    let mut allow_backend_fallback = None;
    let mut solution_probabilities = false;
    let mut queue_observation_policy = QueueObservationPolicy::default();
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--lines" | "-l" => {
                lines = parse_u8_option(args, index, "--lines")?;
                index += 2;
            }
            "--queue" | "-q" => {
                queue = option_value(args, index, "--queue")?.to_owned();
                index += 2;
            }
            "--fixed" | "--fixed-queue" => {
                fixed_queue = true;
                index += 1;
            }
            "--observed" | "--observed-queue" => {
                fixed_queue = false;
                index += 1;
            }
            "--hold" => {
                hold_enabled = true;
                hold_option = Some(true);
                index += 1;
            }
            "--no-hold" => {
                hold_enabled = false;
                hold_option = Some(false);
                index += 1;
            }
            "--objective" | "-o" => {
                objective = Some(option_value(args, index, "--objective")?.to_owned());
                index += 2;
            }
            "--tiling-only" => {
                tiling_only = true;
                index += 1;
            }
            "--score" => {
                score_requested = true;
                index += 1;
            }
            "--score-profile" => {
                score_profile = Some(option_value(args, index, "--score-profile")?.to_owned());
                score_requested = true;
                index += 2;
            }
            "--spin-profile" => {
                spin_profile = Some(option_value(args, index, "--spin-profile")?.to_owned());
                score_requested = true;
                index += 2;
            }
            "--initial-b2b" => {
                initial_b2b = Some(parse_u32_option(args, index, "--initial-b2b")?);
                index += 2;
            }
            "--rule" => {
                rule = Some(option_value(args, index, "--rule")?.to_owned());
                index += 2;
            }
            "--kick-profile-json" => {
                kick_profile_json =
                    Some(option_value(args, index, "--kick-profile-json")?.to_owned());
                index += 2;
            }
            "--backend" => {
                backend = Some(option_value(args, index, "--backend")?.to_owned());
                index += 2;
            }
            "--workers" => {
                workers = Some(parse_usize_option(args, index, "--workers")?);
                index += 2;
            }
            "--cpu-threads" => {
                cpu_threads = Some(parse_usize_option(args, index, "--cpu-threads")?);
                index += 2;
            }
            "--use-all-cpu-threads" => {
                use_all_logical_processors = Some(true);
                index += 1;
            }
            "--cpu-warmup" => {
                cpu_warmup = Some(true);
                index += 1;
            }
            "--gpu-warmup" => {
                gpu_warmup = Some(true);
                index += 1;
            }
            "--tablebase" | "--tb" => {
                tablebase_requested = Some(true);
                index += 1;
            }
            "--no-tablebase" | "--no-tb" => {
                tablebase_requested = Some(false);
                index += 1;
            }
            "--build-dependency-dag" => {
                precompute_build_dependencies = Some(true);
                index += 1;
            }
            "--no-build-dependency-dag" => {
                precompute_build_dependencies = Some(false);
                index += 1;
            }
            "--no-gpu" => {
                no_gpu = true;
                index += 1;
            }
            "--deterministic" => {
                deterministic = Some(true);
                index += 1;
            }
            "--max-frontier-states" => {
                max_frontier_states =
                    Some(parse_usize_option(args, index, "--max-frontier-states")?);
                index += 2;
            }
            "--max-candidates" => {
                max_candidates = Some(parse_usize_option(args, index, "--max-candidates")?);
                index += 2;
            }
            "--max-patterns" => {
                max_patterns = Some(parse_usize_option(args, index, "--max-patterns")?);
                index += 2;
            }
            "--max-memory-mib" => {
                max_memory_mib = Some(parse_usize_option(args, index, "--max-memory-mib")?);
                index += 2;
            }
            "--gpu-device" => {
                gpu_device = Some(option_value(args, index, "--gpu-device")?.to_owned());
                index += 2;
            }
            "--allow-backend-fallback" => {
                allow_backend_fallback = Some(true);
                index += 1;
            }
            "--no-backend-fallback" => {
                allow_backend_fallback = Some(false);
                index += 1;
            }
            "--solution-probabilities" => {
                solution_probabilities = true;
                index += 1;
            }
            "--queue-knowledge" => {
                let value = option_value(args, index, "--queue-knowledge")?;
                queue_observation_policy =
                    QueueObservationPolicy::from_keyword(value).ok_or_else(|| {
                        CliParseError::InvalidValue {
                            option: "--queue-knowledge",
                            value: value.to_owned(),
                        }
                    })?;
                index += 2;
            }
            option => return Err(unknown_option("pc", option)),
        }
    }

    let (backend, workers) = resolve_cpu_execution_aliases(
        backend,
        workers,
        cpu_threads,
        no_gpu,
        gpu_device.as_deref(),
    )?;
    if tiling_only
        && objective.as_deref().is_some_and(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().replace('_', "-").as_str(),
                "tiling"
            )
        })
    {
        return Err(CliParseError::InvalidValue {
            option: "--objective",
            value: objective.unwrap_or_default(),
        });
    }
    let objective = if tiling_only {
        "tiling".to_owned()
    } else {
        objective.unwrap_or_else(|| "all".to_owned())
    };
    if objective.trim().to_ascii_lowercase().replace('_', "-") == "tiling" {
        let incompatible = [
            (hold_option == Some(true), "--hold"),
            (score_requested, "--score"),
            (score_profile.is_some(), "--score-profile"),
            (spin_profile.is_some(), "--spin-profile"),
            (initial_b2b.is_some(), "--initial-b2b"),
            (rule.is_some(), "--rule"),
            (kick_profile_json.is_some(), "--kick-profile-json"),
            (tablebase_requested == Some(true), "--tablebase"),
            (
                precompute_build_dependencies == Some(true),
                "--build-dependency-dag",
            ),
            (solution_probabilities, "--solution-probabilities"),
            (
                queue_observation_policy.requires_observation_policy(),
                "--queue-knowledge",
            ),
        ];
        if let Some((_, option)) = incompatible.into_iter().find(|(enabled, _)| *enabled) {
            return Err(CliParseError::InvalidValue {
                option,
                value: "not available with tiling-only search".to_owned(),
            });
        }
        hold_enabled = false;
    }

    Ok(PcArgs::new(lines)
        .with_queue(queue, fixed_queue)
        .with_hold_enabled(hold_enabled)
        .with_objective(objective)
        .with_score_requested(score_requested)
        .with_score_profile(score_profile)
        .with_spin_profile(spin_profile)
        .with_initial_b2b(initial_b2b)
        .with_rule(rule)
        .with_kick_profile_json(kick_profile_json)
        .with_backend(backend)
        .with_workers(workers)
        .with_use_all_logical_processors(use_all_logical_processors)
        .with_cpu_warmup(cpu_warmup)
        .with_gpu_warmup(gpu_warmup)
        .with_tablebase_requested(tablebase_requested)
        .with_precompute_build_dependencies(precompute_build_dependencies)
        .with_deterministic(deterministic)
        .with_max_frontier_states(max_frontier_states)
        .with_max_candidates(max_candidates)
        .with_max_patterns(max_patterns)
        .with_max_memory_mib(max_memory_mib)
        .with_gpu_device(gpu_device)
        .with_allow_backend_fallback(allow_backend_fallback)
        .with_solution_probabilities(solution_probabilities)
        .with_queue_observation_policy(queue_observation_policy))
}
