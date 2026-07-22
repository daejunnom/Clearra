use std::{collections::BTreeMap, fs, path::Path};

use clearra_fumen::SourceFumenColoredFieldSet;
use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SfinderReference {
    pub solution_count: usize,
    pub input_sequence_count: usize,
    pub rule_profile: Option<String>,
    pub drop_mode: Option<String>,
    pub coverage_counts_by_colored_field: BTreeMap<String, usize>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SfinderReferenceReport {
    pub source_path: String,
    pub expected_solution_count: usize,
    pub expected_input_sequence_count: usize,
    pub reference_rule_profile: Option<String>,
    pub actual_rule_profile: String,
    pub reference_drop_mode: Option<String>,
    pub actual_rotation_mode: String,
    pub rotation_mode_comparable: bool,
    pub coverage_comparable: bool,
    pub matched_solution_count: usize,
    pub missing_colored_field_keys: Vec<String>,
    pub unexpected_colored_field_keys: Vec<String>,
    pub coverage_count_mismatches: Vec<SfinderCoverageCountMismatch>,
    pub solution_set_exact_match: bool,
    pub coverage_counts_exact_match: bool,
    pub validation_passed: bool,
    pub exact_match: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct SfinderCoverageCountMismatch {
    pub colored_field_key: String,
    pub expected: usize,
    pub actual: usize,
}

impl SfinderReference {
    pub fn read(path: &Path) -> Result<Self, String> {
        let html = fs::read_to_string(path)
            .map_err(|error| format!("failed to read Sfinder HTML {}: {error}", path.display()))?;
        let solution_count = number_before(&html, " solutions")?;
        let input_sequence_count = number_between(&html, "[", " input sequences]")?;
        let mut coverage_counts_by_colored_field = BTreeMap::new();

        for fragment in html.split("<div><a href='").skip(1) {
            let Some((url, after_url)) = fragment.split_once("'>") else {
                continue;
            };
            let Some((_, probability_tail)) = after_url.split_once("</a> /") else {
                continue;
            };
            let Some(count) = bracketed_count(probability_tail) else {
                continue;
            };
            let decoded = SourceFumenColoredFieldSet::decode(url)
                .map_err(|error| format!("failed to decode Sfinder solution Fumen: {error:?}"))?;
            if decoded.keys().len() != 1 {
                return Err(
                    "a Sfinder per-solution Fumen must contain exactly one field".to_owned(),
                );
            }
            let key = decoded.keys().iter().next().expect("one key").clone();
            if coverage_counts_by_colored_field
                .insert(key, count)
                .is_some()
            {
                return Err("Sfinder HTML contains a duplicate colored solution field".to_owned());
            }
        }

        if coverage_counts_by_colored_field.len() != solution_count {
            return Err(format!(
                "Sfinder HTML declares {solution_count} solutions but contains {} per-solution records",
                coverage_counts_by_colored_field.len()
            ));
        }
        let metadata = read_run_metadata(path)?;
        Ok(Self {
            solution_count,
            input_sequence_count,
            rule_profile: metadata.rule_profile,
            drop_mode: metadata.drop_mode,
            coverage_counts_by_colored_field,
        })
    }

    pub fn compare(
        &self,
        path: &Path,
        actual_rule_profile: &str,
        actual_supports_180: bool,
        actual: &BTreeMap<String, usize>,
    ) -> SfinderReferenceReport {
        let missing_colored_field_keys = self
            .coverage_counts_by_colored_field
            .keys()
            .filter(|key| !actual.contains_key(*key))
            .cloned()
            .collect::<Vec<_>>();
        let unexpected_colored_field_keys = actual
            .keys()
            .filter(|key| !self.coverage_counts_by_colored_field.contains_key(*key))
            .cloned()
            .collect::<Vec<_>>();
        let coverage_count_mismatches = self
            .coverage_counts_by_colored_field
            .iter()
            .filter_map(|(key, expected)| {
                actual.get(key).and_then(|actual| {
                    (actual != expected).then(|| SfinderCoverageCountMismatch {
                        colored_field_key: key.clone(),
                        expected: *expected,
                        actual: *actual,
                    })
                })
            })
            .collect::<Vec<_>>();
        let matched_solution_count = self
            .coverage_counts_by_colored_field
            .keys()
            .filter(|key| actual.contains_key(*key))
            .count();
        let solution_set_exact_match = missing_colored_field_keys.is_empty()
            && unexpected_colored_field_keys.is_empty()
            && actual.len() == self.solution_count;
        let rule_profile_comparable = self
            .rule_profile
            .as_deref()
            .is_some_and(|profile| profile == actual_rule_profile);
        let rotation_mode_comparable = self
            .drop_mode
            .as_deref()
            .and_then(sfinder_drop_supports_180)
            .is_some_and(|supports_180| supports_180 == actual_supports_180);
        let coverage_comparable = rule_profile_comparable && rotation_mode_comparable;
        let coverage_counts_exact_match = coverage_count_mismatches.is_empty();
        let exact_match =
            solution_set_exact_match && coverage_comparable && coverage_counts_exact_match;
        let validation_passed =
            solution_set_exact_match && (!coverage_comparable || coverage_counts_exact_match);
        SfinderReferenceReport {
            source_path: path.display().to_string(),
            expected_solution_count: self.solution_count,
            expected_input_sequence_count: self.input_sequence_count,
            reference_rule_profile: self.rule_profile.clone(),
            actual_rule_profile: actual_rule_profile.to_owned(),
            reference_drop_mode: self.drop_mode.clone(),
            actual_rotation_mode: if actual_supports_180 {
                "locked-180-reverse-graph"
            } else {
                "locked-reverse-graph"
            }
            .to_owned(),
            rotation_mode_comparable,
            coverage_comparable,
            matched_solution_count,
            missing_colored_field_keys,
            unexpected_colored_field_keys,
            coverage_count_mismatches,
            solution_set_exact_match,
            coverage_counts_exact_match,
            validation_passed,
            exact_match,
        }
    }
}

struct SfinderRunMetadata {
    rule_profile: Option<String>,
    drop_mode: Option<String>,
}

fn read_run_metadata(html_path: &Path) -> Result<SfinderRunMetadata, String> {
    let Some(directory) = html_path.parent() else {
        return Ok(SfinderRunMetadata {
            rule_profile: None,
            drop_mode: None,
        });
    };
    let metadata_path = [
        directory.join("last_output.txt"),
        directory.parent().map_or_else(
            || directory.join("last_output.txt"),
            |parent| parent.join("last_output.txt"),
        ),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file());
    let Some(metadata_path) = metadata_path else {
        return Ok(SfinderRunMetadata {
            rule_profile: None,
            drop_mode: None,
        });
    };
    let metadata = fs::read_to_string(&metadata_path).map_err(|error| {
        format!(
            "failed to read Sfinder metadata {}: {error}",
            metadata_path.display()
        )
    })?;
    let rule_profile = metadata.lines().find_map(|line| {
        line.trim()
            .strip_prefix("Kicks:")
            .map(|value| {
                value
                    .trim()
                    .trim_start_matches('@')
                    .trim_end_matches(".properties")
                    .to_ascii_lowercase()
            })
            .filter(|value| !value.is_empty())
    });
    let drop_mode = metadata.lines().find_map(|line| {
        line.trim()
            .strip_prefix("Drop:")
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty())
    });
    Ok(SfinderRunMetadata {
        rule_profile,
        drop_mode,
    })
}

fn sfinder_drop_supports_180(drop_mode: &str) -> Option<bool> {
    match drop_mode {
        "softdrop180" | "180" => Some(true),
        "softdrop" | "harddrop" => Some(false),
        _ => None,
    }
}

fn number_before(source: &str, marker: &str) -> Result<usize, String> {
    let index = source
        .find(marker)
        .ok_or_else(|| format!("missing Sfinder marker: {marker}"))?;
    let prefix = &source[..index];
    let digits = prefix
        .chars()
        .rev()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    digits
        .parse()
        .map_err(|_| format!("missing number before Sfinder marker: {marker}"))
}

fn number_between(source: &str, start: &str, end: &str) -> Result<usize, String> {
    let end_index = source
        .find(end)
        .ok_or_else(|| format!("missing Sfinder marker: {end}"))?;
    let prefix = &source[..end_index];
    let start_index = prefix
        .rfind(start)
        .ok_or_else(|| format!("missing Sfinder marker: {start}"))?;
    prefix[start_index + start.len()..]
        .trim()
        .parse()
        .map_err(|_| "invalid Sfinder input sequence count".to_owned())
}

fn bracketed_count(source: &str) -> Option<usize> {
    let end = source.find("</span>")?;
    let prefix = &source[..end];
    let start = prefix.rfind('[')?;
    let close = prefix[start..].find(']')? + start;
    prefix[start + 1..close].parse().ok()
}
