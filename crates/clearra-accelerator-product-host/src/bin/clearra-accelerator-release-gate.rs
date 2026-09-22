use clearra_accelerator_product_host::{
    embedded_catalog, CatalogProfileStatus, ProductCatalogKind,
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
    Ok(())
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
