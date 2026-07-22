use clearra_core_ffi::CBuildVariantView;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RepresentativeTraceSelection {
    selected_index: usize,
}

impl RepresentativeTraceSelection {
    pub(crate) fn select(variants: &[CBuildVariantView]) -> Option<Self> {
        variants
            .iter()
            .enumerate()
            .min_by_key(|(_, variant)| {
                (
                    variant.candidate_id(),
                    variant.coverage_pattern_id(),
                    variant.trace_identity(),
                    variant.build_variant_id(),
                )
            })
            .map(|(selected_index, _)| Self { selected_index })
    }
}
impl RepresentativeTraceSelection {
    pub(crate) fn selected_variant<'a>(
        self,
        variants: &'a [CBuildVariantView],
    ) -> Option<&'a CBuildVariantView> {
        variants.get(self.selected_index)
    }
}
