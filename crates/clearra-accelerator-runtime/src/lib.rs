//! Native host adapter for accelerator payloads. File stores and CLI surfaces
//! never import the solver core: this crate alone translates signed product
//! authority into parsed, profile-bound immutable solver data.

use clearra_accelerator_product_host::{ProductCatalogKind, QualifiedCatalogAsset};
use clearra_core_executor::{
    active_qualified_exact_legal_board_identity, active_qualified_local_relation_identity,
    built_in_legal_board_binding, built_in_local_relation_binding,
    install_qualified_exact_legal_board, install_qualified_local_relation_pack,
    load_local_relation_candidate_pack, remove_qualified_exact_legal_board,
    remove_qualified_local_relation_pack, ExactLegalBoard, LegalBoardExpectation,
    QualifiedExactLegalBoard, QualifiedLocalRelationPack,
};
use clearra_rules::kicks::KickTableProfileId;
use std::sync::Arc;

pub enum QualifiedAccelerator {
    Exact(QualifiedExactLegalBoard),
    Conditioned(QualifiedLocalRelationPack),
}

pub fn active_identity(kind: ProductCatalogKind, profile: &str) -> Option<([u8; 32], [u8; 32])> {
    let kick = KickTableProfileId::parse(profile)?;
    match kind {
        ProductCatalogKind::ExactLegalBoard => active_qualified_exact_legal_board_identity(kick),
        ProductCatalogKind::BoardConditionedReachability => {
            active_qualified_local_relation_identity(kick)
        }
    }
}

pub fn qualify_signed(
    kind: ProductCatalogKind,
    profile: &str,
    bytes: Arc<[u8]>,
    asset: &QualifiedCatalogAsset,
) -> Result<QualifiedAccelerator, &'static str> {
    let kick = KickTableProfileId::parse(profile)
        .ok_or("accelerator: profile is not connected to a kick table")?;
    match kind {
        ProductCatalogKind::ExactLegalBoard => {
            let binding = built_in_legal_board_binding(kick)
                .map_err(|_| "accelerator: legal-board binding unavailable")?;
            let loaded = ExactLegalBoard::load(
                bytes,
                LegalBoardExpectation {
                    binding,
                    generation_identity: Some(asset.authority().generation_identity()),
                },
            )
            .map_err(|_| "accelerator: exact legal-board payload invalid")?;
            let qualified = QualifiedExactLegalBoard::qualify(loaded, asset.authority())
                .map_err(|_| "accelerator: exact legal-board qualification mismatch")?;
            if qualified.shared_bytes() as u64 > asset.metadata().active_session_shared_bytes() {
                return Err("accelerator: exact legal-board exceeds resident-size proof");
            }
            Ok(QualifiedAccelerator::Exact(qualified))
        }
        ProductCatalogKind::BoardConditionedReachability => {
            let binding = built_in_local_relation_binding(kick)
                .map_err(|_| "accelerator: conditioned-reachability binding unavailable")?;
            let loaded = load_local_relation_candidate_pack(
                &bytes,
                binding,
                Some(asset.authority().generation_identity()),
            )
            .map_err(|_| "accelerator: conditioned-reachability payload invalid")?;
            let qualified = QualifiedLocalRelationPack::qualify(loaded, asset.authority())
                .map_err(|_| "accelerator: conditioned-reachability qualification mismatch")?;
            if qualified.accounted_bytes() as u64 > asset.metadata().active_session_shared_bytes() {
                return Err("accelerator: conditioned-reachability exceeds resident-size proof");
            }
            if !matches!(asset.metadata(),
                clearra_accelerator_product_host::QualifiedProductMetadata::BoardConditionedReachability { record_count, .. }
                    if *record_count == qualified.record_count() as u64
            ) {
                return Err("accelerator: conditioned-reachability record-count mismatch");
            }
            Ok(QualifiedAccelerator::Conditioned(qualified))
        }
    }
}

pub fn install(asset: QualifiedAccelerator) -> Result<(), &'static str> {
    match asset {
        QualifiedAccelerator::Exact(board) => install_qualified_exact_legal_board(board)
            .map(|_| ())
            .map_err(|_| "accelerator: exact legal-board installation rejected"),
        QualifiedAccelerator::Conditioned(pack) => install_qualified_local_relation_pack(pack)
            .map(|_| ())
            .map_err(|_| "accelerator: conditioned-reachability installation rejected"),
    }
}

/// Removing a locally installed generation must also revoke its in-process
/// lookup authority. An active session retains its immutable snapshot and
/// makes removal fail before the persistent pointer is changed.
pub fn remove(kind: ProductCatalogKind, profile: &str) -> Result<(), &'static str> {
    let kick = KickTableProfileId::parse(profile)
        .ok_or("accelerator: profile is not connected to a kick table")?;
    match kind {
        ProductCatalogKind::ExactLegalBoard => remove_qualified_exact_legal_board(kick)
            .map(|_| ())
            .map_err(|_| "accelerator: legal-board generation is in use"),
        ProductCatalogKind::BoardConditionedReachability => {
            remove_qualified_local_relation_pack(kick)
                .map(|_| ())
                .map_err(|_| "accelerator: conditioned-reachability generation is in use")
        }
    }
}

pub fn structurally_valid_candidate(
    kind: ProductCatalogKind,
    profile: &str,
    bytes: Arc<[u8]>,
) -> bool {
    let Some(kick) = KickTableProfileId::parse(profile) else {
        return false;
    };
    match kind {
        ProductCatalogKind::ExactLegalBoard => {
            built_in_legal_board_binding(kick)
                .ok()
                .is_some_and(|binding| {
                    ExactLegalBoard::load(
                        bytes,
                        LegalBoardExpectation {
                            binding,
                            generation_identity: None,
                        },
                    )
                    .is_ok()
                })
        }
        ProductCatalogKind::BoardConditionedReachability => built_in_local_relation_binding(kick)
            .ok()
            .is_some_and(|binding| {
                load_local_relation_candidate_pack(&bytes, binding, None).is_ok()
            }),
    }
}

/// Diagnostic-only compatibility for a pre-v0.8.1 sparse candidate. The
/// product catalog, installer and solver session never consume this format.
pub fn structurally_valid_legacy_sparse_candidate(profile: &str, bytes: Arc<[u8]>) -> bool {
    let Some(kick) = KickTableProfileId::parse(profile) else {
        return false;
    };
    let Ok(binding) = clearra_core_executor::built_in_conditioned_reachability_binding(kick) else {
        return false;
    };
    clearra_core_executor::BoardConditionedReachability::load(
        bytes,
        clearra_core_executor::ConditionedReachabilityExpectation {
            binding,
            generation_identity: None,
        },
    )
    .is_ok()
}

#[cfg(test)]
mod tests {
    use clearra_accelerator_product_host::ProductCatalogKind;

    #[test]
    fn catalog_scopes_match_the_exact_product_parsers() {
        assert_eq!(
            ProductCatalogKind::ExactLegalBoard.completeness_scope(),
            clearra_core_executor::EXACT_LEGAL_BOARD_COMPLETENESS_SCOPE
        );
        assert_eq!(
            ProductCatalogKind::BoardConditionedReachability.completeness_scope(),
            clearra_core_executor::LOCAL_RELATION_COMPLETENESS_SCOPE
        );
    }
}
