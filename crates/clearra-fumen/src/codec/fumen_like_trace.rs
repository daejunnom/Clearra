#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FumenLikeTrace {
    pages: Vec<String>,
}

impl FumenLikeTrace {
    pub fn try_new(pages: Vec<String>) -> Result<Self, FumenLikeTraceError> {
        if pages.is_empty() {
            return Err(FumenLikeTraceError::EmptyTrace);
        }

        for (index, page) in pages.iter().enumerate() {
            if page.is_empty() {
                return Err(FumenLikeTraceError::EmptyPage { index });
            }
            if page.contains('\r') {
                return Err(FumenLikeTraceError::CarriageReturn { index });
            }
            if !page.is_ascii() {
                return Err(FumenLikeTraceError::NonAsciiPage { index });
            }
        }

        Ok(Self { pages })
    }
}
impl FumenLikeTrace {
    pub fn new(pages: Vec<String>) -> Self {
        Self::try_new(pages).expect("fumen-like trace pages must satisfy the Clearra contract")
    }
}
impl FumenLikeTrace {
    pub fn pages(&self) -> &[String] {
        &self.pages
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FumenLikeTraceError {
    EmptyTrace,
    EmptyPage { index: usize },
    CarriageReturn { index: usize },
    NonAsciiPage { index: usize },
}

#[cfg(test)]
#[path = "fumen_like_trace_tests.rs"]
mod tests;
