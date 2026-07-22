#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetImportLimits {
    pub max_svg_bytes: usize,
    pub max_decompressed_bytes: usize,
    pub max_elements: usize,
    pub max_group_depth: usize,
    pub max_path_commands: usize,
    pub max_path_segments_per_path: usize,
    pub max_gradients: usize,
    pub max_filters: usize,
    pub max_external_references: usize,
    pub max_css_rules: usize,
    pub max_viewbox_width: u32,
    pub max_viewbox_height: u32,
    pub max_raster_pixels: u64,
    pub max_import_time_ms: u64,
    pub max_memory_mib: u64,
}

impl Default for AssetImportLimits {
    fn default() -> Self {
        Self {
            max_svg_bytes: 1_048_576,
            max_decompressed_bytes: 4_194_304,
            max_elements: 2_000,
            max_group_depth: 32,
            max_path_commands: 20_000,
            max_path_segments_per_path: 4_096,
            max_gradients: 128,
            max_filters: 0,
            max_external_references: 0,
            max_css_rules: 512,
            max_viewbox_width: 4096,
            max_viewbox_height: 4096,
            max_raster_pixels: 16_777_216,
            max_import_time_ms: 10_000,
            max_memory_mib: 256,
        }
    }
}
