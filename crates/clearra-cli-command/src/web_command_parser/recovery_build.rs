//! New two-input request grammar. Legacy role/limit flags are deliberately not
//! accepted here; neither an ignored constraint nor a hidden cutoff is injected.
use super::*;
use clearra_forward_search::{CrossStageEarlyLimit, RecoveryBuildFields, RecoveryBuildQuery};

pub(super) fn parse(tokens: &[String]) -> Result<WebCommandRequest, WebCommandError> {
    let fail = |message: &str| WebCommandError::new(WebCommandErrorCode::InvalidValue, message);
    let mut height = 8;
    let mut initial = Board256Mask::EMPTY;
    let mut middle = None;
    let mut result = None;
    let mut first = None;
    let mut second = None;
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
            "--no-piece-exchange" => "--allow-piece-exchange",
            "--no-preserve-b2b" => "--preserve-b2b",
            other => other,
        };
        if !seen.insert(identity) {
            return Err(fail("recovery-build option occurs more than once"));
        }
        match option {
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
    let query = RecoveryBuildQuery {
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
    Ok(WebCommandRequest::recovery_build(query))
}
