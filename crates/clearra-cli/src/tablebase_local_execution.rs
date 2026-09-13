//! SRP: native host adapter supplying verified local slices to the shared App.
//! No downloads, graph algorithms, new product reducers or implicit fallback.
use super::{active, reject_links, Result, FILES};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    path::Path,
};

/// Lease installed files and supply bounded slices to the shared App driver.
pub(super) fn execute_local_at(
    root: &Path,
    context: clearra_app::AppContext,
    request: clearra_app::AppRequest,
) -> Result<clearra_app::AppResponse> {
    use super::host_execution::{drive, HostSlice};
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
    let execution = context.start_pc4_execution_for_surface(
        request,
        snapshot,
        clearra_app::Pc4InputSurface::NonInteractiveCli,
    )?;
    drive(
        execution,
        |path, total, expected_identity, offset, requested| {
            let (file, length, identity) = handles
                .get_mut(path)
                .ok_or("tablebase: unexpected artifact request")?;
            if *length != total
                || identity.as_str() != expected_identity
                || offset
                    .checked_add(requested)
                    .is_none_or(|end| end > *length)
                || requested == 0
                || requested > 65_536
            {
                return Err("tablebase: local slice identity or bounds mismatch");
            }
            file.seek(SeekFrom::Start(offset))
                .map_err(|_| "tablebase: cannot seek local artifact")?;
            let mut bytes = vec![0_u8; requested as usize];
            file.read_exact(&mut bytes)
                .map_err(|_| "tablebase: local artifact was truncated")?;
            Ok(HostSlice::Local(bytes))
        },
    )
}
