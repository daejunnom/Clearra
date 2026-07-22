use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_pc_graph::request::{PcContinuationTokenCodec, PcHoldPolicy};
use clearra_problem::PcQuery;

use crate::service::field;

pub(crate) fn opening_continuation_fields(
    pc_query: &PcQuery,
    fixed_pieces: Option<&[PieceKind]>,
    consumed: usize,
) -> Vec<(String, String)> {
    let remaining = fixed_pieces
        .map(|pieces| pieces.get(consumed..).unwrap_or(&[]))
        .unwrap_or(&[]);
    let next_candidate = next_pc_candidate(remaining.len());
    let token = if next_candidate.is_some() && !remaining.is_empty() {
        let query = pc_query.to_opening_query();
        Some(PcContinuationTokenCodec::encode_opening_continuation(
            &query,
            pc_query.hold_policy().initial_piece(),
            remaining,
        ))
    } else {
        None
    };
    let token_value = token.clone().unwrap_or_else(|| "none".to_owned());
    vec![
        field(
            "continuation_enough_queue_for_next_pc",
            next_candidate.is_some(),
        ),
        field("remaining_queue_len", remaining.len()),
        field("remaining_queue_preview", pieces_preview(remaining)),
        field("remaining_hold", hold_policy_name(pc_query.hold_policy())),
        field("next_pc_available", next_candidate.is_some()),
        field("next_pc_candidate", next_candidate.unwrap_or("none")),
        field("continuation_token_available", token.is_some()),
        field(
            "continuation_token_unavailable_reason",
            if token.is_some() {
                "none"
            } else {
                "insufficient_remaining_queue"
            },
        ),
        field("continue_available", token.is_some()),
        field("continuation_available", token.is_some()),
        field(
            "continuation_token_version",
            if token.is_some() { "pc2" } else { "none" },
        ),
        field("continuation_token", token_value.clone()),
        field(
            "continue_hint",
            if token_value == "none" {
                "none".to_owned()
            } else {
                format!("clearra continue {token_value}")
            },
        ),
    ]
}

pub(crate) fn remaining_preview(pieces: Option<&[PieceKind]>, consumed: usize) -> String {
    pieces
        .map(|pieces| pieces_preview(pieces.get(consumed..).unwrap_or(&[])))
        .unwrap_or_else(|| "none".to_owned())
}

fn next_pc_candidate(remaining_len: usize) -> Option<&'static str> {
    if remaining_len >= 15 {
        Some("6L")
    } else if remaining_len >= 10 {
        Some("4L")
    } else if remaining_len >= 5 {
        Some("2L")
    } else {
        None
    }
}

fn hold_policy_name(policy: PcHoldPolicy) -> String {
    match policy {
        PcHoldPolicy::Disabled | PcHoldPolicy::EnabledEmpty => "none".to_owned(),
        PcHoldPolicy::EnabledWithPiece(piece) => piece.as_ascii().to_string(),
    }
}

fn pieces_preview(pieces: &[PieceKind]) -> String {
    if pieces.is_empty() {
        return "none".to_owned();
    }
    pieces.iter().map(|piece| piece.as_ascii()).collect()
}
