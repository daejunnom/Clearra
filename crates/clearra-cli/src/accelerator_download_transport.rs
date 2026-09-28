//! Bounded, explicit public GitHub Release transport for accelerator assets.
//! No search request calls this module. Curl is invoked without a shell and
//! with `-q` first so user configuration and credential files are never read.

use std::{
    io::Read,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    sync::mpsc::{sync_channel, RecvTimeoutError},
    thread,
    time::Duration,
};

const RELEASE_PREFIX: &str = "https://github.com/daejunnom/Clearra/releases/download/";

pub(crate) fn stream_release_asset(
    url: &str,
    exact_bytes: u64,
    cancelled: &AtomicBool,
    sink: &mut dyn FnMut(&[u8]) -> Result<(), &'static str>,
) -> Result<(), &'static str> {
    if !url.starts_with(RELEASE_PREFIX) || exact_bytes == 0 || exact_bytes > 64 * 1024 * 1024 {
        return Err("accelerator: release transport authority rejected");
    }
    if cancelled.load(Ordering::Acquire) {
        return Err("accelerator: download cancelled");
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
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err("accelerator: download stream unavailable");
    };
    let result = thread::scope(|scope| {
        enum ReadChunk {
            Data(Vec<u8>),
            End,
            Error,
        }
        // Bounded buffering lets the caller observe cancellation even while
        // curl is blocked on a slow response. Killing this exact child closes
        // the reader pipe; no other process or download is affected.
        let (sender, receiver) = sync_channel::<ReadChunk>(4);
        let reader = scope.spawn(move || {
            let mut stdout = stdout;
            let mut buffer = [0_u8; 65_536];
            loop {
                let message = match stdout.read(&mut buffer) {
                    Ok(0) => ReadChunk::End,
                    Ok(count) => ReadChunk::Data(buffer[..count].to_vec()),
                    Err(_) => ReadChunk::Error,
                };
                let terminal = !matches!(&message, ReadChunk::Data(_));
                if sender.send(message).is_err() || terminal {
                    break;
                }
            }
        });
        let mut total = 0_u64;
        let result = loop {
            if cancelled.load(Ordering::Acquire) {
                break Err("accelerator: download cancelled");
            }
            match receiver.recv_timeout(Duration::from_millis(50)) {
                Ok(ReadChunk::Data(bytes)) => {
                    let Some(next_total) = total.checked_add(bytes.len() as u64) else {
                        break Err("accelerator: response size overflow");
                    };
                    if next_total > exact_bytes {
                        break Err("accelerator: response exceeds signed byte length");
                    }
                    if let Err(error) = sink(&bytes) {
                        break Err(error);
                    }
                    total = next_total;
                }
                Ok(ReadChunk::End) => {
                    break if total == exact_bytes {
                        Ok(())
                    } else {
                        Err("accelerator: response is shorter than signed byte length")
                    };
                }
                Ok(ReadChunk::Error) => break Err("accelerator: download interrupted"),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    break Err("accelerator: download interrupted");
                }
            }
        };
        if result.is_err() {
            let _ = child.kill();
        }
        drop(receiver);
        if reader.join().is_err() {
            return Err("accelerator: download reader failed");
        }
        result
    });
    if result.is_err() {
        let _ = child.kill();
    }
    let successful = child.wait().map(|status| status.success()).unwrap_or(false);
    result?;
    if cancelled.load(Ordering::Acquire) {
        return Err("accelerator: download cancelled");
    }
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
        let cancelled = AtomicBool::new(false);
        assert!(
            stream_release_asset("https://example.com/asset", 1, &cancelled, &mut sink).is_err()
        );
        assert!(stream_release_asset(RELEASE_PREFIX, 0, &cancelled, &mut sink).is_err());
        cancelled.store(true, Ordering::Release);
        assert_eq!(
            stream_release_asset(RELEASE_PREFIX, 1, &cancelled, &mut sink),
            Err("accelerator: download cancelled")
        );
    }
}
