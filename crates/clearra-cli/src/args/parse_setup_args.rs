use super::{
    parse_option_value::{option_value, unknown_option},
    CliHelpTopic, CliParseError, ParsedCliCommand, SetupArgs,
};

pub(crate) fn parse_setup(args: &[String]) -> Result<ParsedCliCommand, CliParseError> {
    let mut remaining = "IOTSZJL".to_owned();
    let mut allow_post_cycle_borrow = false;
    let mut candidate_priority = clearra_setup_search::query::SetupCandidatePriority::All;
    let mut length_preference = clearra_setup_search::query::SetupLengthPreference::Auto;
    let mut max_setup_pieces = 9_u8;
    let mut explicit_search_mode = None;
    let mut queue_based_pieces = None;
    let mut rule = None;
    let mut initial_hold = None;
    let mut path_detail_setup_id = None;
    let mut path_detail_condition_id = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--remaining" | "--queue" | "-q" => {
                remaining = option_value(args, index, "--remaining")?.to_owned();
                index += 2;
            }
            "--allow-post-cycle-borrow" => {
                allow_post_cycle_borrow = true;
                index += 1;
            }
            "--qb" => {
                queue_based_pieces = Some(option_value(args, index, "--qb")?.to_owned());
                index += 2;
            }
            "--initial-hold" => {
                initial_hold = Some(option_value(args, index, "--initial-hold")?.to_owned());
                index += 2;
            }
            "--rule" => {
                rule = Some(option_value(args, index, "--rule")?.to_owned());
                index += 2;
            }
            "--mode" => {
                let value = option_value(args, index, "--mode")?;
                explicit_search_mode = Some(
                    clearra_setup_search::query::SetupSearchMode::from_keyword(value).ok_or_else(
                        || CliParseError::InvalidValue {
                            option: "--mode",
                            value: value.to_owned(),
                        },
                    )?,
                );
                index += 2;
            }
            "--priority" => {
                let value = option_value(args, index, "--priority")?;
                candidate_priority =
                    clearra_setup_search::query::SetupCandidatePriority::from_keyword(value)
                        .ok_or_else(|| CliParseError::InvalidValue {
                            option: "--priority",
                            value: value.to_owned(),
                        })?;
                index += 2;
            }
            "--setup-length" => {
                let value = option_value(args, index, "--setup-length")?;
                length_preference =
                    clearra_setup_search::query::SetupLengthPreference::from_keyword(value)
                        .ok_or_else(|| CliParseError::InvalidValue {
                            option: "--setup-length",
                            value: value.to_owned(),
                        })?;
                index += 2;
            }
            "--max-setup-pieces" => {
                let value = option_value(args, index, "--max-setup-pieces")?;
                max_setup_pieces = value
                    .parse::<u8>()
                    .ok()
                    .filter(|count| (1..=10).contains(count))
                    .ok_or_else(|| CliParseError::InvalidValue {
                        option: "--max-setup-pieces",
                        value: value.to_owned(),
                    })?;
                index += 2;
            }
            "--paths-for" => {
                path_detail_setup_id = Some(option_value(args, index, "--paths-for")?.to_owned());
                index += 2;
            }
            "--condition" => {
                path_detail_condition_id =
                    Some(option_value(args, index, "--condition")?.to_owned());
                index += 2;
            }
            "--help" | "-h" => return Ok(ParsedCliCommand::Help(CliHelpTopic::Setup)),
            option => return Err(unknown_option("setup", option)),
        }
    }

    let search_mode = match (explicit_search_mode, queue_based_pieces.is_some()) {
        (Some(clearra_setup_search::query::SetupSearchMode::ShapeOracle), true) => {
            return Err(CliParseError::InvalidValue {
                option: "--mode",
                value: "oracle with --qb".to_owned(),
            });
        }
        (Some(mode), _) => mode,
        (None, true) => clearra_setup_search::query::SetupSearchMode::QueueBased,
        (None, false) => clearra_setup_search::query::SetupSearchMode::ShapeOracle,
    };
    let mut setup_args = SetupArgs::new(remaining, allow_post_cycle_borrow)
        .with_candidate_priority(candidate_priority)
        .with_length_preference(length_preference)
        .with_max_setup_pieces(max_setup_pieces)
        .with_search_mode(search_mode);
    if let Some(pieces) = queue_based_pieces {
        setup_args = setup_args.with_queue_based_pieces(pieces);
    }
    if let Some(value) = rule {
        setup_args = setup_args.with_rule(value);
    }
    if let Some(value) = initial_hold {
        setup_args = setup_args.with_initial_hold(value);
    }
    match (path_detail_setup_id, path_detail_condition_id) {
        (Some(setup_id), Some(condition_id)) => {
            setup_args = setup_args.with_path_detail(setup_id, condition_id);
        }
        (Some(_), None) => {
            return Err(CliParseError::InvalidValue {
                option: "--condition",
                value: "missing for --paths-for".to_owned(),
            });
        }
        (None, Some(_)) => {
            return Err(CliParseError::InvalidValue {
                option: "--paths-for",
                value: "missing for --condition".to_owned(),
            });
        }
        (None, None) => {}
    }
    Ok(ParsedCliCommand::Setup(setup_args))
}
