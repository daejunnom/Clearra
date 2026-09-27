//! SRP rationale: trusted local-owner relation queries between WASM quanta.
//! No network I/O and no whole-pack peer transfer. Asset installation/removal
//! retains the stricter idle-owner admission in accelerator_exports.

use super::*;
use clearra_accelerator_product_host::{
    embedded_catalog, CatalogProfileStatus, ProductCatalogKind,
};
use clearra_core_executor::{
    active_qualified_local_relation_identity, answer_qualified_local_relation_peer_queries,
    drain_local_relation_peer_queries, export_qualified_local_relation_peer_seed,
    import_trusted_local_relation_peer_reply, install_trusted_local_relation_peer,
    MAX_RELATION_PEER_WIRE_BYTES,
};
use clearra_rules::kicks::KickTableProfileId;

const PROFILES: [(&str, KickTableProfileId); 5] = [
    ("srs", KickTableProfileId::Srs90),
    ("srs-plus", KickTableProfileId::SrsPlus),
    ("srs-x", KickTableProfileId::SrsX),
    ("jstris-180", KickTableProfileId::Jstris180),
    ("no-kick", KickTableProfileId::NoKick),
];

pub(super) fn seed(profile: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        if state.has_worker_job_start_conflict() {
            state.set_error(
                "accelerator_session_in_use",
                "peer seed export requires an idle full owner",
            );
            return ABI_ERROR;
        }
        let result = (|| {
            let (name, kick) = *PROFILES
                .get(profile as usize)
                .ok_or("accelerator_selection_invalid")?;
            let catalog = embedded_catalog(ProductCatalogKind::BoardConditionedReachability)
                .map_err(|error| error.code())?;
            let Some(CatalogProfileStatus::Qualified(asset)) = catalog.profile(name) else {
                return Err("accelerator_not_qualified");
            };
            if active_qualified_local_relation_identity(kick)
                != Some((
                    asset.authority().generation_identity(),
                    asset.authority().statement_identity(),
                ))
            {
                return Err("accelerator_snapshot_mismatch");
            }
            export_qualified_local_relation_peer_seed(kick).map_err(|error| error.code())
        })();
        match result {
            Ok(wire) => {
                state.set_output_bytes(wire);
                ABI_OK
            }
            Err(code) => {
                state.set_error(code, "qualified relation peer seed unavailable");
                ABI_ERROR
            }
        }
    })
}

pub(super) fn admit(profile: u32, reserved_bytes: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_mutation_admission() {
            return status;
        }
        if state.has_external_compute_owner() {
            state.set_error(
                "accelerator_session_in_use",
                "peer installation requires an idle verifier",
            );
            return ABI_ERROR;
        }
        let wire = std::mem::take(&mut state.transfer_input);
        let result = (|| {
            let (name, _) = *PROFILES
                .get(profile as usize)
                .ok_or("accelerator_selection_invalid")?;
            let catalog = embedded_catalog(ProductCatalogKind::BoardConditionedReachability)
                .map_err(|error| error.code())?;
            let Some(CatalogProfileStatus::Qualified(asset)) = catalog.profile(name) else {
                return Err("accelerator_not_qualified");
            };
            install_trusted_local_relation_peer(&wire, asset.authority(), reserved_bytes as usize)
                .map_err(|error| error.code())
        })();
        match result {
            Ok(()) => {
                state.set_output_bytes(Vec::new());
                ABI_OK
            }
            Err(code) => {
                state.set_error(code, "trusted peer admission failed; use exact search");
                ABI_ERROR
            }
        }
    })
}

pub(super) fn drain(profile: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_worker_lifecycle_admission() {
            return status;
        }
        let Some((_, kick)) = PROFILES.get(profile as usize) else {
            state.set_error("accelerator_selection_invalid", "unknown peer profile");
            return ABI_ERROR;
        };
        match drain_local_relation_peer_queries(*kick) {
            Ok(wire) => {
                state.set_output_bytes(wire.unwrap_or_default());
                ABI_OK
            }
            Err(error) => {
                state.set_error(error.code(), "relation peer drain failed");
                ABI_ERROR
            }
        }
    })
}

/// Read-only full-owner lookup is permitted BETWEEN active producer quanta.
/// It cannot install an asset, change generation, or inspect solver state.
pub(super) fn answer(profile: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_worker_lifecycle_admission() {
            return status;
        }
        let wire = std::mem::take(&mut state.transfer_input);
        let Some((_, kick)) = PROFILES.get(profile as usize) else {
            state.set_error(
                "accelerator_selection_invalid",
                "unknown full-owner profile",
            );
            return ABI_ERROR;
        };
        match answer_qualified_local_relation_peer_queries(*kick, &wire) {
            Ok(reply) => {
                state.set_output_bytes(reply);
                ABI_OK
            }
            Err(error) => {
                state.set_error(error.code(), "full-owner query failed");
                ABI_ERROR
            }
        }
    })
}

/// Failure is fatal to the CURRENT search result, including already applied
/// partials. Hosts must never swallow a post-admission import error.
pub(super) fn import(profile: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_worker_lifecycle_admission() {
            return status;
        }
        let wire = std::mem::take(&mut state.transfer_input);
        let Some((_, kick)) = PROFILES.get(profile as usize) else {
            state.set_error("accelerator_selection_invalid", "unknown peer profile");
            return ABI_ERROR;
        };
        match import_trusted_local_relation_peer_reply(*kick, &wire) {
            Ok(()) => {
                state.set_output_bytes(Vec::new());
                ABI_OK
            }
            Err(error) => {
                state.set_error(
                    error.code(),
                    "relation cache invalidated; discard current search result",
                );
                ABI_ERROR
            }
        }
    })
}

/// Dedicated bounded transport staging remains usable at worker yield points
/// without changing the finite-job/large-transfer admission rules. It cannot
/// overwrite a pending candidate packet or product request.
pub(super) fn transfer_resize(byte_len: u32) -> i32 {
    ABI_STATE.with(|state| {
        let mut state = state.borrow_mut();
        if let Err(status) = state.require_worker_lifecycle_admission() {
            return status;
        }
        if byte_len as usize > MAX_RELATION_PEER_WIRE_BYTES || !state.transfer_input.is_empty() {
            state.set_error(
                "accelerator_peer_transfer_rejected",
                "bounded empty transfer slot required",
            );
            return ABI_ERROR;
        }
        state.transfer_input.resize(byte_len as usize, 0);
        ABI_OK
    })
}
