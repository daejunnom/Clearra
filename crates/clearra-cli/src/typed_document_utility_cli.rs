use std::path::{Path, PathBuf};

use clearra_app::{FieldDocumentFormat, FieldDocumentTransformKind};

use crate::{
    args::ParsedCliCommand, error::CliErrorCode, input::file_input_guard, output::CliOutput,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeTypedUtilitySurface {
    Parity,
    Fumen { split: bool },
    Render,
    FieldDocumentTransform(FieldDocumentTransformKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeTypedUtilityOutputKind {
    CanonicalDocument,
    CanonicalDocumentSet,
    Png,
    Gif,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NativeTypedUtilityOutput {
    target: PathBuf,
    kind: NativeTypedUtilityOutputKind,
}

impl NativeTypedUtilityOutput {
    pub(crate) fn target(&self) -> &Path {
        &self.target
    }

    pub(crate) const fn kind(&self) -> NativeTypedUtilityOutputKind {
        self.kind
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NativeTypedUtilityPlan {
    surface: NativeTypedUtilitySurface,
    output: Option<NativeTypedUtilityOutput>,
}

impl NativeTypedUtilityPlan {
    pub(crate) const fn surface(&self) -> NativeTypedUtilitySurface {
        self.surface
    }

    pub(crate) fn output(&self) -> Option<&NativeTypedUtilityOutput> {
        self.output.as_ref()
    }
}

/// Converts the native-only document/file grammar into the explicit typed
/// envelope consumed by the shared Web/App parser. No file path crosses that
/// boundary and no format is inferred anywhere else.
pub(crate) fn prepare_native_typed_utility(
    command: ParsedCliCommand,
) -> Result<(ParsedCliCommand, Option<NativeTypedUtilityPlan>), CliOutput> {
    let ParsedCliCommand::Product(tokens) = command else {
        return Ok((command, None));
    };
    if tokens.get(1).map(String::as_str) != Some("utility") {
        return Ok((ParsedCliCommand::Product(tokens), None));
    }
    let Some(subcommand) = tokens.get(2).map(String::as_str) else {
        return Ok((ParsedCliCommand::Product(tokens), None));
    };
    if !matches!(
        subcommand,
        "parity" | "fumen" | "render" | "to-gray" | "mirror"
    ) {
        return Ok((ParsedCliCommand::Product(tokens), None));
    }

    let transform = (subcommand == "fumen")
        .then(|| tokens.get(3).cloned())
        .flatten();
    if subcommand == "fumen" && transform.is_none() {
        return Err(invalid("utility fumen requires a closed transform"));
    }
    let option_start = if subcommand == "fumen" { 4 } else { 3 };
    let mut forwarded = tokens[..option_start].to_vec();
    let mut documents = Vec::new();
    let mut source_mode = None;
    let mut output = None;
    let mut cursor = option_start;
    while cursor < tokens.len() {
        match tokens[cursor].as_str() {
            "--document" | "--document-file" => {
                let option = tokens[cursor].as_str();
                let value = tokens
                    .get(cursor + 1)
                    .ok_or_else(|| invalid(format!("{option} requires a value")))?;
                let mode = if option == "--document" {
                    DocumentSourceMode::Inline
                } else {
                    DocumentSourceMode::File
                };
                if source_mode.is_some_and(|seen| seen != mode) {
                    return Err(invalid(
                        "--document and --document-file cannot be mixed in one command",
                    ));
                }
                source_mode = Some(mode);
                let document = match mode {
                    DocumentSourceMode::Inline => value.to_owned(),
                    DocumentSourceMode::File => file_input_guard::read_typed_document_file(value)
                        .map_err(|error| invalid(error.to_string()))?,
                };
                documents.push(document);
                cursor += 2;
            }
            "--output" => {
                if output.is_some() {
                    return Err(invalid("utility command repeats --output"));
                }
                let value = tokens
                    .get(cursor + 1)
                    .ok_or_else(|| invalid("--output requires a path"))?;
                if value.is_empty() {
                    return Err(invalid("--output path must not be empty"));
                }
                output = Some(PathBuf::from(value));
                cursor += 2;
            }
            // The CLI's global --format is an output selector. Input format
            // is never accepted as an independent native authority.
            "--format" => {
                return Err(invalid(
                    "native typed-document utilities infer input format from one canonical prefix",
                ))
            }
            _ => {
                forwarded.push(tokens[cursor].clone());
                cursor += 1;
            }
        }
    }

    let surface = match subcommand {
        "parity" => {
            require_document_count(&documents, 1, "utility parity")?;
            if output.is_some() {
                return Err(invalid("utility parity does not accept --output"));
            }
            NativeTypedUtilitySurface::Parity
        }
        "fumen" => {
            let transform = transform.as_deref().expect("checked above");
            match transform {
                "combine" if documents.is_empty() => {
                    return Err(invalid(
                        "utility fumen combine requires one or more documents",
                    ))
                }
                "combine" => {}
                "text-to-fumen" => {
                    require_document_count(&documents, 0, "utility fumen text-to-fumen")?
                }
                _ => require_document_count(&documents, 1, "utility fumen transform")?,
            }
            NativeTypedUtilitySurface::Fumen {
                split: transform == "split",
            }
        }
        "render" => {
            require_document_count(&documents, 1, "utility render")?;
            NativeTypedUtilitySurface::Render
        }
        "to-gray" | "mirror" => {
            require_document_count(&documents, 1, &format!("utility {subcommand}"))?;
            NativeTypedUtilitySurface::FieldDocumentTransform(
                FieldDocumentTransformKind::parse(subcommand)
                    .map_err(|error| invalid(error.to_string()))?,
            )
        }
        _ => unreachable!("closed subcommand"),
    };

    let format = if matches!(transform.as_deref(), Some("text-to-fumen")) {
        FieldDocumentFormat::Fumen
    } else {
        infer_one_format(&documents)?
    };
    if subcommand == "fumen" && format != FieldDocumentFormat::Fumen {
        return Err(invalid(
            "utility fumen accepts only canonical v115 Fumen documents",
        ));
    }
    forwarded.push("--format".to_owned());
    forwarded.push(format.as_str().to_owned());
    for document in documents {
        forwarded.push("--document".to_owned());
        forwarded.push(document);
    }

    let output = match surface {
        NativeTypedUtilitySurface::Parity => None,
        NativeTypedUtilitySurface::Fumen { split: true } => {
            output.map(|target| NativeTypedUtilityOutput {
                target,
                kind: NativeTypedUtilityOutputKind::CanonicalDocumentSet,
            })
        }
        NativeTypedUtilitySurface::Fumen { split: false } => output
            .map(|target| {
                require_extension(&target, "txt", "Fumen document")?;
                Ok(NativeTypedUtilityOutput {
                    target,
                    kind: NativeTypedUtilityOutputKind::CanonicalDocument,
                })
            })
            .transpose()?,
        NativeTypedUtilitySurface::Render => {
            let target = output.ok_or_else(|| {
                invalid("utility render requires --output PATH for its binary artifact")
            })?;
            let artifact = unique_option_value(&forwarded, "--artifact-format")?
                .ok_or_else(|| invalid("utility render requires --artifact-format png|gif"))?;
            let kind = match artifact {
                "png" => {
                    require_extension(&target, "png", "PNG render")?;
                    NativeTypedUtilityOutputKind::Png
                }
                "gif" => {
                    require_extension(&target, "gif", "GIF render")?;
                    NativeTypedUtilityOutputKind::Gif
                }
                value => {
                    return Err(invalid(format!(
                        "utility render artifact format must be png|gif, got '{value}'"
                    )))
                }
            };
            Some(NativeTypedUtilityOutput { target, kind })
        }
        NativeTypedUtilitySurface::FieldDocumentTransform(_) => output
            .map(|target| {
                let (extension, label) = match format {
                    FieldDocumentFormat::Ctk3 => ("ctk3", "CTK3 document"),
                    FieldDocumentFormat::Fumen => ("txt", "Fumen document"),
                };
                require_extension(&target, extension, label)?;
                Ok(NativeTypedUtilityOutput {
                    target,
                    kind: NativeTypedUtilityOutputKind::CanonicalDocument,
                })
            })
            .transpose()?,
    };

    Ok((
        ParsedCliCommand::Product(forwarded),
        Some(NativeTypedUtilityPlan { surface, output }),
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DocumentSourceMode {
    Inline,
    File,
}

fn require_document_count(
    documents: &[String],
    expected: usize,
    command: &str,
) -> Result<(), CliOutput> {
    if documents.len() == expected {
        Ok(())
    } else {
        Err(invalid(format!(
            "{command} requires exactly {expected} document source(s), got {}",
            documents.len()
        )))
    }
}

fn infer_one_format(documents: &[String]) -> Result<FieldDocumentFormat, CliOutput> {
    let mut inferred = None;
    for document in documents {
        let format = FieldDocumentFormat::infer_canonical(document)
            .map_err(|error| invalid(format!("typed document format inference failed: {error}")))?;
        if inferred.is_some_and(|seen| seen != format) {
            return Err(invalid(
                "all repeated documents must use the same canonical format",
            ));
        }
        inferred = Some(format);
    }
    inferred.ok_or_else(|| invalid("typed document input is missing"))
}

fn unique_option_value<'a>(
    tokens: &'a [String],
    option: &str,
) -> Result<Option<&'a str>, CliOutput> {
    let mut found = None;
    let mut cursor = 0;
    while cursor < tokens.len() {
        if tokens[cursor] == option {
            if found.is_some() {
                return Err(invalid(format!("utility command repeats {option}")));
            }
            found = Some(
                tokens
                    .get(cursor + 1)
                    .ok_or_else(|| invalid(format!("{option} requires a value")))?
                    .as_str(),
            );
            cursor += 2;
        } else {
            cursor += 1;
        }
    }
    Ok(found)
}

fn require_extension(path: &Path, expected: &str, label: &str) -> Result<(), CliOutput> {
    let actual = path.extension().and_then(|value| value.to_str());
    if actual.is_some_and(|value| value.eq_ignore_ascii_case(expected)) {
        return Ok(());
    }
    Err(invalid(format!(
        "{label} output path must have the .{expected} extension"
    )))
}

fn invalid(message: impl Into<String>) -> CliOutput {
    CliOutput::error(CliErrorCode::CliInvalidValue, message)
}

#[cfg(test)]
mod tests {
    use clearra_app::{encode_ctk3_compact, Ctk3Document, Ctk3Page};
    use clearra_fumen::ActualFumenDocumentTransform;

    use super::*;

    fn utility_tokens(values: &[&str]) -> ParsedCliCommand {
        ParsedCliCommand::Product(values.iter().map(|value| (*value).to_owned()).collect())
    }

    #[test]
    fn native_input_infers_exactly_one_canonical_prefix_and_render_requires_matching_output() {
        let ctk3 = encode_ctk3_compact(&Ctk3Document::new(1, vec![Ctk3Page::new(0, vec![])]))
            .expect("ctk3");
        let (command, plan) = prepare_native_typed_utility(utility_tokens(&[
            "clearra",
            "utility",
            "render",
            "--document",
            &ctk3,
            "--artifact-format",
            "png",
            "--page",
            "1",
            "--output",
            "board.png",
        ]))
        .expect("native render");
        let ParsedCliCommand::Product(tokens) = command else {
            panic!("product")
        };
        assert!(tokens.windows(2).any(|pair| pair == ["--format", "ctk3"]));
        assert_eq!(
            plan.expect("plan").output().expect("output").kind(),
            NativeTypedUtilityOutputKind::Png
        );

        let bad = prepare_native_typed_utility(utility_tokens(&[
            "clearra",
            "utility",
            "render",
            "--document",
            &ctk3,
            "--artifact-format",
            "png",
            "--output",
            "board.gif",
        ]));
        assert!(bad.is_err());
    }

    #[test]
    fn fumen_combine_allows_one_source_mode_and_split_output_is_a_directory_plan() {
        let fumen =
            ActualFumenDocumentTransform::text_to_fumen(&["page".to_owned()]).expect("fumen");
        let (_, plan) = prepare_native_typed_utility(utility_tokens(&[
            "clearra",
            "utility",
            "fumen",
            "split",
            "--document",
            &fumen,
            "--output",
            "pages",
        ]))
        .expect("split");
        assert_eq!(
            plan.expect("plan").output().expect("output").kind(),
            NativeTypedUtilityOutputKind::CanonicalDocumentSet
        );

        assert!(prepare_native_typed_utility(utility_tokens(&[
            "clearra",
            "utility",
            "fumen",
            "combine",
            "--document",
            &fumen,
            "--document-file",
            "pages.txt",
        ]))
        .is_err());
    }
}
