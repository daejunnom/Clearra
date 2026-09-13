//! SRP: choose the explicitly requested profile's installed data or online
//! Range adapter. Never install data, select moves, or start offline fallback.
use super::{
    active, default_directory, format,
    host_execution::{drive, HostSlice},
    http_range::{content_range, HttpReply, OnlineRangeReader},
    local_execution::execute_local_at,
    reject_links, transport, Artifact, Result,
};
use clearra_app::{AppCommand, AppContext, AppRequest, AppResponse, Pc4InputSurface};
use clearra_rules::profile::rule_profile::RuleProfileId;
use std::path::Path;

pub(crate) fn execute(context: AppContext, request: AppRequest) -> Result<AppResponse> {
    let root = default_directory()?.join("pc4-v1/jstris-180");
    execute_with_online(&root, context, request, |context, request| {
        let (revision, files) = transport::discover()?;
        execute_online_with(
            context,
            request,
            &revision,
            &files,
            |artifact, offset, length| transport::curl_range(&revision, artifact, offset, length),
        )
    })
}

pub(super) fn execute_with_online(
    root: &Path,
    context: AppContext,
    request: AppRequest,
    online: impl FnOnce(AppContext, AppRequest) -> Result<AppResponse>,
) -> Result<AppResponse> {
    preflight(&request)?;
    reject_links(root)?;
    // Only absence permits Range. Corrupt metadata, missing active files,
    // busy leases and stale generations stay errors rather than silent updates.
    if active(root)?.is_some() {
        return execute_local_at(root, context, request);
    }
    online(context, request)
}

fn preflight(request: &AppRequest) -> Result<()> {
    let (rule, observed) = match request.command() {
        AppCommand::Pc(command) => (
            command.query().rule(),
            command.query().queue().observed_queue().is_some(),
        ),
        AppCommand::Scenario(command) => (
            command.query().rule(),
            command.query().remaining_queue().observed_queue().is_some(),
        ),
        _ => return Err("pc4_online_product_not_supported"),
    };
    if rule.id() != RuleProfileId::Jstris180 {
        return Err("pc4_online_profile_or_target_unavailable");
    }
    if observed {
        return Err("pc4_online_disclosure_required");
    }
    // This is only an I/O preflight. Exact rule/kick binding and target scope
    // are still validated by App after generation qualification.
    Ok(())
}

pub(super) fn execute_online_with(
    context: AppContext,
    request: AppRequest,
    revision: &str,
    files: &[Artifact],
    fetch: impl FnMut(&Artifact, u64, u64) -> Result<HttpReply>,
) -> Result<AppResponse> {
    preflight(&request)?;
    let mut reader = OnlineRangeReader::new(files.to_vec(), fetch);
    let generation = format::qualify_with_reader(revision, files, |role, offset, length| {
        reader.read(role, offset, length as u64)
    })?;
    let snapshot = clearra_app::activate_pc4_host_generation(&generation.to_string())?
        .ok_or("pc4_online_generation_unavailable")?;
    let field_count = generation["profiles"]
        .as_array()
        .and_then(|profiles| profiles.iter().find(|p| p["profile"] == "jstris-180"))
        .and_then(|p| p["field_count"].as_u64())
        .and_then(|count| u32::try_from(count).ok())
        .ok_or("pc4_online_index_layout_mismatch")?;
    let execution = context.start_pc4_execution_for_surface(
        request,
        snapshot,
        Pc4InputSurface::NonInteractiveCli,
    )?;
    drive(
        execution,
        |path, total, identity, offset, length, frontier| {
            let role = files
                .iter()
                .position(|file| file.path == path)
                .ok_or("pc4_online_artifact_invalid")?;
            if total != files[role].size || identity != format!("sha256:{}", files[role].digest) {
                return Err("pc4_online_artifact_identity_mismatch");
            }
            if role == 1 {
                super::http_frontier::prefetch(
                    &mut reader,
                    field_count,
                    files[2].size,
                    offset,
                    length,
                    frontier,
                )?;
            }
            // The containing HTTP window's status, Content-Range and exact size
            // were validated before cache admission. This is its bounded projection.
            let bytes = reader.read(role, offset, length)?;
            Ok(HostSlice::Http {
                bytes,
                content_range: content_range(offset, length, total),
            })
        },
    )
}
