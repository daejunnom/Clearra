use super::SkinProvenance;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SkinManifest {
    skin_id: String,
    atlas_path: String,
    provenance: SkinProvenance,
}

impl SkinManifest {
    pub fn new(
        skin_id: impl Into<String>,
        atlas_path: impl Into<String>,
        provenance: SkinProvenance,
    ) -> Self {
        Self {
            skin_id: skin_id.into(),
            atlas_path: atlas_path.into(),
            provenance,
        }
    }
}
impl SkinManifest {
    pub fn skin_id(&self) -> &str {
        &self.skin_id
    }
}
impl SkinManifest {
    pub fn atlas_path(&self) -> &str {
        &self.atlas_path
    }
}
impl SkinManifest {
    pub fn provenance(&self) -> &SkinProvenance {
        &self.provenance
    }
}
