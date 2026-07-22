use super::FfiProblemError;

pub(crate) fn to_u16(
    value: usize,
    error: impl FnOnce(usize) -> FfiProblemError,
) -> Result<u16, FfiProblemError> {
    u16::try_from(value).map_err(|_| error(value))
}

pub(crate) fn to_u32(field: &'static str, value: usize) -> Result<u32, FfiProblemError> {
    u32::try_from(value).map_err(|_| FfiProblemError::BudgetTooLarge { field, value })
}
