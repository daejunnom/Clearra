use super::{
    execution_backend_aliases::resolve_cpu_execution_aliases,
    parse_option_value::{option_value, parse_u16_option, parse_usize_option, unknown_option},
    parse_piece_arg::parse_single_char,
    CliHelpTopic, CliParseError, ParsedCliCommand, PcScenarioArgs,
};

pub(crate) fn parse_pc_scenario(args: &[String]) -> Result<ParsedCliCommand, CliParseError> {
    let mut fixture = None;
    let mut field = None;
    let mut queue = None;
    let mut hold = None;
    let mut rule = None;
    let mut kick_profile_json = None;
    let mut requires_180 = false;
    let mut board_width = None;
    let mut visible_height = None;
    let mut max_pieces = None;
    let mut exact_pieces = None;
    let mut min_remaining_queue = None;
    let mut allow_hold = None;
    let mut count_policy = None;
    let mut retained_trace_limit = None;
    let mut backend = None;
    let mut workers = None;
    let mut use_all_logical_processors = None;
    let mut cpu_warmup = None;
    let mut gpu_warmup = None;
    let mut cpu_threads = None;
    let mut no_gpu = false;
    let mut deterministic = None;
    let mut max_frontier_states = None;
    let mut max_candidates = None;
    let mut max_patterns = None;
    let mut max_memory_mib = None;
    let mut gpu_device = None;
    let mut allow_backend_fallback = None;
    let mut verify_expected = false;
    let mut solution_probabilities = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--fixture" => {
                fixture = Some(option_value(args, index, "--fixture")?.to_owned());
                index += 2;
            }
            "--field" => {
                field = Some(option_value(args, index, "--field")?.to_owned());
                index += 2;
            }
            "--queue" | "-q" => {
                queue = Some(option_value(args, index, "--queue")?.to_owned());
                index += 2;
            }
            "--hold" => {
                let value = option_value(args, index, "--hold")?;
                hold = parse_single_char("--hold", value).map(Some)?;
                allow_hold = Some(true);
                index += 2;
            }
            "--no-hold" => {
                allow_hold = Some(false);
                index += 1;
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
            "--requires-180" => {
                requires_180 = true;
                index += 1;
            }
            "--board-width" => {
                board_width = Some(parse_u16_option(args, index, "--board-width")?);
                index += 2;
            }
            "--visible-height" => {
                visible_height = Some(parse_u16_option(args, index, "--visible-height")?);
                index += 2;
            }
            "--max-pieces" => {
                max_pieces = Some(parse_usize_option(args, index, "--max-pieces")?);
                index += 2;
            }
            "--exact-pieces" => {
                exact_pieces = Some(parse_usize_option(args, index, "--exact-pieces")?);
                index += 2;
            }
            "--min-remaining-queue" => {
                min_remaining_queue =
                    Some(parse_usize_option(args, index, "--min-remaining-queue")?);
                index += 2;
            }
            "--count-policy" => {
                count_policy = Some(option_value(args, index, "--count-policy")?.to_owned());
                index += 2;
            }
            "--retained-trace-limit" => {
                retained_trace_limit =
                    Some(parse_usize_option(args, index, "--retained-trace-limit")?);
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
            "--verify-expected" => {
                verify_expected = true;
                index += 1;
            }
            "--solution-probabilities" => {
                solution_probabilities = true;
                index += 1;
            }
            "--help" | "-h" => return Ok(ParsedCliCommand::Help(CliHelpTopic::PcScenario)),
            option => return Err(unknown_option("pc-scenario", option)),
        }
    }

    let (backend, workers) = resolve_cpu_execution_aliases(
        backend,
        workers,
        cpu_threads,
        no_gpu,
        gpu_device.as_deref(),
    )?;

    Ok(ParsedCliCommand::PcScenario(
        PcScenarioArgs::new(fixture)
            .with_field(field)
            .with_queue(queue)
            .with_hold(hold)
            .with_rule(rule)
            .with_kick_profile_json(kick_profile_json)
            .with_requires_180(requires_180)
            .with_board_width(board_width)
            .with_visible_height(visible_height)
            .with_max_pieces(max_pieces)
            .with_exact_pieces(exact_pieces)
            .with_min_remaining_queue(min_remaining_queue)
            .with_allow_hold(allow_hold)
            .with_count_policy(count_policy)
            .with_retained_trace_limit(retained_trace_limit)
            .with_backend(backend)
            .with_workers(workers)
            .with_use_all_logical_processors(use_all_logical_processors)
            .with_cpu_warmup(cpu_warmup)
            .with_gpu_warmup(gpu_warmup)
            .with_deterministic(deterministic)
            .with_max_frontier_states(max_frontier_states)
            .with_max_candidates(max_candidates)
            .with_max_patterns(max_patterns)
            .with_max_memory_mib(max_memory_mib)
            .with_gpu_device(gpu_device)
            .with_allow_backend_fallback(allow_backend_fallback)
            .with_verify_expected(verify_expected)
            .with_solution_probabilities(solution_probabilities),
    ))
}
