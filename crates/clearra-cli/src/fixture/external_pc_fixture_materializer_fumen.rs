use std::{
    fs,
    path::{Path, PathBuf},
};

use clearra_fumen::{SourceFumenDiagramSet, SourceFumenSetup};
use serde_json::{Map, Value};

use super::external_pc_fixture_materializer_fields::optional_string;

pub(super) fn read_initial_setup(
    fixture_path: &Path,
    input: &Map<String, Value>,
) -> Result<SourceFumenSetup, String> {
    let initial_fumen = optional_string(input, "initial_fumen")
        .ok_or_else(|| "external PC fixture missing input.initial_fumen".to_owned())?;
    let resolved_path = resolve_material_path(fixture_path, initial_fumen);
    let text = fs::read_to_string(&resolved_path).map_err(|error| {
        format!(
            "external PC fixture could not read input.initial_fumen '{}': {error}",
            resolved_path.display()
        )
    })?;
    let setup = SourceFumenSetup::decode(&text).map_err(|error| {
        format!(
            "external PC fixture could not decode input.initial_fumen '{}': {error:?}",
            resolved_path.display()
        )
    })?;
    validate_setup_rows(input, setup)?;
    Ok(setup)
}

fn validate_setup_rows(input: &Map<String, Value>, setup: SourceFumenSetup) -> Result<(), String> {
    let rows = input
        .get("expected_setup_rows_top_down")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            "external PC fixture missing input.expected_setup_rows_top_down".to_owned()
        })?;
    if rows.is_empty() || rows.len() > 6 {
        return Err(
            "external PC fixture expected setup must contain between one and six rows".to_owned(),
        );
    }

    let mut expected_mask = 0u64;
    for (top_index, row) in rows.iter().enumerate() {
        let row = row
            .as_str()
            .ok_or_else(|| "external PC fixture expected setup rows must be strings".to_owned())?;
        if row.chars().count() != 10 {
            return Err(format!(
                "external PC fixture expected setup row {top_index} must be exactly 10 cells"
            ));
        }
        let y = rows.len() - 1 - top_index;
        for (x, cell) in row.chars().enumerate() {
            match cell {
                'O' => expected_mask |= 1u64 << (y * 10 + x),
                'X' => {}
                _ => {
                    return Err(format!(
                        "external PC fixture expected setup row {top_index} uses '{cell}'; only O=occupied and X=empty are allowed"
                    ));
                }
            }
        }
    }

    if setup.initial_board_mask() != expected_mask {
        return Err(format!(
            "external PC fixture decoded setup mask {:#018x} does not match top-down O/X contract {expected_mask:#018x}",
            setup.initial_board_mask()
        ));
    }
    Ok(())
}

pub(super) fn read_source_fumen_diagrams_if_requested(
    fixture_path: &Path,
    source_id: &str,
    input: &Map<String, Value>,
) -> Result<Option<SourceFumenDiagramSet>, String> {
    let requested = input
        .get("source_fumen_from_registry")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !requested {
        return Ok(None);
    }

    let registry_path = fixture_path
        .parent()
        .ok_or_else(|| "external PC fixture has no parent directory".to_owned())?
        .join("source_registry.json");
    let registry_text = fs::read_to_string(&registry_path).map_err(|error| {
        format!(
            "external PC fixture could not read source registry '{}': {error}",
            registry_path.display()
        )
    })?;
    let registry = serde_json::from_str::<Value>(registry_text.trim_start_matches('\u{feff}'))
        .map_err(|error| format!("external PC source registry is invalid JSON: {error}"))?;
    let source = registry
        .get("sources")
        .and_then(Value::as_array)
        .and_then(|sources| {
            sources
                .iter()
                .find(|source| source.get("source_id").and_then(Value::as_str) == Some(source_id))
        })
        .ok_or_else(|| format!("external PC source registry missing source_id '{source_id}'"))?;
    let source_url = source
        .get("preferred_fumen_source_url")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            format!("external PC source '{source_id}' has no preferred_fumen_source_url")
        })?;
    SourceFumenDiagramSet::decode(source_url)
        .map(Some)
        .map_err(|error| format!("external PC source fumen decode failed: {error:?}"))
}

fn resolve_material_path(fixture_path: &Path, material_path: &str) -> PathBuf {
    let path = Path::new(material_path);
    if path.is_absolute() || path.exists() {
        return path.to_path_buf();
    }
    if let Some(parent) = fixture_path.parent() {
        let sibling = parent.join(path);
        if sibling.exists() {
            return sibling;
        }
    }
    workspace_root().join(path)
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("clearra-cli lives under workspace/crates")
        .to_path_buf()
}
