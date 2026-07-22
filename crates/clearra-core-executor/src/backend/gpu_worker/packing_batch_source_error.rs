use super::PackingBatchValidationError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackingBatchSourceError {
    Validation(PackingBatchValidationError),
}

impl From<PackingBatchValidationError> for PackingBatchSourceError {
    fn from(error: PackingBatchValidationError) -> Self {
        Self::Validation(error)
    }
}

impl From<PackingBatchSourceError> for PackingBatchValidationError {
    fn from(error: PackingBatchSourceError) -> Self {
        match error {
            PackingBatchSourceError::Validation(error) => error,
        }
    }
}
