use super::*;

#[test]
fn rejects_sensitive_names_before_file_metadata() {
    let error = read_json_file("credentials.json").expect_err("sensitive path");

    assert_eq!(
        error.to_string(),
        "refusing to read sensitive-looking file path 'credentials.json'"
    );
}

#[test]
fn rejects_non_json_paths_before_reading() {
    let error = read_json_file("fixture.txt").expect_err("extension");

    assert_eq!(
        error.to_string(),
        "file path 'fixture.txt' must be a .json file"
    );
}

#[test]
fn redacts_absolute_paths_by_default() {
    let path = std::env::temp_dir().join(format!("clearra-secret-path-{}.txt", unique_suffix()));
    let error = read_json_file(&path).expect_err("extension");
    let message = error.to_string();

    assert!(message.contains(".../"));
    assert!(message.contains("clearra-secret-path-"));
    assert!(!message.contains(&std::env::temp_dir().display().to_string()));
}

#[test]
fn verbose_paths_show_original_path_when_explicitly_enabled() {
    let path = std::env::temp_dir().join(format!("clearra-verbose-path-{}.txt", unique_suffix()));

    let error = with_verbose_paths(true, || read_json_file(&path)).expect_err("extension");

    assert!(error.to_string().contains(&path.display().to_string()));
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos()
}
