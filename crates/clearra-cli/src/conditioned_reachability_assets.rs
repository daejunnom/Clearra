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
    generate_conditioned_local_relation, generate_conditioned_reachability,
    validate_conditioned_local_candidate_catalog, ConditionedLocalRelationGenerationOptions,
    ConditionedReachabilityGenerationOptions,
};
use clearra_rules::kicks::KickTableProfileId;
use serde_json::{json, Value};
use std::{fs, io::Read, path::PathBuf, sync::atomic::AtomicBool};

const PRODUCT: ProductCatalogKind = ProductCatalogKind::BoardConditionedReachability;
const PROFILES: [&str; 5] = ["srs", "srs-plus", "srs-x", "jstris-180", "no-kick"];
const MAX_PRODUCT_PACK_BYTES: u64 = 16 * 1024 * 1024;
const MAX_CANDIDATE_CATALOG_BYTES: u64 = 512 * 1024;

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
        LanguageId::Ko => "profile별 조건부 도달성 pack을 확인·다운로드·생성·삭제합니다. check는 네트워크를 사용하지 않습니다. generate는 구형 sparse 후보, generate-local-candidate는 명시한 entry/exit query set의 새 미자격 후보를 만들며 어느 쪽도 제품에 설치하지 않습니다.",
        LanguageId::Ja => "プロファイル別の条件付き到達性packを確認・ダウンロード・生成・削除します。checkはネットワークを使用しません。generateは旧sparse候補、generate-local-candidateは指定したentry/exit query setの新しい未適格候補を作成し、どちらも製品にインストールしません。",
        LanguageId::En => "Inspect, download, generate, or remove one profile's conditioned-reachability pack. check uses no network. generate makes a legacy sparse candidate; generate-local-candidate makes a new unqualified entry/exit candidate from an explicit query set. Neither installs a product asset.",
    };
    format!(
        "clearra reachability-pack <check|download|status|remove|generate|generate-local-candidate> --profile srs|srs-plus|srs-x|jstris-180|no-kick [--directory DIRECTORY] [--queries FILE] [--workers N for legacy generate]\n{body}"
    )
}

fn execute(args: &[String]) -> Result<Value, &'static str> {
    let action = args
        .first()
        .map(String::as_str)
        .ok_or("reachability-pack: an action is required")?;
    if !matches!(
        action,
        "check" | "download" | "status" | "remove" | "generate" | "generate-local-candidate"
    ) {
        return Err("reachability-pack: use check, download, status, remove, generate or generate-local-candidate");
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
    if !matches!(action, "generate" | "generate-local-candidate")
        && (queries.is_some() || workers.is_some())
    {
        return Err(
            "reachability-pack: query and worker options belong only to candidate generation",
        );
    }
    if action == "generate-local-candidate" && workers.is_some() {
        return Err("reachability-pack: --workers belongs only to legacy generate");
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
        "generate-local-candidate" => generate_local_candidate(
            profile,
            &root,
            queries
                .ok_or("reachability-pack: --queries is required for generate-local-candidate")?,
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
    let candidate_bytes = candidate_size(&candidate)?;
    let candidate_validation = if candidate_bytes
        .is_some_and(|bytes| bytes > MAX_PRODUCT_PACK_BYTES)
    {
        "oversized_unqualified_candidate"
    } else if candidate_bytes.is_some() {
        let bytes = read_candidate_bounded(&candidate, MAX_PRODUCT_PACK_BYTES)?;
        if clearra_accelerator_runtime::structurally_valid_candidate(PRODUCT, profile, bytes.into())
        {
            "structurally_valid_unqualified"
        } else {
            "invalid_asset"
        }
    } else {
        "not_loaded"
    };
    let local_candidate = root.join(format!("conditioned-local-{profile}.cllr"));
    let local_catalog = root.join(format!("conditioned-local-{profile}.catalog.json"));
    let local_candidate_bytes = candidate_size(&local_candidate)?;
    let local_catalog_bytes = candidate_size(&local_catalog)?;
    let mut local_summary = None;
    let local_candidate_validation = match (local_candidate_bytes, local_catalog_bytes) {
        (None, None) => "not_loaded",
        (Some(bytes), _) if bytes > MAX_PRODUCT_PACK_BYTES => "oversized_unqualified_candidate",
        (_, Some(bytes)) if bytes > MAX_CANDIDATE_CATALOG_BYTES => {
            "oversized_unqualified_candidate"
        }
        (Some(_), Some(_)) => {
            let bytes = read_candidate_bounded(&local_candidate, MAX_PRODUCT_PACK_BYTES)?;
            let catalog = read_candidate_bounded(&local_catalog, MAX_CANDIDATE_CATALOG_BYTES)?;
            let kick = KickTableProfileId::parse(profile)
                .ok_or("reachability-pack: profile is not connected to a kick table")?;
            local_summary =
                validate_conditioned_local_candidate_catalog(kick, &bytes, &catalog).ok();
            if local_summary.is_some() {
                "structurally_valid_unqualified"
            } else {
                "invalid_asset"
            }
        }
        _ => "incomplete_unqualified_candidate",
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
        "candidate_contract": "legacy_sparse_spawn_to_lock",
        "local_candidate_bundle_bytes": local_candidate_bytes,
        "local_candidate_catalog_bytes": local_catalog_bytes,
        "local_candidate_generation_identity": local_summary.as_ref().map(|summary| accelerator_asset_store::hex(summary.generation_identity)),
        "local_candidate_record_count": local_summary.as_ref().map(|summary| summary.record_count),
        "local_candidate_logical_resident_bytes": local_summary.as_ref().map(|summary| summary.logical_resident_bytes),
        "local_candidate_validation": local_candidate_validation,
        "local_candidate_contract": "entry_to_first_exit",
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
        remove_file_if_present(&root.join(format!("conditioned-local-{profile}.cllr")))?;
        remove_file_if_present(&root.join(format!("conditioned-local-{profile}.catalog.json")))?;
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

fn generate_local_candidate(
    profile: &str,
    root: &std::path::Path,
    queries: PathBuf,
) -> Result<Value, &'static str> {
    if !queries.is_absolute() {
        return Err("reachability-pack: generate-local-candidate requires an absolute query path");
    }
    accelerator_asset_store::ensure_real_directory(root)?;
    let kick_profile = KickTableProfileId::parse(profile)
        .ok_or("reachability-pack: profile is not connected to a kick table")?;
    generate_conditioned_local_relation(&ConditionedLocalRelationGenerationOptions {
        profile: kick_profile,
        queries,
        pack: root.join(format!("conditioned-local-{profile}.cllr")),
        catalog: root.join(format!("conditioned-local-{profile}.catalog.json")),
    })
    .map_err(|_| "reachability-pack: local entry/exit candidate generation failed")?;
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

fn candidate_size(path: &std::path::Path) -> Result<Option<u64>, &'static str> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err("reachability-pack: candidate path must be a regular file")
        }
        Ok(metadata) => Ok(Some(metadata.len())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("reachability-pack: candidate metadata is unreadable"),
    }
}

fn read_candidate_bounded(path: &std::path::Path, limit: u64) -> Result<Vec<u8>, &'static str> {
    let file = fs::File::open(path).map_err(|_| "reachability-pack: candidate is unreadable")?;
    let metadata = file
        .metadata()
        .map_err(|_| "reachability-pack: candidate metadata is unreadable")?;
    if !metadata.is_file() || metadata.len() > limit {
        return Err("reachability-pack: candidate is not a bounded regular file");
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "reachability-pack: candidate is unreadable")?;
    if bytes.len() as u64 > limit {
        return Err("reachability-pack: candidate grew beyond its bound");
    }
    Ok(bytes)
}

fn default_workers() -> usize {
    std::thread::available_parallelism().map_or(1, usize::from)
}

fn render_text(value: &Value, language: LanguageId) -> String {
    let profile = value["profile"].as_str().unwrap_or("unknown");
    let qualified = value["qualified"].as_bool().unwrap_or(false);
    let installed = value["installed"].as_bool().unwrap_or(false);
    let mut output = match language {
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
    };
    if let Some(state) = value["local_candidate_validation"].as_str() {
        let (label, description) = match (language, state) {
            (LanguageId::Ko, "structurally_valid_unqualified") => {
                ("국소 관계 후보", "형식 검증됨, 제품 자격 미완료")
            }
            (LanguageId::Ko, "incomplete_unqualified_candidate") => {
                ("국소 관계 후보", "파일 일부만 존재함")
            }
            (LanguageId::Ko, "invalid_asset") => ("국소 관계 후보", "파일 검증 실패"),
            (LanguageId::Ko, "oversized_unqualified_candidate") => {
                ("국소 관계 후보", "허용 크기 초과")
            }
            (LanguageId::Ko, _) => ("국소 관계 후보", "없음"),
            (LanguageId::Ja, "structurally_valid_unqualified") => {
                ("局所関係候補", "形式検証済み、製品適格性は未確認")
            }
            (LanguageId::Ja, "incomplete_unqualified_candidate") => {
                ("局所関係候補", "ファイルが不足")
            }
            (LanguageId::Ja, "invalid_asset") => ("局所関係候補", "ファイル検証失敗"),
            (LanguageId::Ja, "oversized_unqualified_candidate") => {
                ("局所関係候補", "サイズ上限超過")
            }
            (LanguageId::Ja, _) => ("局所関係候補", "なし"),
            (LanguageId::En, "structurally_valid_unqualified") => (
                "Local relation candidate",
                "format verified, not qualified for product use",
            ),
            (LanguageId::En, "incomplete_unqualified_candidate") => {
                ("Local relation candidate", "missing candidate file")
            }
            (LanguageId::En, "invalid_asset") => {
                ("Local relation candidate", "candidate validation failed")
            }
            (LanguageId::En, "oversized_unqualified_candidate") => {
                ("Local relation candidate", "size limit exceeded")
            }
            (LanguageId::En, _) => ("Local relation candidate", "none"),
        };
        output.push_str(&format!("\n{label}: {description}"));
    }
    output
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

    #[test]
    fn local_entry_exit_candidate_is_explicit_and_never_accepts_legacy_workers() {
        assert!(help(LanguageId::En).contains("generate-local-candidate"));
        assert!(execute(&[
            "generate-local-candidate".into(),
            "--profile".into(),
            "srs".into(),
        ])
        .is_err());
        assert_eq!(
            execute(&[
                "generate-local-candidate".into(),
                "--profile".into(),
                "srs".into(),
                "--queries".into(),
                "queries.json".into(),
                "--workers".into(),
                "4".into(),
            ]),
            Err("reachability-pack: --workers belongs only to legacy generate")
        );
    }

    #[test]
    fn candidate_status_never_reads_a_directory_as_a_payload() {
        let source_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        assert_eq!(
            candidate_size(source_dir),
            Err("reachability-pack: candidate path must be a regular file")
        );
        assert_eq!(
            candidate_size(&source_dir.join("absent-local-candidate.cllr")),
            Ok(None)
        );
    }

    #[test]
    fn local_candidate_status_requires_both_pack_and_catalog() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "clearra-local-relation-status-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&root).expect("isolated candidate directory");
        let catalog = root.join("conditioned-local-srs.catalog.json");
        fs::write(&catalog, b"{}").expect("incomplete candidate fixture");
        let result = status("srs", &root).expect("candidate status");
        assert_eq!(
            result["local_candidate_validation"],
            "incomplete_unqualified_candidate"
        );
        assert_eq!(result["local_candidate_bundle_bytes"], Value::Null);
        assert_eq!(result["local_candidate_catalog_bytes"], 2);
        assert!(render_text(&result, LanguageId::Ko).contains("파일 일부만 존재함"));
        fs::remove_file(catalog).expect("remove fixture file");
        fs::remove_dir(root).expect("remove fixture directory");
    }
}
