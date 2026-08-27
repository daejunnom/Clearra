use std::{io, path::Path};

use crate::{
    NativePublicationError, NativePublicationErrorCode, NativePublicationOutcome,
    PreCommitCrashRecovery,
};

#[derive(Debug)]
pub(crate) enum StagedFile {}

impl StagedFile {
    pub(crate) fn begin(_target: &Path) -> Result<Self, NativePublicationError> {
        Err(NativePublicationError::new(
            NativePublicationErrorCode::UnsupportedPlatform,
        ))
    }

    pub(crate) fn abort(self) -> Result<(), NativePublicationError> {
        match self {}
    }

    pub(crate) fn precommit_crash_recovery(&self) -> PreCommitCrashRecovery {
        match *self {}
    }

    pub(crate) fn commit(
        self,
        _byte_count: u64,
    ) -> Result<NativePublicationOutcome, NativePublicationError> {
        match self {}
    }
}

impl io::Write for StagedFile {
    fn write(&mut self, _bytes: &[u8]) -> io::Result<usize> {
        match *self {}
    }

    fn flush(&mut self) -> io::Result<()> {
        match *self {}
    }
}
