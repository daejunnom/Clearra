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
    ActualFumenPageParityObservation, ActualFumenParityDocument, ActualFumenParityDocumentError,
    ActualFumenRenderColor, ActualFumenRenderDocument, ActualFumenRenderDocumentError,
    ActualFumenRenderPage, FumenLikeReadError, FumenLikeReader, FumenLikeTrace, FumenLikeWriter,
    SourceFumenBoard, SourceFumenColoredFieldSet, SourceFumenDiagramError, SourceFumenDiagramSet,
    SourceFumenDocumentOperation, SourceFumenOperationDocument, SourceFumenOperationDocumentError,
    SourceFumenSetup, COLORED_FIELD_SOLUTION_KEY_ALGORITHM,
    COLORED_FIELD_SOLUTION_SET_HASH_ALGORITHM, FUMEN_MAX_INPUT_BYTES, FUMEN_MAX_PAGES,
};
pub use normalize::{
    FumenNormalizeError, FumenNormalizer, NormalizedFumenDocument, NormalizedFumenPage,
    NormalizedSolutionKey,
};
pub use transform::{
    ActualFumenDocumentTransform, ActualFumenTransformError, BuildTemplateDraft,
    BuildTemplateError, FumenToBuildTemplateAdapter, FumenTransformContract, FumenTransformError,
};
