//! Locale resolution and translation catalog contracts for Clearra UI surfaces.

pub mod catalog;
pub mod export;
pub mod format;
pub mod language;

pub use catalog::{TranslationCatalog, TranslationKey};
pub use export::{TranslationEntry, UiTranslationExport};
pub use format::{interpolate, InterpolationValue, LocalizedText};
pub use language::{
    LanguageId, LanguagePreference, LanguageResolutionReport, LanguageResolver,
    SystemLocaleDetectionReport, SystemLocaleDetector, SystemLocaleEnvDetector,
    SystemLocalePlatformDetector, SystemLocaleSource,
};
