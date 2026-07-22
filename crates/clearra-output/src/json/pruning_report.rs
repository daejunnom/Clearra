use clearra_core_domain::pruning::{PruningProofLedger, PruningProofLedgerEntry};

use super::JsonValue;

pub struct PruningReportJson;

impl PruningReportJson {
    pub fn from_ledger(ledger: &PruningProofLedger) -> JsonValue {
        JsonValue::object([
            (
                "pruning_evidence_policy",
                JsonValue::string(ledger.evidence_policy().as_str()),
            ),
            (
                "pruning_proof_ledger_entry_count",
                JsonValue::number(ledger.entries().len().to_string()),
            ),
            (
                "pruning_evidence_truncated",
                JsonValue::Bool(ledger.evidence_truncated()),
            ),
            (
                "pruning_dropped_evidence_count",
                JsonValue::number(ledger.dropped_evidence_count().to_string()),
            ),
            (
                "pruning_complete_required_capacity_hit",
                JsonValue::Bool(ledger.complete_required_capacity_hit()),
            ),
            (
                "pruning_candidates_kept_due_to_evidence_capacity",
                JsonValue::number(
                    ledger
                        .candidates_kept_due_to_evidence_capacity()
                        .to_string(),
                ),
            ),
            (
                "pruning_entries",
                JsonValue::array(ledger.entries().iter().map(entry_json)),
            ),
        ])
    }
}

fn entry_json(entry: &PruningProofLedgerEntry) -> JsonValue {
    JsonValue::object([
        (
            "batch_id",
            JsonValue::number(entry.batch_id().0.to_string()),
        ),
        (
            "state_layer",
            JsonValue::number(entry.state_layer().to_string()),
        ),
        (
            "prune_reason",
            JsonValue::string(entry.prune_reason().as_str()),
        ),
        (
            "affected_candidate_count",
            JsonValue::number(entry.affected_candidate_count().to_string()),
        ),
        (
            "proof_level",
            JsonValue::string(entry.proof_level().as_str()),
        ),
        (
            "clear_state_key",
            entry
                .clear_state_key()
                .map(|key| JsonValue::number(key.0.to_string()))
                .unwrap_or(JsonValue::Null),
        ),
        (
            "fallback_if_invalid",
            JsonValue::string(match entry.fallback_if_invalid() {
                clearra_core_domain::pruning::FallbackAction::KeepCandidate => "keep-candidate",
                clearra_core_domain::pruning::FallbackAction::RunBuildUp => "run-buildup",
                clearra_core_domain::pruning::FallbackAction::DisableDomainPruning => {
                    "disable-domain-pruning"
                }
            }),
        ),
        (
            "evidence_digest",
            JsonValue::number(entry.evidence_digest().0.to_string()),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::pruning::{
        BatchId, EvidenceDigest, FallbackAction, PruneEvidenceContext, PruneReason,
        PruningProofLedger,
    };

    use super::*;

    #[test]
    fn pruning_report_includes_reason_proof_and_evidence() {
        let mut ledger = PruningProofLedger::default();
        let context = PruneEvidenceContext::new(BatchId(7), 2, 3, EvidenceDigest(0xabc)).unwrap();
        ledger.record_engine_drop_evidence(
            PruneReason::PlacementCollision,
            context,
            FallbackAction::RunBuildUp,
        );

        let report = PruningReportJson::from_ledger(&ledger);

        let JsonValue::Object(fields) = report else {
            panic!("object report");
        };
        assert!(fields
            .iter()
            .any(|field| field.key() == "pruning_proof_ledger_entry_count"));
        assert!(fields
            .iter()
            .any(|field| field.key() == "pruning_evidence_policy"));
        assert!(fields
            .iter()
            .any(|field| field.key() == "pruning_evidence_truncated"));
        assert!(fields
            .iter()
            .any(|field| field.key() == "pruning_complete_required_capacity_hit"));
        assert!(fields
            .iter()
            .any(|field| { field.key() == "pruning_candidates_kept_due_to_evidence_capacity" }));
        assert!(fields.iter().any(|field| field.key() == "pruning_entries"));
    }
}
