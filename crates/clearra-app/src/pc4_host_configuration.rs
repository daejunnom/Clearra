//! SRP: turn the trusted host's generation qualification receipt into the pure
//! reader's activated snapshot. This is not an upstream signature verifier:
//! HTTPS discovery and bounded byte validation belong to the host adapter.
use clearra_pc4_tablebase::*;
use serde_json::Value;
use sha2::{Digest, Sha256};

const CONTRACT: &str = "hydra-jstris-180-complete-graph-v1";
const TARGET_RECEIPT_SCHEMA: &str = "clearra.pc4.exact-target-qualification.v1";
const SETUP_TARGET_RECEIPT_SCHEMA: &str = "clearra.pc4.exact-setup-target-qualification.v1";
const PC_TERMINAL_SEMANTICS: &str = "clearra.pc4.full-bottom-rows-after-clear.v1";
const SETUP_TERMINAL_SEMANTICS: &str = "clearra.pc4.setup-complete-bottom-rows-after-clear.v1";
pub fn configure(json: &str) -> Result<Option<ActivatedSnapshot>, &'static str> {
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
        let target_qualifications =
            target_qualifications(slot, repository, revision, profile, count, terminal)?;
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
        // CONTRACT qualifies Hydra's sorted graph records and their inline
        // 40-bit source bitmap, so following a graph ID needs no FHID read.
        .with_graph_source_field_encoding(GraphSourceFieldEncoding::HydraU40BigEndianPrefix)
        .with_target_qualifications(target_qualifications)
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

fn target_qualifications(
    slot: &Value,
    repository: &str,
    revision: &str,
    profile: Pc4RuleProfile,
    field_count: u32,
    terminal: u32,
) -> Result<Vec<ProfileTargetCompletenessQualification>, &'static str> {
    let pc_lines = declared_target_lines(slot, "pc_search_target_lines")?;
    let setup_lines = declared_target_lines(slot, "setup_search_target_lines")?;
    let receipts = match &slot["target_qualification_receipts"] {
        Value::Null => &[][..],
        Value::Array(receipts) => receipts.as_slice(),
        _ => return Err("pc4_online_target_qualification_invalid"),
    };
    if pc_lines.len().saturating_add(setup_lines.len()) != receipts.len() {
        return Err("pc4_online_target_qualification_invalid");
    }
    let mut qualifications = Vec::with_capacity(receipts.len());
    let mut seen_pc_lines = Vec::new();
    let mut seen_setup_lines = Vec::new();
    for receipt in receipts {
        if receipt["repository"] != repository
            || receipt["revision"] != revision
            || receipt["profile"] != profile.as_str()
            || receipt["reader_contract"] != CONTRACT
            || receipt["terminal_id"].as_u64() != Some(u64::from(terminal))
        {
            return Err("pc4_online_target_qualification_invalid");
        }
        let use_case = match (receipt["schema"].as_str(), receipt["use_case"].as_str()) {
            (Some(TARGET_RECEIPT_SCHEMA), Some("pc-search")) => {
                if !receipt["setup_differential_identities"].is_null() {
                    return Err("pc4_online_target_qualification_invalid");
                }
                Pc4TerminalUseCase::PcSearch
            }
            (Some(SETUP_TARGET_RECEIPT_SCHEMA), Some("setup-search")) => {
                Pc4TerminalUseCase::SetupSearch
            }
            _ => return Err("pc4_online_target_qualification_invalid"),
        };
        let line = receipt["target_lines"]
            .as_u64()
            .and_then(|lines| u8::try_from(lines).ok())
            .ok_or("pc4_online_target_qualification_invalid")?;
        let lines =
            Pc4TargetLines::new(line).map_err(|_| "pc4_online_target_qualification_invalid")?;
        if lines.get() != 4 {
            return Err("pc4_online_target_qualification_invalid");
        }
        match use_case {
            Pc4TerminalUseCase::PcSearch => seen_pc_lines.push(lines.get()),
            Pc4TerminalUseCase::SetupSearch => seen_setup_lines.push(lines.get()),
        }
        let terminal_field = Pc4TerminalFieldIdentity::new(
            lines,
            terminal,
            receipt["terminal_hash"]
                .as_u64()
                .ok_or("pc4_online_target_qualification_invalid")?,
        )
        .map_err(|_| "pc4_online_target_qualification_invalid")?;
        let terminal_semantics = match use_case {
            Pc4TerminalUseCase::PcSearch => PC_TERMINAL_SEMANTICS,
            Pc4TerminalUseCase::SetupSearch => SETUP_TERMINAL_SEMANTICS,
        };
        if receipt["terminal_semantics_identity"] != terminal_semantics
            || terminal_field.field_id() >= field_count
        {
            return Err("pc4_online_target_qualification_invalid");
        }
        let qualification = ProfileTargetCompletenessQualification::new(
            use_case,
            lines,
            terminal_field,
            terminal_semantics,
            exact_evidence_identity(receipt, "outgoing_edge_completeness_identity")?,
            exact_evidence_identity(receipt, "known_answer_identity")?,
            exact_evidence_identity(receipt, "offline_exact_parity_identity")?,
        )
        .map_err(|_| "pc4_online_target_qualification_invalid")?;
        let qualification = if use_case == Pc4TerminalUseCase::SetupSearch {
            qualification
                .with_setup_search_differential(setup_differential_qualification(receipt)?)
                .map_err(|_| "pc4_online_target_qualification_invalid")?
        } else {
            qualification
        };
        qualifications.push(qualification);
    }
    seen_pc_lines.sort_unstable();
    seen_setup_lines.sort_unstable();
    if seen_pc_lines != pc_lines || seen_setup_lines != setup_lines {
        return Err("pc4_online_target_qualification_invalid");
    }
    Ok(qualifications)
}

fn setup_differential_qualification(
    receipt: &Value,
) -> Result<SetupSearchDifferentialQualification, &'static str> {
    let identities = receipt["setup_differential_identities"]
        .as_object()
        .filter(|identities| identities.len() == 4)
        .ok_or("pc4_online_target_qualification_invalid")?;
    SetupSearchDifferentialQualification::new(
        exact_evidence_identity_from(identities, "ranked_joint_identity")?,
        exact_evidence_identity_from(identities, "ranked_build_probability_identity")?,
        exact_evidence_identity_from(identities, "ranked_conditional_pc_identity")?,
        exact_evidence_identity_from(identities, "exact_path_detail_identity")?,
    )
    .map_err(|_| "pc4_online_target_qualification_invalid")
}

fn declared_target_lines(slot: &Value, key: &str) -> Result<Vec<u8>, &'static str> {
    let values = match &slot[key] {
        Value::Null => return Ok(Vec::new()),
        Value::Array(values) => values,
        _ => return Err("pc4_online_target_qualification_invalid"),
    };
    let mut lines = Vec::with_capacity(values.len());
    for value in values {
        let line = value
            .as_u64()
            .and_then(|line| u8::try_from(line).ok())
            .ok_or("pc4_online_target_qualification_invalid")?;
        Pc4TargetLines::new(line).map_err(|_| "pc4_online_target_qualification_invalid")?;
        if lines.last().is_some_and(|previous| *previous >= line) {
            return Err("pc4_online_target_qualification_invalid");
        }
        lines.push(line);
    }
    Ok(lines)
}

fn exact_evidence_identity<'a>(receipt: &'a Value, key: &str) -> Result<&'a str, &'static str> {
    exact_evidence_value(&receipt[key])
}

fn exact_evidence_identity_from<'a>(
    identities: &'a serde_json::Map<String, Value>,
    key: &str,
) -> Result<&'a str, &'static str> {
    exact_evidence_value(
        identities
            .get(key)
            .ok_or("pc4_online_target_qualification_invalid")?,
    )
}

fn exact_evidence_value(value: &Value) -> Result<&str, &'static str> {
    let identity = value
        .as_str()
        .ok_or("pc4_online_target_qualification_invalid")?;
    let digest = identity
        .strip_prefix("sha256:")
        .ok_or("pc4_online_target_qualification_invalid")?;
    if digest.len() != 64
        || digest
            .bytes()
            .any(|byte| !byte.is_ascii_hexdigit() || byte.is_ascii_uppercase())
        || digest.bytes().all(|byte| byte == b'0')
    {
        return Err("pc4_online_target_qualification_invalid");
    }
    Ok(identity)
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

    fn exact_receipt(revision: &str) -> Value {
        serde_json::json!({
            "schema": TARGET_RECEIPT_SCHEMA,
            "repository": "example/pc4",
            "revision": revision,
            "profile": "jstris-180",
            "reader_contract": CONTRACT,
            "use_case": "pc-search",
            "target_lines": 4,
            "terminal_id": 1,
            "terminal_hash": 0xff_ffff_ffff_u64,
            "terminal_semantics_identity": PC_TERMINAL_SEMANTICS,
            "outgoing_edge_completeness_identity": format!("sha256:{}", "0123456789abcdef".repeat(4)),
            "known_answer_identity": format!("sha256:{}", "123456789abcdef0".repeat(4)),
            "offline_exact_parity_identity": format!("sha256:{}", "23456789abcdef01".repeat(4)),
        })
    }

    fn setup_exact_receipt(revision: &str) -> Value {
        let mut receipt = exact_receipt(revision);
        receipt["schema"] = serde_json::json!(SETUP_TARGET_RECEIPT_SCHEMA);
        receipt["use_case"] = serde_json::json!("setup-search");
        receipt["terminal_semantics_identity"] = serde_json::json!(SETUP_TERMINAL_SEMANTICS);
        receipt["setup_differential_identities"] = serde_json::json!({
            "ranked_joint_identity": format!("sha256:{}", "3456789abcdef012".repeat(4)),
            "ranked_build_probability_identity": format!("sha256:{}", "456789abcdef0123".repeat(4)),
            "ranked_conditional_pc_identity": format!("sha256:{}", "56789abcdef01234".repeat(4)),
            "exact_path_detail_identity": format!("sha256:{}", "6789abcdef012345".repeat(4)),
        });
        receipt
    }

    fn host_generation(receipt: Option<Value>) -> Value {
        host_generation_with_receipts(receipt.into_iter().collect())
    }

    fn host_generation_with_receipts(receipts: Vec<Value>) -> Value {
        let revision = "a".repeat(40);
        let pc_lines = receipts
            .iter()
            .filter(|receipt| receipt["use_case"] == "pc-search")
            .map(|_| 4)
            .collect::<Vec<_>>();
        let setup_lines = receipts
            .iter()
            .filter(|receipt| receipt["use_case"] == "setup-search")
            .map(|_| 4)
            .collect::<Vec<_>>();
        let profiles = Pc4RuleProfile::ALL
            .into_iter()
            .map(|profile| {
                if profile != Pc4RuleProfile::Jstris180 {
                    return serde_json::json!({
                        "profile": profile.as_str(), "upstream_complete": false,
                        "status": "unavailable", "reason": "missing-profile-specific-index"
                    });
                }
                serde_json::json!({
                    "profile": profile.as_str(), "upstream_complete": true, "status": "ready",
                    "reader_contract": CONTRACT, "field_count": 2, "target_width": 3,
                    "target_lines": [4], "pc_search_target_lines": pc_lines.clone(),
                    "setup_search_target_lines": setup_lines.clone(),
                    "target_qualification_receipts": receipts.clone(),
                    "terminal_id": 1,
                    "artifacts": {
                        "fields": { "path": "field_hash_to_id.v1.bin", "byte_length": 32,
                            "content_identity": format!("sha256:{}", "1".repeat(64)) },
                        "offsets": { "path": "graph_offsets.u32.bin", "byte_length": 28,
                            "content_identity": format!("sha256:{}", "2".repeat(64)) },
                        "graph": { "path": "graph.bin", "byte_length": 27,
                            "content_identity": format!("sha256:{}", "3".repeat(64)) }
                    },
                    "evidence": [
                        { "id": 0, "hash": 0, "start": 0, "end": 15 },
                        { "id": 1, "hash": 0xff_ffff_ffff_u64, "start": 15, "end": 27 }
                    ]
                })
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "schema": "clearra.pc4.host-generation.v1", "repository": "example/pc4",
            "revision": revision, "profiles": profiles, "transferred_bytes": 0
        })
    }

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

    #[test]
    fn reader_ready_profile_without_exact_receipt_cannot_mint_a_pc_target() {
        let generation = host_generation(None);
        let snapshot = configure(&generation.to_string()).unwrap().unwrap();
        let profile = snapshot.profile(Pc4RuleProfile::Jstris180).unwrap();
        assert!(profile.target_qualifications().is_empty());
        assert!(snapshot
            .qualified_target(
                Pc4RuleProfile::Jstris180,
                Pc4TerminalUseCase::PcSearch,
                Pc4TargetLines::new(4).unwrap(),
            )
            .is_err());
    }

    #[test]
    fn exact_receipt_mints_only_its_generation_bound_pc_target() {
        let receipt = exact_receipt(&"a".repeat(40));
        let generation = host_generation(Some(receipt.clone()));
        let snapshot = configure(&generation.to_string()).unwrap().unwrap();
        let target = snapshot
            .qualified_target(
                Pc4RuleProfile::Jstris180,
                Pc4TerminalUseCase::PcSearch,
                Pc4TargetLines::new(4).unwrap(),
            )
            .unwrap();
        assert_eq!(
            target.qualification().outgoing_edge_completeness_identity(),
            receipt["outgoing_edge_completeness_identity"]
                .as_str()
                .unwrap()
        );
        assert_eq!(
            target.qualification().offline_exact_parity_identity(),
            receipt["offline_exact_parity_identity"].as_str().unwrap()
        );
    }

    #[test]
    fn setup_receipt_mints_only_setup_target_and_binds_each_objective() {
        let setup = setup_exact_receipt(&"a".repeat(40));
        let generation = host_generation_with_receipts(vec![setup.clone()]);
        let snapshot = configure(&generation.to_string()).unwrap().unwrap();
        assert!(snapshot
            .qualified_target(
                Pc4RuleProfile::Jstris180,
                Pc4TerminalUseCase::PcSearch,
                Pc4TargetLines::new(4).unwrap(),
            )
            .is_err());
        let target = snapshot
            .qualified_target(
                Pc4RuleProfile::Jstris180,
                Pc4TerminalUseCase::SetupSearch,
                Pc4TargetLines::new(4).unwrap(),
            )
            .unwrap();
        for (objective, key) in [
            (
                Pc4SetupDifferentialObjective::RankedJoint,
                "ranked_joint_identity",
            ),
            (
                Pc4SetupDifferentialObjective::RankedBuildProbability,
                "ranked_build_probability_identity",
            ),
            (
                Pc4SetupDifferentialObjective::RankedConditionalPc,
                "ranked_conditional_pc_identity",
            ),
            (
                Pc4SetupDifferentialObjective::ExactPathDetail,
                "exact_path_detail_identity",
            ),
        ] {
            assert_eq!(
                target.setup_differential_identity(objective),
                setup["setup_differential_identities"][key].as_str(),
            );
        }
    }

    #[test]
    fn setup_receipt_missing_or_placeholder_objective_evidence_fails_closed() {
        for mutate in [
            |receipt: &mut Value| {
                receipt["setup_differential_identities"]["ranked_joint_identity"] =
                    serde_json::json!("placeholder")
            },
            |receipt: &mut Value| {
                receipt["setup_differential_identities"]
                    .as_object_mut()
                    .unwrap()
                    .remove("exact_path_detail_identity");
            },
        ] {
            let mut receipt = setup_exact_receipt(&"a".repeat(40));
            mutate(&mut receipt);
            let generation = host_generation_with_receipts(vec![receipt]);
            assert_eq!(
                configure(&generation.to_string()).unwrap_err(),
                "pc4_online_target_qualification_invalid"
            );
        }
    }

    #[test]
    fn stale_mismatched_or_placeholder_exact_receipts_are_rejected() {
        let mut mutations: Vec<Box<dyn Fn(&mut Value)>> = vec![
            Box::new(|receipt| receipt["revision"] = serde_json::json!("b".repeat(40))),
            Box::new(|receipt| receipt["profile"] = serde_json::json!("srs")),
            Box::new(|receipt| receipt["use_case"] = serde_json::json!("setup-search")),
            Box::new(|receipt| receipt["target_lines"] = serde_json::json!(3)),
            Box::new(|receipt| receipt["terminal_id"] = serde_json::json!(0)),
            Box::new(|receipt| receipt["terminal_hash"] = serde_json::json!(0)),
            Box::new(|receipt| receipt["reader_contract"] = serde_json::json!("other")),
            Box::new(|receipt| {
                receipt["offline_exact_parity_identity"] = serde_json::json!("placeholder")
            }),
        ];
        for mutate in mutations.drain(..) {
            let mut receipt = exact_receipt(&"a".repeat(40));
            mutate(&mut receipt);
            let generation = host_generation(Some(receipt));
            assert_eq!(
                configure(&generation.to_string()).unwrap_err(),
                "pc4_online_target_qualification_invalid"
            );
        }
        let mut missing_receipt = host_generation(None);
        missing_receipt["profiles"][3]["pc_search_target_lines"] = serde_json::json!([4]);
        assert_eq!(
            configure(&missing_receipt.to_string()).unwrap_err(),
            "pc4_online_target_qualification_invalid"
        );
    }
}
