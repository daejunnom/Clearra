//! Host-owned, signed accelerator admission for the browser's single WASM
//! owner. Downloaded bytes have no authority until the embedded catalog and
//! product parser agree; no search job may replace a live generation.

use super::*;
use clearra_accelerator_product_host::{
    embedded_catalog, CatalogProfileStatus, ProductCatalogKind, QualifiedCatalogAsset,
    QualifiedProductMetadata,
};
use clearra_core_executor::{
    active_qualified_exact_legal_board_identity, built_in_legal_board_binding,
    built_in_local_relation_binding, export_qualified_legal_board_synopsis,
    install_qualified_exact_legal_board, install_qualified_local_relation_pack,
    install_trusted_legal_board_synopsis, load_local_relation_candidate_pack,
    remove_qualified_exact_legal_board, remove_qualified_local_relation_pack,
    remove_trusted_legal_board_synopsis, ExactLegalBoard, LegalBoardExpectation,
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

/// Parse the same typed request that the executor will run. The browser host
/// must not infer a profile or an opt-out from command-text regular expressions:
/// defaults and command-family policies belong to the shared Rust parser.
pub(super) fn request_policy() -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        let command_text = match String::from_utf8(std::mem::take(&mut state.input)) {
            Ok(command_text) => command_text,
            Err(error) => {
                state.set_error("E_WASM_COMMAND_UTF8", error.utf8_error());
                return ABI_ERROR;
            }
        };
        let request = match state
            .runtime
            .command_runtime()
            .compile_command_text(&command_text)
        {
            Ok(request) => request,
            Err(error) => {
                state.set_runtime_error(&error);
                return ABI_ERROR;
            }
        };
        let (legal_board, conditioned_reachability) = request
            .command()
            .exact_accelerator_policy()
            .unwrap_or((false, false));
        let rule = request.request_profiles().rule();
        let profile = PROFILES.iter().position(|(name, _)| *name == rule.as_str());
        state.set_output(
            json!({
                "profile": profile,
                "legal_board": legal_board,
                "conditioned_reachability": conditioned_reachability,
            })
            .to_string(),
        );
        ABI_OK
    })
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
                "url": null, "catalog_identity": hex(catalog.catalog_identity()),
                "active_session_shared_bytes": null
            }),
            Some(CatalogProfileStatus::Qualified(asset)) => json!({
                "product": kind.as_str(), "profile": name, "state": "qualified",
                "payload_bytes": asset.authority().payload_bytes(),
                "generation": hex(asset.authority().generation_identity()),
                "payload_identity": hex(asset.authority().payload_identity()),
                "url": asset.authority().asset_url(),
                "active_session_shared_bytes": asset.metadata().active_session_shared_bytes(),
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
            if !matches!(asset.metadata(),
                QualifiedProductMetadata::ExactLegalBoard {
                    layer_counts, layer_payload_identities, ..
                } if board.matches_layer_manifest(layer_counts, layer_payload_identities)
            ) {
                return Err("accelerator_legal_board_layer_manifest_mismatch");
            }
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
        // Admission consumes transfer_input. A staged transfer is not an
        // executing worker; has_worker_job_start_conflict() intentionally
        // counts it for job starts and must not guard this consumer.
        if state.has_external_compute_owner() || activate > 1 {
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
                .and_then(|full| {
                    remove_trusted_legal_board_synopsis(kick)
                        .map(|summary| full.is_some() || summary)
                })
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

/// Only a WASM owner with the complete signed bundle can derive this small
/// negative-only worker transport. A null result means no qualified source;
/// peers then keep the ordinary exact verifier.
pub(super) fn export_negative_synopsis(profile: u32, maximum_bytes: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        if state.has_worker_job_start_conflict() {
            state.set_error(
                "accelerator_session_in_use",
                "synopsis export requires an idle owner",
            );
            return ABI_ERROR;
        }
        let Some((ProductCatalogKind::ExactLegalBoard, name, kick)) = selection(0, profile) else {
            state.set_error(
                "accelerator_selection_invalid",
                "unknown legal-board profile",
            );
            return ABI_ERROR;
        };
        let catalog = match embedded_catalog(ProductCatalogKind::ExactLegalBoard) {
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
        if active_qualified_exact_legal_board_identity(kick)
            != Some((
                asset.authority().generation_identity(),
                asset.authority().statement_identity(),
            ))
        {
            state.set_error(
                "accelerator_snapshot_mismatch",
                "complete owner is not installed",
            );
            return ABI_ERROR;
        }
        match export_qualified_legal_board_synopsis(kick, maximum_bytes as usize) {
            Ok(Some(wire)) => {
                state.set_output_bytes(wire);
                ABI_OK
            }
            Ok(None) => {
                state.set_error("accelerator_not_loaded", "complete owner is not installed");
                ABI_ERROR
            }
            Err(_) => {
                state.set_error(
                    "accelerator_synopsis_budget",
                    "bounded derivative unavailable",
                );
                ABI_ERROR
            }
        }
    })
}

/// Accept a derivative only from the trusted in-app worker transport. Its
/// generation, rule and complete-domain statement must match the embedded
/// signed catalog; no network response may call this export directly.
pub(super) fn admit_negative_synopsis(profile: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        // The signed derivative arrives in transfer_input, so only an
        // executing owner (not the staged bytes) conflicts with admission.
        if state.has_external_compute_owner() {
            state.set_error(
                "accelerator_session_in_use",
                "synopsis installation requires an idle worker",
            );
            return ABI_ERROR;
        }
        let Some((ProductCatalogKind::ExactLegalBoard, name, _)) = selection(0, profile) else {
            state.set_error(
                "accelerator_selection_invalid",
                "unknown legal-board profile",
            );
            return ABI_ERROR;
        };
        let catalog = match embedded_catalog(ProductCatalogKind::ExactLegalBoard) {
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
        let wire = std::mem::take(&mut state.transfer_input);
        match install_trusted_legal_board_synopsis(&wire, asset.authority()) {
            Ok(()) => {
                state.set_output(
                    json!({"state": "ready", "profile": name,
                    "generation": hex(asset.authority().generation_identity())})
                    .to_string(),
                );
                ABI_OK
            }
            Err(error) => {
                state.set_error(error.code(), "trusted synopsis admission failed");
                ABI_ERROR
            }
        }
    })
}
