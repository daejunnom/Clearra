//! Fumen-like codec, transforms, and typed replay adapters.

pub mod adapter;
pub mod codec;
pub mod normalize;
pub mod transform;

pub use adapter::{
    ColoredSolutionFumenError, ColoredSolutionFumenExporter, ColoredSolutionPage,
    ColoredSolutionPlacement,
};
pub use codec::{
    FumenLikeReadError, FumenLikeReader, FumenLikeTrace, FumenLikeWriter,
    SourceFumenColoredFieldSet, SourceFumenDiagramError, SourceFumenDiagramSet, SourceFumenSetup,
    COLORED_FIELD_SOLUTION_KEY_ALGORITHM, COLORED_FIELD_SOLUTION_SET_HASH_ALGORITHM,
};
pub use normalize::{
    FumenNormalizeError, FumenNormalizer, NormalizedFumenDocument, NormalizedFumenPage,
    NormalizedSolutionKey,
};
pub use transform::{
    BuildTemplateDraft, BuildTemplateError, FumenToBuildTemplateAdapter, FumenTransformContract,
    FumenTransformError,
};
