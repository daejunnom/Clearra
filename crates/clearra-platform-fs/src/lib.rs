#![deny(unsafe_code)]

//! Capability-bound native atomic file publication.
//!
//! The public API is safe. Platform FFI and raw handle ownership are confined
//! to this crate's private target modules. Unsupported operating systems and
//! filesystems fail closed; there is no named-path fallback.

mod atomic_publication;
mod error;

#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
mod linux;
#[cfg(not(any(target_os = "linux", windows)))]
mod unsupported;
#[cfg(windows)]
#[allow(unsafe_code)]
mod windows;

pub use atomic_publication::{
    AtomicFileStage, NativeFileCommit, NativePublicationOutcome, NeverCancelled,
    PostCommitCrashRecovery, PreCommitCrashRecovery, PublicationCheckpoint, PublicationControl,
};
pub use error::{
    DurabilityUncertainReason, NativePublicationError, NativePublicationErrorCode,
    PublicationResidue,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FileIdentity {
    Windows {
        volume_serial_number: u64,
        file_id: [u8; 16],
    },
    Linux {
        device: u64,
        inode: u64,
    },
}

impl FileIdentity {
    pub const fn platform(&self) -> &'static str {
        match self {
            Self::Windows { .. } => "windows-file-id-info",
            Self::Linux { .. } => "linux-device-inode",
        }
    }

    pub fn stable_value(&self) -> String {
        match self {
            Self::Windows {
                volume_serial_number,
                file_id,
            } => {
                let mut value = format!("{volume_serial_number:016x}:");
                for byte in file_id {
                    value.push_str(&format!("{byte:02x}"));
                }
                value
            }
            Self::Linux { device, inode } => format!("{device:016x}:{inode:016x}"),
        }
    }
}

#[cfg(test)]
mod unsafe_scope_tests {
    #[test]
    fn unsafe_syntax_is_scoped_only_to_the_two_native_backend_modules() {
        let facade = include_str!("lib.rs");
        let deny_attribute = ["#![deny(", "unsafe_code", ")]"].concat();
        let allow_attribute = ["#[allow(", "unsafe_code", ")]"].concat();
        assert_eq!(facade.matches(&deny_attribute).count(), 1);
        assert_eq!(facade.matches(&allow_attribute).count(), 2);

        let unsafe_block = ["unsafe", " {"].concat();
        let unsafe_function = ["unsafe", " fn"].concat();
        for (name, source) in [
            (
                "atomic_publication.rs",
                include_str!("atomic_publication.rs"),
            ),
            ("error.rs", include_str!("error.rs")),
            ("unsupported.rs", include_str!("unsupported.rs")),
        ] {
            assert!(
                !source.contains(&unsafe_block) && !source.contains(&unsafe_function),
                "safe facade module contains unsafe syntax: {name}"
            );
        }

        assert!(include_str!("linux.rs").contains(&unsafe_block));
        assert!(include_str!("windows.rs").contains(&unsafe_block));
    }
}
