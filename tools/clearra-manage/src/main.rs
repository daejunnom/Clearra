mod policy;
mod runtime;
mod storage;
mod wsl;

use serde_json::json;
use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub struct Error {
    kind: &'static str,
    message: String,
}

impl Error {
    pub fn policy(message: impl Into<String>) -> Self {
        Self {
            kind: "policy",
            message: message.into(),
        }
    }
    pub fn storage(message: impl Into<String>) -> Self {
        Self {
            kind: storage::PATH_ERROR,
            message: message.into(),
        }
    }
    pub fn runtime(message: impl Into<String>) -> Self {
        Self {
            kind: "runtime",
            message: message.into(),
        }
    }
    pub fn usage(message: impl Into<String>) -> Self {
        Self {
            kind: "usage",
            message: message.into(),
        }
    }
    pub fn io(context: &str, error: std::io::Error) -> Self {
        Self::runtime(format!("{context}: {error}"))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for Error {}

fn main() {
    let code = match execute(std::env::args_os().skip(1).collect()) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            if error.kind == "usage" {
                2
            } else {
                70
            }
        }
    };
    std::process::exit(code);
}

fn execute(mut arguments: Vec<OsString>) -> Result<i32> {
    if arguments
        .iter()
        .any(|value| value == "--help" || value == "-h")
        || arguments.is_empty()
    {
        print_help();
        return Ok(0);
    }
    let explicit_root = take_option(&mut arguments, "--root").map(PathBuf::from);
    let repository = policy::discover_root(explicit_root)?;
    let policy = policy::Policy::load(&repository)?;
    let domain = take_front(&mut arguments, "domain")?;
    match domain.to_string_lossy().as_ref() {
        "storage" => storage_command(&repository, &policy, arguments),
        "runtime" => runtime_command(&repository, &policy, arguments),
        other => Err(Error::usage(format!(
            "unknown domain {other}; the Rust v2 manager intentionally supports only storage and runtime"
        ))),
    }
}

fn storage_command(
    repository: &std::path::Path,
    policy: &policy::Policy,
    mut arguments: Vec<OsString>,
) -> Result<i32> {
    let action = take_front(&mut arguments, "storage action")?;
    match action.to_string_lossy().as_ref() {
        "audit" => {
            require_empty(&arguments)?;
            let value = storage::audit(repository, policy)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&value)
                    .map_err(|error| Error::runtime(error.to_string()))?
            );
            if value["valid"].as_bool() == Some(true) {
                Ok(0)
            } else {
                Ok(1)
            }
        }
        "verify" => {
            let path = PathBuf::from(required_option(&mut arguments, "--path")?);
            let force = take_flag(&mut arguments, "--force-unmanaged-output");
            let reason = take_option(&mut arguments, "--force-reason")
                .map(|value| value.to_string_lossy().into_owned());
            require_empty(&arguments)?;
            let value = storage::verify(repository, policy, &path, force, reason.as_deref())?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_id": "clearra.storage-verification.v2",
                    "path": value.path,
                    "root_id": value.root_id,
                    "lifecycle": value.lifecycle,
                }))
                .map_err(|error| Error::runtime(error.to_string()))?
            );
            Ok(0)
        }
        "run" => run_command(repository, policy, arguments),
        other => Err(Error::usage(format!("unknown storage action: {other}"))),
    }
}

fn runtime_command(
    repository: &std::path::Path,
    policy: &policy::Policy,
    mut arguments: Vec<OsString>,
) -> Result<i32> {
    let action = take_front(&mut arguments, "runtime action")?;
    match action.to_string_lossy().as_ref() {
        "audit" => {
            require_empty(&arguments)?;
            let value = runtime::audit(repository, policy)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&value)
                    .map_err(|error| Error::runtime(error.to_string()))?
            );
            Ok(0)
        }
        "run" => run_command(repository, policy, arguments),
        "wsl" => wsl_command(repository, policy, arguments),
        other => Err(Error::usage(format!("unknown runtime action: {other}"))),
    }
}

fn run_command(
    repository: &std::path::Path,
    policy: &policy::Policy,
    mut arguments: Vec<OsString>,
) -> Result<i32> {
    let separator = arguments
        .iter()
        .position(|value| value == "--")
        .ok_or_else(|| Error::usage("runtime run requires -- before the command"))?;
    let command = arguments.split_off(separator + 1);
    arguments.pop();
    let producer = required_option(&mut arguments, "--producer")?
        .to_string_lossy()
        .into_owned();
    let profile = required_option(&mut arguments, "--profile")?
        .to_string_lossy()
        .into_owned();
    let timeout_seconds = take_timeout(&mut arguments)?;
    let cancel_on_stdin_eof = take_flag(&mut arguments, "--cancel-on-stdin-eof");
    require_empty(&arguments)?;
    let outcome = runtime::run(
        repository,
        policy,
        runtime::RunOptions {
            producer,
            profile,
            timeout_seconds,
            command,
            keep_stdin_open: false,
            cancel_on_stdin_eof,
            extra_env: Default::default(),
            echo: true,
        },
    )?;
    eprintln!("clearra_runtime_receipt={}", outcome.receipt.display());
    if let Some(code) = &outcome.error_code {
        eprintln!(
            "{code}: supervised process ended with reason={}",
            outcome.reason
        );
    }
    Ok(outcome.return_code)
}

fn wsl_command(
    repository: &std::path::Path,
    policy: &policy::Policy,
    mut arguments: Vec<OsString>,
) -> Result<i32> {
    let action = take_front(&mut arguments, "WSL action")?;
    let action_text = action.to_string_lossy();
    if action_text == "verify" {
        let distribution = take_option(&mut arguments, "--distribution");
        require_empty(&arguments)?;
        let outcome = wsl::run_entry(
            repository,
            policy,
            distribution.as_deref(),
            "verify",
            &[],
            None,
        )?;
        eprintln!("clearra_runtime_receipt={}", outcome.receipt.display());
        return Ok(outcome.return_code);
    }
    if action_text != "run" {
        return Err(Error::usage("Rust v2 supports runtime wsl verify and run"));
    }
    let separator = arguments.iter().position(|value| value == "--");
    let guest_arguments = separator
        .map(|index| arguments.split_off(index + 1))
        .unwrap_or_default();
    if separator.is_some() {
        arguments.pop();
    }
    let entry = required_option(&mut arguments, "--entry")?
        .to_string_lossy()
        .into_owned();
    let distribution = take_option(&mut arguments, "--distribution");
    let timeout_seconds = take_timeout(&mut arguments)?;
    require_empty(&arguments)?;
    let outcome = wsl::run_entry(
        repository,
        policy,
        distribution.as_deref(),
        &entry,
        &guest_arguments,
        timeout_seconds,
    )?;
    eprintln!("clearra_runtime_receipt={}", outcome.receipt.display());
    if let Some(code) = &outcome.error_code {
        eprintln!(
            "{code}: supervised WSL process ended with reason={}",
            outcome.reason
        );
    }
    Ok(outcome.return_code)
}

fn take_front(arguments: &mut Vec<OsString>, label: &str) -> Result<OsString> {
    if arguments.is_empty() {
        Err(Error::usage(format!("missing {label}")))
    } else {
        Ok(arguments.remove(0))
    }
}

fn take_option(arguments: &mut Vec<OsString>, name: &str) -> Option<OsString> {
    let index = arguments.iter().position(|value| value == name)?;
    if index + 1 >= arguments.len() {
        return None;
    }
    arguments.remove(index);
    Some(arguments.remove(index))
}

fn required_option(arguments: &mut Vec<OsString>, name: &str) -> Result<OsString> {
    take_option(arguments, name).ok_or_else(|| Error::usage(format!("missing {name} <value>")))
}

fn take_flag(arguments: &mut Vec<OsString>, name: &str) -> bool {
    if let Some(index) = arguments.iter().position(|value| value == name) {
        arguments.remove(index);
        true
    } else {
        false
    }
}

fn take_timeout(arguments: &mut Vec<OsString>) -> Result<Option<u64>> {
    let unlimited = take_flag(arguments, "--no-timeout");
    let explicit = take_option(arguments, "--timeout");
    if unlimited && explicit.is_some() {
        return Err(Error::usage(
            "--no-timeout and --timeout are mutually exclusive",
        ));
    }
    if unlimited {
        return Ok(Some(0));
    }
    explicit
        .map(|value| {
            value
                .to_string_lossy()
                .parse::<u64>()
                .map_err(|_| Error::usage("--timeout must be an integer; 0 means unlimited"))
        })
        .transpose()
}

fn require_empty(arguments: &[OsString]) -> Result<()> {
    if arguments.is_empty() {
        Ok(())
    } else {
        Err(Error::usage(format!(
            "unexpected argument: {}",
            arguments[0].to_string_lossy()
        )))
    }
}

fn print_help() {
    println!(
        "Clearra Rust management CLI v2\n\n\
         Scope: generated-output paths and process/WSL memory safety.\n\
         Git, file reads, dependency installation, toolchain installation, and package publishing are intentionally outside this CLI.\n\n\
         clearra-manage [--root PATH] storage audit\n\
         clearra-manage [--root PATH] storage verify --path PATH [--force-unmanaged-output --force-reason REASON]\n\
         clearra-manage [--root PATH] runtime audit\n\
         clearra-manage [--root PATH] runtime run --producer ID --profile ID [--timeout SECONDS | --no-timeout] [--cancel-on-stdin-eof] -- COMMAND...\n\
         clearra-manage [--root PATH] runtime wsl verify [--distribution NAME]\n\
         clearra-manage [--root PATH] runtime wsl run [--distribution NAME] [--timeout SECONDS | --no-timeout] --entry ID -- ARGUMENTS..."
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_parser_removes_name_and_value() {
        let mut values = vec![
            OsString::from("--profile"),
            OsString::from("control"),
            OsString::from("tail"),
        ];
        assert_eq!(
            take_option(&mut values, "--profile"),
            Some(OsString::from("control"))
        );
        assert_eq!(values, vec![OsString::from("tail")]);
    }

    #[test]
    fn timeout_parser_preserves_explicit_unlimited_and_finite_choices() {
        assert_eq!(take_timeout(&mut Vec::new()).unwrap(), None);
        for (arguments, expected) in [
            (vec!["--no-timeout"], 0),
            (vec!["--timeout", "0"], 0),
            (vec!["--timeout", "900"], 900),
        ] {
            let mut arguments = arguments.into_iter().map(OsString::from).collect();
            assert_eq!(take_timeout(&mut arguments).unwrap(), Some(expected));
            assert!(arguments.is_empty());
        }
    }

    #[test]
    fn timeout_parser_rejects_conflicting_or_malformed_choices() {
        for arguments in [
            vec!["--no-timeout", "--timeout", "0"],
            vec!["--timeout", "-1"],
            vec!["--timeout", "1.5"],
        ] {
            let mut arguments = arguments.into_iter().map(OsString::from).collect();
            assert!(take_timeout(&mut arguments).is_err());
        }
    }
}
