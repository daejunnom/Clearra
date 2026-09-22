//! Bounded, explicit public GitHub Release transport for accelerator assets.
//! No search request calls this module. Curl is invoked without a shell and
//! with `-q` first so user configuration and credential files are never read.

use std::{
    io::Read,
    process::{Command, Stdio},
};

const RELEASE_PREFIX: &str = "https://github.com/daejunnom/Clearra/releases/download/";

pub(crate) fn stream_release_asset(
    url: &str,
    exact_bytes: u64,
    sink: &mut dyn FnMut(&[u8]) -> Result<(), &'static str>,
) -> Result<(), &'static str> {
    if !url.starts_with(RELEASE_PREFIX) || exact_bytes == 0 || exact_bytes > 64 * 1024 * 1024 {
        return Err("accelerator: release transport authority rejected");
    }
    let mut command = Command::new("curl");
    command
        .arg("-q")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--no-buffer",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--connect-timeout",
            "30",
            "--max-time",
            "1800",
            "--max-redirs",
            "5",
            "--speed-limit",
            "1",
            "--speed-time",
            "60",
            "--max-filesize",
        ])
        .arg(exact_bytes.to_string())
        .arg("--url")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "accelerator: curl is required for explicit downloads")?;
    let result = (|| {
        let mut stdout = child
            .stdout
            .take()
            .ok_or("accelerator: download stream unavailable")?;
        let mut total = 0_u64;
        let mut buffer = [0_u8; 65_536];
        loop {
            let count = stdout
                .read(&mut buffer)
                .map_err(|_| "accelerator: download interrupted")?;
            if count == 0 {
                break;
            }
            total = total
                .checked_add(count as u64)
                .ok_or("accelerator: response size overflow")?;
            if total > exact_bytes {
                return Err("accelerator: response exceeds signed byte length");
            }
            sink(&buffer[..count])?;
        }
        if total != exact_bytes {
            return Err("accelerator: response is shorter than signed byte length");
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = child.kill();
    }
    let successful = child.wait().map(|status| status.success()).unwrap_or(false);
    result?;
    if !successful {
        return Err("accelerator: release download failed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_rejects_non_release_origins_before_process_start() {
        let mut sink = |_bytes: &[u8]| Ok(());
        assert!(stream_release_asset("https://example.com/asset", 1, &mut sink).is_err());
        assert!(stream_release_asset(RELEASE_PREFIX, 0, &mut sink).is_err());
    }
}
