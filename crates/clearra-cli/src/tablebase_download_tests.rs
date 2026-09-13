use super::*;

struct Fixture {
    root: PathBuf,
    files: Vec<Artifact>,
    bytes: Vec<Vec<u8>>,
}
impl Fixture {
    fn new() -> Self {
        let mut entropy = [0; 16];
        getrandom::fill(&mut entropy).unwrap();
        let root = std::env::temp_dir().join(format!(
            "clearra-pc4-download-{:x}",
            u128::from_le_bytes(entropy)
        ));
        fs::create_dir(&root).unwrap();
        let header = |magic: &[u8; 8]| {
            let mut b = magic.to_vec();
            b.extend_from_slice(&1_u32.to_le_bytes());
            b.extend_from_slice(&2_u32.to_le_bytes());
            b
        };
        let mut fields = header(b"FHIDIDX1");
        fields.extend_from_slice(&[0; 8]);
        fields.extend_from_slice(&[255; 5]);
        fields.extend_from_slice(&[1, 0, 0]);
        let mut offsets = header(b"GOFFIDX1");
        for n in [0_u32, 12, 24] {
            offsets.extend_from_slice(&n.to_le_bytes());
        }
        let mut graph = vec![0; 12];
        graph.extend_from_slice(&[255; 5]);
        graph.extend_from_slice(&[0; 7]);
        let bytes = vec![fields, offsets, graph];
        let files = FILES
            .into_iter()
            .zip(&bytes)
            .map(|(path, bytes)| Artifact {
                path,
                size: bytes.len() as u64,
                digest: format!("{:x}", Sha256::digest(bytes)),
            })
            .collect();
        Self { root, files, bytes }
    }
    fn install(&self, revision: &str, corrupt: bool) -> Result<Value> {
        install(&self.root, revision, &self.files, |url, limit, sink| {
            let i = FILES.iter().position(|name| url.ends_with(name)).unwrap();
            assert!(url.contains(revision));
            assert_eq!(limit, self.files[i].size);
            let mut bytes = self.bytes[i].clone();
            if corrupt && i == 2 {
                bytes[0] ^= 1;
            }
            for chunk in bytes.chunks(7) {
                sink(chunk)?;
            }
            Ok(())
        })
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = clean_generations(&self.root, None);
        for name in [
            "active.json",
            "active.pending",
            "unrelated.txt",
            "store.lock",
        ] {
            let _ = fs::remove_file(self.root.join(name));
        }
        let _ = fs::remove_dir(&self.root);
    }
}

#[test]
fn tablebase_download_streams_verified_bytes_and_keeps_only_one_active_generation() {
    let f = Fixture::new();
    let first = "a".repeat(40);
    let second = "b".repeat(40);
    assert_eq!(f.install(&first, false).unwrap()["installed"], true);
    assert_eq!(f.install(&second, false).unwrap()["revision"], second);
    let names = fs::read_dir(&f.root)
        .unwrap()
        .flatten()
        .filter(|e| generation_name(&e.file_name().to_string_lossy()))
        .count();
    assert_eq!(names, 1);
    let pointer = active(&f.root).unwrap().unwrap();
    assert_eq!(
        pointer["generation"]["profiles"][3]["reader_contract"],
        "hydra-jstris-180-complete-graph-v1"
    );
}

#[test]
fn tablebase_download_failed_update_preserves_active_generation_and_unrelated_files() {
    let f = Fixture::new();
    let first = "a".repeat(40);
    f.install(&first, false).unwrap();
    File::create_new(f.root.join("unrelated.txt")).unwrap();
    assert!(f.install(&"b".repeat(40), true).is_err());
    assert_eq!(status(&f.root).unwrap()["revision"], first);
    assert!(f.root.join("unrelated.txt").is_file());
    assert_eq!(
        fs::read_dir(&f.root)
            .unwrap()
            .flatten()
            .filter(|e| generation_name(&e.file_name().to_string_lossy()))
            .count(),
        1
    );
}

#[test]
fn tablebase_download_rejects_invalid_pointer_without_following_it() {
    let f = Fixture::new();
    fs::write(
        f.root.join("active.json"),
        json!({ "schema": "clearra.pc4.local-files.v1", "directory": "../../unrelated" })
            .to_string(),
    )
    .unwrap();
    assert!(active(&f.root).is_err());
    assert!(remove_generation(&f.root, "../../unrelated").is_err());
    assert!(parse_and_execute(&["download".into(), "--profile".into(), "srs".into()]).is_err());
    assert!(parse_and_execute(&["download".into(), "--directory".into()]).is_err());
}

#[test]
fn tablebase_download_rechecks_explicit_update_integrity_even_for_same_revision() {
    let f = Fixture::new();
    let revision = "a".repeat(40);
    f.install(&revision, false).unwrap();
    let pointer = active(&f.root).unwrap().unwrap();
    let path = f
        .root
        .join(pointer["directory"].as_str().unwrap())
        .join("graph.bin");
    fs::write(path, [99; 24]).unwrap();
    assert!(verify_installed(&f.root, &pointer, &f.files).is_err());
    f.install(&revision, false).unwrap();
    verify_installed(&f.root, &active(&f.root).unwrap().unwrap(), &f.files).unwrap();
}

#[test]
fn tablebase_download_requires_explicit_profile_and_lists_all_five_without_io() {
    for action in ["check", "download", "status", "remove"] {
        assert!(parse_and_execute(&[action.into()])
            .unwrap_err()
            .contains("--profile is required"));
    }
    for language in [LanguageId::En, LanguageId::Ko, LanguageId::Ja] {
        let help = run(&["--help".into()], language, false);
        for profile in ["srs", "srs-plus", "srs-x", "jstris-180", "no-kick"] {
            assert!(help.stdout().contains(profile));
        }
        assert!(!help.stdout().contains("[--profile"));
    }
    for profile in ["srs", "srs-plus", "srs-x", "no-kick"] {
        assert!(
            parse_and_execute(&["download".into(), "--profile".into(), profile.into()])
                .unwrap_err()
                .contains("not yet qualified")
        );
    }
}

#[cfg(all(feature = "online-pc4-tablebase", feature = "wasm-cpu-runtime"))]
#[test]
fn tablebase_download_native_search_uses_shared_app_and_rejects_cross_profile_reuse() {
    use crate::{args::CliParser, assemble::CliAppRequestAssembler, output::RenderFormat};
    use clearra_app::{AppContext, AppCoreExecutorService, AppServices, AppStatus};
    let f = Fixture::new();
    f.install(&"a".repeat(40), false).unwrap();
    File::create_new(f.root.join("store.lock")).unwrap();
    for profile in ["jstris-180", "srs"] {
        let tokens = [
            "clearra",
            "pc",
            "--lines",
            "4",
            "--queue",
            "I",
            "--fixed",
            "--no-hold",
            "--rule",
            profile,
            "--tablebase",
        ];
        let invocation = CliParser::parse(tokens).unwrap();
        let request =
            CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json)
                .unwrap()
                .request();
        let context = AppContext::new(
            AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
        );
        let result = execute_local_at(&f.root, context, request);
        if profile == "jstris-180" {
            assert_eq!(result.unwrap().status(), AppStatus::Success);
        } else {
            assert!(result.is_err(), "SRS cannot reuse Jstris graph data");
        }
    }
}

#[cfg(all(feature = "online-pc4-tablebase", feature = "wasm-cpu-runtime"))]
#[test]
fn tablebase_download_observed_queue_is_not_silently_relabelled_as_fixed() {
    use crate::{args::CliParser, assemble::CliAppRequestAssembler, output::RenderFormat};
    use clearra_app::{AppContext, AppCoreExecutorService, AppServices};
    let f = Fixture::new();
    f.install(&"a".repeat(40), false).unwrap();
    File::create_new(f.root.join("store.lock")).unwrap();
    let invocation = CliParser::parse([
        "clearra",
        "pc",
        "--lines",
        "4",
        "--queue",
        "I",
        "--no-hold",
        "--rule",
        "jstris-180",
        "--tablebase",
    ])
    .unwrap();
    let request = CliAppRequestAssembler::assemble(invocation.into_command(), RenderFormat::Json)
        .unwrap()
        .request();
    let context = AppContext::new(
        AppServices::default().with_core_executor(AppCoreExecutorService::wasm_cpu()),
    );
    assert_eq!(
        execute_local_at(&f.root, context, request).unwrap_err(),
        "pc4_online_disclosure_required"
    );
}
