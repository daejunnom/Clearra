use super::*;

use std::{
    io::{self, Write},
    path::PathBuf,
};

use clearra_output::artifact::{
    AtomicBytesArtifactSink, CompactSolutionSetEncoder, Ctk3SolutionSetEncoder,
    DurabilityUncertainReason, FileIdentity, FumenSolutionSetEncoder, JsonSolutionSetEncoder,
    NeverCancelled, SolutionArtifactAnnotation, SolutionArtifactEncoder, SolutionArtifactEntry,
    SolutionSetArtifact, DEFAULT_MAX_ARTIFACT_BYTES,
};
use clearra_validation::diagnostic::{diagnostic::Diagnostic, diagnostic_code::DiagnosticCode};

use crate::output::{
    solution_artifact_output::PendingSolutionArtifact, SolutionArtifactOutputFormat,
};

fn artifact() -> SolutionSetArtifact {
    SolutionSetArtifact::try_new(
        "test-solution-set",
        "key-v1",
        "hash-v1",
        "hash:1",
        1,
        vec![
            SolutionArtifactEntry::try_new("solution-a", SolutionArtifactAnnotation::new())
                .expect("entry"),
        ],
    )
    .expect("artifact")
}

fn native_document_artifact() -> SolutionSetArtifact {
    SolutionSetArtifact::try_new(
        "test-solution-set",
        "key-v1",
        "hash-v1",
        "hash:1",
        1,
        vec![SolutionArtifactEntry::try_new(
            "ctk1|initial=0000000000000000|placements=I:000000000000000f",
            SolutionArtifactAnnotation::new(),
        )
        .expect("entry")],
    )
    .expect("artifact")
}

fn pending(format: SolutionArtifactOutputFormat, stdout: &str) -> PendingSolutionArtifact {
    PendingSolutionArtifact::try_new(
        PathBuf::from("solutions.csa"),
        format,
        artifact(),
        DEFAULT_MAX_ARTIFACT_BYTES,
        stdout.to_owned(),
        RenderFormat::Text,
    )
    .expect("pending")
}

fn committed_outcome(format: SolutionArtifactOutputFormat) -> ArtifactPublicationOutcome {
    let source = match format {
        SolutionArtifactOutputFormat::Compact | SolutionArtifactOutputFormat::Json => artifact(),
        SolutionArtifactOutputFormat::Ctk3 | SolutionArtifactOutputFormat::Fumen => {
            native_document_artifact()
        }
    };
    let encoder: &dyn SolutionArtifactEncoder = match format {
        SolutionArtifactOutputFormat::Compact => &CompactSolutionSetEncoder,
        SolutionArtifactOutputFormat::Json => &JsonSolutionSetEncoder,
        SolutionArtifactOutputFormat::Ctk3 => &Ctk3SolutionSetEncoder,
        SolutionArtifactOutputFormat::Fumen => &FumenSolutionSetEncoder,
    };
    let plan = encoder
        .measure_checked(&source, DEFAULT_MAX_ARTIFACT_BYTES, &NeverCancelled)
        .expect("plan");
    let mut bytes = Vec::new();
    let receipt = encoder
        .encode_into(&source, &plan, &mut bytes, &NeverCancelled)
        .expect("receipt");
    ArtifactPublicationOutcome::Committed(ArtifactCommit::from_native_receipt(
        &receipt,
        FileIdentity::Linux {
            device: 0x11,
            inode: 0x22,
        },
    ))
}

#[test]
fn typed_render_is_published_atomic_new_and_json_reports_the_owned_file() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "clearra-cli-render-publication-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).expect("temporary directory");
    let target = directory.join("board.png");
    let document = clearra_app::encode_ctk3_compact(&clearra_app::Ctk3Document::new(
        2,
        vec![clearra_app::Ctk3Page::new(
            1,
            vec![clearra_app::Ctk3Color::Gray, clearra_app::Ctk3Color::Empty],
        )],
    ))
    .expect("CTK3 fixture");
    let output = crate::run_with_args([
        "clearra",
        "--format",
        "json",
        "utility",
        "render",
        "--document",
        document.as_str(),
        "--artifact-format",
        "png",
        "--page",
        "1",
        "--output",
        target.to_str().expect("UTF-8 target"),
    ]);
    assert_eq!(output.exit_code(), ExitCode::Success, "{}", output.stderr());

    let publish = |file: &PendingDocumentUtilityFile| {
        let mut sink = AtomicBytesArtifactSink::new(file.target());
        sink.publish(file.bytes(), file.maximum_bytes(), &NeverCancelled)
            .map_err(|error| format!("{:?}", error.code()))
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = CliOutputDispatcher::dispatch_with_publishers(
        &output,
        &mut stdout,
        &mut stderr,
        |_| Err("solution publisher must not run".to_owned()),
        publish,
    );
    assert_eq!(code, ExitCode::Success.code());
    assert!(stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&stdout).expect("committed JSON");
    assert_eq!(value["kind"], "render-artifact.v1");
    assert_eq!(value["payload"]["selected_page_number"], 1);
    assert_eq!(value["generated_files"][0]["target_owned"], true);
    assert_eq!(value["generated_files"][0]["document_page_number"], 1);
    let bytes = std::fs::read(&target).expect("published PNG");
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));

    let mut second_stdout = Vec::new();
    let mut second_stderr = Vec::new();
    let second_code = CliOutputDispatcher::dispatch_with_publishers(
        &output,
        &mut second_stdout,
        &mut second_stderr,
        |_| Err("solution publisher must not run".to_owned()),
        |file| {
            let mut sink = AtomicBytesArtifactSink::new(file.target());
            sink.publish(file.bytes(), file.maximum_bytes(), &NeverCancelled)
                .map_err(|error| format!("{:?}", error.code()))
        },
    );
    assert_eq!(second_code, ExitCode::InternalError.code());
    assert!(second_stdout.is_empty());
    assert!(String::from_utf8(second_stderr)
        .expect("UTF-8 error")
        .contains("E_CLI_ARTIFACT_PUBLISH_FAILED"));
    assert_eq!(std::fs::read(&target).expect("unchanged PNG"), bytes);

    std::fs::remove_file(target).expect("target cleanup");
    std::fs::remove_dir(directory).expect("directory cleanup");
}

#[test]
fn native_document_commits_are_accepted_only_with_the_v2_schema() {
    for (format, keyword) in [
        (SolutionArtifactOutputFormat::Ctk3, "ctk3"),
        (SolutionArtifactOutputFormat::Fumen, "fumen"),
    ] {
        let pending = PendingSolutionArtifact::try_new(
            PathBuf::from("solutions.bin"),
            format,
            native_document_artifact(),
            DEFAULT_MAX_ARTIFACT_BYTES,
            "ready".to_owned(),
            RenderFormat::Text,
        )
        .expect("pending native document");
        let ArtifactPublicationOutcome::Committed(commit) = committed_outcome(format) else {
            unreachable!()
        };
        assert_eq!(commit.schema(), "solution-set-artifact.v2");
        let stdout = pending.committed_stdout(&commit).expect("committed stdout");
        assert!(stdout.contains("solution_artifact_schema: solution-set-artifact.v2"));
        assert!(stdout.contains(&format!("solution_artifact_encoding: {keyword}")));
    }
}

#[test]
fn success_output_carries_zero_exit_code() {
    let output = CliOutput::success("ready");
    assert_eq!(output.exit_code(), ExitCode::Success);
    assert_eq!(output.stdout(), "ready");
    assert_eq!(output.stderr(), "");
}

#[test]
fn validation_error_carries_requested_exit_code() {
    let mut report = DiagnosticReport::new();
    report.push(Diagnostic::new(
        DiagnosticCode::ECoreFfiBufferBounds,
        "invalid",
    ));
    let output = CliOutput::validation_failed(&report);
    assert_eq!(output.exit_code(), ExitCode::ValidationFailed);
    assert_eq!(output.stdout(), "");
    assert_eq!(output.stderr(), "error E_CORE_FFI_BUFFER_BOUNDS invalid");
}

#[test]
fn json_validation_failure_uses_stdout_json_contract() {
    let mut report = DiagnosticReport::new();
    report.push(Diagnostic::new(
        DiagnosticCode::ECoreFfiBufferBounds,
        "output exceeds caller buffer",
    ));
    let output = CliOutput::validation_failed_with_format(&report, RenderFormat::Json);
    assert_eq!(output.exit_code(), ExitCode::ValidationFailed);
    assert!(output.stdout().contains("\"kind\":\"diagnostic\""));
    assert!(output.stdout().contains("\"E_CORE_FFI_BUFFER_BOUNDS\""));
    assert_eq!(output.stderr(), "");
}

#[test]
fn artifact_commit_happens_before_committed_metadata_reaches_stdout() {
    let output = CliOutput::success("ready")
        .with_pending_solution_artifact(pending(SolutionArtifactOutputFormat::Compact, "ready"));
    assert_eq!(output.stdout(), "ready");

    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let outcome = committed_outcome(SolutionArtifactOutputFormat::Compact);
    let code =
        CliOutputDispatcher::dispatch_with_writers(&output, &mut stdout, &mut stderr, |_| {
            Ok(outcome.clone())
        });

    assert_eq!(code, ExitCode::Success.code());
    let stdout = String::from_utf8(stdout).expect("UTF-8 stdout");
    assert!(stdout.starts_with("ready\nsolution_artifact_status: committed\n"));
    assert!(stdout.contains("solution_artifact_encoding: compact-v1"));
    assert!(stdout.contains("solution_artifact_target_owned: true"));
    assert!(stdout.contains("solution_artifact_file_identity_kind: linux-device-inode"));
    assert!(stderr.is_empty());
}

#[test]
fn precommit_publish_failure_suppresses_success_stdout_and_unrelated_warnings() {
    let output = CliOutput::success("must-not-print")
        .with_surrounding_warning("must-not-warn")
        .with_pending_solution_artifact(pending(
            SolutionArtifactOutputFormat::Compact,
            "must-not-print",
        ));
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code =
        CliOutputDispatcher::dispatch_with_writers(&output, &mut stdout, &mut stderr, |_| {
            Err("artifact-target-exists".to_owned())
        });

    assert_eq!(code, ExitCode::InternalError.code());
    assert!(stdout.is_empty());
    assert_eq!(
        String::from_utf8(stderr).expect("UTF-8 stderr"),
        "error E_CLI_ARTIFACT_PUBLISH_FAILED artifact-target-exists\n"
    );
}

#[test]
fn durability_uncertain_is_not_success_and_reports_owned_target_evidence() {
    let ArtifactPublicationOutcome::Committed(commit) =
        committed_outcome(SolutionArtifactOutputFormat::Compact)
    else {
        unreachable!()
    };
    let outcome = ArtifactPublicationOutcome::DurabilityUncertain {
        commit,
        reason: DurabilityUncertainReason::PostPublishParentSyncFailed,
    };
    let output = CliOutput::success("must-not-print").with_pending_solution_artifact(pending(
        SolutionArtifactOutputFormat::Compact,
        "must-not-print",
    ));
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code =
        CliOutputDispatcher::dispatch_with_writers(&output, &mut stdout, &mut stderr, |_| {
            Ok(outcome.clone())
        });

    assert_eq!(code, ExitCode::InternalError.code());
    assert!(stdout.is_empty());
    let stderr = String::from_utf8(stderr).expect("UTF-8 stderr");
    assert!(stderr.contains("E_CLI_ARTIFACT_DURABILITY_UNCERTAIN"));
    assert!(stderr.contains("target_owned=true"));
    assert!(stderr.contains("checksum=crc32:"));
    assert!(stderr.contains("solution_count=1"));
    assert!(stderr.contains("file_identity_kind=linux-device-inode"));
    assert!(stderr.contains("reason=postpublish-parent-sync-failed"));
}

#[test]
fn committed_metadata_mismatch_is_typed_as_output_failure_not_publish_failure() {
    let output = CliOutput::success("must-not-print").with_pending_solution_artifact(pending(
        SolutionArtifactOutputFormat::Compact,
        "must-not-print",
    ));
    let mismatched = committed_outcome(SolutionArtifactOutputFormat::Json);
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code =
        CliOutputDispatcher::dispatch_with_writers(&output, &mut stdout, &mut stderr, |_| {
            Ok(mismatched.clone())
        });

    assert_eq!(code, ExitCode::InternalError.code());
    assert!(stdout.is_empty());
    let stderr = String::from_utf8(stderr).expect("UTF-8 stderr");
    assert!(stderr.contains("E_CLI_ARTIFACT_COMMITTED_BUT_OUTPUT_FAILED"));
    assert!(stderr.contains("target_owned=true"));
    assert!(stderr.contains("reason=artifact-commit-metadata-mismatch"));
    assert!(!stderr.contains("E_CLI_ARTIFACT_PUBLISH_FAILED"));
}

#[derive(Default)]
struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _buffer: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::BrokenPipe, "closed"))
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn stdout_failure_after_commit_reports_committed_but_output_failed() {
    let output = CliOutput::success("ready")
        .with_pending_solution_artifact(pending(SolutionArtifactOutputFormat::Compact, "ready"));
    let outcome = committed_outcome(SolutionArtifactOutputFormat::Compact);
    let mut stdout = FailingWriter;
    let mut stderr = Vec::new();
    let code =
        CliOutputDispatcher::dispatch_with_writers(&output, &mut stdout, &mut stderr, |_| {
            Ok(outcome.clone())
        });

    assert_eq!(code, ExitCode::InternalError.code());
    let stderr = String::from_utf8(stderr).expect("UTF-8 stderr");
    assert!(stderr.contains("E_CLI_ARTIFACT_COMMITTED_BUT_OUTPUT_FAILED"));
    assert!(stderr.contains("phase=stdout-write-or-flush"));
    assert!(stderr.contains("target_owned=true"));
    assert!(stderr.contains("checksum=crc32:"));
}

#[test]
fn to_gray_ctk3_is_published_atomic_new_and_collision_safe() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "clearra-cli-transform-publication-{}-{nonce}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).expect("temporary directory");
    let target = directory.join("gray.ctk3");
    let document = clearra_app::encode_ctk3_compact(&clearra_app::Ctk3Document::new(
        2,
        vec![clearra_app::Ctk3Page::new(
            1,
            vec![
                clearra_app::Ctk3Color::Piece(clearra_app::Ctk3Piece::J),
                clearra_app::Ctk3Color::Empty,
            ],
        )],
    ))
    .expect("CTK3 fixture");
    let output = crate::run_with_args([
        "clearra",
        "--format",
        "json",
        "utility",
        "to-gray",
        "--document",
        document.as_str(),
        "--output",
        target.to_str().expect("UTF-8 target"),
    ]);
    assert_eq!(output.exit_code(), ExitCode::Success, "{}", output.stderr());

    let publish = |file: &PendingDocumentUtilityFile| {
        let mut sink = AtomicBytesArtifactSink::new(file.target());
        sink.publish(file.bytes(), file.maximum_bytes(), &NeverCancelled)
            .map_err(|error| format!("{:?}", error.code()))
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let code = CliOutputDispatcher::dispatch_with_publishers(
        &output,
        &mut stdout,
        &mut stderr,
        |_| Err("solution publisher must not run".to_owned()),
        publish,
    );
    assert_eq!(code, ExitCode::Success.code());
    assert!(stderr.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&stdout).expect("committed JSON");
    assert_eq!(value["kind"], "field-document.v1");
    assert_eq!(value["result_kind"], "to-gray");
    assert_eq!(value["generated_files"][0]["target_owned"], true);
    let published = std::fs::read_to_string(&target).expect("published CTK3");
    let decoded = clearra_app::decode_ctk3_exact(&published).expect("canonical CTK3");
    assert!(decoded.pages[0].cells.iter().all(|cell| matches!(
        cell,
        clearra_app::Ctk3Color::Empty | clearra_app::Ctk3Color::Gray
    )));

    let mut second_stdout = Vec::new();
    let mut second_stderr = Vec::new();
    let second_code = CliOutputDispatcher::dispatch_with_publishers(
        &output,
        &mut second_stdout,
        &mut second_stderr,
        |_| Err("solution publisher must not run".to_owned()),
        |file| {
            let mut sink = AtomicBytesArtifactSink::new(file.target());
            sink.publish(file.bytes(), file.maximum_bytes(), &NeverCancelled)
                .map_err(|error| format!("{:?}", error.code()))
        },
    );
    assert_eq!(second_code, ExitCode::InternalError.code());
    assert!(second_stdout.is_empty());
    assert!(String::from_utf8(second_stderr)
        .expect("UTF-8 error")
        .contains("E_CLI_ARTIFACT_PUBLISH_FAILED"));
    assert_eq!(std::fs::read_to_string(&target).unwrap(), published);

    std::fs::remove_file(target).expect("target cleanup");
    std::fs::remove_dir(directory).expect("directory cleanup");
}
