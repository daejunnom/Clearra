//! Typed, format-independent solution-set artifacts and host-owned sinks.
//!
//! CTK3 and Fumen codecs intentionally remain outside this module. External
//! codec adapters consume the same [`SolutionSetArtifact`] model, while native
//! compact/JSON sinks consume the typed encoder trait; `clearra-output` does
//! not become a second document-codec owner.

mod artifact_sink;
#[cfg(not(target_arch = "wasm32"))]
mod atomic_bytes_sink;
#[cfg(not(target_arch = "wasm32"))]
mod atomic_file_sink;
mod solution_comment_layout;
mod solution_document;
mod solution_set_artifact;
mod solution_set_encoder;

pub use artifact_sink::{
    ArtifactCommit, ArtifactPublicationOutcome, ArtifactSinkError, ArtifactSinkErrorCode,
    MemoryArtifactSink, SolutionArtifactSink,
};
#[cfg(not(target_arch = "wasm32"))]
pub use atomic_bytes_sink::{
    AtomicBytesArtifactSink, ByteArtifactCommit, ByteArtifactPublicationOutcome,
};
#[cfg(not(target_arch = "wasm32"))]
pub use atomic_file_sink::AtomicFileArtifactSink;
pub use clearra_platform_fs::{
    DurabilityUncertainReason, FileIdentity, NeverCancelled, PostCommitCrashRecovery,
    PreCommitCrashRecovery, PublicationCheckpoint, PublicationControl, PublicationResidue,
};
pub use solution_comment_layout::{
    SolutionArtifactAnnotation, SolutionCommentLayout, SolutionCommentLayoutError,
};
pub use solution_set_artifact::{
    SolutionArtifactEntry, SolutionSetArtifact, SolutionSetArtifactError,
    SOLUTION_SET_ARTIFACT_SCHEMA, SOLUTION_SET_ARTIFACT_SCHEMA_V1, SOLUTION_SET_ARTIFACT_SCHEMA_V2,
};
pub use solution_set_encoder::{
    ArtifactEncodingPlan, ArtifactEncodingReceipt, CompactSolutionSetEncoder,
    Ctk3SolutionSetEncoder, EncodedSolutionSetArtifact, FumenSolutionSetEncoder,
    JsonSolutionSetEncoder, SolutionArtifactEncoder, SolutionArtifactEncoding,
    SolutionArtifactEncodingError, DEFAULT_MAX_ARTIFACT_BYTES, MAX_IN_MEMORY_ARTIFACT_BYTES,
};
