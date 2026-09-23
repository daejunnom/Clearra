//! Signed, immutable product ownership for actual-entry/first-exit relations.
//!
//! A structurally valid `CLLR0002` file is only a candidate. Qualification
//! requires an externally verified signature over its complete-file digest,
//! rule identity, generation, profile and bounded proof identity. Even a
//! qualified record only answers for caller-proven entry poses after exact
//! composition of all exits; a missing record never means unreachable.

use std::sync::{Arc, OnceLock, RwLock};

use clearra_accelerator_activation::{AcceleratorProduct, VerifiedAcceleratorAuthority};
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_rules::kicks::KickTableProfileId;

use crate::conditioned_local_index::LocalRelationCandidateLookup;
use crate::conditioned_local_pack::{LocalRelationBinding, LocalRelationCandidatePack};
use crate::conditioned_local_relation::{ConditionedPoseWindow, LocalRelationRowFrame};
use crate::conditioned_reachability::ConditionedReachabilityEntryPose;
use crate::legal_board::{accelerator_profile_name, ProviderStatus};

pub const LOCAL_RELATION_COMPLETENESS_SCOPE: &str =
    "width10-height1to6-solver-sky-bottom8-56-contexts-entry-first-exit-boolean";
const PROFILE_SLOTS: usize = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalRelationProductError {
    NotQualified,
    UnsupportedProfile,
    ActiveSessionTooLarge,
    ActiveSessionInUse,
    RegistryUnavailable,
}

impl LocalRelationProductError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::NotQualified => "local_relation_product_not_qualified",
            Self::UnsupportedProfile => "local_relation_product_profile_unsupported",
            Self::ActiveSessionTooLarge => "local_relation_product_active_session_too_large",
            Self::ActiveSessionInUse => "local_relation_product_active_session_in_use",
            Self::RegistryUnavailable => "local_relation_product_registry_unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalRelationProductLookup {
    /// Complete Boolean lock set from exactly the caller-proven entries.
    ComposedForProvenEntries([u64; 4]),
    PassThrough(ProviderStatus),
}

pub struct QualifiedLocalRelationPack {
    pack: LocalRelationCandidatePack,
    signed_catalog_identity: [u8; 32],
    bounded_exhaustive_identity: [u8; 32],
}

impl QualifiedLocalRelationPack {
    pub fn qualify(
        pack: LocalRelationCandidatePack,
        authority: &VerifiedAcceleratorAuthority,
    ) -> Result<Self, LocalRelationProductError> {
        let profile = accelerator_profile_name(pack.binding().kick_profile)
            .map_err(|_| LocalRelationProductError::UnsupportedProfile)?;
        if authority.product() != AcceleratorProduct::BoardConditionedReachability
            || authority.profile() != profile
            || authority.generation_identity() != pack.generation_identity()
            || authority.rule_identity() != pack.binding().rule_identity
            || authority.payload_bytes() != pack.encoded_bytes() as u64
            || authority.payload_identity() != pack.encoded_identity()
            || authority.completeness_scope() != LOCAL_RELATION_COMPLETENESS_SCOPE
            || authority.statement_identity() == [0; 32]
            || authority.qualification_identity() == [0; 32]
        {
            return Err(LocalRelationProductError::NotQualified);
        }
        Ok(Self {
            pack,
            signed_catalog_identity: authority.statement_identity(),
            bounded_exhaustive_identity: authority.qualification_identity(),
        })
    }

    pub const fn binding(&self) -> LocalRelationBinding {
        self.pack.binding()
    }

    pub const fn generation_identity(&self) -> [u8; 32] {
        self.pack.generation_identity()
    }

    pub const fn signed_catalog_identity(&self) -> [u8; 32] {
        self.signed_catalog_identity
    }

    pub const fn bounded_exhaustive_identity(&self) -> [u8; 32] {
        self.bounded_exhaustive_identity
    }

    pub fn record_count(&self) -> usize {
        self.pack.record_count()
    }

    /// Conservative accounting for decoded storage plus serialized bytes
    /// during admission. This is not a measured allocator/process peak: the
    /// release size gate still needs an independent real-memory receipt.
    pub fn accounted_bytes(&self) -> usize {
        self.pack
            .logical_resident_bytes()
            .saturating_add(self.pack.encoded_bytes())
    }

    /// A hit is not a global negative until all first exits have been
    /// continued on the query board. The caller, not this pack, must prove
    /// that `entries` are globally reachable under its search semantics.
    #[allow(clippy::too_many_arguments)]
    pub fn lookup_composed_for_proven_entries(
        &self,
        width: u8,
        height: u8,
        board: u64,
        frame: LocalRelationRowFrame,
        piece: PieceKind,
        profile: KickTableProfileId,
        window: ConditionedPoseWindow,
        entries: &[ConditionedReachabilityEntryPose],
    ) -> LocalRelationProductLookup {
        self.lookup_composed_for_proven_entries_with(
            width,
            height,
            board,
            frame,
            piece,
            profile,
            window,
            entries,
            |exits| {
                crate::backend::exact_entry_lock_anchors(
                    width, height, board, piece, profile, exits,
                )
            },
        )
    }

    /// The solver can supply its already compiled exact template and scratch
    /// for exit continuation. A full-height closed relation has no exits and
    /// never invokes the continuation. A missing/invalid relation still falls
    /// through to the existing exact search.
    #[allow(clippy::too_many_arguments)]
    pub fn lookup_composed_for_proven_entries_with<F>(
        &self,
        width: u8,
        height: u8,
        board: u64,
        frame: LocalRelationRowFrame,
        piece: PieceKind,
        profile: KickTableProfileId,
        window: ConditionedPoseWindow,
        entries: &[ConditionedReachabilityEntryPose],
        continuation: F,
    ) -> LocalRelationProductLookup
    where
        F: FnOnce(&[ConditionedReachabilityEntryPose]) -> Option<[u64; 4]>,
    {
        match self
            .pack
            .lookup_with_frame(width, height, board, frame, piece, profile, window, entries)
        {
            LocalRelationCandidateLookup::Hit(record) => {
                let mut anchors = record.grounded_lock_anchors();
                if !record.exits().is_empty() {
                    let Some(continued) = continuation(record.exits()) else {
                        return LocalRelationProductLookup::PassThrough(
                            ProviderStatus::InvalidAsset,
                        );
                    };
                    for (local, global) in anchors.iter_mut().zip(continued) {
                        *local |= global;
                    }
                }
                LocalRelationProductLookup::ComposedForProvenEntries(anchors)
            }
            LocalRelationCandidateLookup::Miss => {
                LocalRelationProductLookup::PassThrough(ProviderStatus::Miss)
            }
            LocalRelationCandidateLookup::OutOfScope => {
                LocalRelationProductLookup::PassThrough(ProviderStatus::OutOfScope)
            }
        }
    }
}

#[derive(Default)]
struct Registry {
    slots: [Option<Arc<QualifiedLocalRelationPack>>; PROFILE_SLOTS],
}

static REGISTRY: OnceLock<RwLock<Registry>> = OnceLock::new();

pub fn install_qualified_local_relation_pack(
    pack: QualifiedLocalRelationPack,
) -> Result<Option<Arc<QualifiedLocalRelationPack>>, LocalRelationProductError> {
    let _mutation = crate::legal_board::accelerator_registry_mutation_lock()
        .lock()
        .map_err(|_| LocalRelationProductError::RegistryUnavailable)?;
    let slot = profile_slot(pack.binding().kick_profile)?;
    let combined = pack
        .accounted_bytes()
        .saturating_add(installed_local_relation_bytes(Some(slot))?)
        .saturating_add(
            crate::legal_board::installed_legal_board_bytes(None)
                .map_err(|_| LocalRelationProductError::RegistryUnavailable)?,
        )
        .saturating_add(
            crate::conditioned_reachability::installed_conditioned_reachability_bytes(None)
                .map_err(|_| LocalRelationProductError::RegistryUnavailable)?,
        );
    if combined > crate::legal_board::MAX_ACTIVE_ACCELERATOR_BYTES {
        return Err(LocalRelationProductError::ActiveSessionTooLarge);
    }
    let registry = REGISTRY.get_or_init(|| RwLock::new(Registry::default()));
    let mut guard = registry
        .write()
        .map_err(|_| LocalRelationProductError::RegistryUnavailable)?;
    if let Some(active) = guard.slots[slot].as_ref() {
        if active.generation_identity() == pack.generation_identity()
            && active.signed_catalog_identity() == pack.signed_catalog_identity()
        {
            return Ok(Some(Arc::clone(active)));
        }
        if Arc::strong_count(active) > 1 {
            return Err(LocalRelationProductError::ActiveSessionInUse);
        }
    }
    Ok(guard.slots[slot].replace(Arc::new(pack)))
}

pub fn remove_qualified_local_relation_pack(
    profile: KickTableProfileId,
) -> Result<Option<Arc<QualifiedLocalRelationPack>>, LocalRelationProductError> {
    let _mutation = crate::legal_board::accelerator_registry_mutation_lock()
        .lock()
        .map_err(|_| LocalRelationProductError::RegistryUnavailable)?;
    let slot = profile_slot(profile)?;
    let registry = REGISTRY.get_or_init(|| RwLock::new(Registry::default()));
    let mut guard = registry
        .write()
        .map_err(|_| LocalRelationProductError::RegistryUnavailable)?;
    if guard.slots[slot]
        .as_ref()
        .is_some_and(|active| Arc::strong_count(active) > 1)
    {
        return Err(LocalRelationProductError::ActiveSessionInUse);
    }
    Ok(guard.slots[slot].take())
}

pub(crate) fn qualified_local_relation_snapshot(
    profile: KickTableProfileId,
) -> Option<Arc<QualifiedLocalRelationPack>> {
    let slot = profile_slot(profile).ok()?;
    REGISTRY
        .get_or_init(|| RwLock::new(Registry::default()))
        .read()
        .ok()?
        .slots[slot]
        .clone()
}

pub(crate) fn installed_local_relation_bytes(
    exclude_slot: Option<usize>,
) -> Result<usize, LocalRelationProductError> {
    let Some(registry) = REGISTRY.get() else {
        return Ok(0);
    };
    let guard = registry
        .read()
        .map_err(|_| LocalRelationProductError::RegistryUnavailable)?;
    Ok(guard
        .slots
        .iter()
        .enumerate()
        .filter(|(index, _)| Some(*index) != exclude_slot)
        .filter_map(|(_, slot)| slot.as_ref())
        .fold(0_usize, |total, pack| {
            total.saturating_add(pack.accounted_bytes())
        }))
}

pub fn active_qualified_local_relation_identity(
    profile: KickTableProfileId,
) -> Option<([u8; 32], [u8; 32])> {
    let pack = qualified_local_relation_snapshot(profile)?;
    Some((pack.generation_identity(), pack.signed_catalog_identity()))
}

#[cfg(test)]
pub(crate) fn qualified_local_relation_for_solver_test(
    pack: LocalRelationCandidatePack,
) -> QualifiedLocalRelationPack {
    // The signature admission itself is tested separately. This constructor
    // exists only in unit-test builds so the solver can exercise a pinned
    // qualified snapshot without introducing a production signature bypass.
    QualifiedLocalRelationPack {
        pack,
        signed_catalog_identity: [17; 32],
        bounded_exhaustive_identity: [19; 32],
    }
}

fn profile_slot(profile: KickTableProfileId) -> Result<usize, LocalRelationProductError> {
    Ok(match profile {
        KickTableProfileId::Srs90 => 0,
        KickTableProfileId::SrsPlus => 1,
        KickTableProfileId::SrsX => 2,
        KickTableProfileId::Jstris180 => 3,
        KickTableProfileId::NoKick => 4,
        _ => return Err(LocalRelationProductError::UnsupportedProfile),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conditioned_local_pack::{
        built_in_local_relation_binding, encode_local_relation_candidate_pack,
        load_local_relation_candidate_pack,
    };
    use crate::conditioned_local_relation::derive_exact_conditioned_local_relation;
    use clearra_accelerator_activation::{
        verify_accelerator_envelope, PinnedPublicKey, StaticPublicKeyring, ASSET_STATEMENT_SCHEMA,
        SIGNATURE_ALGORITHM, SIGNED_ASSET_ENVELOPE_SCHEMA,
    };
    use clearra_core_domain::piece::rotation::RotationState;
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::json;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    fn fixture() -> (
        Vec<u8>,
        LocalRelationBinding,
        ConditionedPoseWindow,
        ConditionedReachabilityEntryPose,
    ) {
        let profile = KickTableProfileId::SrsPlus;
        let binding = built_in_local_relation_binding(profile).unwrap();
        let window = ConditionedPoseWindow {
            min_x: 4,
            max_x: 4,
            min_y: 4,
            max_y: 4,
        };
        let entry = ConditionedReachabilityEntryPose {
            rotation: RotationState::Zero,
            x: 4,
            y: 4,
        };
        let record = derive_exact_conditioned_local_relation(
            10,
            4,
            0,
            PieceKind::T,
            profile,
            window,
            &[entry],
        )
        .unwrap();
        let bytes = encode_local_relation_candidate_pack(binding, &[record]).unwrap();
        (bytes, binding, window, entry)
    }

    fn signed_authority(
        pack: &LocalRelationCandidatePack,
        scope: &str,
        payload_identity: [u8; 32],
    ) -> VerifiedAcceleratorAuthority {
        // This fixed test-only key is unrelated to the release keyring and
        // never enters a product catalog or runtime configuration.
        let signing = SigningKey::from_bytes(&[71_u8; 32]);
        let statement = serde_json::to_string(&json!({
            "algorithm": SIGNATURE_ALGORITHM,
            "asset_url": "https://github.com/daejunnom/Clearra/releases/download/test/local.cllr",
            "completeness_scope": scope,
            "generation_identity": hex(&pack.generation_identity()),
            "key_id": "test-only-local-relation",
            "payload_bytes": pack.encoded_bytes().to_string(),
            "payload_identity": hex(&payload_identity),
            "product": AcceleratorProduct::BoardConditionedReachability.as_str(),
            "profile": "srs-plus",
            "qualification_identity": "aa".repeat(32),
            "repository": "daejunnom/Clearra",
            "revision": "bb".repeat(20),
            "rule_identity": hex(&pack.binding().rule_identity),
            "schema": ASSET_STATEMENT_SCHEMA
        }))
        .unwrap();
        let mut signed = b"clearra.accelerator.asset-statement.v1\0".to_vec();
        signed.extend_from_slice(statement.as_bytes());
        let envelope = serde_json::to_string(&json!({
            "schema": SIGNED_ASSET_ENVELOPE_SCHEMA,
            "signature_hex": hex(&signing.sign(&signed).to_bytes()),
            "statement_json": statement
        }))
        .unwrap();
        let keys = [PinnedPublicKey {
            key_id: "test-only-local-relation",
            public_key: signing.verifying_key().to_bytes(),
        }];
        verify_accelerator_envelope(&envelope, StaticPublicKeyring::new(&keys)).unwrap()
    }

    #[test]
    fn complete_file_signature_and_scope_are_required_before_composed_lookup() {
        let (bytes, binding, window, entry) = fixture();
        let load = || load_local_relation_candidate_pack(&bytes, binding, None).unwrap();
        let expected = crate::backend::exact_entry_lock_anchors(
            10,
            4,
            0,
            PieceKind::T,
            KickTableProfileId::SrsPlus,
            &[entry],
        )
        .unwrap();
        let authority = signed_authority(
            &load(),
            LOCAL_RELATION_COMPLETENESS_SCOPE,
            load().encoded_identity(),
        );
        let qualified = QualifiedLocalRelationPack::qualify(load(), &authority)
            .unwrap_or_else(|error| panic!("test authority must qualify: {error:?}"));
        assert_eq!(
            qualified.lookup_composed_for_proven_entries(
                10,
                4,
                0,
                LocalRelationRowFrame::new(4, 0).unwrap(),
                PieceKind::T,
                KickTableProfileId::SrsPlus,
                window,
                &[entry],
            ),
            LocalRelationProductLookup::ComposedForProvenEntries(expected)
        );
        let mut continued = false;
        assert_eq!(
            qualified.lookup_composed_for_proven_entries_with(
                10,
                4,
                0,
                LocalRelationRowFrame::new(4, 0).unwrap(),
                PieceKind::T,
                KickTableProfileId::SrsPlus,
                window,
                &[entry],
                |exits| {
                    continued = true;
                    crate::backend::exact_entry_lock_anchors(
                        10,
                        4,
                        0,
                        PieceKind::T,
                        KickTableProfileId::SrsPlus,
                        exits,
                    )
                },
            ),
            LocalRelationProductLookup::ComposedForProvenEntries(expected)
        );
        assert!(continued);
        assert_eq!(
            qualified.lookup_composed_for_proven_entries_with(
                10,
                4,
                0,
                LocalRelationRowFrame::new(4, 0).unwrap(),
                PieceKind::T,
                KickTableProfileId::SrsPlus,
                window,
                &[entry],
                |_| None,
            ),
            LocalRelationProductLookup::PassThrough(ProviderStatus::InvalidAsset)
        );
        assert_eq!(
            qualified.lookup_composed_for_proven_entries(
                10,
                4,
                0,
                LocalRelationRowFrame::new(4, 0).unwrap(),
                PieceKind::T,
                KickTableProfileId::SrsPlus,
                window,
                &[],
            ),
            LocalRelationProductLookup::PassThrough(ProviderStatus::OutOfScope)
        );
        let wrong_scope = signed_authority(
            &load(),
            "legacy-sparse-spawn-lock",
            load().encoded_identity(),
        );
        assert!(matches!(
            QualifiedLocalRelationPack::qualify(load(), &wrong_scope),
            Err(LocalRelationProductError::NotQualified)
        ));
        let wrong_payload = signed_authority(&load(), LOCAL_RELATION_COMPLETENESS_SCOPE, [3; 32]);
        assert!(matches!(
            QualifiedLocalRelationPack::qualify(load(), &wrong_payload),
            Err(LocalRelationProductError::NotQualified)
        ));
    }
}
