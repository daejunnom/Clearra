//! Pure protocol and reader core for the online PC4 solution tablebase.
//!
//! The crate intentionally has no I/O dependencies. Hosts resolve a dynamic
//! upstream snapshot, qualify it, and satisfy the emitted bounded Range
//! requests. No dataset revision or digest is compiled into the product.

mod graph;
mod lookup;
mod manifest;
mod protocol;

pub use graph::{decode_graph_target_sequence, GraphTargetDecodeError};
pub use lookup::{
    FormatMismatch, LookupFailure, LookupHit, LookupMachine, LookupStartError, LookupStep,
    SupplyError,
};
pub use manifest::{
    ActivatedSnapshot, ActivationError, ArtifactDescriptor, DatasetSnapshotManifest,
    GraphTargetEncoding, ManifestError, Pc4ArtifactRole, Pc4ProfileManifest, Pc4RuleProfile,
    ProfileAvailability, ProfileQualification, SnapshotIdentity, UnsupportedProfileReason,
};
pub use protocol::{RangeRequest, RangeResponse, RangeResponseKind, RangeTransportFailure};
