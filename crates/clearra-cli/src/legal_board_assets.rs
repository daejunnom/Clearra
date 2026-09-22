//! SRP: explicit legal-board asset lifecycle and local generator dispatch.
//! Search requests never call this module and no network access occurs until a
//! profile has a signed, release-qualified catalog entry.

use crate::{error::CliErrorCode, output::CliOutput};
use clearra_i18n::LanguageId;
use clearra_pc4_qualifier::{generate_legal_board, LegalBoardGenerationOptions};
use clearra_rules::kicks::KickTableProfileId;
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, OnceLock},
};

const CATALOG: &str = include_str!("../../../config/legal-board-product-catalog.v1.json");
const PROFILES: [&str; 5] = ["srs", "srs-plus", "srs-x", "jstris-180", "no-kick"];
static CATALOG_VALUE: OnceLock<Result<Value, &'static str>> = OnceLock::new();

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
        LanguageId::Ko => "profile별 exact legal-board를 확인·다운로드·생성·삭제합니다. check는 내장 catalog만 읽고, download는 서명되고 자격을 통과한 Release 자산만 허용합니다. generate는 명시 요청에서 로컬 후보를 재개 생성하지만 자동 활성화하지 않습니다.",
        LanguageId::Ja => "プロファイル別のexact legal-boardを確認・ダウンロード・生成・削除します。checkは内蔵catalogだけを読み、downloadは署名済みで適格なRelease assetだけを許可します。generateは明示要求でローカル候補生成を再開しますが、自動的に有効化しません。",
        LanguageId::En => "Inspect, download, generate, or remove one profile's exact legal-board. check reads only the embedded catalog; download accepts only a signed, qualified Release asset. generate explicitly resumes a local candidate and never activates it automatically.",
    };
    format!(
        "clearra legal-board <check|download|status|remove|generate> --profile srs|srs-plus|srs-x|jstris-180|no-kick [--directory DIRECTORY] [--workers N --max-new-steps N]\n{body}"
    )
}

fn execute(args: &[String]) -> Result<Value, &'static str> {
    let action = args
        .first()
        .map(String::as_str)
        .ok_or("legal-board: an action is required")?;
    if !matches!(
        action,
        "check" | "download" | "status" | "remove" | "generate"
    ) {
        return Err("legal-board: use check, download, status, remove or generate");
    }
    let mut profile = None;
    let mut directory = None;
    let mut workers = None;
    let mut max_new_steps = None;
    let mut index = 1;
    while index < args.len() {
        let value = args
            .get(index + 1)
            .ok_or("legal-board: option requires a value")?;
        match args[index].as_str() {
            "--profile" if profile.is_none() => profile = Some(value.as_str()),
            "--directory" if directory.is_none() => directory = Some(PathBuf::from(value)),
            "--workers" if workers.is_none() => {
                workers = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| "legal-board: --workers must be an integer")?,
                )
            }
            "--max-new-steps" if max_new_steps.is_none() => {
                max_new_steps = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| "legal-board: --max-new-steps must be an integer")?,
                )
            }
            _ => return Err("legal-board: unknown or repeated option"),
        }
        index += 2;
    }
    let profile = profile.ok_or("legal-board: --profile is required")?;
    if !PROFILES.contains(&profile) {
        return Err("legal-board: unknown profile");
    }
    if action != "generate" && (workers.is_some() || max_new_steps.is_some()) {
        return Err("legal-board: worker and step options belong only to generate");
    }
    let catalog = catalog_entry(profile)?;
    if action == "check" {
        return Ok(json!({
            "action": action,
            "profile": profile,
            "catalog_status": catalog["status"],
            "compressed_bytes": catalog["compressed_bytes"],
            "network_used": false,
            "qualified": catalog["status"] == "qualified",
        }));
    }
    if action == "download" {
        // A candidate or unsigned ledger must never become a negative-filter
        // authority merely because its URL is reachable.
        if catalog["status"] != "qualified" {
            return Err("legal-board: this profile has no signed qualified Release asset");
        }
        return Err(
            "legal-board: qualified download transport is not present in this catalog generation",
        );
    }

    let base = directory.map(Ok).unwrap_or_else(default_directory)?;
    let root = checked_profile_root(&base, profile)?;
    match action {
        "status" => status(profile, &root, catalog),
        "remove" => remove(profile, &root),
        "generate" => generate(
            profile,
            &root,
            workers.unwrap_or_else(default_workers),
            max_new_steps.unwrap_or(1),
        ),
        _ => unreachable!(),
    }
}

fn catalog_entry(profile: &str) -> Result<&'static Value, &'static str> {
    let catalog = CATALOG_VALUE
        .get_or_init(|| {
            serde_json::from_str(CATALOG).map_err(|_| "legal-board: embedded catalog is invalid")
        })
        .as_ref()
        .map_err(|error| *error)?;
    if catalog["schema"] != "clearra.legal-board.product-catalog.v1" {
        return Err("legal-board: embedded catalog schema is invalid");
    }
    let profiles = catalog["profiles"]
        .as_array()
        .filter(|values| values.len() == PROFILES.len())
        .ok_or("legal-board: embedded profile catalog is incomplete")?;
    profiles
        .iter()
        .find(|entry| entry["profile"] == profile)
        .ok_or("legal-board: profile is absent from the embedded catalog")
}

fn default_directory() -> Result<PathBuf, &'static str> {
    if let Some(path) = std::env::var_os("CLEARRA_LEGAL_BOARD_DIRECTORY") {
        return Ok(PathBuf::from(path));
    }
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(|path| PathBuf::from(path).join("Clearra").join("legal-board"))
            .ok_or("legal-board: supply --directory when LOCALAPPDATA is unavailable")
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|path| PathBuf::from(path).join(".local/share"))
            })
            .map(|path| path.join("clearra/legal-board"))
            .ok_or("legal-board: supply --directory when the data directory is unavailable")
    }
}

fn checked_profile_root(base: &std::path::Path, profile: &str) -> Result<PathBuf, &'static str> {
    if !PROFILES.contains(&profile) {
        return Err("legal-board: unknown profile");
    }
    if base.exists() {
        let metadata = fs::symlink_metadata(base)
            .map_err(|_| "legal-board: could not inspect storage root")?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err("legal-board: storage root must be a real directory");
        }
    }
    Ok(base.join("legal-board-v1").join(profile))
}

fn status(profile: &str, root: &std::path::Path, catalog: &Value) -> Result<Value, &'static str> {
    if !root.exists() {
        return Ok(json!({
            "action": "status", "profile": profile, "installed": false,
            "catalog_status": catalog["status"], "qualified": false
        }));
    }
    reject_link(root)?;
    let mut forward_layers = 0_u8;
    let mut legal_layers = 0_u8;
    for layer in 0_u8..=10 {
        forward_layers += u8::from(
            root.join(format!("forward-reachable-layer-{layer:02}.bin"))
                .is_file(),
        );
        legal_layers += u8::from(root.join(format!("legal-layer-{layer:02}.bin")).is_file());
    }
    let bundle = root.join(format!("legal-board-{profile}.cllb"));
    let bundle_bytes = bundle.metadata().ok().map(|metadata| metadata.len());
    let candidate_validation = if bundle_bytes.is_some() {
        let bytes = fs::read(&bundle).map_err(|_| "legal-board: candidate bundle is unreadable")?;
        let kick_profile = KickTableProfileId::parse(profile)
            .ok_or("legal-board: profile is not connected to a kick table")?;
        let binding = clearra_core_executor::built_in_legal_board_binding(kick_profile)
            .map_err(|_| "legal-board: profile binding is unavailable")?;
        match clearra_core_executor::ExactLegalBoard::load(
            Arc::from(bytes),
            clearra_core_executor::LegalBoardExpectation {
                binding,
                generation_identity: None,
            },
        ) {
            Ok(_) => "structurally_valid_unqualified",
            Err(_) => "invalid_asset",
        }
    } else {
        "not_loaded"
    };
    Ok(json!({
        "action": "status", "profile": profile, "installed": bundle_bytes.is_some(),
        "qualified": false, "catalog_status": catalog["status"],
        "forward_layer_count": forward_layers, "legal_layer_count": legal_layers,
        "candidate_bundle_bytes": bundle_bytes,
        "candidate_validation": candidate_validation,
        "candidate_only": true
    }))
}

fn remove(profile: &str, root: &std::path::Path) -> Result<Value, &'static str> {
    if root.exists() {
        reject_link(root)?;
        fs::remove_dir_all(root).map_err(|_| "legal-board: could not remove owned profile data")?;
    }
    Ok(json!({ "action": "remove", "profile": profile, "removed": true, "installed": false }))
}

fn generate(
    profile: &str,
    root: &std::path::Path,
    workers: usize,
    max_new_steps: usize,
) -> Result<Value, &'static str> {
    if !(1..=64).contains(&workers) || !(1..=21).contains(&max_new_steps) {
        return Err("legal-board: generate limits are workers 1..=64 and max-new-steps 1..=21");
    }
    if let Some(parent) = root.parent() {
        fs::create_dir_all(parent).map_err(|_| "legal-board: could not create storage parent")?;
        reject_link(parent)?;
    }
    fs::create_dir_all(root).map_err(|_| "legal-board: could not create profile storage")?;
    reject_link(root)?;
    let kick_profile = KickTableProfileId::parse(profile)
        .ok_or("legal-board: profile is not connected to a kick table")?;
    let bundle = root.join(format!("legal-board-{profile}.cllb"));
    let catalog = root.join(format!("legal-board-{profile}.catalog.json"));
    generate_legal_board(&LegalBoardGenerationOptions {
        profile: kick_profile,
        layers: root.to_path_buf(),
        bundle,
        catalog,
        workers,
        max_new_steps,
    })
    .map_err(|_| "legal-board: local candidate generation failed")?;
    status(profile, root, catalog_entry(profile)?)
}

fn reject_link(path: &std::path::Path) -> Result<(), &'static str> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| "legal-board: could not inspect owned path")?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("legal-board: link or non-directory storage rejected");
    }
    Ok(())
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
            "Legal-board 프로필: {profile}\n자격 완료: {}\n설치됨: {}",
            if qualified { "예" } else { "아니요" },
            if installed { "예" } else { "아니요" }
        ),
        LanguageId::Ja => format!(
            "Legal-board profile: {profile}\n適格済み: {}\nインストール済み: {}",
            if qualified { "はい" } else { "いいえ" },
            if installed { "はい" } else { "いいえ" }
        ),
        LanguageId::En => format!(
            "Legal-board profile: {profile}\nQualified: {}\nInstalled: {}",
            if qualified { "yes" } else { "no" },
            if installed { "yes" } else { "no" }
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_profiles_are_explicit_and_unqualified_catalog_never_downloads() {
        for profile in PROFILES {
            let value = execute(&["check".into(), "--profile".into(), profile.into()]).unwrap();
            assert_eq!(value["profile"], profile);
            assert_eq!(value["catalog_status"], "not_qualified");
            assert_eq!(value["network_used"], false);
            assert!(execute(&["download".into(), "--profile".into(), profile.into()]).is_err());
        }
    }
}
