use clearra_core_ffi::CPackingProblem;
use clearra_problem::SearchProblem;

use super::{
    PackingBatchDescriptor, PackingBatchId, PackingBatchSource, PackingBatchValidationError,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PackingBatchDescriptorBuilder {
    batch_id: Option<PackingBatchId>,
    pattern_universe_id: Option<u64>,
    pattern_weight_model_id: Option<u64>,
}

impl PackingBatchDescriptorBuilder {
    pub const fn new() -> Self {
        Self {
            batch_id: None,
            pattern_universe_id: None,
            pattern_weight_model_id: None,
        }
    }
}
impl PackingBatchDescriptorBuilder {
    pub const fn with_batch_id(mut self, batch_id: PackingBatchId) -> Self {
        self.batch_id = Some(batch_id);
        self
    }
}
impl PackingBatchDescriptorBuilder {
    pub const fn with_pattern_identity(
        mut self,
        pattern_universe_id: u64,
        pattern_weight_model_id: u64,
    ) -> Self {
        self.pattern_universe_id = Some(pattern_universe_id);
        self.pattern_weight_model_id = Some(pattern_weight_model_id);
        self
    }
}
impl PackingBatchDescriptorBuilder {
    pub fn from_search_problem(
        self,
        problem: &SearchProblem,
        compact: &CPackingProblem,
    ) -> Result<PackingBatchDescriptor, PackingBatchValidationError> {
        let source = PackingBatchSource::from_search_problem(
            problem,
            compact,
            self.batch_id,
            self.pattern_universe_id,
            self.pattern_weight_model_id,
        )?;
        self.from_source(source)
    }
}
impl PackingBatchDescriptorBuilder {
    pub fn from_compact_problem_with_identity(
        self,
        compact: &CPackingProblem,
        pattern_universe_id: u64,
        pattern_weight_model_id: u64,
    ) -> Result<PackingBatchDescriptor, PackingBatchValidationError> {
        let source = PackingBatchSource::from_compact_problem_with_identity(
            compact,
            self.batch_id,
            pattern_universe_id,
            pattern_weight_model_id,
            None,
        )?;
        self.from_source(source)
    }
}
impl PackingBatchDescriptorBuilder {
    pub fn from_source(
        self,
        source: PackingBatchSource,
    ) -> Result<PackingBatchDescriptor, PackingBatchValidationError> {
        let source = PackingBatchSource {
            batch_id: self.batch_id.unwrap_or(source.batch_id),
            pattern_universe_id: self
                .pattern_universe_id
                .unwrap_or(source.pattern_universe_id),
            pattern_weight_model_id: self
                .pattern_weight_model_id
                .unwrap_or(source.pattern_weight_model_id),
            ..source
        };
        source.into_descriptor()
    }
}
