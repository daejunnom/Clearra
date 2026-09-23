use clearra_accelerator_product_host::{
    embedded_catalog, CatalogProfileStatus, ConditionedRelationContract, ProductCatalogKind,
};
use std::process::ExitCode;

const LEGAL_BOARD_AGGREGATE_LIMIT: u64 = 320 * 1024 * 1024;
const CONDITIONED_AGGREGATE_LIMIT: u64 = 80 * 1024 * 1024;
const ACTIVE_PROFILE_LIMIT: u64 = 128 * 1024 * 1024;

fn main() -> ExitCode {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments != ["--require-all-v0.8.1"] {
        eprintln!("usage: clearra-accelerator-release-gate --require-all-v0.8.1");
        return ExitCode::from(2);
    }

    match require_v081_qualification() {
        Ok(()) => {
            println!(
                "accelerator_release_gate=passed legal_board_profiles=5 conditioned_reachability_profiles=5"
            );
            ExitCode::SUCCESS
        }
        Err(reason) => {
            eprintln!("accelerator_release_gate=failed reason={reason}");
            ExitCode::from(1)
        }
    }
}

fn require_v081_qualification() -> Result<(), String> {
    let legal = embedded_catalog(ProductCatalogKind::ExactLegalBoard)
        .map_err(|error| format!("legal_board_catalog_{}", error.code()))?;
    let conditioned = embedded_catalog(ProductCatalogKind::BoardConditionedReachability)
        .map_err(|error| format!("conditioned_catalog_{}", error.code()))?;

    let legal_total = qualified_total(&legal, "legal_board")?;
    let conditioned_total = qualified_total(&conditioned, "conditioned_reachability")?;
    if legal_total > LEGAL_BOARD_AGGREGATE_LIMIT {
        return Err("legal_board_aggregate_limit".to_owned());
    }
    if conditioned_total > CONDITIONED_AGGREGATE_LIMIT {
        return Err("conditioned_reachability_aggregate_limit".to_owned());
    }

    for profile in legal.profiles() {
        let legal_bytes =
            qualified_resident_bytes(profile.status(), "legal_board", profile.profile())?;
        let conditioned_status = conditioned.profile(profile.profile()).ok_or_else(|| {
            format!(
                "conditioned_reachability_profile_missing_{}",
                profile.profile()
            )
        })?;
        let conditioned_bytes = qualified_resident_bytes(
            conditioned_status,
            "conditioned_reachability",
            profile.profile(),
        )?;
        if legal_bytes.saturating_add(conditioned_bytes) > ACTIVE_PROFILE_LIMIT {
            return Err(format!("active_profile_limit_{}", profile.profile()));
        }
    }
    require_local_entry_exit_contract()?;
    Ok(())
}

fn require_local_entry_exit_contract() -> Result<(), String> {
    // The checked-in conditioned catalog currently authorizes only sparse
    // spawn-to-lock records. Even five signed qualified profiles cannot
    // satisfy the planned entry-to-first-exit BuildUp relation with that
    // parser. Check this *after* catalog/profile/size diagnostics so the gate
    // still reports those independently.
    if ProductCatalogKind::BoardConditionedReachability.conditioned_relation_contract()
        != Some(ConditionedRelationContract::ActualEntryToFirstExit)
    {
        return Err("conditioned_local_entry_exit_product_not_implemented".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::require_local_entry_exit_contract;

    #[test]
    fn sparse_spawn_to_lock_catalog_cannot_satisfy_local_relation_release_gate() {
        assert_eq!(
            require_local_entry_exit_contract(),
            Err("conditioned_local_entry_exit_product_not_implemented".to_owned())
        );
    }
}

fn qualified_resident_bytes(
    status: &CatalogProfileStatus,
    product: &str,
    profile: &str,
) -> Result<u64, String> {
    match status {
        CatalogProfileStatus::Qualified(asset) => {
            Ok(asset.metadata().active_session_shared_bytes())
        }
        CatalogProfileStatus::NotQualified => {
            Err(format!("{product}_profile_not_qualified_{profile}"))
        }
    }
}

fn qualified_total(
    catalog: &clearra_accelerator_product_host::VerifiedProductCatalog,
    product: &str,
) -> Result<u64, String> {
    catalog.profiles().iter().try_fold(0_u64, |total, profile| {
        qualified_bytes(profile.status(), product, profile.profile())
            .map(|bytes| total.saturating_add(bytes))
    })
}

fn qualified_bytes(
    status: &CatalogProfileStatus,
    product: &str,
    profile: &str,
) -> Result<u64, String> {
    match status {
        CatalogProfileStatus::Qualified(asset) => Ok(asset.authority().payload_bytes()),
        CatalogProfileStatus::NotQualified => {
            Err(format!("{product}_profile_not_qualified_{profile}"))
        }
    }
}
