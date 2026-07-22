pub mod asset_import_limits;
pub mod asset_import_report;
pub mod runtime_asset_gate;
mod svg_sanitizer;
pub mod svg_security_scanner;

#[cfg(feature = "asset-import")]
mod asset_import_pipeline;

pub use asset_import_limits::AssetImportLimits;
pub use asset_import_report::AssetImportReportValidator;
pub use runtime_asset_gate::RuntimeAssetGate;
pub use svg_sanitizer::sanitize_svg;
pub use svg_security_scanner::SvgSecurityScanner;

#[cfg(feature = "asset-import")]
pub use asset_import_pipeline::{
    rasterize_sanitized_svg, AssetImportBundle, AssetImportMetadata, AssetImportPipeline,
};
