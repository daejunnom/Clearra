use std::{
    ffi::OsStr,
    fs::{File, OpenOptions},
    io::{self, Write},
    mem::{size_of, MaybeUninit},
    os::windows::{
        ffi::OsStrExt,
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::{Component, Path, PathBuf, Prefix},
    ptr,
};

use windows_sys::Wdk::{
    Foundation::OBJECT_ATTRIBUTES,
    Storage::FileSystem::{
        FileLinkInformationEx, NtCreateFile, NtSetInformationFile, FILE_CREATE,
        FILE_DELETE_ON_CLOSE, FILE_DIRECTORY_FILE, FILE_LINK_INFORMATION, FILE_NON_DIRECTORY_FILE,
        FILE_OPEN, FILE_OPEN_REPARSE_POINT, FILE_SYNCHRONOUS_IO_NONALERT, FILE_WRITE_THROUGH,
    },
};
use windows_sys::Win32::{
    Foundation::{
        RtlNtStatusToDosError, HANDLE, OBJ_CASE_INSENSITIVE, OBJ_DONT_REPARSE, UNICODE_STRING,
    },
    Security::Cryptography::{BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG},
    Storage::FileSystem::{
        FileAttributeTagInfo, FileIdInfo, FlushFileBuffers, GetFileInformationByHandleEx, DELETE,
        FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_TAG_INFO,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_ID_INFO,
        FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, SYNCHRONIZE,
    },
    System::IO::IO_STATUS_BLOCK,
};

use crate::{
    atomic_publication::{committed, durability_uncertain, PreCommitCrashRecovery},
    DurabilityUncertainReason, FileIdentity, NativePublicationError, NativePublicationErrorCode,
    NativePublicationOutcome,
};

const MAX_STAGE_ATTEMPTS: usize = 32;
const ERROR_FILE_EXISTS_CODE: i32 = 80;
const ERROR_ALREADY_EXISTS_CODE: i32 = 183;
const ERROR_NOT_SUPPORTED_CODE: i32 = 50;
const ERROR_INVALID_FUNCTION_CODE: i32 = 1;
const ERROR_NOT_SAME_DEVICE_CODE: i32 = 17;
const ERROR_INVALID_PARAMETER_CODE: i32 = 87;
const ERROR_CLOUD_FILE_INCOMPATIBLE_HARDLINKS_CODE: i32 = 396;
const ERROR_TOO_MANY_LINKS_CODE: i32 = 1142;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SharePolicy {
    /// Other readers/writers may keep using an ancestor, but nobody may gain
    /// delete sharing while the capability chain is retained. This pins every
    /// opened directory against rename or removal until publication ends.
    ParentCapability,
    /// The staged object is private to its retained handle. In particular, no
    /// competing writer may alter bytes after the encoder receipt is formed.
    ExclusiveStaging,
}

impl SharePolicy {
    const fn mask(self) -> u32 {
        match self {
            Self::ParentCapability => FILE_SHARE_READ | FILE_SHARE_WRITE,
            Self::ExclusiveStaging => 0,
        }
    }
}

#[derive(Debug)]
pub(crate) struct StagedFile {
    file: File,
    // Every ancestor remains open until commit/abort. The final element is the
    // parent capability used by relative create and relative link.
    ancestors: Vec<File>,
    target_leaf: Vec<u16>,
    staging_leaf: String,
    identity: FileIdentity,
}

impl StagedFile {
    pub(crate) fn begin(target: &Path) -> Result<Self, NativePublicationError> {
        let (ancestors, target_leaf) = open_parent_capabilities(target)?;
        let parent = ancestors.last().ok_or_else(|| {
            NativePublicationError::new(NativePublicationErrorCode::ParentMissing)
        })?;

        for _ in 0..MAX_STAGE_ATTEMPTS {
            let staging_leaf = random_staging_leaf()?;
            let staging_utf16 = utf16_component(OsStr::new(&staging_leaf))?;
            let file = match nt_create_relative_file(parent, &staging_utf16) {
                Ok(file) => file,
                Err(error) if error.raw_os_error() == Some(ERROR_FILE_EXISTS_CODE) => continue,
                Err(error) if error.raw_os_error() == Some(ERROR_ALREADY_EXISTS_CODE) => continue,
                Err(error) => {
                    return Err(NativePublicationError::from_io(
                        map_staging_create_error(&error),
                        &error,
                    ))
                }
            };
            // Delete-on-close is part of the atomic create operation. Every
            // error after this point, including reparse and identity probes,
            // closes the exact handle and removes the staging link; there is
            // no post-create arming window and no pathname cleanup.
            reject_reparse(&file)?;
            let identity = file_identity(&file)?;
            return Ok(Self {
                file,
                ancestors,
                target_leaf,
                staging_leaf,
                identity,
            });
        }
        Err(NativePublicationError::new(
            NativePublicationErrorCode::StagingFailed,
        ))
    }

    pub(crate) fn abort(self) -> Result<(), NativePublicationError> {
        // Construction succeeds only after delete-on-close is armed on this
        // exact retained handle. Closing it is therefore the cleanup
        // operation; no pathname lookup or identity revalidation is involved.
        drop(self);
        Ok(())
    }

    pub(crate) fn precommit_crash_recovery(&self) -> PreCommitCrashRecovery {
        PreCommitCrashRecovery::DeleteOnCloseHandle {
            staging_leaf: self.staging_leaf.clone(),
            file_identity: self.identity.clone(),
        }
    }

    pub(crate) fn commit(
        self,
        byte_count: u64,
    ) -> Result<NativePublicationOutcome, NativePublicationError> {
        self.file.sync_all().map_err(|error| {
            NativePublicationError::from_io(NativePublicationErrorCode::PreCommitSyncFailed, &error)
        })?;
        let parent = self.ancestors.last().ok_or_else(|| {
            NativePublicationError::new(NativePublicationErrorCode::ParentMissing)
        })?;
        if let Err(error) =
            link_same_handle_no_replace(&self.file, raw_handle(parent), self.target_leaf.as_slice())
        {
            return Err(NativePublicationError::from_io(
                map_link_error(&error),
                &error,
            ));
        }

        // Hard-link creation is the commit linearization point: the final name
        // now references the exact retained file object and is independent of
        // the delete-on-close staging link. Closing this handle removes only
        // that staging link. No failure below may path-delete the final link.
        let postpublish_sync = flush_file(&self.file);
        let identity = self.identity.clone();
        drop(self);
        if postpublish_sync.is_err() {
            return Ok(durability_uncertain(
                identity,
                byte_count,
                DurabilityUncertainReason::PostPublishFileSyncFailed,
            ));
        }
        Ok(committed(identity, byte_count))
    }
}

impl Write for StagedFile {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.file.write(bytes)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

fn open_parent_capabilities(
    target: &Path,
) -> Result<(Vec<File>, Vec<u16>), NativePublicationError> {
    validate_original_target(target)?;
    let absolute = std::path::absolute(target).map_err(|error| {
        NativePublicationError::from_io(NativePublicationErrorCode::InvalidTarget, &error)
    })?;
    let target_leaf = absolute
        .file_name()
        .ok_or_else(|| NativePublicationError::new(NativePublicationErrorCode::InvalidTarget))?;
    validate_leaf(target_leaf)?;
    let target_leaf = utf16_component(target_leaf)?;
    let parent = absolute
        .parent()
        .ok_or_else(|| NativePublicationError::new(NativePublicationErrorCode::ParentMissing))?;
    let root = absolute_root(parent)?;
    let root_handle = open_root_directory(&root)?;
    reject_reparse(&root_handle)?;

    let mut ancestors = vec![root_handle];
    let relative_parent = parent
        .strip_prefix(&root)
        .map_err(|_| NativePublicationError::new(NativePublicationErrorCode::InvalidTarget))?;
    for component in relative_parent.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(name) => {
                let name = utf16_component(name)?;
                let next =
                    nt_open_relative_directory(ancestors.last().expect("root capability"), &name)?;
                reject_reparse(&next)?;
                ancestors.push(next);
            }
            Component::RootDir | Component::Prefix(_) | Component::ParentDir => {
                return Err(NativePublicationError::new(
                    NativePublicationErrorCode::InvalidTarget,
                ))
            }
        }
    }
    Ok((ancestors, target_leaf))
}

fn validate_original_target(target: &Path) -> Result<(), NativePublicationError> {
    if target.as_os_str().is_empty()
        || target
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(NativePublicationError::new(
            NativePublicationErrorCode::InvalidTarget,
        ));
    }
    for component in target.components() {
        if let Component::Prefix(prefix) = component {
            match prefix.kind() {
                Prefix::Disk(_)
                | Prefix::UNC(_, _)
                | Prefix::VerbatimDisk(_)
                | Prefix::VerbatimUNC(_, _) => {}
                Prefix::Verbatim(_) | Prefix::DeviceNS(_) => {
                    return Err(NativePublicationError::new(
                        NativePublicationErrorCode::InvalidTarget,
                    ))
                }
            }
        }
    }
    Ok(())
}

fn validate_leaf(leaf: &OsStr) -> Result<(), NativePublicationError> {
    let text = leaf.to_string_lossy();
    if text.is_empty()
        || text.contains(':')
        || text.ends_with('.')
        || text.ends_with(' ')
        || text == "."
        || text == ".."
    {
        return Err(NativePublicationError::new(
            NativePublicationErrorCode::InvalidTarget,
        ));
    }
    Ok(())
}

fn absolute_root(path: &Path) -> Result<PathBuf, NativePublicationError> {
    path.ancestors()
        .last()
        .filter(|root| root.has_root())
        .map(Path::to_path_buf)
        .ok_or_else(|| NativePublicationError::new(NativePublicationErrorCode::InvalidTarget))
}

fn open_root_directory(root: &Path) -> Result<File, NativePublicationError> {
    OpenOptions::new()
        .read(true)
        .access_mode(FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE)
        .share_mode(SharePolicy::ParentCapability.mask())
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(root)
        .map_err(|error| {
            NativePublicationError::from_io(
                match error.kind() {
                    io::ErrorKind::NotFound => NativePublicationErrorCode::ParentMissing,
                    _ => NativePublicationErrorCode::ParentNotDirectory,
                },
                &error,
            )
        })
}

fn nt_open_relative_directory(parent: &File, name: &[u16]) -> Result<File, NativePublicationError> {
    nt_create_relative(
        raw_handle(parent),
        name,
        FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
        FILE_OPEN,
        FILE_DIRECTORY_FILE | FILE_OPEN_REPARSE_POINT | FILE_SYNCHRONOUS_IO_NONALERT,
        FILE_ATTRIBUTE_NORMAL,
        SharePolicy::ParentCapability,
    )
    .map_err(|error| {
        NativePublicationError::from_io(
            match error.kind() {
                io::ErrorKind::NotFound => NativePublicationErrorCode::ParentMissing,
                _ => NativePublicationErrorCode::ParentNotDirectory,
            },
            &error,
        )
    })
}

fn nt_create_relative_file(parent: &File, name: &[u16]) -> io::Result<File> {
    nt_create_relative(
        raw_handle(parent),
        name,
        FILE_READ_DATA
            | FILE_WRITE_DATA
            | FILE_READ_ATTRIBUTES
            | FILE_WRITE_ATTRIBUTES
            | DELETE
            | SYNCHRONIZE,
        FILE_CREATE,
        FILE_NON_DIRECTORY_FILE
            | FILE_OPEN_REPARSE_POINT
            | FILE_SYNCHRONOUS_IO_NONALERT
            | FILE_WRITE_THROUGH
            | FILE_DELETE_ON_CLOSE,
        FILE_ATTRIBUTE_NORMAL,
        SharePolicy::ExclusiveStaging,
    )
}

fn nt_create_relative(
    parent: HANDLE,
    name: &[u16],
    desired_access: u32,
    disposition: u32,
    options: u32,
    file_attributes: u32,
    share_policy: SharePolicy,
) -> io::Result<File> {
    let byte_length = name
        .len()
        .checked_mul(size_of::<u16>())
        .and_then(|length| u16::try_from(length).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "component is too long"))?;
    let unicode = UNICODE_STRING {
        Length: byte_length,
        MaximumLength: byte_length,
        Buffer: name.as_ptr().cast_mut(),
    };
    let object_attributes = OBJECT_ATTRIBUTES {
        Length: u32::try_from(size_of::<OBJECT_ATTRIBUTES>()).expect("OBJECT_ATTRIBUTES size"),
        RootDirectory: parent,
        ObjectName: &unicode,
        Attributes: OBJ_CASE_INSENSITIVE | OBJ_DONT_REPARSE,
        SecurityDescriptor: ptr::null(),
        SecurityQualityOfService: ptr::null(),
    };
    let mut handle: HANDLE = ptr::null_mut();
    let mut status_block = IO_STATUS_BLOCK::default();
    // SAFETY: all pointers refer to initialized storage for the duration of
    // the synchronous call, the name buffer outlives OBJECT_ATTRIBUTES, and a
    // successful handle is transferred to exactly one `File` owner.
    let status = unsafe {
        NtCreateFile(
            &mut handle,
            desired_access,
            &object_attributes,
            &mut status_block,
            ptr::null(),
            file_attributes,
            share_policy.mask(),
            disposition,
            options,
            ptr::null(),
            0,
        )
    };
    if status < 0 {
        return Err(ntstatus_error(status));
    }
    // SAFETY: NtCreateFile succeeded and returned one owned Windows handle.
    Ok(unsafe { File::from_raw_handle(handle.cast()) })
}

fn reject_reparse(file: &File) -> Result<(), NativePublicationError> {
    let mut info = MaybeUninit::<FILE_ATTRIBUTE_TAG_INFO>::zeroed();
    // SAFETY: the handle is live and the output buffer has the exact structure
    // size required by FileAttributeTagInfo.
    let result = unsafe {
        GetFileInformationByHandleEx(
            raw_handle(file),
            FileAttributeTagInfo,
            info.as_mut_ptr().cast(),
            u32::try_from(size_of::<FILE_ATTRIBUTE_TAG_INFO>()).expect("attribute info size"),
        )
    };
    if result == 0 {
        let error = io::Error::last_os_error();
        return Err(NativePublicationError::from_io(
            NativePublicationErrorCode::StagingFailed,
            &error,
        ));
    }
    // SAFETY: successful API call initialized the complete structure.
    let info = unsafe { info.assume_init() };
    if info.FileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(NativePublicationError::new(
            NativePublicationErrorCode::ReparsePointRejected,
        ));
    }
    Ok(())
}

fn file_identity(file: &File) -> Result<FileIdentity, NativePublicationError> {
    let mut info = MaybeUninit::<FILE_ID_INFO>::zeroed();
    // SAFETY: the handle is live and the output buffer exactly matches
    // FILE_ID_INFO for FileIdInfo.
    let result = unsafe {
        GetFileInformationByHandleEx(
            raw_handle(file),
            FileIdInfo,
            info.as_mut_ptr().cast(),
            u32::try_from(size_of::<FILE_ID_INFO>()).expect("file id info size"),
        )
    };
    if result == 0 {
        let error = io::Error::last_os_error();
        return Err(NativePublicationError::from_io(
            NativePublicationErrorCode::StagingFailed,
            &error,
        ));
    }
    // SAFETY: successful API call initialized the complete structure.
    let info = unsafe { info.assume_init() };
    Ok(FileIdentity::Windows {
        volume_serial_number: info.VolumeSerialNumber,
        file_id: info.FileId.Identifier,
    })
}

fn link_same_handle_no_replace(file: &File, parent: HANDLE, target_leaf: &[u16]) -> io::Result<()> {
    let name_bytes = target_leaf
        .len()
        .checked_mul(size_of::<u16>())
        .and_then(|length| u32::try_from(length).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "target is too long"))?;
    let total = size_of::<FILE_LINK_INFORMATION>()
        .checked_add(usize::try_from(name_bytes).expect("u32 fits usize"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "link buffer overflow"))?;
    let words = total
        .checked_add(size_of::<u64>() - 1)
        .map(|size| size / size_of::<u64>())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "link buffer overflow"))?;
    let mut buffer = vec![0_u64; words];
    let info = buffer.as_mut_ptr().cast::<FILE_LINK_INFORMATION>();
    let mut status_block = IO_STATUS_BLOCK::default();
    // SAFETY: `buffer` is aligned and large enough for the fixed header and
    // exact UTF-16 tail. FileLinkInformationEx interprets Flags=0 as strict
    // no-replacement and resolves FileName relative to the retained parent
    // capability. NtSetInformationFile operates on the exact staged handle.
    unsafe {
        (*info).Anonymous.Flags = 0;
        (*info).RootDirectory = parent;
        (*info).FileNameLength = name_bytes;
        ptr::copy_nonoverlapping(
            target_leaf.as_ptr(),
            (*info).FileName.as_mut_ptr(),
            target_leaf.len(),
        );
        let status = NtSetInformationFile(
            raw_handle(file),
            &mut status_block,
            buffer.as_ptr().cast(),
            u32::try_from(total).expect("link buffer size"),
            FileLinkInformationEx,
        );
        if status < 0 {
            return Err(ntstatus_error(status));
        }
    }
    Ok(())
}

fn flush_file(file: &File) -> io::Result<()> {
    // SAFETY: file is a live write-capable handle.
    if unsafe { FlushFileBuffers(raw_handle(file)) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn random_staging_leaf() -> Result<String, NativePublicationError> {
    let mut random = [0_u8; 16];
    // SAFETY: the buffer is writable for its exact length and the null
    // algorithm handle is required with BCRYPT_USE_SYSTEM_PREFERRED_RNG.
    let status = unsafe {
        BCryptGenRandom(
            ptr::null_mut(),
            random.as_mut_ptr(),
            u32::try_from(random.len()).expect("random length"),
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status < 0 {
        return Err(NativePublicationError::from_io(
            NativePublicationErrorCode::StagingFailed,
            &ntstatus_error(status),
        ));
    }
    let mut leaf = String::from(".clearra-artifact-stage-v1-");
    for byte in random {
        leaf.push_str(&format!("{byte:02x}"));
    }
    leaf.push_str(".tmp");
    Ok(leaf)
}

fn utf16_component(value: &OsStr) -> Result<Vec<u16>, NativePublicationError> {
    let encoded = value.encode_wide().collect::<Vec<_>>();
    if encoded.is_empty()
        || encoded.iter().any(|unit| {
            *unit == 0
                || *unit == u16::from(b'\\')
                || *unit == u16::from(b'/')
                || *unit == u16::from(b':')
        })
        || encoded.len() > 32_767
    {
        return Err(NativePublicationError::new(
            NativePublicationErrorCode::InvalidTarget,
        ));
    }
    Ok(encoded)
}

fn raw_handle(file: &File) -> HANDLE {
    file.as_raw_handle().cast()
}

fn ntstatus_error(status: i32) -> io::Error {
    // SAFETY: conversion has no pointer preconditions.
    let code = unsafe { RtlNtStatusToDosError(status) };
    io::Error::from_raw_os_error(code as i32)
}

fn map_link_error(error: &io::Error) -> NativePublicationErrorCode {
    match error.raw_os_error() {
        Some(ERROR_FILE_EXISTS_CODE) | Some(ERROR_ALREADY_EXISTS_CODE) => {
            NativePublicationErrorCode::TargetExists
        }
        Some(ERROR_NOT_SUPPORTED_CODE)
        | Some(ERROR_INVALID_FUNCTION_CODE)
        | Some(ERROR_NOT_SAME_DEVICE_CODE)
        | Some(ERROR_INVALID_PARAMETER_CODE)
        | Some(ERROR_CLOUD_FILE_INCOMPATIBLE_HARDLINKS_CODE)
        | Some(ERROR_TOO_MANY_LINKS_CODE) => NativePublicationErrorCode::UnsupportedFilesystem,
        _ => NativePublicationErrorCode::PublicationFailed,
    }
}

fn map_staging_create_error(error: &io::Error) -> NativePublicationErrorCode {
    match error.raw_os_error() {
        Some(ERROR_NOT_SUPPORTED_CODE)
        | Some(ERROR_INVALID_FUNCTION_CODE)
        | Some(ERROR_INVALID_PARAMETER_CODE) => NativePublicationErrorCode::UnsupportedFilesystem,
        _ => NativePublicationErrorCode::StagingFailed,
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, time::SystemTime};

    use super::*;

    fn unique_directory(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "clearra-platform-fs-{label}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn create_time_delete_on_close_closes_every_postcreate_failure_window() {
        let directory = unique_directory("delete-on-close-create");
        fs::create_dir(&directory).expect("directory");
        let requested_target = directory.join("target.csa");
        let (ancestors, _) =
            open_parent_capabilities(&requested_target).expect("parent capabilities");
        let parent = ancestors.last().expect("parent capability");
        let staging_leaf = random_staging_leaf().expect("random staging leaf");
        let staging_utf16 = utf16_component(OsStr::new(&staging_leaf)).expect("staging component");
        let file = nt_create_relative_file(parent, &staging_utf16).expect("atomic staging create");

        assert_eq!(
            fs::read_dir(&directory).expect("staging directory").count(),
            1,
            "the test must observe the live staging link"
        );
        drop(file);
        assert_eq!(
            fs::read_dir(&directory).expect("cleaned directory").count(),
            0,
            "closing the raw post-create handle must require no later arming call"
        );

        drop(ancestors);
        fs::remove_dir(&directory).expect("directory cleanup");
    }

    #[test]
    fn hard_link_linearization_survives_process_exit_and_removes_staging_link() {
        const CHILD_TARGET: &str = "CLEARRA_PLATFORM_FS_LINK_CRASH_TARGET";
        if let Some(target) = std::env::var_os(CHILD_TARGET) {
            let target = PathBuf::from(target);
            let (ancestors, target_leaf) =
                open_parent_capabilities(&target).expect("child parent capabilities");
            let parent = ancestors.last().expect("child parent capability");
            let staging_leaf = random_staging_leaf().expect("child staging leaf");
            let staging_utf16 =
                utf16_component(OsStr::new(&staging_leaf)).expect("child staging component");
            let mut file =
                nt_create_relative_file(parent, &staging_utf16).expect("child atomic stage");
            file.write_all(b"exact hard-link bytes")
                .expect("child staged bytes");
            file.sync_all().expect("child prelink sync");
            link_same_handle_no_replace(&file, raw_handle(parent), &target_leaf)
                .expect("child exact-handle link");
            flush_file(&file).expect("child postlink sync");
            std::process::abort();
        }

        let directory = unique_directory("hard-link-crash");
        fs::create_dir(&directory).expect("directory");
        let target = directory.join("target.csa");
        let status = std::process::Command::new(std::env::current_exe().expect("test executable"))
            .arg("--exact")
            .arg("windows::tests::hard_link_linearization_survives_process_exit_and_removes_staging_link")
            .arg("--nocapture")
            .env(CHILD_TARGET, &target)
            .status()
            .expect("crash child");

        assert!(!status.success(), "child must terminate without unwinding");
        assert_eq!(
            fs::read(&target).expect("committed hard link"),
            b"exact hard-link bytes"
        );
        assert_eq!(
            fs::read_dir(&directory)
                .expect("post-crash directory")
                .count(),
            1,
            "delete-on-close must remove only the staging link"
        );
        fs::remove_file(&target).expect("target cleanup");
        fs::remove_dir(&directory).expect("directory cleanup");
    }

    #[test]
    fn hard_link_failures_have_fail_closed_typed_boundaries() {
        for raw in [
            ERROR_NOT_SUPPORTED_CODE,
            ERROR_INVALID_FUNCTION_CODE,
            ERROR_NOT_SAME_DEVICE_CODE,
            ERROR_INVALID_PARAMETER_CODE,
            ERROR_CLOUD_FILE_INCOMPATIBLE_HARDLINKS_CODE,
            ERROR_TOO_MANY_LINKS_CODE,
        ] {
            assert_eq!(
                map_link_error(&io::Error::from_raw_os_error(raw)),
                NativePublicationErrorCode::UnsupportedFilesystem
            );
        }
        for raw in [
            ERROR_NOT_SUPPORTED_CODE,
            ERROR_INVALID_FUNCTION_CODE,
            ERROR_INVALID_PARAMETER_CODE,
        ] {
            assert_eq!(
                map_staging_create_error(&io::Error::from_raw_os_error(raw)),
                NativePublicationErrorCode::UnsupportedFilesystem
            );
        }
        for raw in [ERROR_FILE_EXISTS_CODE, ERROR_ALREADY_EXISTS_CODE] {
            assert_eq!(
                map_link_error(&io::Error::from_raw_os_error(raw)),
                NativePublicationErrorCode::TargetExists
            );
        }
    }
}
