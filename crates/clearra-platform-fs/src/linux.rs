use std::{
    ffi::{CString, OsStr},
    fs::File,
    io::{self, Write},
    os::fd::{AsRawFd, FromRawFd, RawFd},
    os::unix::ffi::OsStrExt,
    path::{Component, Path},
};

use crate::{
    atomic_publication::{committed, durability_uncertain, PreCommitCrashRecovery},
    DurabilityUncertainReason, FileIdentity, NativePublicationError, NativePublicationErrorCode,
    NativePublicationOutcome,
};

#[derive(Debug)]
pub(crate) struct StagedFile {
    file: File,
    parent: File,
    proc_fd_directory: File,
    proc_fd_leaf: CString,
    target_leaf: CString,
    identity: FileIdentity,
}

impl StagedFile {
    pub(crate) fn begin(target: &Path) -> Result<Self, NativePublicationError> {
        let (parent, target_leaf) = open_parent_capability(target)?;
        let dot = c_string(OsStr::new("."))?;
        let flags =
            libc::O_TMPFILE | libc::O_RDWR | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_SYNC;
        // SAFETY: `dot` is NUL-terminated, `parent` remains alive, flags and
        // mode follow openat(2), and the returned descriptor is uniquely
        // transferred into `File` exactly once on success.
        let descriptor = unsafe { libc::openat(parent.as_raw_fd(), dot.as_ptr(), flags, 0o600) };
        if descriptor < 0 {
            let error = io::Error::last_os_error();
            return Err(map_tmpfile_error(&error));
        }
        // SAFETY: successful `openat` returned a new owned descriptor and no
        // other owner is constructed for it.
        let file = unsafe { File::from_raw_fd(descriptor) };
        let identity = file_identity(file.as_raw_fd())?;
        let (proc_fd_directory, proc_fd_leaf) =
            open_proc_fd_capability(file.as_raw_fd(), &identity)?;
        Ok(Self {
            file,
            parent,
            proc_fd_directory,
            proc_fd_leaf,
            target_leaf,
            identity,
        })
    }

    pub(crate) fn abort(self) -> Result<(), NativePublicationError> {
        // O_TMPFILE has no directory entry. Closing the exact staged fd is the
        // complete cleanup operation, including process-crash cleanup.
        drop(self);
        Ok(())
    }

    pub(crate) fn precommit_crash_recovery(&self) -> PreCommitCrashRecovery {
        PreCommitCrashRecovery::UnnamedFileReleasedOnLastHandleClose
    }

    pub(crate) fn commit(
        self,
        byte_count: u64,
    ) -> Result<NativePublicationOutcome, NativePublicationError> {
        self.file.sync_all().map_err(|error| {
            NativePublicationError::from_io(NativePublicationErrorCode::PreCommitSyncFailed, &error)
        })?;

        // `AT_EMPTY_PATH` requires CAP_DAC_READ_SEARCH even for an fd owned by
        // the caller. Instead, retain the verified procfs directory capability
        // opened at staging time and follow its kernel-owned `<stage-fd>` magic
        // link. The staged fd remains live, so its number cannot be reused;
        // neither a staging pathname nor a pathname reopen is involved.
        verify_proc_fd_identity(
            self.proc_fd_directory.as_raw_fd(),
            &self.proc_fd_leaf,
            &self.identity,
        )?;
        // SAFETY: both retained directory descriptors and both C strings are
        // live for the call. `AT_SYMLINK_FOLLOW` resolves the procfs magic link
        // to the exact retained O_TMPFILE inode. With no replacement flag,
        // every existing file, directory, or symlink leaf causes EEXIST.
        let result = unsafe {
            libc::linkat(
                self.proc_fd_directory.as_raw_fd(),
                self.proc_fd_leaf.as_ptr(),
                self.parent.as_raw_fd(),
                self.target_leaf.as_ptr(),
                libc::AT_SYMLINK_FOLLOW,
            )
        };
        if result != 0 {
            let error = io::Error::last_os_error();
            return Err(map_link_error(&error));
        }

        // Publication has linearized. No failure below is allowed to unlink
        // the target pathname; target ownership is returned explicitly.
        if self.parent.sync_all().is_err() {
            return Ok(durability_uncertain(
                self.identity,
                byte_count,
                DurabilityUncertainReason::PostPublishParentSyncFailed,
            ));
        }
        Ok(committed(self.identity, byte_count))
    }
}

const PROC_SUPER_MAGIC: libc::c_long = 0x9fa0;

fn open_proc_fd_capability(
    staged_descriptor: RawFd,
    expected_identity: &FileIdentity,
) -> Result<(File, CString), NativePublicationError> {
    let path = c_string(OsStr::new("/proc/self/fd"))?;
    let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    // SAFETY: `path` is NUL-terminated and a successful new descriptor is
    // transferred into exactly one `File` owner.
    let descriptor = unsafe { libc::open(path.as_ptr(), flags) };
    if descriptor < 0 {
        return Err(proc_fd_unsupported(&io::Error::last_os_error()));
    }
    // SAFETY: successful `open` returned one newly owned descriptor.
    let directory = unsafe { File::from_raw_fd(descriptor) };

    // A bind-mounted lookalike must not become publication authority. The
    // retained directory must be an actual procfs instance before its magic
    // links are used as kernel capabilities.
    // SAFETY: zero is a valid initial representation for `statfs`, and the
    // live directory descriptor and writable output remain valid for fstatfs.
    let mut filesystem: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstatfs(directory.as_raw_fd(), &mut filesystem) } != 0 {
        return Err(proc_fd_unsupported(&io::Error::last_os_error()));
    }
    if filesystem.f_type as libc::c_long != PROC_SUPER_MAGIC {
        return Err(NativePublicationError::new(
            NativePublicationErrorCode::UnsupportedFilesystem,
        ));
    }

    let leaf = CString::new(staged_descriptor.to_string()).map_err(|_| {
        NativePublicationError::new(NativePublicationErrorCode::UnsupportedFilesystem)
    })?;
    verify_proc_fd_identity(directory.as_raw_fd(), &leaf, expected_identity)?;
    Ok((directory, leaf))
}

fn verify_proc_fd_identity(
    proc_fd_directory: RawFd,
    proc_fd_leaf: &CString,
    expected_identity: &FileIdentity,
) -> Result<(), NativePublicationError> {
    // SAFETY: zero is a valid initial representation for `stat`; fstatat
    // follows the procfs magic link by default and initializes the structure
    // before any field is read on success.
    let mut metadata: libc::stat = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstatat(proc_fd_directory, proc_fd_leaf.as_ptr(), &mut metadata, 0) } != 0 {
        return Err(proc_fd_unsupported(&io::Error::last_os_error()));
    }
    let actual = FileIdentity::Linux {
        device: metadata.st_dev as u64,
        inode: metadata.st_ino as u64,
    };
    if &actual != expected_identity {
        return Err(NativePublicationError::new(
            NativePublicationErrorCode::UnsupportedFilesystem,
        ));
    }
    Ok(())
}

fn proc_fd_unsupported(error: &io::Error) -> NativePublicationError {
    NativePublicationError::from_io(NativePublicationErrorCode::UnsupportedFilesystem, error)
}

impl Write for StagedFile {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.file.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

fn open_parent_capability(target: &Path) -> Result<(File, CString), NativePublicationError> {
    let leaf = match target.file_name() {
        Some(leaf) if !leaf.is_empty() => c_string(leaf)?,
        _ => {
            return Err(NativePublicationError::new(
                NativePublicationErrorCode::InvalidTarget,
            ))
        }
    };
    let parent = target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    let anchor = if parent.is_absolute() { "/" } else { "." };
    let mut current = open_directory_at(libc::AT_FDCWD, OsStr::new(anchor))?;
    for component in parent.components() {
        match component {
            Component::RootDir | Component::CurDir => {}
            Component::Normal(name) => {
                let next = open_directory_at(current.as_raw_fd(), name)?;
                current = next;
            }
            Component::ParentDir | Component::Prefix(_) => {
                return Err(NativePublicationError::new(
                    NativePublicationErrorCode::InvalidTarget,
                ))
            }
        }
    }
    Ok((current, leaf))
}

fn open_directory_at(parent: RawFd, name: &OsStr) -> Result<File, NativePublicationError> {
    let name = c_string(name)?;
    let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
    // SAFETY: `name` is NUL-terminated, `parent` is either AT_FDCWD or a live
    // directory descriptor, and ownership of a successful descriptor is
    // transferred exactly once to `File`.
    let descriptor = unsafe { libc::openat(parent, name.as_ptr(), flags) };
    if descriptor < 0 {
        let error = io::Error::last_os_error();
        return Err(NativePublicationError::from_io(
            match error.raw_os_error() {
                Some(libc::ENOENT) => NativePublicationErrorCode::ParentMissing,
                Some(libc::ELOOP) => NativePublicationErrorCode::ReparsePointRejected,
                _ => NativePublicationErrorCode::ParentNotDirectory,
            },
            &error,
        ));
    }
    // SAFETY: successful openat returned one newly owned descriptor.
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

fn c_string(value: &OsStr) -> Result<CString, NativePublicationError> {
    CString::new(value.as_bytes())
        .map_err(|_| NativePublicationError::new(NativePublicationErrorCode::InvalidTarget))
}

fn file_identity(descriptor: RawFd) -> Result<FileIdentity, NativePublicationError> {
    // SAFETY: zero is a valid initial representation for `stat`, and fstat
    // initializes it before any field is read on success.
    let mut metadata: libc::stat = unsafe { std::mem::zeroed() };
    // SAFETY: descriptor is live and metadata points to writable storage.
    if unsafe { libc::fstat(descriptor, &mut metadata) } != 0 {
        let error = io::Error::last_os_error();
        return Err(NativePublicationError::from_io(
            NativePublicationErrorCode::StagingFailed,
            &error,
        ));
    }
    Ok(FileIdentity::Linux {
        device: metadata.st_dev as u64,
        inode: metadata.st_ino as u64,
    })
}

fn map_tmpfile_error(error: &io::Error) -> NativePublicationError {
    let code = match error.raw_os_error() {
        Some(libc::EOPNOTSUPP)
        | Some(libc::EINVAL)
        | Some(libc::EISDIR)
        | Some(libc::ENOSYS)
        | Some(libc::ENOENT) => NativePublicationErrorCode::UnsupportedFilesystem,
        _ => NativePublicationErrorCode::StagingFailed,
    };
    NativePublicationError::from_io(code, error)
}

fn map_link_error(error: &io::Error) -> NativePublicationError {
    let code = match error.raw_os_error() {
        Some(libc::EEXIST) => NativePublicationErrorCode::TargetExists,
        Some(libc::EOPNOTSUPP)
        | Some(libc::EINVAL)
        | Some(libc::ENOSYS)
        | Some(libc::EPERM)
        | Some(libc::EACCES)
        | Some(libc::ENOENT)
        | Some(libc::EXDEV)
        | Some(libc::ELOOP)
        | Some(libc::ENOTDIR)
        | Some(libc::EMLINK) => NativePublicationErrorCode::UnsupportedFilesystem,
        _ => NativePublicationErrorCode::PublicationFailed,
    };
    NativePublicationError::from_io(code, error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_and_procfs_link_failures_are_typed_unsupported() {
        for raw in [
            libc::EOPNOTSUPP,
            libc::EINVAL,
            libc::ENOSYS,
            libc::EPERM,
            libc::EACCES,
            libc::ENOENT,
            libc::EXDEV,
            libc::ELOOP,
            libc::ENOTDIR,
            libc::EMLINK,
        ] {
            assert_eq!(
                map_link_error(&io::Error::from_raw_os_error(raw)).code(),
                NativePublicationErrorCode::UnsupportedFilesystem
            );
        }
    }
}
