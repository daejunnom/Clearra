//! Exact end-to-end parity between the immutable graph materializer and the
//! ordinary offline solver. This is a local qualification producer only.

use super::{
    field_hash, lookup_field_id, offline_materialization, read_json, sha256_identity,
    validate_index_bytes, validate_receipt_identity, with_identity, write_json_atomic, Dataset,
};
use clearra_app::{
    AppCommand, AppContext, AppRequest, AppStatus, CooperativeAppAdvance,
    DistributedSearchPreparation, PcAppCommand,
};
use clearra_core_domain::{execution_cancellation::ExecutionControl, pc::pc_target::PcTarget};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc4_tablebase::{
    ActivatedSnapshot, ArtifactDescriptor, DatasetSnapshotManifest, DatasetSnapshotVerifier,
    FieldIdIndexRelation, GraphSourceFieldEncoding, ManifestContentIdentity, Pc4ArtifactRole,
    Pc4ProfileManifest, Pc4RuleProfile, Pc4TargetLines, Pc4TerminalFieldIdentity,
    Pc4TerminalUseCase, ProfileAvailability, ProfileQualification,
    ProfileTargetCompletenessQualification, SnapshotIdentity, SnapshotVerificationAttestation,
    SnapshotVerificationFailure, SnapshotVerificationRequest, UnsupportedProfileReason,
};
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcCountPolicy, PcExecutionPolicy, PcHoldPolicy, PcQueueInput,
    RequestedSearchBackend,
};
use clearra_rules::profile::builtin_rules::jstris_180;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, VecDeque},
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    time::Instant,
};

const SCHEMA: &str = "clearra.pc4.tablebase-offline-exact-family-parity.v1";
const OUTGOING_SCHEMA: &str = "clearra.pc4.indexed-domain-outgoing-proof-merge.v2";
const BOUNDARY_SCHEMA: &str = "clearra.pc4.outside-boundary-dead-proof-shard.v3";
const OFFLINE_SCHEMA: &str = "clearra.pc4.offline-exact-result-family.v1";
const INPUT_IDENTITY: &str = "empty-board-4l-jstris-180-standard-7-bag-hold-empty-v1";
const LOCAL_INDEX_PAGE_BYTES: u64 = 4_096;
const LOCAL_INDEX_MAX_PAGES: usize = 512;

#[allow(clippy::too_many_arguments)]
pub(crate) fn prove(
    dataset: &Dataset,
    workers: usize,
    expected_count: usize,
    outgoing_path: &Path,
    boundary_path: &Path,
    offline_path: &Path,
    offline_family_path: &Path,
    offline_materialization_path: &Path,
    output: &Path,
) -> Result<(), String> {
    if dataset.profile != "jstris-180" || dataset.kick_profile.as_str() != "jstris-180" {
        return Err("tablebase family proof currently requires jstris-180".to_owned());
    }
    if workers == 0 || workers > 64 || expected_count == 0 {
        return Err("tablebase family proof workers/count invalid".to_owned());
    }
    let outgoing = read_json(outgoing_path, 16 * 1024 * 1024)?;
    let boundary = read_json(boundary_path, 16 * 1024 * 1024)?;
    let offline = read_json(offline_path, 16 * 1024 * 1024)?;
    let materialization = read_json(offline_materialization_path, 16 * 1024 * 1024)?;
    for receipt in [&outgoing, &boundary, &offline] {
        validate_receipt_identity(receipt)?;
        validate_generation(receipt, dataset)?;
    }
    validate_receipt_identity(&materialization)?;
    if outgoing["schema"] != OUTGOING_SCHEMA
        || outgoing["qualification_status"] != "indexed-path-and-boundary-unclassified"
    {
        return Err("outgoing proof receipt is not the complete boundary-bearing merge".to_owned());
    }
    if boundary["schema"] != BOUNDARY_SCHEMA
        || boundary["qualification_status"] != "outside-boundary-terminal-dead"
        || boundary["outside_boundary"]["content_identity"]
            != outgoing["outside_boundary"]["content_identity"]
        || boundary["outside_boundary"]["field_count"]
            != outgoing["outside_boundary"]["field_count"]
    {
        return Err("boundary dead proof does not close the merged outgoing boundary".to_owned());
    }
    if offline["schema"] != OFFLINE_SCHEMA
        || offline["qualification_status"] != "offline-exact-family-complete"
        || offline["input_identity"] != INPUT_IDENTITY
        || offline["expected_unique_solution_count"].as_u64() != u64::try_from(expected_count).ok()
        || offline["unique_solution_count"].as_u64() != u64::try_from(expected_count).ok()
    {
        return Err("offline exact family proof does not match the requested KAT".to_owned());
    }

    if output.exists() {
        let existing = read_json(output, 16 * 1024 * 1024)?;
        validate_receipt_identity(&existing)?;
        if existing["schema"] != SCHEMA
            || existing["repository"].as_str() != Some(&dataset.repository)
            || existing["revision"].as_str() != Some(&dataset.revision)
            || existing["profile"].as_str() != Some(&dataset.profile)
            || existing["outgoing_proof_receipt_identity"] != outgoing["receipt_identity"]
            || existing["boundary_dead_proof_receipt_identity"] != boundary["receipt_identity"]
            || existing["offline_family_receipt_identity"] != offline["receipt_identity"]
            || existing["offline_family_materialization_receipt_identity"]
                != materialization["receipt_identity"]
        {
            return Err(
                "existing tablebase family parity receipt does not match inputs".to_owned(),
            );
        }
        println!(
            "pc4_tablebase_family_proof=already-complete solutions={} receipt={}",
            existing["unique_solution_count"].as_u64().unwrap_or(0),
            existing["receipt_identity"].as_str().unwrap_or("invalid")
        );
        return Ok(());
    }

    // Reject an incompatible product request before recomputing the expensive
    // offline reference family. This preflight deliberately preserves the
    // complete AppResponse so qualification failures name the contract that
    // rejected the request instead of collapsing to `pc4_online_request_rejected`.
    let request = tablebase_request(workers);
    if let DistributedSearchPreparation::Ready(response) =
        AppContext::default().prepare_distributed_search(request.clone())
    {
        return Err(format!(
            "tablebase request preparation rejected: status={:?} response={response:?}",
            response.status()
        ));
    }

    let offline_started = Instant::now();
    let exact = offline_materialization::load(
        dataset,
        expected_count,
        &offline,
        offline_family_path,
        offline_materialization_path,
    )?;
    let offline_elapsed_ms = offline_started.elapsed().as_millis();

    let fields = std::fs::read(&dataset.fields.path).map_err(io_error)?;
    validate_index_bytes(
        &fields,
        b"FHIDIDX1",
        dataset.field_count,
        dataset.fields.byte_len,
    )?;
    if sha256_identity(&fields) != dataset.fields.identity {
        return Err("field index identity drift".to_owned());
    }
    let terminal_hash = (1_u64 << 40) - 1;
    let terminal_id = lookup_field_id(&fields, dataset.field_count, terminal_hash)?
        .ok_or("four-row terminal is absent from field index")?;
    if field_hash(&fields, terminal_id)? != terminal_hash {
        return Err("four-row terminal field identity mismatch".to_owned());
    }
    drop(fields);

    let snapshot = provisional_snapshot(dataset, terminal_id, &outgoing, &boundary, &offline)?;
    let mut execution = AppContext::default()
        .start_online_pc4_execution(request, snapshot)
        .map_err(str::to_owned)?;
    let control = ExecutionControl::default();
    let mut files = ArtifactFiles::open(dataset)?;
    let tablebase_started = Instant::now();
    let mut range_requests = 0_u64;
    let mut admitted_range_bytes = 0_u64;
    let response = loop {
        let advance = match execution.advance(8192, &control) {
            Ok(advance) => advance,
            Err(reason) => {
                return Err(match execution.compact_failure_diagnostic() {
                    Some(diagnostic) => format!("{reason}; {diagnostic}"),
                    None => reason.to_owned(),
                });
            }
        };
        match advance {
            CooperativeAppAdvance::Completed(response) => break response,
            CooperativeAppAdvance::CompletedGoverned(response) => break response.into_parts().0,
            CooperativeAppAdvance::FailedFinite(error) => {
                return Err(format!(
                    "tablebase product exhausted finite authority: {error:?}"
                ))
            }
            CooperativeAppAdvance::Cancelled => {
                return Err("tablebase family proof cancelled".to_owned())
            }
            CooperativeAppAdvance::Pending | CooperativeAppAdvance::Progress => {}
        }
        let ranges = execution
            .pending_ranges()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        if ranges.is_empty() {
            if !execution.has_ready_work() {
                return Err("tablebase family proof stalled without work or ranges".to_owned());
            }
            continue;
        }
        for range in ranges {
            let bytes = files.read(&range)?;
            range_requests = range_requests
                .checked_add(1)
                .ok_or("tablebase range request count overflow")?;
            admitted_range_bytes = admitted_range_bytes
                .checked_add(bytes.len() as u64)
                .ok_or("tablebase range byte count overflow")?;
            execution
                .admit_local_slice(
                    range.lookup_session().get(),
                    range.request_id(),
                    bytes,
                    &control,
                )
                .map_err(str::to_owned)?;
        }
    };
    let tablebase_elapsed_ms = tablebase_started.elapsed().as_millis();
    let (physical_file_reads, physical_file_bytes) = files.usage()?;
    if response.status() != AppStatus::Success {
        return Err(format!("tablebase product failed: {response:?}"));
    }
    let result = response
        .render_model()
        .and_then(clearra_app::AppRenderModel::core_result)
        .ok_or("tablebase product lacks exact core result")?;
    let count = result
        .usize_field("normalized_unique_solution_count")
        .ok_or("tablebase result lacks normalized unique solution count")?;
    let normalized_hash = result
        .field("normalized_solution_set_hash")
        .ok_or("tablebase result lacks normalized set hash")?;
    if count != exact.count()
        || normalized_hash != exact.normalized_hash
        || result.field("count_complete") != Some("true")
        || result.field("resource_truncated") != Some("false")
    {
        return Err("tablebase result summary differs from offline exact family".to_owned());
    }
    compare_exact_family(result, &exact.identities)?;

    let core = json!({
        "schema": SCHEMA,
        "authority": "target-qualification-evidence",
        "qualification_status": "tablebase-offline-exact-family-parity",
        "repository": dataset.repository,
        "revision": dataset.revision,
        "profile": dataset.profile,
        "kick_profile": dataset.kick_profile.as_str(),
        "reader_contract": dataset.reader_contract,
        "artifacts": dataset.public_artifacts(),
        "input_identity": INPUT_IDENTITY,
        "initial_board_mask": 0,
        "target_lines": 4,
        "queue": "P7P4",
        "hold": "enabled-empty",
        "objective": "unique",
        "unique_solution_count": count,
        "normalized_solution_set_hash_algorithm": exact.normalized_hash_algorithm,
        "normalized_solution_set_hash": normalized_hash,
        "comparison": "exact-canonical-identity-sequence-equality",
        "local_io": {
            "logical_range_requests": range_requests,
            "admitted_range_bytes": admitted_range_bytes,
            "physical_file_reads": physical_file_reads,
            "physical_file_bytes": physical_file_bytes,
            "index_page_bytes": LOCAL_INDEX_PAGE_BYTES,
            "index_page_capacity_per_artifact": LOCAL_INDEX_MAX_PAGES,
            "graph_policy": "exact-record",
        },
        "outgoing_proof_receipt_identity": outgoing["receipt_identity"],
        "boundary_dead_proof_receipt_identity": boundary["receipt_identity"],
        "offline_family_receipt_identity": offline["receipt_identity"],
        "offline_family_materialization_receipt_identity":
            materialization["receipt_identity"],
    });
    let receipt = with_identity(core)?;
    write_json_atomic(output, &receipt)?;
    println!(
        "pc4_tablebase_family_proof=passed solutions={} logical_requests={} admitted_bytes={} physical_reads={} physical_bytes={} offline_elapsed_ms={} tablebase_elapsed_ms={} receipt={}",
        count,
        range_requests,
        admitted_range_bytes,
        physical_file_reads,
        physical_file_bytes,
        offline_elapsed_ms,
        tablebase_elapsed_ms,
        receipt["receipt_identity"].as_str().unwrap_or("invalid")
    );
    Ok(())
}

fn tablebase_request(workers: usize) -> AppRequest {
    let policy = PcExecutionPolicy::mvp_default()
        .with_requested_backend(RequestedSearchBackend::Cpu)
        .with_workers(workers)
        .with_cpu_warmup(true)
        .with_tablebase_requested(true);
    let query = OpeningPcSearchQuery::new(PcTarget::four_lines())
        .with_queue(PcQueueInput::standard_7_bag())
        .with_hold_policy(PcHoldPolicy::EnabledEmpty)
        .with_count_policy(PcCountPolicy::CountUnique)
        .with_objective(ObjectivePolicy::unique())
        .with_rule(jstris_180())
        .with_execution_policy(policy);
    AppRequest::new(AppCommand::Pc(PcAppCommand::new(query)))
}

fn validate_generation(receipt: &Value, dataset: &Dataset) -> Result<(), String> {
    if receipt["repository"].as_str() != Some(&dataset.repository)
        || receipt["revision"].as_str() != Some(&dataset.revision)
        || receipt["profile"].as_str() != Some(&dataset.profile)
        || receipt["artifacts"] != dataset.public_artifacts()
    {
        return Err("qualification receipt generation mismatch".to_owned());
    }
    Ok(())
}

fn provisional_snapshot(
    dataset: &Dataset,
    terminal_id: u32,
    outgoing: &Value,
    boundary: &Value,
    offline: &Value,
) -> Result<ActivatedSnapshot, String> {
    let evidence = with_identity(json!({
        "schema": "clearra.pc4.local-parity-provisional-binding.v1",
        "outgoing": outgoing["receipt_identity"],
        "boundary": boundary["receipt_identity"],
        "offline": offline["receipt_identity"],
    }))?;
    let evidence_identity = evidence["receipt_identity"]
        .as_str()
        .ok_or("provisional evidence identity missing")?;
    let descriptor = |role, suffix: &str, artifact: &super::Artifact| {
        ArtifactDescriptor::new(
            role,
            format!("jstris-180/{suffix}"),
            artifact.byte_len,
            artifact.identity.clone(),
        )
        .map_err(|error| format!("artifact descriptor invalid: {error:?}"))
    };
    let target_lines = Pc4TargetLines::new(4).map_err(|error| format!("target: {error:?}"))?;
    let profile_manifest = Pc4ProfileManifest::new(
        Pc4RuleProfile::Jstris180,
        dataset.field_count,
        dataset.target_encoding,
        FieldIdIndexRelation::RecordOrdinal,
        16_384,
        descriptor(
            Pc4ArtifactRole::FieldHashIndex,
            "field_hash_to_id.bin",
            &dataset.fields,
        )?,
        descriptor(
            Pc4ArtifactRole::GraphOffsets,
            "graph_offsets.bin",
            &dataset.offsets,
        )?,
        descriptor(Pc4ArtifactRole::Graph, "graph.bin", &dataset.graph)?,
        ProfileQualification::new(
            dataset.fields.identity.clone(),
            dataset.graph.identity.clone(),
            evidence_identity,
            offline["receipt_identity"].as_str().unwrap_or("invalid"),
        )
        .map_err(|error| format!("profile qualification invalid: {error:?}"))?,
    )
    .map_err(|error| format!("profile manifest invalid: {error:?}"))?
    .with_graph_source_field_encoding(GraphSourceFieldEncoding::HydraU40BigEndianPrefix)
    .with_target_qualifications(vec![ProfileTargetCompletenessQualification::new(
        Pc4TerminalUseCase::PcSearch,
        target_lines,
        Pc4TerminalFieldIdentity::full_rows(target_lines, terminal_id),
        "clearra.pc4.full-four-row-terminal.v1",
        evidence_identity,
        offline["receipt_identity"].as_str().unwrap_or("invalid"),
        offline["receipt_identity"].as_str().unwrap_or("invalid"),
    )
    .map_err(|error| format!("target qualification invalid: {error:?}"))?])
    .map_err(|error| format!("target qualification set invalid: {error:?}"))?;
    let profiles = Pc4RuleProfile::ALL
        .into_iter()
        .map(|profile| {
            if profile == Pc4RuleProfile::Jstris180 {
                ProfileAvailability::qualified(profile_manifest.clone())
            } else {
                ProfileAvailability::Unsupported {
                    profile,
                    reason: UnsupportedProfileReason::MissingProfileArtifacts,
                }
            }
        })
        .collect();
    let manifest_identity = ManifestContentIdentity::new(evidence_identity)
        .map_err(|error| format!("manifest identity invalid: {error:?}"))?;
    let manifest = DatasetSnapshotManifest::new(
        SnapshotIdentity::new(
            dataset.repository.clone(),
            dataset.revision.clone(),
            evidence_identity,
        )
        .map_err(|error| format!("snapshot identity invalid: {error:?}"))?,
        manifest_identity,
        profiles,
    )
    .map_err(|error| format!("snapshot manifest invalid: {error:?}"))?;
    manifest
        .activate(&mut LocalVerifier)
        .map_err(|error| format!("provisional snapshot activation failed: {error:?}"))
}

struct LocalVerifier;

impl DatasetSnapshotVerifier for LocalVerifier {
    fn verify(
        &mut self,
        request: SnapshotVerificationRequest<'_>,
    ) -> Result<SnapshotVerificationAttestation, SnapshotVerificationFailure> {
        SnapshotVerificationAttestation::new(
            request.snapshot_identity().clone(),
            request.manifest_content_identity().clone(),
            "clearra-pc4-local-exact-parity-verifier-v1",
        )
        .map_err(|_| SnapshotVerificationFailure::Rejected)
    }
}

struct ArtifactFiles {
    fields: LocalArtifactFile,
    offsets: LocalArtifactFile,
    graph: LocalArtifactFile,
}

impl ArtifactFiles {
    fn open(dataset: &Dataset) -> Result<Self, String> {
        Ok(Self {
            fields: LocalArtifactFile::open(&dataset.fields.path, dataset.fields.byte_len, true)?,
            offsets: LocalArtifactFile::open(
                &dataset.offsets.path,
                dataset.offsets.byte_len,
                true,
            )?,
            graph: LocalArtifactFile::open(&dataset.graph.path, dataset.graph.byte_len, false)?,
        })
    }

    fn read(&mut self, range: &clearra_pc4_tablebase::RangeRequest) -> Result<Vec<u8>, String> {
        let length = usize::try_from(range.end_exclusive() - range.offset())
            .map_err(|_| "range length overflow")?;
        let file = match range.artifact() {
            Pc4ArtifactRole::FieldHashIndex => &mut self.fields,
            Pc4ArtifactRole::GraphOffsets => &mut self.offsets,
            Pc4ArtifactRole::Graph => &mut self.graph,
        };
        file.read(range.offset(), length)
    }

    fn usage(&self) -> Result<(u64, u64), String> {
        let reads = self
            .fields
            .physical_reads
            .checked_add(self.offsets.physical_reads)
            .and_then(|value| value.checked_add(self.graph.physical_reads))
            .ok_or("physical file read count overflow")?;
        let bytes = self
            .fields
            .physical_bytes
            .checked_add(self.offsets.physical_bytes)
            .and_then(|value| value.checked_add(self.graph.physical_bytes))
            .ok_or("physical file byte count overflow")?;
        Ok((reads, bytes))
    }
}

struct LocalArtifactFile {
    source: File,
    length: u64,
    cache_index: bool,
    pages: BTreeMap<u64, Vec<u8>>,
    insertion_order: VecDeque<u64>,
    physical_reads: u64,
    physical_bytes: u64,
}

impl LocalArtifactFile {
    fn open(path: &Path, length: u64, cache_index: bool) -> Result<Self, String> {
        Ok(Self {
            source: File::open(path).map_err(io_error)?,
            length,
            cache_index,
            pages: BTreeMap::new(),
            insertion_order: VecDeque::new(),
            physical_reads: 0,
            physical_bytes: 0,
        })
    }

    fn read(&mut self, offset: u64, length: usize) -> Result<Vec<u8>, String> {
        let length_u64 = u64::try_from(length).map_err(|_| "local range length overflow")?;
        if length == 0
            || length > 65_536
            || offset
                .checked_add(length_u64)
                .is_none_or(|end| end > self.length)
        {
            return Err("local tablebase range outside artifact".to_owned());
        }
        if !self.cache_index {
            return self.read_exact_at(offset, length);
        }
        let mut result = vec![0_u8; length];
        let end = offset + length_u64;
        let mut page_start = offset / LOCAL_INDEX_PAGE_BYTES * LOCAL_INDEX_PAGE_BYTES;
        while page_start < end {
            if !self.pages.contains_key(&page_start) {
                let page_length =
                    usize::try_from(LOCAL_INDEX_PAGE_BYTES.min(self.length - page_start))
                        .map_err(|_| "local index page length overflow")?;
                let page = self.read_exact_at(page_start, page_length)?;
                if self.pages.len() == LOCAL_INDEX_MAX_PAGES {
                    let evicted = self
                        .insertion_order
                        .pop_front()
                        .ok_or("local index page order drift")?;
                    self.pages.remove(&evicted);
                }
                self.pages.insert(page_start, page);
                self.insertion_order.push_back(page_start);
            }
            let page = self
                .pages
                .get(&page_start)
                .ok_or("local index page missing after admission")?;
            let begin = offset.max(page_start);
            let finish = end.min(page_start + page.len() as u64);
            result[(begin - offset) as usize..(finish - offset) as usize].copy_from_slice(
                &page[(begin - page_start) as usize..(finish - page_start) as usize],
            );
            page_start = page_start
                .checked_add(LOCAL_INDEX_PAGE_BYTES)
                .ok_or("local index page offset overflow")?;
        }
        Ok(result)
    }

    fn read_exact_at(&mut self, offset: u64, length: usize) -> Result<Vec<u8>, String> {
        self.source
            .seek(SeekFrom::Start(offset))
            .map_err(io_error)?;
        let mut bytes = vec![0_u8; length];
        self.source.read_exact(&mut bytes).map_err(io_error)?;
        self.physical_reads = self
            .physical_reads
            .checked_add(1)
            .ok_or("physical file read count overflow")?;
        self.physical_bytes = self
            .physical_bytes
            .checked_add(length as u64)
            .ok_or("physical file byte count overflow")?;
        Ok(bytes)
    }
}

fn compare_exact_family(
    result: &clearra_core_executor::CoreExecutionResult,
    expected: &[clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64TilingIdentity],
) -> Result<(), String> {
    if result.normalized_solution_identities().len() == expected.len() {
        if result.normalized_solution_identities() != expected {
            return Err("tablebase and offline canonical identities differ".to_owned());
        }
        return Ok(());
    }
    let store = result
        .tiling_solution_page_store()
        .ok_or("tablebase result lacks exact family storage")?;
    if store.len() != expected.len() {
        return Err("tablebase family storage count differs from offline".to_owned());
    }
    let mut index = 0_usize;
    let mut mismatch = false;
    store
        .for_each_page_identity(0, expected.len(), |identity| {
            if expected.get(index) != Some(&identity) {
                mismatch = true;
            }
            index += 1;
        })
        .map_err(str::to_owned)?;
    if mismatch || index != expected.len() {
        return Err("tablebase and offline canonical identities differ".to_owned());
    }
    Ok(())
}

fn io_error(error: std::io::Error) -> String {
    format!("I/O error: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT_FILE: AtomicU64 = AtomicU64::new(1);

    struct TemporaryFile(std::path::PathBuf);

    impl TemporaryFile {
        fn new(bytes: &[u8]) -> Self {
            let path = std::env::temp_dir().join(format!(
                "clearra-pc4-qualifier-{}-{}.bin",
                std::process::id(),
                NEXT_FILE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::write(&path, bytes).expect("write local reader fixture");
            Self(path)
        }
    }

    impl Drop for TemporaryFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }

    #[test]
    fn local_qualification_reader_matches_product_index_page_and_exact_graph_policy() {
        let source = (0..10_000)
            .map(|value| (value % 251) as u8)
            .collect::<Vec<_>>();
        let file = TemporaryFile::new(&source);
        let mut paged = LocalArtifactFile::open(&file.0, source.len() as u64, true).unwrap();
        assert_eq!(paged.read(17, 73).unwrap(), source[17..90]);
        assert_eq!(paged.read(79, 41).unwrap(), source[79..120]);
        assert_eq!(paged.physical_reads, 1);
        assert_eq!(paged.physical_bytes, LOCAL_INDEX_PAGE_BYTES);

        let mut exact = LocalArtifactFile::open(&file.0, source.len() as u64, false).unwrap();
        assert_eq!(exact.read(17, 73).unwrap(), source[17..90]);
        assert_eq!(exact.read(79, 41).unwrap(), source[79..120]);
        assert_eq!(exact.physical_reads, 2);
        assert_eq!(exact.physical_bytes, 114);
    }
}
