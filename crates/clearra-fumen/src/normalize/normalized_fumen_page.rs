use std::collections::BTreeMap;

use super::normalized_solution_key::NormalizedSolutionKey;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NormalizedFumenPage {
    page_index: usize,
    kind: String,
    fields: BTreeMap<String, String>,
    initial_board_mask: u64,
    final_board_mask: u64,
    piece_sequence: Vec<String>,
    hold_decision_sequence: Vec<String>,
    operation_sequence: Vec<String>,
    cleared_line_sequence: Vec<String>,
    normalized_shape_key: String,
    normalized_tiling_key: String,
    mirror_transform: String,
    grayout_normalized: bool,
    field_repeat_expanded: bool,
}

impl NormalizedFumenPage {
    pub fn from_comment_page(page_index: usize, comment_page: &str) -> Self {
        let fields = parse_comment_fields(comment_page);
        let kind = fields
            .get("kind")
            .cloned()
            .unwrap_or_else(|| "comment-only".to_owned());
        let initial_board_mask = fields
            .get("initial_board_mask")
            .and_then(|value| parse_mask(value))
            .unwrap_or(0);
        let final_board_mask = fields
            .get("final_board_mask")
            .and_then(|value| parse_mask(value))
            .or_else(|| {
                fields
                    .get("final_board_empty")
                    .filter(|value| is_true(value))
                    .map(|_| 0)
            })
            .unwrap_or(initial_board_mask);
        let piece_sequence = fields
            .get("piece_sequence")
            .map(|value| normalize_piece_sequence(value))
            .unwrap_or_default();
        let hold_decision_sequence = fields
            .get("hold_decision_sequence")
            .map(|value| normalize_list(value))
            .unwrap_or_default();
        let operation_sequence = fields
            .get("operation_sequence")
            .map(|value| normalize_list(value))
            .unwrap_or_default();
        let cleared_line_sequence = fields
            .get("cleared_line_sequence")
            .or_else(|| fields.get("line_clear_events"))
            .map(|value| normalize_list(value))
            .unwrap_or_default();
        let normalized_shape_key = fields
            .get("normalized_shape_key")
            .cloned()
            .unwrap_or_else(|| derived_key("shape", &operation_sequence));
        let normalized_tiling_key = fields
            .get("normalized_tiling_key")
            .cloned()
            .unwrap_or_else(|| derived_key("tiling", &operation_sequence));
        let mirror_transform = fields
            .get("mirror_policy")
            .or_else(|| fields.get("mirror_transform"))
            .cloned()
            .unwrap_or_else(|| "none".to_owned());
        let grayout_normalized = fields
            .get("grayout_normalized")
            .map(|value| is_true(value))
            .unwrap_or(true);

        Self {
            page_index,
            kind,
            fields,
            initial_board_mask,
            final_board_mask,
            piece_sequence,
            hold_decision_sequence,
            operation_sequence,
            cleared_line_sequence,
            normalized_shape_key,
            normalized_tiling_key,
            mirror_transform,
            grayout_normalized,
            field_repeat_expanded: true,
        }
    }
}
impl NormalizedFumenPage {
    pub const fn page_index(&self) -> usize {
        self.page_index
    }
}
impl NormalizedFumenPage {
    pub fn kind(&self) -> &str {
        &self.kind
    }
}
impl NormalizedFumenPage {
    pub fn field(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(String::as_str)
    }
}
impl NormalizedFumenPage {
    pub fn fields(&self) -> &BTreeMap<String, String> {
        &self.fields
    }
}
impl NormalizedFumenPage {
    pub const fn initial_board_mask(&self) -> u64 {
        self.initial_board_mask
    }
}
impl NormalizedFumenPage {
    pub const fn final_board_mask(&self) -> u64 {
        self.final_board_mask
    }
}
impl NormalizedFumenPage {
    pub fn piece_sequence(&self) -> &[String] {
        &self.piece_sequence
    }
}
impl NormalizedFumenPage {
    pub fn operation_sequence(&self) -> &[String] {
        &self.operation_sequence
    }
}
impl NormalizedFumenPage {
    pub fn mirror_transform(&self) -> &str {
        &self.mirror_transform
    }
}
impl NormalizedFumenPage {
    pub const fn grayout_normalized(&self) -> bool {
        self.grayout_normalized
    }
}
impl NormalizedFumenPage {
    pub const fn field_repeat_expanded(&self) -> bool {
        self.field_repeat_expanded
    }
}
impl NormalizedFumenPage {
    pub fn is_solution_page(&self) -> bool {
        self.kind == "normalized-solution"
            || self.kind.ends_with("-solution")
            || self
                .fields
                .get("normalized_solution_page")
                .is_some_and(|value| is_true(value))
    }
}
impl NormalizedFumenPage {
    pub fn solution_key(&self) -> Option<NormalizedSolutionKey> {
        self.is_solution_page().then(|| {
            NormalizedSolutionKey::new(
                self.initial_board_mask,
                self.final_board_mask,
                self.piece_sequence.clone(),
                self.hold_decision_sequence.clone(),
                self.operation_sequence.clone(),
                self.cleared_line_sequence.clone(),
                self.mirror_transform.clone(),
                self.normalized_shape_key.clone(),
                self.normalized_tiling_key.clone(),
            )
        })
    }
}

fn parse_comment_fields(comment_page: &str) -> BTreeMap<String, String> {
    comment_page
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| {
            (
                key.trim().to_ascii_lowercase(),
                normalize_scalar(value.trim()),
            )
        })
        .collect()
}

fn normalize_scalar(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_piece_sequence(value: &str) -> Vec<String> {
    let tokens = normalize_list(value);
    if tokens.len() == 1 && tokens[0].chars().all(is_piece_char) {
        return tokens[0].chars().map(|piece| piece.to_string()).collect();
    }

    tokens
        .into_iter()
        .map(|piece| piece.to_ascii_uppercase())
        .collect()
}

fn normalize_list(value: &str) -> Vec<String> {
    value
        .split(['|', ',', ' '])
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(normalize_scalar)
        .collect()
}

fn parse_mask(value: &str) -> Option<u64> {
    value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .map(|hex| u64::from_str_radix(hex, 16))
        .unwrap_or_else(|| value.parse())
        .ok()
}

fn is_true(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "true" | "1" | "yes"
    )
}

fn is_piece_char(ch: char) -> bool {
    matches!(
        ch.to_ascii_uppercase(),
        'I' | 'J' | 'L' | 'O' | 'S' | 'T' | 'Z'
    )
}

fn derived_key(prefix: &str, operation_sequence: &[String]) -> String {
    if operation_sequence.is_empty() {
        return format!("{prefix}:empty");
    }

    format!("{prefix}:{}", operation_sequence.join("/"))
}
