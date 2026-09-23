//! Explicit lifecycle for the profile-bound BoardConditionedReachability data
//! product. This namespace never reads or writes exact legal-board assets.

use crate::{
    accelerator_asset_store::{self, LocalAssetState},
    error::CliErrorCode,
    output::CliOutput,
};
use clearra_accelerator_product_host::ProductCatalogKind;
use clearra_i18n::LanguageId;
use clearra_pc4_qualifier::{
    generate_conditioned_reachability, ConditionedReachabilityGenerationOptions,
};
use clearra_rules::kicks::KickTableProfileId;
use serde_json::{json, Value};
use std::{fs, path::PathBuf, sync::atomic::AtomicBool};

const PRODUCT: ProductCatalogKind = ProductCatalogKind::BoardConditionedReachability;
const PROFILES: [&str; 5] = ["srs", "srs-plus", "srs-x", "jstris-180", "no-kick"];
const MAX_PRODUCT_PACK_BYTES: u64 = 16 * 1024 * 1024;

pub(crate) fn activate_for_request(request: &clearra_app::AppRequest) {
    if !request
        .command()
        .exact_accelerator_policy()
        .is_some_and(|(_, enabled)| enabled)
    {
        return;
    }
    if !matches!(
        request.command_kind(),
        clearra_host_contract::AppCommandKind::Pc
            | clearra_host_contract::AppCommandKind::Path
            | clearra_host_contract::AppCommandKind::Percent
            | clearra_host_contract::AppCommandKind::Setup
            | clearra_host_contract::AppCommandKind::BuildProbability
    ) {
        return;
    }
    let profile = request.request_profiles().rule().as_str();
    let Ok(base) = default_directory() else {
        return;
    };
    let root = accelerator_asset_store::profile_root(&base, PRODUCT, profile);
    let _ = accelerator_asset_store::activate_installed(PRODUCT, profile, &root);
}

pub(crate) fn run(args: &[String], language: LanguageId, json_output: bool) -> CliOutput {
    if args.is_empty()
        || args
            .iter()
            .any(|value| matches!(value.as_str(), "--help" | "-h"))
    {
        return CliOutput::success(help(language));
    }
    match execute(args) {
        Ok(value) if json_output => CliOutput::success(value.to_string()),
        Ok(value) => CliOutput::success(render_text(&value, language)),
        Err(reason) => CliOutput::error(CliErrorCode::CliInvalidValue, reason),
    }
}

fn help(language: LanguageId) -> String {
    let body = match language {
        LanguageId::Ko => "profile별 조건부 도달성 pack을 독립적으로 확인·다운로드·생성·삭제합니다. check는 네트워크를 사용하지 않으며, generate는 명시한 canonical query set으로 미자격 후보만 만듭니다.",
        LanguageId::Ja => "プロファイル別の条件付き到達性packを独立して確認・ダウンロード・生成・削除します。checkはネットワークを使用せず、generateは明示したcanonical query setから未適格候補だけを作成します。",
        LanguageId::En => "Inspect, download, generate, or remove one profile's conditioned-reachability pack independently. check uses no network; generate produces only an unqualified candidate from an explicit canonical query set.",
    };
    format!(
        "clearra reachability-pack <check|download|status|remove|generate> --profile srs|srs-plus|srs-x|jstris-180|no-kick [--directory DIRECTORY] [--queries FILE --workers N]\n{body}"
    )
}

fn execute(args: &[String]) -> Result<Value, &'static str> {
    let action = args
        .first()
        .map(String::as_str)
        .ok_or("reachability-pack: an action is required")?;
    if !matches!(
        action,
        "check" | "download" | "status" | "remove" | "generate"
    ) {
        return Err("reachability-pack: use check, download, status, remove or generate");
    }
    let mut profile = None;
    let mut directory = None;
    let mut queries = None;
    let mut workers = None;
    let mut index = 1;
    while index < args.len() {
        let value = args
            .get(index + 1)
            .ok_or("reachability-pack: option requires a value")?;
        match args[index].as_str() {
            "--profile" if profile.is_none() => profile = Some(value.as_str()),
            "--directory" if directory.is_none() => directory = Some(PathBuf::from(value)),
            "--queries" if queries.is_none() => queries = Some(PathBuf::from(value)),
            "--workers" if workers.is_none() => {
                workers = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| "reachability-pack: --workers must be an integer")?,
                )
            }
            _ => return Err("reachability-pack: unknown or repeated option"),
        }
        index += 2;
    }
    let profile = profile.ok_or("reachability-pack: --profile is required")?;
    if !PROFILES.contains(&profile) {
        return Err("reachability-pack: unknown profile");
    }
    if action != "generate" && (queries.is_some() || workers.is_some()) {
        return Err("reachability-pack: query and worker options belong only to generate");
    }
    if action == "check" {
        let catalog = accelerator_asset_store::catalog_summary(PRODUCT, profile)?;
        return Ok(json!({
            "action": "check",
            "profile": profile,
            "catalog_status": catalog.state.as_str(),
            "catalog_identity": accelerator_asset_store::hex(catalog.catalog_identity),
            "compressed_bytes": catalog.payload_bytes,
            "generation_identity": catalog.generation_identity.map(accelerator_asset_store::hex),
            "network_used": false,
            "qualified": catalog.state == LocalAssetState::Ready,
        }));
    }
    let base = directory.map(Ok).unwrap_or_else(default_directory)?;
    checked_base(&base)?;
    let root = accelerator_asset_store::profile_root(&base, PRODUCT, profile);
    accelerator_asset_store::validate_real_directory_if_present(&root)?;
    match action {
        "download" => report_value(
            "download",
            profile,
            accelerator_asset_store::download(PRODUCT, profile, &root)?,
        ),
        "status" => status(profile, &root),
        "remove" => remove(profile, &root),
        "generate" => generate(
            profile,
            &root,
            queries.ok_or("reachability-pack: --queries is required for generate")?,
            workers.unwrap_or_else(default_workers),
        ),
        _ => unreachable!(),
    }
}

pub(crate) fn download_observed(
    profile: &str,
    cancelled: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<Value, &'static str> {
    let base = default_directory()?;
    checked_base(&base)?;
    let root = accelerator_asset_store::profile_root(&base, PRODUCT, profile);
    accelerator_asset_store::validate_real_directory_if_present(&root)?;
    report_value(
        "download",
        profile,
        accelerator_asset_store::download_observed(PRODUCT, profile, &root, cancelled, progress)?,
    )
}

fn default_directory() -> Result<PathBuf, &'static str> {
    if let Some(path) = std::env::var_os("CLEARRA_CONDITIONED_REACHABILITY_DIRECTORY") {
        return Ok(PathBuf::from(path));
    }
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(|path| PathBuf::from(path).join("Clearra").join("accelerators"))
            .ok_or("reachability-pack: supply --directory when LOCALAPPDATA is unavailable")
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|path| PathBuf::from(path).join(".local/share"))
            })
            .map(|path| path.join("clearra/accelerators"))
            .ok_or("reachability-pack: supply --directory when the data directory is unavailable")
    }
}

fn checked_base(base: &std::path::Path) -> Result<(), &'static str> {
    accelerator_asset_store::validate_real_directory_if_present(base)
}

fn status(profile: &str, root: &std::path::Path) -> Result<Value, &'static str> {
    let installed = accelerator_asset_store::status(PRODUCT, profile, root)?;
    let candidate = root.join(format!("conditioned-reachability-{profile}.clbr"));
    let candidate_bytes = candidate.metadata().ok().map(|metadata| metadata.len());
    let candidate_validation = if candidate_bytes
        .is_some_and(|bytes| bytes > MAX_PRODUCT_PACK_BYTES)
    {
        "oversized_unqualified_candidate"
    } else if candidate_bytes.is_some() {
        let bytes = fs::read(&candidate)
            .map_err(|_| "reachability-pack: candidate payload is unreadable")?;
        if clearra_accelerator_runtime::structurally_valid_candidate(PRODUCT, profile, bytes.into())
        {
            "structurally_valid_unqualified"
        } else {
            "invalid_asset"
        }
    } else {
        "not_loaded"
    };
    Ok(json!({
        "action": "status",
        "profile": profile,
        "installed": installed.installed,
        "qualified": installed.state == LocalAssetState::Ready,
        "validation": installed.state.as_str(),
        "catalog_status": accelerator_asset_store::catalog_summary(PRODUCT, profile)?.state.as_str(),
        "installed_payload_bytes": installed.payload_bytes,
        "installed_generation_identity": installed.generation_identity.map(accelerator_asset_store::hex),
        "catalog_identity": accelerator_asset_store::hex(installed.catalog_identity),
        "candidate_bundle_bytes": candidate_bytes,
        "candidate_validation": candidate_validation,
        "candidate_only": true,
    }))
}

fn remove(profile: &str, root: &std::path::Path) -> Result<Value, &'static str> {
    // An absent store must not leave a once-loaded generation active in this
    // process. The shared store boundary handles both cases.
    accelerator_asset_store::remove(PRODUCT, profile, root)?;
    if root.exists() {
        remove_file_if_present(&root.join(format!("conditioned-reachability-{profile}.clbr")))?;
        remove_file_if_present(
            &root.join(format!("conditioned-reachability-{profile}.catalog.json")),
        )?;
        remove_file_if_present(&root.join("store.lock"))?;
        match fs::remove_dir(root) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("reachability-pack: profile directory contains unexpected files"),
        }
    }
    Ok(json!({
        "action": "remove", "profile": profile, "removed": true, "installed": false
    }))
}

fn generate(
    profile: &str,
    root: &std::path::Path,
    queries: PathBuf,
    workers: usize,
) -> Result<Value, &'static str> {
    if !(1..=64).contains(&workers) || !queries.is_absolute() {
        return Err(
            "reachability-pack: generate requires 1..=64 workers and an absolute query path",
        );
    }
    accelerator_asset_store::ensure_real_directory(root)?;
    let kick_profile = KickTableProfileId::parse(profile)
        .ok_or("reachability-pack: profile is not connected to a kick table")?;
    generate_conditioned_reachability(&ConditionedReachabilityGenerationOptions {
        profile: kick_profile,
        queries,
        pack: root.join(format!("conditioned-reachability-{profile}.clbr")),
        catalog: root.join(format!("conditioned-reachability-{profile}.catalog.json")),
        workers,
    })
    .map_err(|_| "reachability-pack: local candidate generation failed")?;
    status(profile, root)
}

fn report_value(
    action: &str,
    profile: &str,
    report: accelerator_asset_store::LocalAssetReport,
) -> Result<Value, &'static str> {
    Ok(json!({
        "action": action,
        "profile": profile,
        "installed": report.installed,
        "qualified": report.state == LocalAssetState::Ready,
        "validation": report.state.as_str(),
        "installed_payload_bytes": report.payload_bytes,
        "installed_generation_identity": report.generation_identity.map(accelerator_asset_store::hex),
        "catalog_identity": accelerator_asset_store::hex(report.catalog_identity),
    }))
}

fn remove_file_if_present(path: &std::path::Path) -> Result<(), &'static str> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("reachability-pack: could not remove managed candidate file"),
    }
}

fn default_workers() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

fn render_text(value: &Value, language: LanguageId) -> String {
    let profile = value["profile"].as_str().unwrap_or("unknown");
    let qualified = value["qualified"].as_bool().unwrap_or(false);
    let installed = value["installed"].as_bool().unwrap_or(false);
    match language {
        LanguageId::Ko => format!(
            "조건부 도달성 프로필: {profile}\n자격 완료: {}\n설치됨: {}",
            if qualified { "예" } else { "아니요" },
            if installed { "예" } else { "아니요" }
        ),
        LanguageId::Ja => format!(
            "条件付き到達性profile: {profile}\n適格済み: {}\nインストール済み: {}",
            if qualified { "はい" } else { "いいえ" },
            if installed { "はい" } else { "いいえ" }
        ),
        LanguageId::En => format!(
            "Conditioned-reachability profile: {profile}\nQualified: {}\nInstalled: {}",
            if qualified { "yes" } else { "no" },
            if installed { "yes" } else { "no" }
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_profiles_are_explicit_and_unqualified_downloads_never_start() {
        for profile in PROFILES {
            let checked = execute(&["check".into(), "--profile".into(), profile.into()]).unwrap();
            assert_eq!(checked["catalog_status"], "not_qualified");
            assert_eq!(checked["network_used"], false);
            assert!(execute(&["download".into(), "--profile".into(), profile.into()]).is_err());
        }
    }
}
