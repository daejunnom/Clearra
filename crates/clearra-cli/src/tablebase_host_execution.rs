//! SRP: drive the shared cooperative App with host-supplied, typed byte slices.
//! The driver knows no filesystem paths, HTTP client, graph algorithm or reducer.
use super::Result;
use clearra_app::{CooperativeAppAdvance, Pc4OnlineHostExecution};
use clearra_core_domain::execution_cancellation::ExecutionControl;

pub(super) enum HostSlice {
    Local(Vec<u8>),
    #[cfg(all(test, feature = "wasm-cpu-runtime"))]
    Http {
        bytes: Vec<u8>,
        content_range: String,
    },
}

pub(super) fn drive(
    mut execution: Pc4OnlineHostExecution,
    mut reader: impl FnMut(&str, u64, &str, u64, u64, &[u32]) -> Result<HostSlice>,
) -> Result<clearra_app::AppResponse> {
    let control = ExecutionControl::default();
    loop {
        match execution.advance(2_048, &control)? {
            CooperativeAppAdvance::Completed(response) => return Ok(response),
            CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
            CooperativeAppAdvance::Cancelled => return Err("tablebase: search cancelled"),
            _ => return Err("tablebase: search did not complete; no offline fallback was started"),
        }
        // Take one stable snapshot of every independent lookup exposed by the
        // App. Local files are still read sequentially so this adapter does
        // not create filesystem fan-out, but all responses are admitted before
        // the next App advance. That preserves the 64-lookup logical window
        // instead of collapsing it to one host/App round trip per record.
        let frontier = execution.pending_lookup_frontier().to_vec();
        let ranges = execution
            .pending_ranges()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        for range in ranges {
            let artifact = range.artifact_descriptor();
            let bytes = reader(
                artifact.path(),
                artifact.byte_len(),
                artifact.content_identity(),
                range.offset(),
                u64::from(range.length()),
                &frontier,
            )?;
            match bytes {
                HostSlice::Local(bytes) => execution.admit_local_slice(
                    range.lookup_session().get(),
                    range.request_id(),
                    bytes,
                    &control,
                )?,
                #[cfg(all(test, feature = "wasm-cpu-runtime"))]
                HostSlice::Http {
                    bytes,
                    content_range,
                } => execution.admit_range(
                    range.lookup_session().get(),
                    range.request_id(),
                    206,
                    Some(content_range),
                    bytes,
                    &control,
                )?,
            }
        }
    }
}
