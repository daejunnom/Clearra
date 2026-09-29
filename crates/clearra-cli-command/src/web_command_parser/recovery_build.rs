//! New two-input request grammar. Legacy role/limit flags are deliberately not
//! accepted here; neither an ignored constraint nor a hidden cutoff is injected.
use super::*;
use clearra_forward_search::{
    CrossStageEarlyLimit, RecoveryBuildFields, RecoveryBuildQuery, RecoveryBuildStage,
};

pub(super) fn parse(tokens: &[String]) -> Result<WebCommandRequest, WebCommandError> {
    let fail = |message: &str| WebCommandError::new(WebCommandErrorCode::InvalidValue, message);
    let mut height = 8;
    let mut workers = None;
    let mut use_all = false;
    let mut all_solutions = false;
    let mut minimum_solutions = false;
    let mut required_solution_keys = Vec::new();
    let mut minimum_source_identity = None;
    let mut initial = Board256Mask::EMPTY;
    let mut middle = None;
    let mut result = None;
    let mut first = None;
    let mut second = None;
    let mut stage_masks = Vec::new();
    let mut stage_supplies = Vec::new();
    let mut early = CrossStageEarlyLimit::Auto;
    let mut exchange = false;
    let mut hold = true;
    let mut preserve = false;
    let mut initial_b2b = true;
    let mut rule = RuleProfileId::SrsPlus;
    let mut spin = SpinProfileId::AllSpinPlus;
    let mut seen = std::collections::BTreeSet::new();
    let mut cursor = 0;
    while cursor < tokens.len() {
        let option_cursor = cursor;
        let option = tokens[option_cursor].as_str();
        let identity = match option {
            "--no-hold" => "--hold",
            "--use-all-cpu-threads" => "--use-all-logical-processors",
            "--no-piece-exchange" => "--allow-piece-exchange",
            "--no-preserve-b2b" => "--preserve-b2b",
            other => other,
        };
        if !["--required-solution", "--stage-mask", "--stage-supply"].contains(&identity)
            && !seen.insert(identity)
        {
            return Err(fail("recovery-build option occurs more than once"));
        }
        match option {
            "--stage-mask" => {
                if stage_masks.len() >= 60 {
                    return Err(fail("too many recovery targets"));
                }
                stage_masks.push(Board256Mask::from_words(parse_board_words(
                    next_value(tokens, &mut cursor, option)?,
                    option,
                )?));
            }
            "--stage-supply" => {
                if stage_supplies.len() >= 60 {
                    return Err(fail("too many recovery supplies"));
                }
                stage_supplies.push(next_value(tokens, &mut cursor, option)?.to_owned());
            }
            "--start-mask" => {
                initial = Board256Mask::from_words(parse_board_words(
                    next_value(tokens, &mut cursor, option)?,
                    option,
                )?)
            }
            "--middle-mask" => {
                middle = Some(Board256Mask::from_words(parse_board_words(
                    next_value(tokens, &mut cursor, option)?,
                    option,
                )?))
            }
            "--result-mask" => {
                result = Some(Board256Mask::from_words(parse_board_words(
                    next_value(tokens, &mut cursor, option)?,
                    option,
                )?))
            }
            "--workers" => {
                let count: u16 = parse_positive(next_value(tokens, &mut cursor, option)?, option)?;
                workers = Some(usize::from(count));
            }
            "--height" => {
                height = parse_positive(next_value(tokens, &mut cursor, option)?, option)?
            }
            "--first-supply" => first = Some(next_value(tokens, &mut cursor, option)?.to_owned()),
            "--second-supply" => second = Some(next_value(tokens, &mut cursor, option)?.to_owned()),
            "--max-early" => {
                let value = next_value(tokens, &mut cursor, option)?;
                early = if value == "auto" {
                    CrossStageEarlyLimit::Auto
                } else {
                    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                        return Err(fail("--max-early requires auto or a nonnegative integer"));
                    }
                    CrossStageEarlyLimit::AtMost(
                        value
                            .parse()
                            .map_err(|_| fail("--max-early exceeds the native count domain"))?,
                    )
                };
            }
            "--use-all-cpu-threads" | "--use-all-logical-processors" => use_all = true,
            "--all-solutions" => all_solutions = true,
            "--minimum-solutions" => {
                all_solutions = true;
                minimum_solutions = true;
            }
            "--required-solution" => {
                let key = next_value(tokens, &mut cursor, option)?.to_owned();
                if key.is_empty() || required_solution_keys.contains(&key) {
                    return Err(fail("invalid or duplicate required solution"));
                }
                required_solution_keys.push(key);
            }
            "--minimum-source" => {
                minimum_source_identity = Some(next_value(tokens, &mut cursor, option)?.to_owned())
            }
            "--allow-piece-exchange" => exchange = true,
            "--no-piece-exchange" => exchange = false,
            "--hold" => hold = true,
            "--no-hold" => hold = false,
            "--preserve-b2b" => preserve = true,
            "--no-preserve-b2b" => preserve = false,
            "--initial-b2b" => {
                initial_b2b = match next_value(tokens, &mut cursor, option)? {
                    "0" => false,
                    "1" => true,
                    _ => return Err(fail("--initial-b2b requires 0 or 1")),
                }
            }
            "--rule" => {
                rule = RuleProfileId::parse(next_value(tokens, &mut cursor, option)?)
                    .ok_or_else(|| fail("invalid recovery-build rule"))?
            }
            "--spin-profile" => {
                spin = SpinProfileId::parse(next_value(tokens, &mut cursor, option)?)
                    .ok_or_else(|| fail("invalid recovery-build spin profile"))?
            }
            _ => return Err(fail(&format!("unsupported recovery-build option {option}"))),
        }
        // next_value owns both tokens of valued options. Only switches leave
        // the cursor unchanged and require advancing past their single token.
        if cursor == option_cursor {
            cursor += 1;
        }
    }
    let stages = if !stage_masks.is_empty() || !stage_supplies.is_empty() {
        if middle.is_some() || result.is_some() || first.is_some() || second.is_some() {
            return Err(fail(
                "stage arrays cannot be mixed with paired target or supply flags",
            ));
        }
        if stage_masks.len() < 2 || stage_masks.len() != stage_supplies.len() {
            return Err(fail(
                "each recovery stage requires one target and one supply",
            ));
        }
        let n = stage_masks.len();
        middle = Some(stage_masks[0]);
        result = Some(stage_masks[n - 1]);
        first = Some(stage_supplies[0].clone());
        second = Some(stage_supplies[n - 1].clone());
        stage_masks
            .into_iter()
            .zip(stage_supplies)
            .map(|(target, supply)| RecoveryBuildStage { target, supply })
            .collect()
    } else {
        Vec::new()
    };
    let query = RecoveryBuildQuery {
        stages,
        all_solutions,
        minimum_solutions,
        required_solution_keys,
        minimum_source_identity,
        fields: RecoveryBuildFields {
            height,
            initial,
            middle: middle.ok_or_else(|| fail("missing --middle-mask"))?,
            result: result.ok_or_else(|| fail("missing --result-mask"))?,
        },
        first_supply: first.ok_or_else(|| fail("missing --first-supply"))?,
        second_supply: second.ok_or_else(|| fail("missing --second-supply"))?,
        early_limit: early,
        allow_piece_exchange: exchange,
        hold_enabled: hold,
        preserve_b2b: preserve,
        initial_b2b,
        rule_profile: rule,
        spin_profile: spin,
    };
    query
        .validate()
        .map_err(|error| fail(&format!("invalid recovery-build input: {error:?}")))?;
    let request = WebCommandRequest::recovery_build(query).with_use_all_logical_processors(use_all);
    Ok(match workers {
        Some(workers) => request.with_workers(workers),
        None => request,
    })
}
