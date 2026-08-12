use super::CliParseError;

pub(super) fn record_backend_fallback_override(
    current: &mut Option<bool>,
    requested: bool,
) -> Result<(), CliParseError> {
    if current.is_some() {
        return Err(CliParseError::InvalidValue {
            option: "--backend-fallback",
            value: "choose exactly one of --allow-backend-fallback or --no-backend-fallback"
                .to_owned(),
        });
    }
    *current = Some(requested);
    Ok(())
}
