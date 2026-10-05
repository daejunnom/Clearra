use super::CliParseError;

pub(crate) fn option_value<'a>(
    args: &'a [String],
    index: usize,
    option: &'static str,
) -> Result<&'a str, CliParseError> {
    args.get(index + 1)
        .map(String::as_str)
        .ok_or(CliParseError::MissingValue { option })
}

pub(crate) fn unknown_option(command: &'static str, option: &str) -> CliParseError {
    CliParseError::UnknownOption {
        command,
        option: option.to_owned(),
    }
}

/// Native compatibility requests retain their own grammar, but must not give
/// the same two accelerator selectors a last-option-wins meaning.
pub(crate) fn select_exact_accelerator_once(
    slot: &mut Option<bool>,
    enabled: bool,
    option: &'static str,
) -> Result<(), CliParseError> {
    if slot.is_some() {
        return Err(CliParseError::InvalidValue {
            option,
            value: "selected more than once".to_owned(),
        });
    }
    *slot = Some(enabled);
    Ok(())
}

pub(crate) fn parse_u8_option(
    args: &[String],
    index: usize,
    option: &'static str,
) -> Result<u8, CliParseError> {
    let value = option_value(args, index, option)?;
    value
        .parse::<u8>()
        .map_err(|_| CliParseError::InvalidValue {
            option,
            value: value.to_owned(),
        })
}

pub(crate) fn parse_u16_option(
    args: &[String],
    index: usize,
    option: &'static str,
) -> Result<u16, CliParseError> {
    let value = option_value(args, index, option)?;
    value
        .parse::<u16>()
        .map_err(|_| CliParseError::InvalidValue {
            option,
            value: value.to_owned(),
        })
}

pub(crate) fn parse_usize_option(
    args: &[String],
    index: usize,
    option: &'static str,
) -> Result<usize, CliParseError> {
    let value = option_value(args, index, option)?;
    value
        .parse::<usize>()
        .map_err(|_| CliParseError::InvalidValue {
            option,
            value: value.to_owned(),
        })
}
