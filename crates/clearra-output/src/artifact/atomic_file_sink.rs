use std::path::{Path, PathBuf};

use clearra_platform_fs::{AtomicFileStage, NativePublicationOutcome, PublicationControl};

use super::{
    artifact_sink::{
        ArtifactCommit, ArtifactPublicationOutcome, ArtifactSinkError, ArtifactSinkErrorCode,
        SolutionArtifactSink,
    },
    solution_set_artifact::SolutionSetArtifact,
    solution_set_encoder::{
        ArtifactStreamVerifier, SolutionArtifactEncoder, SolutionArtifactEncodingError,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AtomicFileArtifactSink {
    target: PathBuf,
}

impl AtomicFileArtifactSink {
    pub fn new(target: impl Into<PathBuf>) -> Self {
        Self {
            target: target.into(),
        }
    }

    pub fn target(&self) -> &Path {
        &self.target
    }
}

impl SolutionArtifactSink for AtomicFileArtifactSink {
    fn publish(
        &mut self,
        encoder: &dyn SolutionArtifactEncoder,
        artifact: &SolutionSetArtifact,
        maximum_bytes: u64,
        control: &dyn PublicationControl,
    ) -> Result<ArtifactPublicationOutcome, ArtifactSinkError> {
        // The complete byte count and checksum are derived before a native
        // staging handle exists. A limit or cancellation here cannot leave a
        // file or consume the destination name.
        let plan = encoder
            .measure_checked(artifact, maximum_bytes, control)
            .map_err(ArtifactSinkError::from_encoding)?;
        if plan.encoding() != encoder.encoding() {
            return Err(ArtifactSinkError::from_encoding(
                SolutionArtifactEncodingError::PlanMismatch,
            ));
        }
        if plan.byte_count() > maximum_bytes {
            return Err(ArtifactSinkError::new(
                ArtifactSinkErrorCode::CapacityExceeded,
            ));
        }
        let mut stage = AtomicFileStage::begin(&self.target, plan.byte_count(), control)
            .map_err(ArtifactSinkError::from_native)?;
        let encoded = {
            let mut verifier =
                ArtifactStreamVerifier::new(&mut stage, encoder.encoding(), plan.byte_count());
            match encoder.encode_into(artifact, &plan, &mut verifier, control) {
                Ok(receipt) => verifier.verify(&plan, &receipt).map(|()| receipt),
                Err(error) => Err(error),
            }
        };
        let receipt = match encoded {
            Ok(receipt) => receipt,
            Err(error) => return Err(abort_after_encoding_error(stage, error)),
        };
        let native = stage
            .commit(control)
            .map_err(ArtifactSinkError::from_native)?;
        debug_assert_eq!(native.commit().byte_count(), receipt.byte_count());
        debug_assert!(native.commit().target_owned());
        let commit =
            ArtifactCommit::from_native_receipt(&receipt, native.commit().identity().clone());
        Ok(match native {
            NativePublicationOutcome::Committed(_) => ArtifactPublicationOutcome::Committed(commit),
            NativePublicationOutcome::DurabilityUncertain { reason, .. } => {
                ArtifactPublicationOutcome::DurabilityUncertain { commit, reason }
            }
        })
    }
}

fn abort_after_encoding_error(
    stage: AtomicFileStage,
    error: SolutionArtifactEncodingError,
) -> ArtifactSinkError {
    let original = ArtifactSinkError::from_encoding(error);
    match stage.abort() {
        Ok(()) => original,
        Err(cleanup) => ArtifactSinkError::from_native(cleanup),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;
    use crate::artifact::solution_set_encoder::{
        LimitIgnoringEncoder, SameLengthForgedCompactEncoder,
    };
    use crate::artifact::{
        ArtifactSinkErrorCode, CompactSolutionSetEncoder, NeverCancelled, PublicationCheckpoint,
        PublicationControl, SolutionArtifactAnnotation, SolutionArtifactEncoder,
        SolutionArtifactEntry,
    };

    #[test]
    fn target_is_retained_as_an_opaque_request_until_publication() {
        let sink = AtomicFileArtifactSink::new("relative-parent/solutions.csa");
        assert_eq!(sink.target(), Path::new("relative-parent/solutions.csa"));
    }

    #[test]
    fn native_stream_commit_receipt_matches_the_exact_published_bytes() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "clearra-output-native-receipt-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).expect("directory");
        let target = directory.join("solutions.csa");
        let artifact = SolutionSetArtifact::try_new(
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
        .expect("artifact");
        let expected = CompactSolutionSetEncoder
            .encode(&artifact)
            .expect("bounded expected envelope");
        let mut sink = AtomicFileArtifactSink::new(&target);
        let outcome = match sink.publish(
            &CompactSolutionSetEncoder,
            &artifact,
            4_096,
            &NeverCancelled,
        ) {
            Ok(outcome) => outcome,
            Err(error)
                if cfg!(target_os = "linux")
                    && error.code() == ArtifactSinkErrorCode::UnsupportedFilesystem =>
            {
                fs::remove_dir(&directory).expect("unsupported cleanup");
                return;
            }
            Err(error)
                if cfg!(not(any(target_os = "linux", windows)))
                    && error.code() == ArtifactSinkErrorCode::UnsupportedPlatform =>
            {
                fs::remove_dir(&directory).expect("unsupported cleanup");
                return;
            }
            Err(error) => panic!("native publication: {error:?}"),
        };
        let ArtifactPublicationOutcome::Committed(commit) = outcome else {
            panic!("normal local filesystem publication must be durable");
        };
        let published = fs::read(&target).expect("published bytes");
        assert_eq!(published, expected.bytes());
        assert_eq!(commit.byte_count(), published.len() as u64);
        assert_eq!(commit.checksum(), expected.checksum());
        assert_eq!(commit.solution_count(), expected.solution_count());
        assert!(commit.target_owned());
        assert!(commit.file_identity().is_some());

        fs::remove_file(&target).expect("target cleanup");
        fs::remove_dir(&directory).expect("directory cleanup");
    }

    #[test]
    fn byte_limit_is_rejected_before_native_staging_is_created() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "clearra-output-prewrite-limit-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).expect("directory");
        let target = directory.join("solutions.csa");
        let artifact = SolutionSetArtifact::try_new(
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
        .expect("artifact");
        let mut sink = AtomicFileArtifactSink::new(&target);
        let error = sink
            .publish(&CompactSolutionSetEncoder, &artifact, 1, &NeverCancelled)
            .expect_err("limit");
        assert_eq!(error.code(), ArtifactSinkErrorCode::CapacityExceeded);
        assert!(!target.exists());
        assert_eq!(fs::read_dir(&directory).expect("directory").count(), 0);
        fs::remove_dir(&directory).expect("directory cleanup");
    }

    struct CancelAt(PublicationCheckpoint);

    impl PublicationControl for CancelAt {
        fn cancelled_at(&self, checkpoint: PublicationCheckpoint) -> bool {
            checkpoint == self.0
        }
    }

    #[test]
    fn cancellation_during_streaming_closes_exact_stage_without_residue() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "clearra-output-stream-cancel-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).expect("directory");
        let target = directory.join("solutions.csa");
        let artifact = SolutionSetArtifact::try_new(
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
        .expect("artifact");
        let mut sink = AtomicFileArtifactSink::new(&target);
        let error = sink
            .publish(
                &CompactSolutionSetEncoder,
                &artifact,
                4_096,
                &CancelAt(PublicationCheckpoint::EncodingProgress { completed_units: 0 }),
            )
            .expect_err("cancel or typed unsupported filesystem");
        if cfg!(target_os = "linux") && error.code() == ArtifactSinkErrorCode::UnsupportedFilesystem
        {
            fs::remove_dir(&directory).expect("unsupported cleanup");
            return;
        }
        if cfg!(not(any(target_os = "linux", windows)))
            && error.code() == ArtifactSinkErrorCode::UnsupportedPlatform
        {
            fs::remove_dir(&directory).expect("unsupported cleanup");
            return;
        }
        assert_eq!(error.code(), ArtifactSinkErrorCode::Cancelled);
        assert!(!target.exists());
        assert_eq!(fs::read_dir(&directory).expect("directory").count(), 0);
        fs::remove_dir(&directory).expect("directory cleanup");
    }

    #[test]
    fn same_length_forged_encoder_aborts_exact_stage_without_target_or_residue() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "clearra-output-forged-stream-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).expect("directory");
        let target = directory.join("solutions.csa");
        let artifact = SolutionSetArtifact::try_new(
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
        .expect("artifact");
        let mut sink = AtomicFileArtifactSink::new(&target);
        let error = sink
            .publish(
                &SameLengthForgedCompactEncoder,
                &artifact,
                4_096,
                &NeverCancelled,
            )
            .expect_err("forged bytes must fail before the commit barrier");
        if cfg!(target_os = "linux") && error.code() == ArtifactSinkErrorCode::UnsupportedFilesystem
        {
            fs::remove_dir(&directory).expect("unsupported cleanup");
            return;
        }
        if cfg!(not(any(target_os = "linux", windows)))
            && error.code() == ArtifactSinkErrorCode::UnsupportedPlatform
        {
            fs::remove_dir(&directory).expect("unsupported cleanup");
            return;
        }
        assert_eq!(error.code(), ArtifactSinkErrorCode::EncodingFailed);
        assert!(error.residue().is_none());
        assert!(!target.exists());
        assert_eq!(fs::read_dir(&directory).expect("directory").count(), 0);
        fs::remove_dir(&directory).expect("directory cleanup");
    }

    #[test]
    fn limit_ignoring_encoder_is_rejected_before_native_staging_exists() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "clearra-output-limit-ignoring-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&directory).expect("directory");
        let target = directory.join("solutions.csa");
        let artifact = SolutionSetArtifact::try_new(
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
        .expect("artifact");
        let mut sink = AtomicFileArtifactSink::new(&target);
        let error = sink
            .publish(&LimitIgnoringEncoder, &artifact, 1, &NeverCancelled)
            .expect_err("sink must independently enforce its caller limit");

        assert_eq!(error.code(), ArtifactSinkErrorCode::CapacityExceeded);
        assert!(error.residue().is_none());
        assert!(!target.exists());
        assert_eq!(fs::read_dir(&directory).expect("directory").count(), 0);
        fs::remove_dir(&directory).expect("directory cleanup");
    }
}
