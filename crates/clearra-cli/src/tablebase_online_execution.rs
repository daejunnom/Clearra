//! SRP: choose the explicitly requested profile's installed data or online
//! Range adapter. Never install data, select moves, or start offline fallback.
#[cfg(feature = "native-pc4-libcurl")]
use super::curl_batch::NativeCurlPool;
#[cfg(not(feature = "native-pc4-libcurl"))]
use super::curl_batch::{NativeCurlBatch, NativeCurlPlan};
#[cfg(all(test, feature = "wasm-cpu-runtime"))]
use super::host_execution::{drive, HostSlice};
use super::{
    active,
    curl_batch::{NativeCurlPoll, NativeRangeAdmission, NativeRangeDemand},
    default_directory, format,
    http_range::{content_range, HttpReply, OnlineRangeReader},
    local_execution::execute_local_at,
    reject_links, transport, Artifact, Result,
};
use clearra_app::{
    AppCommand, AppContext, AppRequest, AppResponse, CooperativeAppAdvance, Pc4InputSurface,
    Pc4OnlineHostExecution,
};
use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_rules::profile::rule_profile::RuleProfileId;
use std::path::Path;

pub(crate) fn execute(context: AppContext, request: AppRequest) -> Result<AppResponse> {
    let root = default_directory()?.join("pc4-v1/jstris-180");
    execute_with_online(&root, context, request, |context, request| {
        let (revision, files) = transport::discover()?;
        execute_online_native(context, request, &revision, &files)
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

#[cfg(all(test, feature = "wasm-cpu-runtime"))]
pub(super) fn execute_online_with(
    context: AppContext,
    request: AppRequest,
    revision: &str,
    files: &[Artifact],
    fetch: impl FnMut(&Artifact, u64, u64) -> Result<HttpReply>,
) -> Result<AppResponse> {
    let (mut reader, execution, field_count) =
        prepare_online(context, request, revision, files, fetch)?;
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

fn prepare_online<F>(
    context: AppContext,
    request: AppRequest,
    revision: &str,
    files: &[Artifact],
    fetch: F,
) -> Result<(OnlineRangeReader<F>, Pc4OnlineHostExecution, u32)>
where
    F: FnMut(&Artifact, u64, u64) -> Result<HttpReply>,
{
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
    Ok((reader, execution, field_count))
}

fn execute_online_native(
    context: AppContext,
    request: AppRequest,
    revision: &str,
    files: &[Artifact],
) -> Result<AppResponse> {
    let owned_revision = revision.to_owned();
    let fetch_revision = owned_revision.clone();
    let (reader, execution, field_count) = prepare_online(
        context,
        request,
        &owned_revision,
        files,
        move |artifact, offset, length| {
            transport::curl_range(&fetch_revision, artifact, offset, length)
        },
    )?;
    drive_native(execution, reader, &owned_revision, files, field_count)
}

fn drive_native<F>(
    execution: Pc4OnlineHostExecution,
    reader: OnlineRangeReader<F>,
    revision: &str,
    files: &[Artifact],
    field_count: u32,
) -> Result<AppResponse>
where
    F: FnMut(&Artifact, u64, u64) -> Result<HttpReply>,
{
    #[cfg(feature = "native-pc4-libcurl")]
    {
        return drive_native_pool(execution, reader, revision, files, field_count);
    }
    #[cfg(not(feature = "native-pc4-libcurl"))]
    {
        drive_native_batch(execution, reader, revision, files, field_count)
    }
}

#[cfg(not(feature = "native-pc4-libcurl"))]
fn drive_native_batch<F>(
    mut execution: Pc4OnlineHostExecution,
    mut reader: OnlineRangeReader<F>,
    revision: &str,
    files: &[Artifact],
    field_count: u32,
) -> Result<AppResponse>
where
    F: FnMut(&Artifact, u64, u64) -> Result<HttpReply>,
{
    let control = ExecutionControl::default();
    let mut batch: Option<NativeCurlBatch> = None;
    loop {
        drain_batch(&mut batch, &mut execution, &control, false)?;
        match execution.advance(2_048, &control)? {
            CooperativeAppAdvance::Completed(response) => return Ok(response),
            CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
            CooperativeAppAdvance::Cancelled => return Err("tablebase: search cancelled"),
            _ => return Err("tablebase: search did not complete; no offline fallback was started"),
        }

        if batch.is_some() {
            if !execution.has_ready_work() {
                drain_batch(&mut batch, &mut execution, &control, true)?;
            }
            continue;
        }

        let mut demands = Vec::new();
        for range in execution.pending_ranges() {
            let descriptor = range.artifact_descriptor();
            let role = files
                .iter()
                .position(|file| file.path == descriptor.path())
                .ok_or("pc4_online_artifact_invalid")?;
            if descriptor.byte_len() != files[role].size
                || descriptor.content_identity() != format!("sha256:{}", files[role].digest)
            {
                return Err("pc4_online_artifact_identity_mismatch");
            }
            demands.push(NativeRangeDemand {
                role,
                lookup_session: range.lookup_session().get(),
                request_id: range.request_id(),
                artifact: files[role].clone(),
                offset: range.offset(),
                length: u64::from(range.length()),
            });
        }
        if demands.is_empty() {
            continue;
        }

        // A previous bounded frontier span can satisfy exact graph demands
        // without another process. Admit every cached item before taking a new
        // snapshot because admission can expose more independent work.
        let mut admitted_cached = false;
        for demand in &demands {
            if let Some(bytes) = reader.read_cached(demand.role, demand.offset, demand.length)? {
                execution.admit_range(
                    demand.lookup_session,
                    demand.request_id,
                    206,
                    Some(content_range(
                        demand.offset,
                        demand.length,
                        demand.artifact.size,
                    )),
                    bytes,
                    &control,
                )?;
                admitted_cached = true;
            }
        }
        if admitted_cached {
            continue;
        }

        // Compact graph demands are independent and exact. Start them now in
        // one curl multi process, then return to App CPU work. Index demands
        // retain their measured page/frontier cache rather than bypassing it.
        if demands.iter().all(|demand| demand.role == 2) {
            let plan = NativeCurlPlan::new(demands)?;
            reader.reserve_external(&plan.reservations())?;
            batch = Some(plan.spawn(revision)?);
            continue;
        }
        if execution.has_ready_work() {
            continue;
        }

        let demand = &demands[0];
        if demand.role == 1 {
            super::http_frontier::prefetch(
                &mut reader,
                field_count,
                files[2].size,
                demand.offset,
                demand.length,
                execution.pending_lookup_frontier(),
            )?;
        }
        let bytes = reader.read(demand.role, demand.offset, demand.length)?;
        execution.admit_range(
            demand.lookup_session,
            demand.request_id,
            206,
            Some(content_range(
                demand.offset,
                demand.length,
                demand.artifact.size,
            )),
            bytes,
            &control,
        )?;
    }
}

#[cfg(feature = "native-pc4-libcurl")]
fn drive_native_pool<F>(
    mut execution: Pc4OnlineHostExecution,
    mut reader: OnlineRangeReader<F>,
    revision: &str,
    files: &[Artifact],
    field_count: u32,
) -> Result<AppResponse>
where
    F: FnMut(&Artifact, u64, u64) -> Result<HttpReply>,
{
    let control = ExecutionControl::default();
    let mut pool = NativeCurlPool::new(revision)?;
    loop {
        drain_pool(&mut pool, &mut execution, &control, false)?;
        match execution.advance(2_048, &control)? {
            CooperativeAppAdvance::Completed(response) => return Ok(response),
            CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
            CooperativeAppAdvance::Cancelled => return Err("tablebase: search cancelled"),
            _ => return Err("tablebase: search did not complete; no offline fallback was started"),
        }

        let mut demands = Vec::new();
        for range in execution.pending_ranges() {
            let descriptor = range.artifact_descriptor();
            let role = files
                .iter()
                .position(|file| file.path == descriptor.path())
                .ok_or("pc4_online_artifact_invalid")?;
            if descriptor.byte_len() != files[role].size
                || descriptor.content_identity() != format!("sha256:{}", files[role].digest)
            {
                return Err("pc4_online_artifact_identity_mismatch");
            }
            demands.push(NativeRangeDemand {
                role,
                lookup_session: range.lookup_session().get(),
                request_id: range.request_id(),
                artifact: files[role].clone(),
                offset: range.offset(),
                length: u64::from(range.length()),
            });
        }
        if demands.is_empty() {
            if pool.is_active() && !execution.has_ready_work() {
                drain_pool(&mut pool, &mut execution, &control, true)?;
            }
            continue;
        }

        let mut admitted_cached = false;
        for demand in &demands {
            if let Some(bytes) = reader.read_cached(demand.role, demand.offset, demand.length)? {
                execution.admit_range(
                    demand.lookup_session,
                    demand.request_id,
                    206,
                    Some(content_range(
                        demand.offset,
                        demand.length,
                        demand.artifact.size,
                    )),
                    bytes,
                    &control,
                )?;
                admitted_cached = true;
            }
        }
        if admitted_cached {
            continue;
        }

        // Submit newly exposed exact graph records even while older streams are
        // active. The pool adds at most four easy handles and retains the rest
        // in its bounded logical queue, refilling a slot after each completion.
        let graph_demands = demands
            .iter()
            .filter(|demand| demand.role == 2)
            .cloned()
            .collect::<Vec<_>>();
        if !graph_demands.is_empty() {
            if let Some(plan) = pool.plan_fresh(graph_demands)? {
                reader.reserve_external(&plan.reservations())?;
                pool.submit(plan)?;
            }
        }
        if execution.has_ready_work() {
            continue;
        }
        if pool.is_active() {
            drain_pool(&mut pool, &mut execution, &control, true)?;
            continue;
        }

        // Index requests preserve the existing measured page/frontier cache.
        // A future mixed-owner A/B may move them into the same pool, but must
        // retain its bounded 3:1 graph/index fairness and cache semantics.
        let demand = demands
            .iter()
            .find(|demand| demand.role != 2)
            .ok_or("pc4_online_pending_missing")?;
        if demand.role == 1 {
            super::http_frontier::prefetch(
                &mut reader,
                field_count,
                files[2].size,
                demand.offset,
                demand.length,
                execution.pending_lookup_frontier(),
            )?;
        }
        let bytes = reader.read(demand.role, demand.offset, demand.length)?;
        execution.admit_range(
            demand.lookup_session,
            demand.request_id,
            206,
            Some(content_range(
                demand.offset,
                demand.length,
                demand.artifact.size,
            )),
            bytes,
            &control,
        )?;
    }
}

#[cfg(not(feature = "native-pc4-libcurl"))]
fn drain_batch(
    batch: &mut Option<NativeCurlBatch>,
    execution: &mut Pc4OnlineHostExecution,
    control: &ExecutionControl,
    wait: bool,
) -> Result<()> {
    loop {
        let Some(active) = batch.as_mut() else {
            return Ok(());
        };
        match active.poll(wait)? {
            NativeCurlPoll::Pending => return Ok(()),
            NativeCurlPoll::Admissions(admissions) => {
                admit_native_ranges(execution, admissions, control)?;
                if wait {
                    return Ok(());
                }
            }
            NativeCurlPoll::Finished => {
                batch.take();
                return Ok(());
            }
        }
    }
}

#[cfg(feature = "native-pc4-libcurl")]
fn drain_pool(
    pool: &mut NativeCurlPool,
    execution: &mut Pc4OnlineHostExecution,
    control: &ExecutionControl,
    wait: bool,
) -> Result<()> {
    loop {
        match pool.poll(wait)? {
            NativeCurlPoll::Pending | NativeCurlPoll::Finished => return Ok(()),
            NativeCurlPoll::Admissions(admissions) => {
                admit_native_ranges(execution, admissions, control)?;
                if wait {
                    return Ok(());
                }
            }
        }
    }
}

fn admit_native_ranges(
    execution: &mut Pc4OnlineHostExecution,
    admissions: Vec<NativeRangeAdmission>,
    control: &ExecutionControl,
) -> Result<()> {
    for admission in admissions {
        execution.admit_range(
            admission.lookup_session,
            admission.request_id,
            206,
            Some(content_range(
                admission.offset,
                admission.length,
                admission.total,
            )),
            admission.bytes,
            control,
        )?;
    }
    Ok(())
}
