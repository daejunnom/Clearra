use std::{
    fs,
    path::{Component, Path, PathBuf},
};

use clearra_app::{AppResponse, ProductPageSourceOwner, FIELD_DOCUMENT_MAX_INPUT_BYTES};
use clearra_host_contract::{
    FieldDocumentPayload, ParityReportPagePayload, ProductResultPayloadContent,
    RenderArtifactPayload,
};
use clearra_output::artifact::ByteArtifactCommit;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    error::CliErrorCode,
    input::file_input_guard,
    typed_document_utility_cli::{
        NativeTypedUtilityOutputKind, NativeTypedUtilityPlan, NativeTypedUtilitySurface,
    },
};

use super::{CliOutput, RenderFormat};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PendingDocumentUtilityFile {
    target: PathBuf,
    filename: String,
    bytes: Vec<u8>,
    sha256: String,
    maximum_bytes: u64,
    document_page_number: Option<u32>,
}

impl PendingDocumentUtilityFile {
    pub(crate) fn target(&self) -> &Path {
        &self.target
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) const fn maximum_bytes(&self) -> u64 {
        self.maximum_bytes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PendingDocumentUtilityArtifact {
    files: Vec<PendingDocumentUtilityFile>,
    base_stdout: String,
    render_format: RenderFormat,
}

impl PendingDocumentUtilityArtifact {
    pub(crate) fn files(&self) -> &[PendingDocumentUtilityFile] {
        &self.files
    }

    pub(crate) fn committed_stdout(
        &self,
        commits: &[ByteArtifactCommit],
    ) -> Result<String, &'static str> {
        if commits.len() != self.files.len() {
            return Err("typed-document-commit-count-mismatch");
        }
        for (file, commit) in self.files.iter().zip(commits) {
            if commit.byte_count() != file.bytes.len() as u64 || !commit.target_owned() {
                return Err("typed-document-commit-metadata-mismatch");
            }
        }
        match self.render_format {
            RenderFormat::Text | RenderFormat::TextVerbose | RenderFormat::TextDiagnostics => {
                Ok(self.base_stdout.clone())
            }
            RenderFormat::Json => {
                let mut value: Value = serde_json::from_str(&self.base_stdout)
                    .map_err(|_| "typed-document-stdout-json-invalid")?;
                let object = value
                    .as_object_mut()
                    .ok_or("typed-document-stdout-json-invalid")?;
                if object.contains_key("generated_files") {
                    return Err("typed-document-generated-files-collision");
                }
                let generated = self
                    .files
                    .iter()
                    .zip(commits)
                    .map(|(file, commit)| {
                        json!({
                            "filename": file.filename,
                            "target": file_input_guard::display_input_path(&file.target),
                            "document_page_number": file.document_page_number,
                            "bytes": commit.byte_count(),
                            "sha256": file.sha256,
                            "target_owned": true,
                            "file_identity_kind": commit.file_identity().platform(),
                            "file_identity": commit.file_identity().stable_value(),
                        })
                    })
                    .collect::<Vec<_>>();
                object.insert("generated_files".to_owned(), Value::Array(generated));
                serde_json::to_string(&value).map_err(|_| "typed-document-stdout-json-invalid")
            }
            RenderFormat::FumenLike => Err("typed-document-native-format-selector-invalid"),
        }
    }
}

pub(crate) fn render_typed_document_utility_success(
    response: &AppResponse,
    plan: &NativeTypedUtilityPlan,
    format: RenderFormat,
) -> CliOutput {
    if format == RenderFormat::FumenLike {
        return invalid("native typed-document utilities use text or JSON output selection");
    }
    let Some(result) = response.public_result_payload() else {
        return invalid("typed-document response omitted its public result payload");
    };
    let rendered = match (plan.surface(), result.content()) {
        (
            NativeTypedUtilitySurface::Parity,
            ProductResultPayloadContent::ParityReportPage(first),
        ) => render_parity(response, first, format),
        (
            NativeTypedUtilitySurface::Fumen { split: false },
            ProductResultPayloadContent::FieldDocument(document),
        ) => render_document(result.contract(), result.result_kind(), document, format),
        (
            NativeTypedUtilitySurface::FieldDocumentTransform(_),
            ProductResultPayloadContent::FieldDocument(document),
        ) => render_document(result.contract(), result.result_kind(), document, format),
        (
            NativeTypedUtilitySurface::Fumen { split: true },
            ProductResultPayloadContent::FieldDocumentSet(set),
        ) => render_document_set(
            result.contract(),
            result.result_kind(),
            set.documents(),
            format,
        ),
        (
            NativeTypedUtilitySurface::Render,
            ProductResultPayloadContent::RenderArtifact(artifact),
        ) => render_artifact(result.contract(), result.result_kind(), artifact, format),
        _ => Err("typed-document result payload does not match its command surface"),
    };
    let base_stdout = match rendered {
        Ok(value) => value,
        Err(reason) => return invalid(reason),
    };
    let pending = match plan.output() {
        Some(output) => match prepare_pending(
            result.content(),
            output.kind(),
            output.target(),
            base_stdout.clone(),
            format,
        ) {
            Ok(pending) => Some(pending),
            Err(reason) => return invalid(reason),
        },
        None => None,
    };
    let mut output = CliOutput::success(base_stdout);
    if let Some(pending) = pending {
        output = output.with_pending_document_utility_artifact(pending);
    }
    output
}

fn render_parity(
    response: &AppResponse,
    first: &ParityReportPagePayload,
    format: RenderFormat,
) -> Result<String, &'static str> {
    let pages = match response.public_page_source_owner() {
        Some(ProductPageSourceOwner::ParityReport(source)) => {
            let mut pages = Vec::new();
            pages
                .try_reserve_exact(source.pages().len())
                .map_err(|_| "parity-page-output-capacity-exceeded")?;
            for page_number in 1..=source.pages().len() {
                pages.push(
                    source
                        .page_payload(page_number, true)
                        .map_err(|_| "parity-page-owner-invalid")?,
                );
            }
            pages
        }
        _ if first.total_pages() == 1 => vec![first.clone()],
        _ => return Err("parity-page-owner-missing"),
    };
    if pages.first() != Some(first)
        || pages.iter().any(|page| {
            page.feasibility_claim()
                || page.pruning_authority() != "none"
                || page.total_pages() as usize != pages.len()
        })
    {
        return Err("parity-public-authority-invalid");
    }
    match format {
        RenderFormat::Json => serde_json::to_string(&json!({
            "kind": "parity-report.v1",
            "contract_id": "parity-report.v1",
            "result_kind": "parity",
            "payload_kind": "parity-report-page",
            "pages": pages.iter().map(parity_page_json).collect::<Vec<_>>(),
        }))
        .map_err(|_| "parity-json-encoding-failed"),
        RenderFormat::Text | RenderFormat::TextVerbose | RenderFormat::TextDiagnostics => {
            let mut output = String::new();
            for (index, page) in pages.iter().enumerate() {
                if index != 0 {
                    output.push('\n');
                }
                output.push_str(&format!(
                    "page: {}/{}\n",
                    page.page_number(),
                    page.total_pages()
                ));
                output.push_str(&format!("document_format: {}\n", page.document_format()));
                output.push_str(&format!("coordinate_basis: {}\n", page.coordinate_basis()));
                output.push_str(&format!(
                    "width: {}\nheight: {}\n",
                    page.width(),
                    page.height()
                ));
                output.push_str(&format!(
                    "occupied_cell_count: {}\n",
                    page.occupied_cell_count()
                ));
                output.push_str(&format!(
                    "checker_black_count: {}\n",
                    page.checker_black_count()
                ));
                output.push_str(&format!(
                    "checker_white_count: {}\n",
                    page.checker_white_count()
                ));
                output.push_str(&format!("checker_delta: {}\n", page.checker_delta()));
                output.push_str(&format!(
                    "pending_garbage_occupied_cell_count: {}\n",
                    page.pending_garbage_occupied_cell_count()
                ));
                output.push_str("feasibility_claim: false\npruning_authority: none");
            }
            Ok(output)
        }
        RenderFormat::FumenLike => Err("parity-native-output-format-invalid"),
    }
}

fn parity_page_json(page: &ParityReportPagePayload) -> Value {
    json!({
        "document_format": page.document_format(),
        "page_number": page.page_number(),
        "total_pages": page.total_pages(),
        "coordinate_basis": page.coordinate_basis(),
        "width": page.width(),
        "height": page.height(),
        "occupied_cell_count": page.occupied_cell_count(),
        "checker_black_count": page.checker_black_count(),
        "checker_white_count": page.checker_white_count(),
        "checker_delta": page.checker_delta(),
        "four_color_counts": page.four_color_counts(),
        "even_column_count": page.even_column_count(),
        "odd_column_count": page.odd_column_count(),
        "column_parity_delta": page.column_parity_delta(),
        "occupied_area_mod_four": page.occupied_area_mod_four(),
        "pending_garbage_occupied_cell_count": page.pending_garbage_occupied_cell_count(),
        "feasibility_claim": false,
        "pruning_authority": "none",
        "page_handle_available": page.page_handle_available(),
    })
}

fn render_document(
    contract: &str,
    result_kind: &str,
    document: &FieldDocumentPayload,
    format: RenderFormat,
) -> Result<String, &'static str> {
    match format {
        RenderFormat::Json => serde_json::to_string(&json!({
            "kind": contract,
            "contract_id": contract,
            "result_kind": result_kind,
            "payload_kind": "field-document",
            "payload": document_json(document),
        }))
        .map_err(|_| "field-document-json-encoding-failed"),
        RenderFormat::Text | RenderFormat::TextVerbose | RenderFormat::TextDiagnostics => {
            Ok(document.document().to_owned())
        }
        RenderFormat::FumenLike => Err("field-document-native-output-format-invalid"),
    }
}

fn render_document_set(
    contract: &str,
    result_kind: &str,
    documents: &[FieldDocumentPayload],
    format: RenderFormat,
) -> Result<String, &'static str> {
    if documents.is_empty() {
        return Err("field-document-set-empty");
    }
    match format {
        RenderFormat::Json => serde_json::to_string(&json!({
            "kind": contract,
            "contract_id": contract,
            "result_kind": result_kind,
            "payload_kind": "field-document-set",
            "payload": {
                "document_contract": "field-document.v1",
                "documents": documents.iter().map(document_json).collect::<Vec<_>>(),
            },
        }))
        .map_err(|_| "field-document-set-json-encoding-failed"),
        RenderFormat::Text | RenderFormat::TextVerbose | RenderFormat::TextDiagnostics => {
            Ok(documents
                .iter()
                .map(FieldDocumentPayload::document)
                .collect::<Vec<_>>()
                .join("\n"))
        }
        RenderFormat::FumenLike => Err("field-document-set-native-output-format-invalid"),
    }
}

fn document_json(document: &FieldDocumentPayload) -> Value {
    json!({
        "format": document.format(),
        "document": document.document(),
        "page_count": document.page_count(),
        "canonical_sha256": document.canonical_sha256(),
        "filename": document.filename(),
    })
}

fn render_artifact(
    contract: &str,
    result_kind: &str,
    artifact: &RenderArtifactPayload,
    format: RenderFormat,
) -> Result<String, &'static str> {
    if !artifact.render_exact() {
        return Err("render-artifact-is-not-exact");
    }
    match format {
        RenderFormat::Json => serde_json::to_string(&json!({
            "kind": contract,
            "contract_id": contract,
            "result_kind": result_kind,
            "payload_kind": "render-artifact",
            "payload": {
                "document_format": artifact.document_format(),
                "artifact_format": artifact.artifact_format(),
                "selected_page_number": artifact.selected_page_number(),
                "document_page_count": artifact.document_page_count(),
                "media_type": artifact.media_type(),
                "filename": artifact.filename(),
                "byte_length": artifact.byte_length(),
                "sha256": artifact.sha256(),
                "render_exact": true,
                "skin_id": artifact.skin_id(),
                "product_max_bytes": artifact.product_max_bytes(),
                "transport_max_bytes": artifact.transport_max_bytes(),
            },
        }))
        .map_err(|_| "render-artifact-json-encoding-failed"),
        RenderFormat::Text | RenderFormat::TextVerbose | RenderFormat::TextDiagnostics => Ok(format!(
            "contract_id: {contract}\nartifact_format: {}\nmedia_type: {}\nbyte_length: {}\nsha256: {}\nrender_exact: true\nskin_id: {}",
            artifact.artifact_format(),
            artifact.media_type(),
            artifact.byte_length(),
            artifact.sha256(),
            artifact.skin_id(),
        )),
        RenderFormat::FumenLike => Err("render-artifact-native-output-format-invalid"),
    }
}

fn prepare_pending(
    content: &ProductResultPayloadContent,
    output_kind: NativeTypedUtilityOutputKind,
    target: &Path,
    base_stdout: String,
    render_format: RenderFormat,
) -> Result<PendingDocumentUtilityArtifact, &'static str> {
    let files = match (output_kind, content) {
        (
            NativeTypedUtilityOutputKind::CanonicalDocument,
            ProductResultPayloadContent::FieldDocument(document),
        ) => vec![pending_document_file(target.to_owned(), document, None)?],
        (
            NativeTypedUtilityOutputKind::CanonicalDocumentSet,
            ProductResultPayloadContent::FieldDocumentSet(set),
        ) => pending_document_set(target, set.documents())?,
        (
            NativeTypedUtilityOutputKind::Png | NativeTypedUtilityOutputKind::Gif,
            ProductResultPayloadContent::RenderArtifact(artifact),
        ) => vec![pending_render_file(
            target.to_owned(),
            output_kind,
            artifact,
        )?],
        _ => return Err("typed-document-output-kind-mismatch"),
    };
    Ok(PendingDocumentUtilityArtifact {
        files,
        base_stdout,
        render_format,
    })
}

fn pending_document_set(
    directory: &Path,
    documents: &[FieldDocumentPayload],
) -> Result<Vec<PendingDocumentUtilityFile>, &'static str> {
    validate_existing_output_directory(directory)?;
    let mut files = Vec::new();
    files
        .try_reserve_exact(documents.len())
        .map_err(|_| "field-document-set-output-capacity-exceeded")?;
    for (index, document) in documents.iter().enumerate() {
        files.push(pending_document_file(
            directory.join(document.filename()),
            document,
            Some(u32::try_from(index + 1).map_err(|_| "field-document-page-overflow")?),
        )?);
    }
    Ok(files)
}

fn pending_document_file(
    target: PathBuf,
    document: &FieldDocumentPayload,
    document_page_number: Option<u32>,
) -> Result<PendingDocumentUtilityFile, &'static str> {
    let extension = match document.format() {
        "ctk3" => "ctk3",
        "fumen" => "txt",
        _ => return Err("field-document-output-format-invalid"),
    };
    validate_safe_filename(document.filename(), extension)?;
    let bytes = document.document().as_bytes().to_vec();
    if bytes.len() > FIELD_DOCUMENT_MAX_INPUT_BYTES {
        return Err("field-document-output-limit-exceeded");
    }
    if sha256_hex(&bytes) != document.canonical_sha256() {
        return Err("field-document-output-identity-mismatch");
    }
    Ok(PendingDocumentUtilityFile {
        target,
        filename: document.filename().to_owned(),
        bytes,
        sha256: document.canonical_sha256().to_owned(),
        maximum_bytes: FIELD_DOCUMENT_MAX_INPUT_BYTES as u64,
        document_page_number,
    })
}

fn pending_render_file(
    target: PathBuf,
    output_kind: NativeTypedUtilityOutputKind,
    artifact: &RenderArtifactPayload,
) -> Result<PendingDocumentUtilityFile, &'static str> {
    let expected = match output_kind {
        NativeTypedUtilityOutputKind::Png => ("png", "image/png"),
        NativeTypedUtilityOutputKind::Gif => ("gif", "image/gif"),
        _ => return Err("render-output-kind-invalid"),
    };
    if artifact.artifact_format() != expected.0 || artifact.media_type() != expected.1 {
        return Err("render-output-format-mismatch");
    }
    validate_safe_filename(artifact.filename(), expected.0)?;
    let bytes = decode_base64(artifact.bytes_base64())?;
    if bytes.len() as u64 != artifact.byte_length()
        || sha256_hex(&bytes) != artifact.sha256()
        || bytes.len() as u64 > artifact.product_max_bytes()
        || bytes.len() as u64 > artifact.transport_max_bytes()
    {
        return Err("render-output-identity-or-limit-mismatch");
    }
    Ok(PendingDocumentUtilityFile {
        target,
        filename: artifact.filename().to_owned(),
        bytes,
        sha256: artifact.sha256().to_owned(),
        maximum_bytes: artifact
            .product_max_bytes()
            .min(artifact.transport_max_bytes()),
        document_page_number: artifact.selected_page_number(),
    })
}

fn validate_existing_output_directory(path: &Path) -> Result<(), &'static str> {
    let metadata = fs::symlink_metadata(path).map_err(|_| "split-output-directory-missing")?;
    if metadata.file_type().is_symlink() {
        return Err("split-output-directory-link-rejected");
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x0400 != 0 {
            return Err("split-output-directory-link-rejected");
        }
    }
    if !metadata.is_dir() {
        return Err("split-output-path-is-not-a-directory");
    }
    Ok(())
}

fn validate_safe_filename(filename: &str, extension: &str) -> Result<(), &'static str> {
    let path = Path::new(filename);
    if path.components().count() != 1
        || !matches!(path.components().next(), Some(Component::Normal(_)))
        || !path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case(extension))
    {
        return Err("typed-document-safe-filename-invalid");
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn decode_base64(value: &str) -> Result<Vec<u8>, &'static str> {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return Err("render-output-base64-invalid");
    }
    let padding = usize::from(bytes.ends_with(b"=")) + usize::from(bytes.ends_with(b"=="));
    let output_len = bytes
        .len()
        .checked_div(4)
        .and_then(|groups| groups.checked_mul(3))
        .and_then(|length| length.checked_sub(padding))
        .ok_or("render-output-base64-invalid")?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(output_len)
        .map_err(|_| "render-output-capacity-exceeded")?;
    for (group_index, group) in bytes.chunks_exact(4).enumerate() {
        let last = group_index + 1 == bytes.len() / 4;
        let a = base64_value(group[0]).ok_or("render-output-base64-invalid")?;
        let b = base64_value(group[1]).ok_or("render-output-base64-invalid")?;
        let c = if group[2] == b'=' {
            if !last || group[3] != b'=' || b & 0x0f != 0 {
                return Err("render-output-base64-invalid");
            }
            0
        } else {
            base64_value(group[2]).ok_or("render-output-base64-invalid")?
        };
        let d = if group[3] == b'=' {
            if !last || (group[2] != b'=' && c & 0x03 != 0) {
                return Err("render-output-base64-invalid");
            }
            0
        } else {
            base64_value(group[3]).ok_or("render-output-base64-invalid")?
        };
        output.push((a << 2) | (b >> 4));
        if group[2] != b'=' {
            output.push((b << 4) | (c >> 2));
        }
        if group[3] != b'=' {
            output.push((c << 6) | d);
        }
    }
    if output.len() != output_len {
        return Err("render-output-base64-invalid");
    }
    Ok(output)
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn invalid(reason: impl Into<String>) -> CliOutput {
    CliOutput::error(CliErrorCode::CliArtifactInvalid, reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_base64_roundtrip_vectors_are_canonical() {
        assert_eq!(decode_base64("Zg==").unwrap(), b"f");
        assert_eq!(decode_base64("Zm8=").unwrap(), b"fo");
        assert_eq!(decode_base64("Zm9v").unwrap(), b"foo");
        assert!(decode_base64("Zh==").is_err());
        assert!(decode_base64("Zm9=").is_err());
        assert!(decode_base64("Zm=v").is_err());
    }
}
