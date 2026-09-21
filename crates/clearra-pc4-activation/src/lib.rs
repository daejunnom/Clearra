//! Host-owned trust boundary for PC4 tablebase activation.
//!
//! This crate deliberately contains no network discovery, tablebase lookup,
//! signing, persistence, or product adapter. It verifies pinned-key release
//! authority and links that authority to independently qualified reader data.

mod error;
mod link;
mod replay_state;
mod signed_envelope;
mod trusted_keyring;

pub use error::ActivationError;
pub use link::{verify_and_link, EffectiveProfile, LinkedActivation, ProfileActivation};
pub use replay_state::{ReplayDecision, ReplayState};
pub use signed_envelope::{
    verify_generation_envelope, verify_rollout_pointer, RetainedGeneration,
    VerifiedGenerationAuthority, VerifiedRolloutPointer,
};
pub use trusted_keyring::{PinnedPublicKey, StaticPublicKeyring};

pub const PUBLIC_KEYRING_SCHEMA: &str = "clearra.pc4.activation-keyring.v1";
pub const SIGNED_GENERATION_ENVELOPE_SCHEMA: &str = "clearra.pc4.signed-generation-envelope.v1";
pub const GENERATION_STATEMENT_SCHEMA: &str = "clearra.pc4.production-generation-statement.v1";
pub const SIGNED_ROLLOUT_ENVELOPE_SCHEMA: &str = "clearra.pc4.signed-rollout-envelope.v1";
pub const ROLLOUT_STATEMENT_SCHEMA: &str = "clearra.pc4.production-rollout-statement.v1";
pub const HOST_GENERATION_SCHEMA: &str = "clearra.pc4.host-generation.v1";
pub const SIGNATURE_ALGORITHM: &str = "ed25519";
pub const PRODUCTION_CHANNEL: &str = "production";
pub const MAX_ROLLBACK_GENERATIONS: usize = 5;

pub(crate) const GENERATION_SIGNATURE_DOMAIN: &[u8] =
    b"clearra.pc4.production-generation-statement.v1\0";
pub(crate) const ROLLOUT_SIGNATURE_DOMAIN: &[u8] = b"clearra.pc4.production-rollout-statement.v1\0";
pub(crate) const RULE_PROFILES: [&str; 5] = ["srs", "srs-plus", "srs-x", "jstris-180", "no-kick"];
