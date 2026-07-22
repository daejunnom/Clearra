use crate::codec::{FumenLikeReadError, FumenLikeReader, FumenLikeTrace};

use super::{
    normalized_fumen_document::NormalizedFumenDocument, normalized_fumen_page::NormalizedFumenPage,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FumenNormalizer;

impl FumenNormalizer {
    pub fn normalize(input: &str) -> Result<NormalizedFumenDocument, FumenNormalizeError> {
        let trace = FumenLikeReader::read(input).map_err(FumenNormalizeError::Read)?;
        Ok(Self::normalize_trace(&trace))
    }
}
impl FumenNormalizer {
    pub fn normalize_trace(trace: &FumenLikeTrace) -> NormalizedFumenDocument {
        let pages = trace
            .pages()
            .iter()
            .enumerate()
            .map(|(index, page)| NormalizedFumenPage::from_comment_page(index, page))
            .collect();
        NormalizedFumenDocument::new(pages)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FumenNormalizeError {
    Read(FumenLikeReadError),
}

#[cfg(test)]
#[path = "fumen_normalizer_tests.rs"]
mod tests;
