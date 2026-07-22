use clearra_core_domain::ids::setup_id::{BuildVariantId, TilingVariantId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VariantTrace {
    tiling_variant_id: TilingVariantId,
    build_variant_id: BuildVariantId,
    notes: Vec<String>,
}

impl VariantTrace {
    pub fn new(tiling_variant_id: TilingVariantId, build_variant_id: BuildVariantId) -> Self {
        Self {
            tiling_variant_id,
            build_variant_id,
            notes: Vec::new(),
        }
    }
}
impl VariantTrace {
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }
}
impl VariantTrace {
    pub fn tiling_variant_id(&self) -> TilingVariantId {
        self.tiling_variant_id
    }
}
impl VariantTrace {
    pub fn build_variant_id(&self) -> BuildVariantId {
        self.build_variant_id
    }
}
impl VariantTrace {
    pub fn notes(&self) -> &[String] {
        &self.notes
    }
}
