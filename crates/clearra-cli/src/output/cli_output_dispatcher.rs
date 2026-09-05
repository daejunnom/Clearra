use clearra_validation::diagnostic::diagnostic_report::DiagnosticReport;

use std::io::{self, Write};

use clearra_output::artifact::ByteArtifactPublicationOutcome;
use clearra_output::artifact::{ArtifactCommit, ArtifactPublicationOutcome};
#[cfg(not(target_arch = "wasm32"))]
use clearra_output::artifact::{
    AtomicBytesArtifactSink, AtomicFileArtifactSink, CompactSolutionSetEncoder,
    Ctk3SolutionSetEncoder, FumenSolutionSetEncoder, JsonSolutionSetEncoder, NeverCancelled,
    PublicationResidue, SolutionArtifactSink,
};

use crate::{
    error::CliErrorCode,
    exit::ExitCode,
    output::{
        diagnostic_printer::DiagnosticPrinter,
        document_utility_output::{PendingDocumentUtilityArtifact, PendingDocumentUtilityFile},
        solution_artifact_output::PendingSolutionArtifact,
        RenderFormat,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CliOutput {
    exit_code: ExitCode,
    stdout: String,
    stderr: String,
    warning_before: String,
    warning_after: String,
    // Pending artifacts can carry a large in-memory model. Keep that payload
    // off the ordinary success/error return path until the dispatcher commits it.
    pending_solution_artifact: Option<Box<PendingSolutionArtifact>>,
    pending_document_utility_artifact: Option<Box<PendingDocumentUtilityArtifact>>,
}

impl CliOutput {
    pub fn new(exit_code: ExitCode, stdout: impl Into<String>, stderr: impl Into<String>) -> Self {
        Self {
            exit_code,
            stdout: stdout.into(),
            stderr: stderr.into(),
            warning_before: String::new(),
            warning_after: String::new(),
            pending_solution_artifact: None,
            pending_document_utility_artifact: None,
        }
    }
}
impl CliOutput {
    pub fn success(stdout: impl Into<String>) -> Self {
        Self::new(ExitCode::Success, stdout, "")
    }
}
impl CliOutput {
    pub fn error(code: CliErrorCode, message: impl Into<String>) -> Self {
        Self::new(
            code.default_exit_code(),
            "",
            format!("error {} {}", code.as_str(), message.into()),
        )
    }
}
impl CliOutput {
    pub fn validation_failed(report: &DiagnosticReport) -> Self {
        Self::new(
            ExitCode::ValidationFailed,
            "",
            DiagnosticPrinter::render(report),
        )
    }
}
impl CliOutput {
    pub fn validation_failed_with_format(report: &DiagnosticReport, format: RenderFormat) -> Self {
        match format {
            RenderFormat::Json => Self::new(
                ExitCode::ValidationFailed,
                DiagnosticPrinter::render_json(report),
                "",
            ),
            RenderFormat::Text
            | RenderFormat::TextVerbose
            | RenderFormat::TextDiagnostics
            | RenderFormat::FumenLike => Self::validation_failed(report),
        }
    }

    pub fn with_surrounding_warning(mut self, warning: impl Into<String>) -> Self {
        let warning = warning.into();
        self.warning_before = warning.clone();
        self.warning_after = warning;
        self
    }

    pub(crate) fn with_pending_solution_artifact(
        mut self,
        pending: PendingSolutionArtifact,
    ) -> Self {
        debug_assert!(self.pending_document_utility_artifact.is_none());
        self.pending_solution_artifact = Some(Box::new(pending));
        self
    }

    pub(crate) fn with_pending_document_utility_artifact(
        mut self,
        pending: PendingDocumentUtilityArtifact,
    ) -> Self {
        debug_assert!(self.pending_solution_artifact.is_none());
        self.pending_document_utility_artifact = Some(Box::new(pending));
        self
    }
}
impl CliOutput {
    pub fn exit_code(&self) -> ExitCode {
        self.exit_code
    }
}
impl CliOutput {
    pub fn stdout(&self) -> &str {
        &self.stdout
    }
}
impl CliOutput {
    pub fn stderr(&self) -> &str {
        &self.stderr
    }

    pub fn warning_before(&self) -> &str {
        &self.warning_before
    }

    pub fn warning_after(&self) -> &str {
        &self.warning_after
    }

    fn pending_solution_artifact(&self) -> Option<&PendingSolutionArtifact> {
        self.pending_solution_artifact.as_deref()
    }

    fn pending_document_utility_artifact(&self) -> Option<&PendingDocumentUtilityArtifact> {
        self.pending_document_utility_artifact.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CliOutputDispatcher;

impl CliOutputDispatcher {
    pub fn dispatch(output: &CliOutput) -> i32 {
        let stdout = io::stdout();
        let stderr = io::stderr();
        let mut stdout = stdout.lock();
        let mut stderr = stderr.lock();

        #[cfg(not(target_arch = "wasm32"))]
        {
            Self::dispatch_with_publishers(
                output,
                &mut stdout,
                &mut stderr,
                |pending| {
                    let mut sink = AtomicFileArtifactSink::new(pending.target());
                    let outcome = match pending.format() {
                        super::SolutionArtifactOutputFormat::Compact => sink.publish(
                            &CompactSolutionSetEncoder,
                            pending.artifact(),
                            pending.maximum_bytes(),
                            &NeverCancelled,
                        ),
                        super::SolutionArtifactOutputFormat::Json => sink.publish(
                            &JsonSolutionSetEncoder,
                            pending.artifact(),
                            pending.maximum_bytes(),
                            &NeverCancelled,
                        ),
                        super::SolutionArtifactOutputFormat::Ctk3 => sink.publish(
                            &Ctk3SolutionSetEncoder,
                            pending.artifact(),
                            pending.maximum_bytes(),
                            &NeverCancelled,
                        ),
                        super::SolutionArtifactOutputFormat::Fumen => sink.publish(
                            &FumenSolutionSetEncoder,
                            pending.artifact(),
                            pending.maximum_bytes(),
                            &NeverCancelled,
                        ),
                    };
                    outcome.map_err(artifact_sink_error_detail)
                },
                |file| {
                    let mut sink = AtomicBytesArtifactSink::new(file.target());
                    sink.publish(file.bytes(), file.maximum_bytes(), &NeverCancelled)
                        .map_err(artifact_sink_error_detail)
                },
            )
        }

        #[cfg(target_arch = "wasm32")]
        Self::dispatch_with_publishers(
            output,
            &mut stdout,
            &mut stderr,
            |_| Err("artifact-native-file-sink-unavailable".to_owned()),
            |_| Err("artifact-native-file-sink-unavailable".to_owned()),
        )
    }

    #[cfg(test)]
    fn dispatch_with_writers<Stdout, Stderr, Publish>(
        output: &CliOutput,
        stdout: &mut Stdout,
        stderr: &mut Stderr,
        publish: Publish,
    ) -> i32
    where
        Stdout: Write,
        Stderr: Write,
        Publish: FnMut(&PendingSolutionArtifact) -> Result<ArtifactPublicationOutcome, String>,
    {
        Self::dispatch_with_publishers(output, stdout, stderr, publish, |_| {
            Err("typed-document-publisher-not-configured".to_owned())
        })
    }

    fn dispatch_with_publishers<Stdout, Stderr, PublishSolution, PublishDocument>(
        output: &CliOutput,
        stdout: &mut Stdout,
        stderr: &mut Stderr,
        mut publish_solution: PublishSolution,
        mut publish_document: PublishDocument,
    ) -> i32
    where
        Stdout: Write,
        Stderr: Write,
        PublishSolution:
            FnMut(&PendingSolutionArtifact) -> Result<ArtifactPublicationOutcome, String>,
        PublishDocument:
            FnMut(&PendingDocumentUtilityFile) -> Result<ByteArtifactPublicationOutcome, String>,
    {
        let mut published_commit = None;
        let mut published_document_commits = Vec::new();
        let stdout_text = if let Some(pending) = output.pending_solution_artifact() {
            let outcome = match publish_solution(pending) {
                Ok(outcome) => outcome,
                Err(reason) => {
                    let _ = write_error(stderr, CliErrorCode::CliArtifactPublishFailed, &reason);
                    return ExitCode::InternalError.code();
                }
            };
            let commit = match outcome {
                ArtifactPublicationOutcome::Committed(commit) => commit,
                ArtifactPublicationOutcome::DurabilityUncertain { commit, reason } => {
                    let detail = committed_failure_detail(
                        &commit,
                        "durability-uncertain",
                        Some(reason.as_str()),
                    );
                    let _ = write_error(
                        stderr,
                        CliErrorCode::CliArtifactDurabilityUncertain,
                        &detail,
                    );
                    return ExitCode::InternalError.code();
                }
            };
            match pending.committed_stdout(&commit) {
                Ok(stdout) => {
                    published_commit = Some(commit);
                    stdout
                }
                Err(error) => {
                    let detail = committed_failure_detail(
                        &commit,
                        "metadata-render-failed",
                        Some(error.as_str()),
                    );
                    let _ = write_error(
                        stderr,
                        CliErrorCode::CliArtifactCommittedButOutputFailed,
                        &detail,
                    );
                    return ExitCode::InternalError.code();
                }
            }
        } else if let Some(pending) = output.pending_document_utility_artifact() {
            for file in pending.files() {
                let outcome = match publish_document(file) {
                    Ok(outcome) => outcome,
                    Err(reason) => {
                        let detail = format!(
                            "committed_files={} reason={reason}",
                            published_document_commits.len()
                        );
                        let _ =
                            write_error(stderr, CliErrorCode::CliArtifactPublishFailed, &detail);
                        return ExitCode::InternalError.code();
                    }
                };
                match outcome {
                    ByteArtifactPublicationOutcome::Committed(commit) => {
                        published_document_commits.push(commit)
                    }
                    ByteArtifactPublicationOutcome::DurabilityUncertain { commit, reason } => {
                        let detail = format!(
                            "committed_files={} target_owned={} phase=durability-uncertain bytes={} file_identity_kind={} file_identity={} reason={}",
                            published_document_commits.len() + 1,
                            commit.target_owned(),
                            commit.byte_count(),
                            commit.file_identity().platform(),
                            commit.file_identity().stable_value(),
                            reason.as_str(),
                        );
                        let _ = write_error(
                            stderr,
                            CliErrorCode::CliArtifactDurabilityUncertain,
                            &detail,
                        );
                        return ExitCode::InternalError.code();
                    }
                }
            }
            match pending.committed_stdout(&published_document_commits) {
                Ok(stdout) => stdout,
                Err(reason) => {
                    let detail = format!(
                        "committed_files={} phase=metadata-render-failed reason={reason}",
                        published_document_commits.len()
                    );
                    let _ = write_error(
                        stderr,
                        CliErrorCode::CliArtifactCommittedButOutputFailed,
                        &detail,
                    );
                    return ExitCode::InternalError.code();
                }
            }
        } else {
            output.stdout().to_owned()
        };

        let stdout_buffer = line_buffer(&stdout_text);
        let stderr_buffer = stderr_buffer(output);
        if stdout
            .write_all(&stdout_buffer)
            .and_then(|()| stdout.flush())
            .is_err()
        {
            if let Some(commit) = published_commit.as_ref() {
                let _ = write_error(
                    stderr,
                    CliErrorCode::CliArtifactCommittedButOutputFailed,
                    &committed_failure_detail(commit, "stdout-write-or-flush", None),
                );
            }
            if !published_document_commits.is_empty() {
                let _ = write_error(
                    stderr,
                    CliErrorCode::CliArtifactCommittedButOutputFailed,
                    &format!(
                        "committed_files={} phase=stdout-write-or-flush",
                        published_document_commits.len()
                    ),
                );
            }
            return ExitCode::InternalError.code();
        }
        if stderr
            .write_all(&stderr_buffer)
            .and_then(|()| stderr.flush())
            .is_err()
        {
            if let Some(commit) = published_commit.as_ref() {
                let _ = write_error(
                    stderr,
                    CliErrorCode::CliArtifactCommittedButOutputFailed,
                    &committed_failure_detail(commit, "stderr-write", None),
                );
            }
            if !published_document_commits.is_empty() {
                let _ = write_error(
                    stderr,
                    CliErrorCode::CliArtifactCommittedButOutputFailed,
                    &format!(
                        "committed_files={} phase=stderr-write",
                        published_document_commits.len()
                    ),
                );
            }
            return ExitCode::InternalError.code();
        }
        output.exit_code().code()
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn artifact_sink_error_detail(error: clearra_output::artifact::ArtifactSinkError) -> String {
    let mut detail = error.code().as_str().to_owned();
    if let Some(encoding_error) = error.encoding_error() {
        detail.push_str(" encoding_reason=");
        detail.push_str(encoding_error.as_str());
    }
    if let Some(raw) = error.raw_os_error() {
        detail.push_str(&format!(" raw_os_error={raw}"));
    }
    if let PublicationResidue::OperatorActionRequired {
        staging_leaf,
        file_identity,
    } = error.residue()
    {
        detail.push_str(" residue=operator-action-required staging_leaf=");
        detail.push_str(staging_leaf);
        if let Some(identity) = file_identity {
            detail.push_str(" file_identity_kind=");
            detail.push_str(identity.platform());
            detail.push_str(" file_identity=");
            detail.push_str(&identity.stable_value());
        }
    }
    detail
}

fn committed_failure_detail(commit: &ArtifactCommit, phase: &str, reason: Option<&str>) -> String {
    let mut detail = format!(
        "target_owned={} phase={phase} bytes={} checksum={} solution_count={}",
        commit.target_owned(),
        commit.byte_count(),
        commit.checksum(),
        commit.solution_count()
    );
    if let Some(identity) = commit.file_identity() {
        detail.push_str(" file_identity_kind=");
        detail.push_str(identity.platform());
        detail.push_str(" file_identity=");
        detail.push_str(&identity.stable_value());
    }
    if let Some(reason) = reason {
        detail.push_str(" reason=");
        detail.push_str(reason);
    }
    detail
}

fn stderr_buffer(output: &CliOutput) -> Vec<u8> {
    let capacity = output
        .warning_before()
        .len()
        .saturating_add(output.stderr().len())
        .saturating_add(output.warning_after().len())
        .saturating_add(3);
    let mut buffer = Vec::with_capacity(capacity);
    append_line(&mut buffer, output.warning_before());
    append_line(&mut buffer, output.stderr());
    append_line(&mut buffer, output.warning_after());
    buffer
}

fn line_buffer(value: &str) -> Vec<u8> {
    let mut buffer = Vec::with_capacity(value.len().saturating_add(1));
    append_line(&mut buffer, value);
    buffer
}

fn append_line(buffer: &mut Vec<u8>, value: &str) {
    if value.is_empty() {
        return;
    }
    buffer.extend_from_slice(value.as_bytes());
    buffer.push(b'\n');
}

fn write_error(stderr: &mut impl Write, code: CliErrorCode, reason: &str) -> io::Result<()> {
    stderr.write_all(format!("error {} {reason}\n", code.as_str()).as_bytes())
}

#[cfg(test)]
#[path = "cli_output_dispatcher_tests.rs"]
mod tests;
