//! Host-owned, signed accelerator admission for the browser's single WASM
//! owner. Downloaded bytes have no authority until the embedded catalog and
//! product parser agree; no search job may replace a live generation.

use super::*;
use clearra_accelerator_product_host::{
    embedded_catalog, CatalogProfileStatus, ProductCatalogKind, QualifiedCatalogAsset,
};
use clearra_core_executor::{
    built_in_legal_board_binding, built_in_local_relation_binding,
    install_qualified_exact_legal_board, install_qualified_local_relation_pack,
    load_local_relation_candidate_pack, remove_qualified_exact_legal_board,
    remove_qualified_local_relation_pack, ExactLegalBoard, LegalBoardExpectation,
    QualifiedExactLegalBoard, QualifiedLocalRelationPack,
};
use clearra_rules::kicks::KickTableProfileId;
use serde_json::json;

const PROFILES: [(&str, KickTableProfileId); 5] = [
    ("srs", KickTableProfileId::Srs90),
    ("srs-plus", KickTableProfileId::SrsPlus),
    ("srs-x", KickTableProfileId::SrsX),
    ("jstris-180", KickTableProfileId::Jstris180),
    ("no-kick", KickTableProfileId::NoKick),
];

fn selection(
    kind: u32,
    profile: u32,
) -> Option<(ProductCatalogKind, &'static str, KickTableProfileId)> {
    let kind = match kind {
        0 => ProductCatalogKind::ExactLegalBoard,
        1 => ProductCatalogKind::BoardConditionedReachability,
        _ => return None,
    };
    let (name, kick) = *PROFILES.get(profile as usize)?;
    Some((kind, name, kick))
}

fn hex(value: [u8; 32]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Returns a UI/download plan. This reads only the source-embedded catalog;
/// it never follows a mutable ref or opens the network.
pub(super) fn catalog(kind: u32, profile: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        let Some((kind, name, _)) = selection(kind, profile) else {
            state.set_error(
                "accelerator_selection_invalid",
                "unknown product or profile",
            );
            return ABI_ERROR;
        };
        let catalog = match embedded_catalog(kind) {
            Ok(catalog) => catalog,
            Err(error) => {
                state.set_error(error.code(), "embedded signed catalog is invalid");
                return ABI_ERROR;
            }
        };
        let value = match catalog.profile(name) {
            Some(CatalogProfileStatus::NotQualified) => json!({
                "product": kind.as_str(), "profile": name, "state": "not_qualified",
                "payload_bytes": null, "generation": null, "payload_identity": null,
                "url": null, "catalog_identity": hex(catalog.catalog_identity())
            }),
            Some(CatalogProfileStatus::Qualified(asset)) => json!({
                "product": kind.as_str(), "profile": name, "state": "qualified",
                "payload_bytes": asset.authority().payload_bytes(),
                "generation": hex(asset.authority().generation_identity()),
                "payload_identity": hex(asset.authority().payload_identity()),
                "url": asset.authority().asset_url(),
                "catalog_identity": hex(catalog.catalog_identity())
            }),
            None => {
                state.set_error("accelerator_profile_missing", "profile absent from catalog");
                return ABI_ERROR;
            }
        };
        state.set_output(value.to_string());
        ABI_OK
    })
}

fn qualify(
    kind: ProductCatalogKind,
    kick: KickTableProfileId,
    bytes: Arc<[u8]>,
    asset: &QualifiedCatalogAsset,
    install: bool,
) -> Result<(), &'static str> {
    if bytes.len() as u64 != asset.authority().payload_bytes() {
        return Err("accelerator_payload_size_mismatch");
    }
    match kind {
        ProductCatalogKind::ExactLegalBoard => {
            let binding = built_in_legal_board_binding(kick)
                .map_err(|_| "accelerator_rule_binding_unavailable")?;
            let board = ExactLegalBoard::load(
                bytes,
                LegalBoardExpectation {
                    binding,
                    generation_identity: Some(asset.authority().generation_identity()),
                },
            )
            .map_err(|_| "accelerator_legal_board_payload_invalid")?;
            let board = QualifiedExactLegalBoard::qualify(board, asset.authority())
                .map_err(|_| "accelerator_legal_board_not_qualified")?;
            if board.shared_bytes() as u64 > asset.metadata().active_session_shared_bytes() {
                return Err("accelerator_resident_size_exceeded");
            }
            if install {
                install_qualified_exact_legal_board(board)
                    .map_err(|_| "accelerator_legal_board_install_rejected")?;
            }
        }
        ProductCatalogKind::BoardConditionedReachability => {
            let binding = built_in_local_relation_binding(kick)
                .map_err(|_| "accelerator_rule_binding_unavailable")?;
            let pack = load_local_relation_candidate_pack(
                &bytes,
                binding,
                Some(asset.authority().generation_identity()),
            )
            .map_err(|_| "accelerator_reachability_payload_invalid")?;
            let pack = QualifiedLocalRelationPack::qualify(pack, asset.authority())
                .map_err(|_| "accelerator_reachability_not_qualified")?;
            if pack.accounted_bytes() as u64 > asset.metadata().active_session_shared_bytes() {
                return Err("accelerator_resident_size_exceeded");
            }
            if !matches!(asset.metadata(),
                clearra_accelerator_product_host::QualifiedProductMetadata::BoardConditionedReachability { record_count, .. }
                    if *record_count == pack.record_count() as u64
            ) {
                return Err("accelerator_record_count_mismatch");
            }
            if install {
                install_qualified_local_relation_pack(pack)
                    .map_err(|_| "accelerator_reachability_install_rejected")?;
            }
        }
    }
    Ok(())
}

/// The transfer buffer contains exactly one immutable asset. `activate=0`
/// validates a download before publication; `activate=1` additionally pins
/// it in this WASM owner's registry. A worker never copies the active asset to
/// its peers. Peers without an installed generation use the exact fallback.
pub(super) fn admit(kind: u32, profile: u32, activate: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        if state.has_worker_job_start_conflict() || activate > 1 {
            state.set_error(
                "accelerator_session_in_use",
                "asset mutation requires an idle owner",
            );
            return ABI_ERROR;
        }
        let Some((kind, name, kick)) = selection(kind, profile) else {
            state.set_error(
                "accelerator_selection_invalid",
                "unknown product or profile",
            );
            return ABI_ERROR;
        };
        let catalog = match embedded_catalog(kind) {
            Ok(value) => value,
            Err(error) => {
                state.set_error(error.code(), "embedded signed catalog is invalid");
                return ABI_ERROR;
            }
        };
        let Some(CatalogProfileStatus::Qualified(asset)) = catalog.profile(name) else {
            state.set_error(
                "accelerator_not_qualified",
                "profile has no qualified signed asset",
            );
            return ABI_ERROR;
        };
        let bytes: Arc<[u8]> = std::mem::take(&mut state.transfer_input).into();
        match qualify(kind, kick, bytes, asset, activate == 1) {
            Ok(()) => {
                state.set_output(
                    json!({"state": if activate == 1 { "ready" } else { "validated" },
                    "product": kind.as_str(), "profile": name,
                    "generation": hex(asset.authority().generation_identity())})
                    .to_string(),
                );
                ABI_OK
            }
            Err(code) => {
                state.set_error(code, "signed accelerator admission failed");
                ABI_ERROR
            }
        }
    })
}

pub(super) fn remove(kind: u32, profile: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        if state.has_worker_job_start_conflict() {
            state.set_error(
                "accelerator_session_in_use",
                "asset mutation requires an idle owner",
            );
            return ABI_ERROR;
        }
        let Some((kind, name, kick)) = selection(kind, profile) else {
            state.set_error(
                "accelerator_selection_invalid",
                "unknown product or profile",
            );
            return ABI_ERROR;
        };
        let removed = match kind {
            ProductCatalogKind::ExactLegalBoard => remove_qualified_exact_legal_board(kick)
                .map(|value| value.is_some())
                .map_err(|_| ()),
            ProductCatalogKind::BoardConditionedReachability => {
                remove_qualified_local_relation_pack(kick)
                    .map(|value| value.is_some())
                    .map_err(|_| ())
            }
        };
        match removed {
            Ok(removed) => {
                state.set_output(
                    json!({"state": "not_loaded", "product": kind.as_str(),
                    "profile": name, "removed": removed})
                    .to_string(),
                );
                ABI_OK
            }
            Err(_) => {
                state.set_error(
                    "accelerator_session_in_use",
                    "active search retains this generation",
                );
                ABI_ERROR
            }
        }
    })
}
