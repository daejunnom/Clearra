#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GroupingMode {
    #[default]
    ShapeFamily,
    TilingVariant,
    BuildVariant,
}

impl GroupingMode {
    pub const MVP1_SUPPORTED: [Self; 3] =
        [Self::ShapeFamily, Self::TilingVariant, Self::BuildVariant];
}
impl GroupingMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ShapeFamily => "shape-family",
            Self::TilingVariant => "tiling-variant",
            Self::BuildVariant => "build-variant",
        }
    }
}
impl GroupingMode {
    pub fn preserves_shape_family(self) -> bool {
        matches!(self, Self::ShapeFamily)
    }
}
impl GroupingMode {
    pub fn includes_build_variants(self) -> bool {
        matches!(self, Self::BuildVariant)
    }
}
