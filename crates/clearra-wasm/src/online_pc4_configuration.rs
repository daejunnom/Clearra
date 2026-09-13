//! SRP: turn the trusted host's generation qualification receipt into the pure
//! reader's activated snapshot. This is not an upstream signature verifier:
//! HTTPS discovery and bounded byte validation belong to the host adapter.
use clearra_pc4_tablebase::*;
use serde_json::Value;
use sha2::{Digest, Sha256};

const CONTRACT: &str = "hydra-jstris-180-complete-graph-v1";
pub(crate) fn configure(json: &str) -> Result<Option<ActivatedSnapshot>, &'static str> {
    if json == "null" {
        return Ok(None);
    }
    if json.len() > 65_536 {
        return Err("pc4_online_generation_too_large");
    }
    let v: Value = serde_json::from_str(json).map_err(|_| "pc4_online_generation_invalid")?;
    if v["schema"] != "clearra.pc4.host-generation.v1" {
        return Err("pc4_online_generation_contract");
    }
    let repository = string(&v, "repository")?;
    let revision = string(&v, "revision")?;
    if revision.len() != 40
        || !revision
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err("pc4_online_revision_invalid");
    }
    let identity = SnapshotIdentity::new(repository, revision, format!("hf:{revision}"))
        .map_err(|_| "pc4_online_generation_identity_invalid")?;
    let digest = Sha256::digest(json.as_bytes());
    let content = ManifestContentIdentity::new(format!("host-sha256:{digest:x}"))
        .map_err(|_| "pc4_online_generation_identity_invalid")?;
    let slots = v["profiles"]
        .as_array()
        .ok_or("pc4_online_profile_slots_invalid")?;
    if slots.len() != 5 {
        return Err("pc4_online_profile_slots_invalid");
    }
    let mut profiles = Vec::new();
    for profile in Pc4RuleProfile::ALL {
        let found: Vec<_> = slots
            .iter()
            .filter(|s| s["profile"] == profile.as_str())
            .collect();
        if found.len() != 1 {
            return Err("pc4_online_profile_slots_invalid");
        }
        let slot = found[0];
        if slot["status"] != "ready" {
            profiles.push(ProfileAvailability::Unsupported {
                profile,
                reason: UnsupportedProfileReason::MissingProfileSpecificIndex,
            });
            continue;
        }
        // Future profiles require their own format/target declaration, not an
        // alias of this canonical slot. A ready string alone grants nothing.
        if profile != Pc4RuleProfile::Jstris180
            || slot["upstream_complete"] != true
            || slot["reader_contract"] != CONTRACT
            || slot["target_width"] != 3
            || slot["target_lines"] != serde_json::json!([4])
        {
            return Err("pc4_online_profile_contract_unsupported");
        }
        let count = number(slot, "field_count")?;
        let terminal = number(slot, "terminal_id")?;
        if count < 2 || terminal != count - 1 {
            return Err("pc4_online_terminal_mismatch");
        }
        let fields = artifact(
            &slot["artifacts"]["fields"],
            Pc4ArtifactRole::FieldHashIndex,
            "field_hash_to_id.v1.bin",
        )?;
        let offsets = artifact(
            &slot["artifacts"]["offsets"],
            Pc4ArtifactRole::GraphOffsets,
            "graph_offsets.u32.bin",
        )?;
        let graph = artifact(
            &slot["artifacts"]["graph"],
            Pc4ArtifactRole::Graph,
            "graph.bin",
        )?;
        let observations = slot["evidence"]
            .as_array()
            .ok_or("pc4_online_reader_evidence_missing")?;
        let first = observations
            .first()
            .ok_or("pc4_online_reader_evidence_missing")?;
        let last = observations
            .last()
            .ok_or("pc4_online_reader_evidence_missing")?;
        if first["id"] != 0
            || first["hash"] != 0
            || first["start"] != 0
            || last["id"] != terminal
            || last["hash"] != 0xff_ffff_ffff_u64
            || last["end"] != graph.byte_len()
        {
            return Err("pc4_online_reader_evidence_mismatch");
        }
        let qualification = ProfileQualification::new(
            "fhid-goff-v1-record-ordinal",
            CONTRACT,
            "upstream-declared-jstris180-complete",
            format!("host-reader:{digest:x}"),
        )
        .map_err(|_| "pc4_online_profile_qualification_invalid")?;
        let lines = Pc4TargetLines::new(4).unwrap();
        let manifest = Pc4ProfileManifest::new(
            profile,
            count,
            GraphTargetEncoding::U24LittleEndian,
            FieldIdIndexRelation::RecordOrdinal,
            16_384,
            fields,
            offsets,
            graph,
            qualification,
        )
        .map_err(|_| "pc4_online_profile_manifest_invalid")?
        .with_target_qualifications(vec![ProfileTargetCompletenessQualification::new(
            Pc4TerminalUseCase::PcSearch,
            lines,
            Pc4TerminalFieldIdentity::full_rows(lines, terminal),
            "hydra-full-bottom-four-rows",
            "upstream-declared-complete-four-line-pc-graph",
            format!("host-terminal:{digest:x}"),
            "clearra-ilc-jstris180-materializer-v1",
        )
        .map_err(|_| "pc4_online_target_qualification_invalid")?])
        .map_err(|_| "pc4_online_target_qualification_invalid")?;
        profiles.push(ProfileAvailability::qualified(manifest));
    }
    let manifest = DatasetSnapshotManifest::new(identity.clone(), content.clone(), profiles)
        .map_err(|_| "pc4_online_generation_manifest_invalid")?;
    let mut verifier = HostReceiptVerifier { identity, content };
    manifest
        .activate(&mut verifier)
        .map(Some)
        .map_err(|_| "pc4_online_generation_activation_failed")
}
struct HostReceiptVerifier {
    identity: SnapshotIdentity,
    content: ManifestContentIdentity,
}
impl DatasetSnapshotVerifier for HostReceiptVerifier {
    fn verify(
        &mut self,
        request: SnapshotVerificationRequest<'_>,
    ) -> Result<SnapshotVerificationAttestation, SnapshotVerificationFailure> {
        if request.snapshot_identity() != &self.identity
            || request.manifest_content_identity() != &self.content
        {
            return Err(SnapshotVerificationFailure::Rejected);
        }
        SnapshotVerificationAttestation::new(
            self.identity.clone(),
            self.content.clone(),
            "https-host-reader-qualification-v1",
        )
        .map_err(|_| SnapshotVerificationFailure::Rejected)
    }
}
fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str, &'static str> {
    v[key].as_str().ok_or("pc4_online_generation_invalid")
}
fn number(v: &Value, key: &str) -> Result<u32, &'static str> {
    v[key]
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or("pc4_online_generation_invalid")
}
fn artifact(
    v: &Value,
    role: Pc4ArtifactRole,
    path: &str,
) -> Result<ArtifactDescriptor, &'static str> {
    if string(v, "path")? != path {
        return Err("pc4_online_artifact_path_mismatch");
    }
    let digest = string(v, "content_identity")?;
    if digest.len() != 71
        || !digest.starts_with("sha256:")
        || !digest[7..].bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err("pc4_online_content_identity_invalid");
    }
    ArtifactDescriptor::new(
        role,
        path,
        v["byte_length"]
            .as_u64()
            .ok_or("pc4_online_artifact_length_invalid")?,
        digest,
    )
    .map_err(|_| "pc4_online_artifact_invalid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn online_pc4_absent_or_malformed_host_receipt_never_activates_a_profile() {
        assert!(configure("null").unwrap().is_none());
        for value in [
            "{}",
            "[]",
            "true",
            "{\"schema\":\"clearra.pc4.host-generation.v1\"}",
        ] {
            assert!(configure(value).is_err(), "{value}");
        }
        assert!(configure(&" ".repeat(65_537)).is_err());
    }

    #[test]
    fn online_pc4_artifact_identity_is_dynamic_but_never_changes_roles() {
        let value = serde_json::json!({ "path": "graph.bin", "byte_length": 27,
            "content_identity": format!("sha256:{}", "b".repeat(64)) });
        assert!(artifact(&value, Pc4ArtifactRole::Graph, "graph.bin").is_ok());
        assert!(artifact(
            &value,
            Pc4ArtifactRole::GraphOffsets,
            "graph_offsets.u32.bin"
        )
        .is_err());
        let mut invalid = value;
        invalid["content_identity"] = serde_json::json!("not-a-content-identity");
        assert!(artifact(&invalid, Pc4ArtifactRole::Graph, "graph.bin").is_err());
    }
}
