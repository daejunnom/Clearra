//! SRP: native host adapter supplying verified local slices to the shared App.
//! No downloads, graph algorithms, new product reducers or implicit fallback.
use super::{active, default_directory, reject_links, Result, FILES};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    path::Path,
};

/// Native hosts use the same App execution and reducers as WASM. This adapter
/// only leases the installed files and supplies bounded, identity-bound slices.
pub(crate) fn execute_local(
    context: clearra_app::AppContext,
    request: clearra_app::AppRequest,
) -> Result<clearra_app::AppResponse> {
    execute_local_at(
        &default_directory()?.join("pc4-v1/jstris-180"),
        context,
        request,
    )
}

pub(super) fn execute_local_at(
    root: &Path,
    context: clearra_app::AppContext,
    request: clearra_app::AppRequest,
) -> Result<clearra_app::AppResponse> {
    use clearra_app::CooperativeAppAdvance;
    use clearra_core_domain::execution_cancellation::ExecutionControl;
    use std::{
        collections::BTreeMap,
        io::{Seek, SeekFrom},
    };
    reject_links(root)?;
    reject_links(&root.join("store.lock"))?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("store.lock"))
        .map_err(|_| "tablebase: run clearra tablebase download before requesting local TB")?;
    lock.try_lock_shared()
        .map_err(|_| "tablebase: a download or removal is in progress")?;
    let pointer = active(&root)?.ok_or("tablebase: no downloaded generation is active")?;
    let snapshot = clearra_app::activate_pc4_host_generation(&pointer["generation"].to_string())?
        .ok_or("tablebase: no qualified local generation")?;
    let data = root.join(pointer["directory"].as_str().unwrap());
    let artifacts = &pointer["generation"]["profiles"][3]["artifacts"];
    let mut handles = BTreeMap::new();
    for (key, name) in ["fields", "offsets", "graph"].into_iter().zip(FILES) {
        let path = data.join(name);
        reject_links(&path)?;
        let file = File::open(path)
            .map_err(|_| "tablebase: a local artifact is missing; download again")?;
        let length = file
            .metadata()
            .map_err(|_| "tablebase: local artifact metadata unavailable")?
            .len();
        if artifacts[key]["byte_length"] != length {
            return Err("tablebase: local artifact size mismatch; download again");
        }
        handles.insert(
            name,
            (
                file,
                length,
                artifacts[key]["content_identity"]
                    .as_str()
                    .ok_or("tablebase: missing content identity")?
                    .to_owned(),
            ),
        );
    }
    let control = ExecutionControl::default();
    let mut execution = context.start_pc4_execution_for_surface(
        request,
        snapshot,
        clearra_app::Pc4InputSurface::Cli,
    )?;
    loop {
        match execution.advance(2_048, &control)? {
            CooperativeAppAdvance::Completed(response) => return Ok(response),
            CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
            CooperativeAppAdvance::Cancelled => return Err("tablebase: search cancelled"),
            _ => return Err("tablebase: search did not complete; no offline fallback was started"),
        }
        if let Some(range) = execution.pending_range().cloned() {
            let (file, length, identity) = handles
                .get_mut(range.artifact_descriptor().path())
                .ok_or("tablebase: unexpected artifact request")?;
            if *length != range.artifact_descriptor().byte_len()
                || identity.as_str() != range.artifact_descriptor().content_identity()
                || range.end_exclusive() > *length
                || range.length() == 0
                || range.length() > 65_536
            {
                return Err("tablebase: local slice identity or bounds mismatch");
            }
            file.seek(SeekFrom::Start(range.offset()))
                .map_err(|_| "tablebase: cannot seek local artifact")?;
            let mut bytes = vec![0_u8; range.length() as usize];
            file.read_exact(&mut bytes)
                .map_err(|_| "tablebase: local artifact was truncated")?;
            execution.admit_local_slice(
                range.lookup_session().get(),
                range.request_id(),
                bytes,
                &control,
            )?;
        }
    }
}
