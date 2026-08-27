use std::{
    cell::RefCell,
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(target_os = "linux")]
use std::os::unix::fs::PermissionsExt;

#[cfg(any(target_os = "linux", windows))]
use clearra_platform_fs::PreCommitCrashRecovery;
use clearra_platform_fs::{
    AtomicFileStage, NativePublicationErrorCode, NativePublicationOutcome, NeverCancelled,
    PostCommitCrashRecovery, PublicationCheckpoint, PublicationControl,
};

fn unique_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "clearra-native-publication-{label}-{}-{nonce}",
        std::process::id()
    ))
}

fn begin_or_supported_skip(target: &Path, expected_bytes: u64) -> Option<AtomicFileStage> {
    match AtomicFileStage::begin(target, expected_bytes, &NeverCancelled) {
        Ok(stage) => Some(stage),
        Err(error)
            if cfg!(target_os = "linux")
                && error.code() == NativePublicationErrorCode::UnsupportedFilesystem =>
        {
            None
        }
        Err(error)
            if cfg!(not(any(target_os = "linux", windows)))
                && error.code() == NativePublicationErrorCode::UnsupportedPlatform =>
        {
            None
        }
        Err(error) => panic!("stage failed unexpectedly: {error:?}"),
    }
}

#[cfg(target_os = "linux")]
fn linux_effective_capabilities() -> u64 {
    let status = fs::read_to_string("/proc/self/status").expect("Linux process status");
    let hexadecimal = status
        .lines()
        .find_map(|line| line.strip_prefix("CapEff:\t"))
        .expect("CapEff field");
    u64::from_str_radix(hexadecimal, 16).expect("CapEff hexadecimal mask")
}

#[cfg(not(any(target_os = "linux", windows)))]
#[test]
fn unsupported_platform_fails_closed_without_creating_a_target() {
    let target = unique_directory("unsupported").join("solutions.csa");
    let error = AtomicFileStage::begin(&target, 0, &NeverCancelled).expect_err("unsupported");
    assert_eq!(
        error.code(),
        NativePublicationErrorCode::UnsupportedPlatform
    );
    assert!(!target.exists());
}

#[test]
fn same_handle_publication_commits_exact_bytes_and_refuses_overwrite() {
    let directory = unique_directory("commit");
    fs::create_dir(&directory).expect("directory");
    let target = directory.join("solutions.csa");
    let bytes = b"solution-set-artifact.v1 exact bytes";
    let Some(mut stage) = begin_or_supported_skip(&target, bytes.len() as u64) else {
        fs::remove_dir(&directory).expect("unsupported cleanup");
        return;
    };
    assert!(stage.precommit_crash_recovery().is_some());
    stage.write_all(bytes).expect("staged bytes");
    let outcome = stage.commit(&NeverCancelled).expect("commit");
    assert!(matches!(outcome, NativePublicationOutcome::Committed(_)));
    assert_eq!(outcome.commit().byte_count(), bytes.len() as u64);
    assert!(outcome.commit().target_owned());
    assert!(!outcome.commit().identity().stable_value().is_empty());
    assert_eq!(fs::read(&target).expect("target"), bytes);

    let Some(mut second) = begin_or_supported_skip(&target, 3) else {
        unreachable!("filesystem support cannot disappear within one test")
    };
    second.write_all(b"new").expect("second stage");
    let error = second.commit(&NeverCancelled).expect_err("no overwrite");
    assert_eq!(error.code(), NativePublicationErrorCode::TargetExists);
    assert_eq!(fs::read(&target).expect("preserved target"), bytes);

    fs::remove_file(&target).expect("target cleanup");
    fs::remove_dir(&directory).expect("directory cleanup");
}

#[cfg(target_os = "linux")]
#[test]
fn linux_procfd_link_publication_succeeds_without_cap_dac_read_search() {
    const CAP_DAC_READ_SEARCH: u64 = 1 << 2;
    assert_eq!(
        linux_effective_capabilities() & CAP_DAC_READ_SEARCH,
        0,
        "this regression must exercise the ordinary-user linkat path"
    );

    let directory = unique_directory("linux-ordinary-procfd");
    fs::create_dir(&directory).expect("directory");
    let target = directory.join("solutions.csa");
    let exact_bytes = b"ordinary user procfd exact inode";
    let mut stage = AtomicFileStage::begin(&target, exact_bytes.len() as u64, &NeverCancelled)
        .expect("procfs and O_TMPFILE support");
    stage.write_all(exact_bytes).expect("staged bytes");
    let outcome = stage.commit(&NeverCancelled).expect("capability-free link");

    assert!(matches!(outcome, NativePublicationOutcome::Committed(_)));
    assert_eq!(fs::read(&target).expect("published bytes"), exact_bytes);
    assert_eq!(fs::read_dir(&directory).expect("directory").count(), 1);
    fs::remove_file(&target).expect("target cleanup");
    fs::remove_dir(&directory).expect("directory cleanup");
}

#[cfg(target_os = "linux")]
#[test]
fn linux_commit_permission_failure_releases_unnamed_stage_without_residue() {
    const CAP_DAC_OVERRIDE: u64 = 1 << 1;
    const CAP_DAC_READ_SEARCH: u64 = 1 << 2;
    assert_eq!(
        linux_effective_capabilities() & (CAP_DAC_OVERRIDE | CAP_DAC_READ_SEARCH),
        0,
        "this regression requires ordinary directory permission checks"
    );

    let directory = unique_directory("linux-link-permission");
    fs::create_dir(&directory).expect("directory");
    let target = directory.join("solutions.csa");
    let mut stage =
        AtomicFileStage::begin(&target, 3, &NeverCancelled).expect("procfs and O_TMPFILE support");
    stage.write_all(b"new").expect("staged bytes");
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o500))
        .expect("remove directory write permission");
    let result = stage.commit(&NeverCancelled);
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
        .expect("restore directory permission");
    let error = match result {
        Err(error) => error,
        Ok(_) => {
            fs::remove_file(&target).expect("unexpected target cleanup");
            fs::remove_dir(&directory).expect("unexpected directory cleanup");
            panic!("link permission failure was expected");
        }
    };

    assert_eq!(
        error.code(),
        NativePublicationErrorCode::UnsupportedFilesystem
    );
    assert!(matches!(
        error.raw_os_error(),
        Some(libc::EACCES) | Some(libc::EPERM)
    ));
    assert!(error.residue().is_none());
    assert!(!target.exists());
    assert_eq!(fs::read_dir(&directory).expect("directory").count(), 0);
    fs::remove_dir(&directory).expect("directory cleanup");
}

#[test]
fn target_created_after_staging_is_preserved_and_staging_has_no_residue() {
    let directory = unique_directory("target-race");
    fs::create_dir(&directory).expect("directory");
    let target = directory.join("solutions.csa");
    let Some(mut stage) = begin_or_supported_skip(&target, 3) else {
        fs::remove_dir(&directory).expect("unsupported cleanup");
        return;
    };
    stage.write_all(b"new").expect("staged bytes");
    fs::write(&target, b"authoritative").expect("racing target");
    let error = stage.commit(&NeverCancelled).expect_err("no replacement");
    assert_eq!(error.code(), NativePublicationErrorCode::TargetExists);
    assert_eq!(
        fs::read(&target).expect("preserved target"),
        b"authoritative"
    );
    assert_eq!(
        fs::read_dir(&directory).expect("directory").count(),
        1,
        "only the authoritative target may remain"
    );

    fs::remove_file(&target).expect("target cleanup");
    fs::remove_dir(&directory).expect("directory cleanup");
}

#[test]
fn existing_directory_leaf_is_preserved_without_staging_residue() {
    let directory = unique_directory("directory-leaf");
    let target = directory.join("solutions.csa");
    fs::create_dir_all(&target).expect("directory target");
    let Some(mut stage) = begin_or_supported_skip(&target, 3) else {
        fs::remove_dir_all(&directory).expect("unsupported cleanup");
        return;
    };
    stage.write_all(b"new").expect("staged bytes");
    let error = stage
        .commit(&NeverCancelled)
        .expect_err("directory preserved");
    assert_eq!(error.code(), NativePublicationErrorCode::TargetExists);
    assert!(target.is_dir());
    assert_eq!(fs::read_dir(&directory).expect("directory").count(), 1);
    fs::remove_dir(&target).expect("target cleanup");
    fs::remove_dir(&directory).expect("directory cleanup");
}

#[cfg(any(unix, windows))]
#[test]
fn existing_symlink_leaf_is_preserved_without_following_it() {
    let directory = unique_directory("symlink-leaf");
    fs::create_dir(&directory).expect("directory");
    let authoritative = directory.join("authoritative.csa");
    let target = directory.join("solutions.csa");
    fs::write(&authoritative, b"authoritative").expect("authoritative bytes");
    if let Err(error) = create_file_symlink(&authoritative, &target) {
        #[cfg(windows)]
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            fs::remove_file(&authoritative).expect("skip cleanup");
            fs::remove_dir(&directory).expect("skip cleanup");
            return;
        }
        panic!("symlink: {error}");
    }
    let Some(mut stage) = begin_or_supported_skip(&target, 3) else {
        fs::remove_file(&target).expect("symlink cleanup");
        fs::remove_file(&authoritative).expect("file cleanup");
        fs::remove_dir(&directory).expect("unsupported cleanup");
        return;
    };
    stage.write_all(b"new").expect("staged bytes");
    let error = stage
        .commit(&NeverCancelled)
        .expect_err("symlink preserved");
    assert_eq!(error.code(), NativePublicationErrorCode::TargetExists);
    assert!(fs::symlink_metadata(&target)
        .expect("target metadata")
        .file_type()
        .is_symlink());
    assert_eq!(
        fs::read(&authoritative).expect("preserved"),
        b"authoritative"
    );
    assert_eq!(fs::read_dir(&directory).expect("directory").count(), 2);
    fs::remove_file(&target).expect("symlink cleanup");
    fs::remove_file(&authoritative).expect("file cleanup");
    fs::remove_dir(&directory).expect("directory cleanup");
}

#[cfg(unix)]
fn create_file_symlink(source: &Path, target: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, target)
}

#[cfg(windows)]
fn create_file_symlink(source: &Path, target: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_file(source, target)
}

#[cfg(any(unix, windows))]
#[test]
fn symlink_or_reparse_parent_is_rejected_before_staging() {
    let root = unique_directory("reparse-parent");
    let actual = root.join("actual");
    let linked = root.join("linked");
    fs::create_dir_all(&actual).expect("actual parent");
    if let Err(error) = create_directory_symlink(&actual, &linked) {
        #[cfg(windows)]
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            fs::remove_dir(&actual).expect("skip cleanup");
            fs::remove_dir(&root).expect("skip cleanup");
            return;
        }
        panic!("directory symlink: {error}");
    }
    let error = AtomicFileStage::begin(&linked.join("solutions.csa"), 0, &NeverCancelled)
        .expect_err("reparse parent rejected");
    #[cfg(windows)]
    assert_eq!(
        error.code(),
        NativePublicationErrorCode::ReparsePointRejected
    );
    #[cfg(unix)]
    assert!(matches!(
        error.code(),
        NativePublicationErrorCode::ReparsePointRejected
            | NativePublicationErrorCode::ParentNotDirectory
    ));
    assert!(!actual.join("solutions.csa").exists());
    remove_directory_symlink(&linked).expect("symlink cleanup");
    fs::remove_dir(&actual).expect("actual cleanup");
    fs::remove_dir(&root).expect("root cleanup");
}

#[cfg(unix)]
fn create_directory_symlink(source: &Path, target: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, target)
}

#[cfg(windows)]
fn create_directory_symlink(source: &Path, target: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(source, target)
}

#[cfg(unix)]
fn remove_directory_symlink(path: &Path) -> std::io::Result<()> {
    fs::remove_file(path)
}

#[cfg(windows)]
fn remove_directory_symlink(path: &Path) -> std::io::Result<()> {
    fs::remove_dir(path)
}

#[test]
fn held_parent_capability_cannot_be_redirected_by_a_parent_path_swap() {
    let root = unique_directory("parent-swap");
    let original = root.join("original");
    let moved = root.join("moved");
    fs::create_dir_all(&original).expect("original parent");
    let target = original.join("solutions.csa");
    let Some(mut stage) = begin_or_supported_skip(&target, 5) else {
        fs::remove_dir_all(&root).expect("unsupported cleanup");
        return;
    };
    stage.write_all(b"owned").expect("staged bytes");

    #[cfg(windows)]
    assert!(
        fs::rename(&original, &moved).is_err(),
        "a retained Windows directory capability must deny rename sharing"
    );
    #[cfg(not(windows))]
    {
        fs::rename(&original, &moved).expect("parent rename while capability held");
        fs::create_dir(&original).expect("replacement parent");
    }

    let outcome = stage.commit(&NeverCancelled).expect("capability commit");
    assert!(matches!(outcome, NativePublicationOutcome::Committed(_)));

    #[cfg(windows)]
    {
        assert_eq!(
            fs::read(original.join("solutions.csa")).expect("pinned parent target"),
            b"owned"
        );
        assert!(!moved.exists());
        fs::remove_file(original.join("solutions.csa")).expect("target cleanup");
        fs::remove_dir(&original).expect("original cleanup");
        fs::remove_dir(&root).expect("root cleanup");
    }

    #[cfg(not(windows))]
    {
        assert_eq!(
            fs::read(moved.join("solutions.csa")).expect("held parent target"),
            b"owned"
        );
        assert!(!original.join("solutions.csa").exists());

        fs::remove_file(moved.join("solutions.csa")).expect("target cleanup");
        fs::remove_dir(&moved).expect("moved cleanup");
        fs::remove_dir(&original).expect("replacement cleanup");
        fs::remove_dir(&root).expect("root cleanup");
    }
}

#[cfg(windows)]
#[test]
fn windows_retained_handles_deny_competing_mutation_delete_and_retarget() {
    const CHILD_MODE: &str = "CLEARRA_NATIVE_PUBLICATION_SHARE_ATTACK_CHILD";
    const STAGING_PATH: &str = "CLEARRA_NATIVE_PUBLICATION_SHARE_ATTACK_STAGE";
    const PARENT_PATH: &str = "CLEARRA_NATIVE_PUBLICATION_SHARE_ATTACK_PARENT";
    const MOVED_PATH: &str = "CLEARRA_NATIVE_PUBLICATION_SHARE_ATTACK_MOVED";

    if std::env::var_os(CHILD_MODE).is_some() {
        let staging = PathBuf::from(std::env::var_os(STAGING_PATH).expect("staging path"));
        let parent = PathBuf::from(std::env::var_os(PARENT_PATH).expect("parent path"));
        let moved = PathBuf::from(std::env::var_os(MOVED_PATH).expect("moved path"));
        let renamed_stage = staging.with_extension("attacker-renamed");

        assert!(
            fs::File::open(&staging).is_err(),
            "exclusive staged handle must deny a competing open"
        );
        assert!(
            fs::OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&staging)
                .is_err(),
            "exclusive staged handle must deny a competing writer"
        );
        assert!(
            fs::remove_file(&staging).is_err(),
            "exclusive staged handle must deny pathname deletion"
        );
        assert!(
            fs::rename(&staging, &renamed_stage).is_err(),
            "exclusive staged handle must deny pathname rename"
        );
        assert!(
            fs::rename(&parent, &moved).is_err(),
            "retained parent capability must deny ancestor rename sharing"
        );
        assert!(!renamed_stage.exists());
        assert!(!moved.exists());
        return;
    }

    let root = unique_directory("windows-share-hardening");
    let parent = root.join("parent");
    let moved = root.join("attacker-moved");
    fs::create_dir_all(&parent).expect("parent");
    let target = parent.join("solutions.csa");
    let exact_bytes = b"solution-set-artifact.v1 retained-handle exact bytes";
    let mut stage = AtomicFileStage::begin(&target, exact_bytes.len() as u64, &NeverCancelled)
        .expect("Windows stage");
    let (staging_leaf, staged_identity) = match stage.precommit_crash_recovery() {
        Some(PreCommitCrashRecovery::DeleteOnCloseHandle {
            staging_leaf,
            file_identity,
        }) => (staging_leaf, file_identity),
        recovery => panic!("unexpected Windows recovery contract: {recovery:?}"),
    };
    stage.write_all(exact_bytes).expect("exact staged bytes");
    let staging_path = parent.join(staging_leaf);

    let status = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .arg("--exact")
        .arg("windows_retained_handles_deny_competing_mutation_delete_and_retarget")
        .arg("--nocapture")
        .env(CHILD_MODE, "1")
        .env(STAGING_PATH, &staging_path)
        .env(PARENT_PATH, &parent)
        .env(MOVED_PATH, &moved)
        .status()
        .expect("adversarial child");
    assert!(status.success(), "all competing operations must be denied");

    let outcome = stage.commit(&NeverCancelled).expect("commit exact handle");
    assert!(matches!(outcome, NativePublicationOutcome::Committed(_)));
    assert_eq!(outcome.commit().identity(), &staged_identity);
    assert_eq!(outcome.commit().byte_count(), exact_bytes.len() as u64);
    assert_eq!(
        fs::read(&target).expect("published exact bytes"),
        exact_bytes
    );
    assert!(!staging_path.exists());
    assert!(!moved.exists());

    fs::remove_file(&target).expect("target cleanup");
    fs::remove_dir(&parent).expect("parent cleanup");
    fs::remove_dir(&root).expect("root cleanup");
}

struct CancelAt(PublicationCheckpoint);

impl PublicationControl for CancelAt {
    fn cancelled_at(&self, checkpoint: PublicationCheckpoint) -> bool {
        checkpoint == self.0
    }
}

#[test]
fn deterministic_precommit_cancellation_leaves_no_target_or_staging_leaf() {
    let directory = unique_directory("cancel");
    fs::create_dir(&directory).expect("directory");
    let target = directory.join("solutions.csa");
    assert_eq!(
        AtomicFileStage::begin(&target, 3, &CancelAt(PublicationCheckpoint::BeforeStage))
            .expect_err("cancel before stage")
            .code(),
        NativePublicationErrorCode::Cancelled
    );
    assert_eq!(fs::read_dir(&directory).expect("directory").count(), 0);

    let Some(mut stage) = begin_or_supported_skip(&target, 3) else {
        fs::remove_dir(&directory).expect("unsupported cleanup");
        return;
    };
    stage.write_all(b"new").expect("staged bytes");
    assert_eq!(
        stage
            .commit(&CancelAt(PublicationCheckpoint::BeforeCommitBarrier))
            .expect_err("cancel at barrier")
            .code(),
        NativePublicationErrorCode::Cancelled
    );
    assert!(!target.exists());
    assert_eq!(fs::read_dir(&directory).expect("directory").count(), 0);
    fs::remove_dir(&directory).expect("directory cleanup");
}

#[derive(Default)]
struct RecordingControl {
    checkpoints: RefCell<Vec<PublicationCheckpoint>>,
}

impl PublicationControl for RecordingControl {
    fn cancelled_at(&self, checkpoint: PublicationCheckpoint) -> bool {
        self.checkpoints.borrow_mut().push(checkpoint);
        false
    }
}

#[test]
fn commit_barrier_is_the_last_cancellation_checkpoint_and_commit_wins_after_it() {
    let directory = unique_directory("commit-wins");
    fs::create_dir(&directory).expect("directory");
    let target = directory.join("solutions.csa");
    let control = RecordingControl::default();
    let stage = match AtomicFileStage::begin(&target, 0, &control) {
        Ok(stage) => stage,
        Err(error)
            if cfg!(target_os = "linux")
                && error.code() == NativePublicationErrorCode::UnsupportedFilesystem =>
        {
            fs::remove_dir(&directory).expect("unsupported cleanup");
            return;
        }
        Err(error)
            if cfg!(not(any(target_os = "linux", windows)))
                && error.code() == NativePublicationErrorCode::UnsupportedPlatform =>
        {
            fs::remove_dir(&directory).expect("unsupported cleanup");
            return;
        }
        Err(error) => panic!("stage: {error:?}"),
    };
    let outcome = stage.commit(&control).expect("empty commit");
    assert!(matches!(outcome, NativePublicationOutcome::Committed(_)));
    assert_eq!(
        control.checkpoints.into_inner(),
        vec![
            PublicationCheckpoint::BeforeStage,
            PublicationCheckpoint::BeforeCommitBarrier,
        ]
    );
    assert_eq!(
        AtomicFileStage::postcommit_crash_recovery(),
        PostCommitCrashRecovery::TargetMayBeOwnedNeverPathDelete
    );

    fs::remove_file(&target).expect("target cleanup");
    fs::remove_dir(&directory).expect("directory cleanup");
}

#[cfg(windows)]
#[test]
fn windows_rejects_ads_like_leaf_without_creating_any_file() {
    let directory = unique_directory("ads");
    fs::create_dir(&directory).expect("directory");
    let target = directory.join("solutions.csa:stream");
    let error = AtomicFileStage::begin(&target, 0, &NeverCancelled).expect_err("ADS rejected");
    assert_eq!(error.code(), NativePublicationErrorCode::InvalidTarget);
    assert_eq!(fs::read_dir(&directory).expect("directory").count(), 0);
    fs::remove_dir(&directory).expect("directory cleanup");
}

#[cfg(windows)]
#[test]
fn windows_process_exit_closes_delete_on_close_staging_without_residue() {
    const CHILD_ENV: &str = "CLEARra_NATIVE_PUBLICATION_CRASH_CHILD";
    if let Some(target) = std::env::var_os(CHILD_ENV) {
        let mut stage =
            AtomicFileStage::begin(Path::new(&target), 5, &NeverCancelled).expect("child stage");
        stage.write_all(b"stage").expect("child write");
        std::process::abort();
    }

    let directory = unique_directory("process-crash");
    fs::create_dir(&directory).expect("directory");
    let target = directory.join("solutions.csa");
    let status = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .arg("--exact")
        .arg("windows_process_exit_closes_delete_on_close_staging_without_residue")
        .arg("--nocapture")
        .env(CHILD_ENV, &target)
        .status()
        .expect("crash child");
    assert!(!status.success(), "child must terminate without unwinding");
    assert!(!target.exists());
    assert_eq!(fs::read_dir(&directory).expect("directory").count(), 0);
    fs::remove_dir(&directory).expect("directory cleanup");
}

#[cfg(target_os = "linux")]
#[test]
fn linux_process_exit_releases_unnamed_staging_without_residue() {
    const CHILD_ENV: &str = "CLEARRA_NATIVE_PUBLICATION_LINUX_CRASH_CHILD";
    if let Some(target) = std::env::var_os(CHILD_ENV) {
        let mut stage = match AtomicFileStage::begin(Path::new(&target), 5, &NeverCancelled) {
            Ok(stage) => stage,
            Err(error) if error.code() == NativePublicationErrorCode::UnsupportedFilesystem => {
                std::process::exit(77)
            }
            Err(error) => panic!("child stage: {error:?}"),
        };
        assert_eq!(
            stage.precommit_crash_recovery(),
            Some(PreCommitCrashRecovery::UnnamedFileReleasedOnLastHandleClose)
        );
        stage.write_all(b"stage").expect("child write");
        std::process::exit(91);
    }

    let directory = unique_directory("linux-process-crash");
    fs::create_dir(&directory).expect("directory");
    let target = directory.join("solutions.csa");
    let status = std::process::Command::new(std::env::current_exe().expect("test executable"))
        .arg("--exact")
        .arg("linux_process_exit_releases_unnamed_staging_without_residue")
        .arg("--nocapture")
        .env(CHILD_ENV, &target)
        .status()
        .expect("crash child");
    if status.code() == Some(77) {
        fs::remove_dir(&directory).expect("unsupported cleanup");
        return;
    }
    assert_eq!(status.code(), Some(91), "child must exit without unwinding");
    assert!(!target.exists());
    assert_eq!(fs::read_dir(&directory).expect("directory").count(), 0);
    fs::remove_dir(&directory).expect("directory cleanup");
}
