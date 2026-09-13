//! SRP: bounded public HTTPS metadata and streamed bodies. No publication,
//! deletion, graph traversal or user credential/config file access.
use super::{hex, Artifact, Result, FILES, REPOSITORY};
use serde_json::Value;
use std::{
    io::Read,
    process::{Command, Stdio},
};

pub(super) fn discover() -> Result<(String, Vec<Artifact>)> {
    let metadata = curl_json(&format!(
        "https://huggingface.co/api/datasets/{REPOSITORY}/revision/main"
    ))?;
    let revision = metadata["sha"]
        .as_str()
        .filter(|s| hex(s, 40))
        .ok_or("tablebase: invalid upstream revision")?;
    if metadata["id"] != REPOSITORY || metadata["private"] == true || metadata["gated"] == true {
        return Err("tablebase: upstream is not the requested public dataset");
    }
    let tree = curl_json(&format!("https://huggingface.co/api/datasets/{REPOSITORY}/tree/{revision}?recursive=false&expand=false"))?;
    let entries = tree
        .as_array()
        .filter(|t| !t.is_empty() && t.len() <= 512)
        .ok_or("tablebase: upstream inventory exceeds the bound")?;
    let mut files = Vec::new();
    for path in FILES {
        let matches = entries
            .iter()
            .filter(|e| e["type"] == "file" && e["path"] == path)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return Err("tablebase: profile-specific artifact missing or duplicated");
        }
        let entry = matches[0];
        let size = entry["size"]
            .as_u64()
            .filter(|n| *n > 0 && *n <= 1 << 31)
            .ok_or("tablebase: invalid artifact size")?;
        let digest = entry["lfs"]["oid"]
            .as_str()
            .filter(|s| hex(s, 64))
            .ok_or("tablebase: upstream content identity missing")?;
        if entry["lfs"]["size"] != size {
            return Err("tablebase: upstream lengths disagree");
        }
        files.push(Artifact {
            path,
            size,
            digest: digest.to_owned(),
        });
    }
    Ok((revision.to_owned(), files))
}
fn curl_json(url: &str) -> Result<Value> {
    let mut bytes = Vec::new();
    curl_stream(url, 4 * 1024 * 1024, &mut |chunk| {
        bytes.extend_from_slice(chunk);
        Ok(())
    })?;
    serde_json::from_slice(&bytes).map_err(|_| "tablebase: invalid upstream metadata")
}
pub(super) fn curl_stream(
    url: &str,
    limit: u64,
    sink: &mut dyn FnMut(&[u8]) -> Result<()>,
) -> Result<()> {
    let mut command = public_https_command(url, limit, 1800)?;
    command.arg("--fail");
    let mut child = command
        .spawn()
        .map_err(|_| "tablebase: curl is required for explicit downloads")?;
    let result = (|| {
        let mut stdout = child
            .stdout
            .take()
            .ok_or("tablebase: download stream unavailable")?;
        let mut total = 0_u64;
        let mut buffer = [0_u8; 65_536];
        loop {
            let count = stdout
                .read(&mut buffer)
                .map_err(|_| "tablebase: download interrupted")?;
            if count == 0 {
                break;
            }
            total += count as u64;
            if total > limit {
                return Err("tablebase: response exceeds the advertised size");
            }
            sink(&buffer[..count])?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = child.kill();
    }
    let successful = child.wait().map(|s| s.success()).unwrap_or(false);
    result?;
    if !successful {
        return Err(
            "tablebase: download failed; check connection, upstream availability or rate limit",
        );
    }
    Ok(())
}

fn public_https_command(url: &str, limit: u64, timeout: u32) -> Result<Command> {
    if !url.starts_with("https://huggingface.co/") {
        return Err("tablebase: transport origin rejected");
    }
    let mut command = Command::new("curl");
    // -q MUST be first: never load .curlrc or credentials/config from it.
    command
        .args([
            "-q",
            "--location",
            "--silent",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--connect-timeout",
            "30",
            "--max-time",
        ])
        .arg(timeout.to_string())
        .args([
            "--max-redirs",
            "5",
            "--speed-limit",
            "1",
            "--speed-time",
            "60",
            "--max-filesize",
        ])
        .arg(limit.to_string())
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
    Ok(command)
}

#[cfg(feature = "online-pc4-tablebase")]
pub(super) fn curl_range(
    revision: &str,
    artifact: &Artifact,
    offset: u64,
    length: u64,
) -> Result<super::http_range::HttpReply> {
    use super::http_range::HttpReply;
    if !hex(revision, 40)
        || !FILES.contains(&artifact.path)
        || length == 0
        || length > 65_536
        || offset
            .checked_add(length)
            .is_none_or(|end| end > artifact.size)
    {
        return Err("pc4_online_range_request_invalid");
    }
    let url = format!(
        "https://huggingface.co/datasets/{REPOSITORY}/resolve/{revision}/{}",
        artifact.path
    );
    let mut command = public_https_command(&url, length, 30)?;
    command.args([
        "--range",
        &format!("{}-{}", offset, offset + length - 1),
        "--write-out",
        "\nCLEARRA-PC4-HTTP\n%{http_code}\n%header{content-range}\n",
    ]);
    let mut child = command
        .spawn()
        .map_err(|_| "pc4_online_transport_unavailable")?;
    // Only body plus a tiny status/header receipt enters memory. Curl's own
    // --max-filesize also rejects a server ignoring Range before a full body.
    let cap = length + 256;
    let mut output = Vec::new();
    let result = child
        .stdout
        .take()
        .ok_or("pc4_online_transport_unavailable")
        .and_then(|stdout| {
            stdout
                .take(cap + 1)
                .read_to_end(&mut output)
                .map_err(|_| "pc4_online_transport_interrupted")
        });
    if result.is_err() || output.len() as u64 > cap {
        let _ = child.kill();
    }
    let successful = child.wait().map(|s| s.success()).unwrap_or(false);
    result?;
    if output.len() as u64 > cap {
        return Err("pc4_online_response_too_large");
    }
    let reply = HttpReply::from_curl_output(output)?;
    if !successful && reply.status == 206 {
        return Err("pc4_online_transport_interrupted");
    }
    Ok(reply)
}
