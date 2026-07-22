use super::CliParseError;

pub(crate) fn parse_single_char(option: &'static str, value: &str) -> Result<char, CliParseError> {
    let mut chars = value.chars();
    let Some(piece) = chars.next() else {
        return Err(CliParseError::InvalidValue {
            option,
            value: value.to_owned(),
        });
    };
    if chars.next().is_some() {
        return Err(CliParseError::InvalidValue {
            option,
            value: value.to_owned(),
        });
    }
    Ok(piece)
}
