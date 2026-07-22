use clearra_core_ffi::CBuildVariantView;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct ExecutionVariantSet {
    variants: Vec<CBuildVariantView>,
}

impl ExecutionVariantSet {
    pub(crate) fn insert(&mut self, variant: CBuildVariantView) -> bool {
        let duplicate = self.variants.iter().any(|existing| {
            existing.candidate_id() == variant.candidate_id()
                && existing.coverage_pattern_id() == variant.coverage_pattern_id()
                && existing.trace_identity() == variant.trace_identity()
                && existing == &variant
        });
        if duplicate {
            return false;
        }
        self.variants.push(variant);
        true
    }
}
impl ExecutionVariantSet {
    pub(crate) fn variants(&self) -> &[CBuildVariantView] {
        &self.variants
    }
}
impl ExecutionVariantSet {
    pub(crate) fn len(&self) -> usize {
        self.variants.len()
    }
}
impl ExecutionVariantSet {
    pub(crate) fn unique_trace_count(&self) -> usize {
        self.variants
            .iter()
            .enumerate()
            .filter(|(index, variant)| {
                !self.variants[..*index].iter().any(|existing| {
                    existing.candidate_id() == variant.candidate_id()
                        && existing.trace_identity() == variant.trace_identity()
                        && existing.operation_order_ids() == variant.operation_order_ids()
                        && existing.trace_steps() == variant.trace_steps()
                })
            })
            .count()
    }
}
impl ExecutionVariantSet {
    pub(crate) fn into_variants(self) -> Vec<CBuildVariantView> {
        self.variants
    }
}
