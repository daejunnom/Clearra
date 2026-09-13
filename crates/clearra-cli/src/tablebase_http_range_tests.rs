use super::*;
use std::{cell::Cell, rc::Rc};

fn files() -> Vec<Artifact> {
    super::super::FILES
        .iter()
        .map(|path| Artifact {
            path,
            size: 131_072,
            digest: "1".repeat(64),
        })
        .collect()
}
fn reply(artifact: &Artifact, offset: u64, length: u64) -> HttpReply {
    HttpReply {
        status: 206,
        content_range: content_range(offset, length, artifact.size),
        bytes: (offset..offset + length).map(|i| i as u8).collect(),
    }
}

#[test]
fn tablebase_download_http_windows_reuse_verified_slices_without_cross_artifact_aliasing() {
    let calls = Rc::new(Cell::new(0));
    let count = Rc::clone(&calls);
    let mut reader = OnlineRangeReader::new(files(), move |a, offset, length| {
        count.set(count.get() + 1);
        assert!(length <= 65_536);
        Ok(reply(a, offset, length))
    });
    for offset in [0, 8, 16, 1_024, 3_333, 16_376] {
        assert_eq!(
            reader.read(0, offset, 8).unwrap(),
            (offset..offset + 8).map(|i| i as u8).collect::<Vec<_>>()
        );
    }
    assert_eq!(calls.get(), 1);
    reader.read(1, 8, 8).unwrap();
    assert_eq!(calls.get(), 2, "another file needs its own verified window");
    assert_eq!(reader.read(0, 64, 65_536).unwrap().len(), 65_536);
    assert_eq!(
        calls.get(),
        3,
        "a crossing large range is still one bounded request"
    );
    for (role, offset, length) in [(3, 0, 8), (0, 0, 0), (0, 0, 65_537), (0, u64::MAX, 8)] {
        assert!(reader.read(role, offset, length).is_err());
    }
    assert_eq!(calls.get(), 3, "bad input cannot touch transport");
}

#[test]
fn tablebase_download_http_requires_real_partial_content_before_cache_admission() {
    for (status, expected) in [
        (200, "pc4_online_whole_content_rejected"),
        (429, "pc4_online_rate_limited"),
        (416, "pc4_online_range_unsatisfiable"),
        (503, "pc4_online_range_response_invalid"),
    ] {
        let mut reader = OnlineRangeReader::new(files(), |a, o, n| {
            let mut r = reply(a, o, n);
            r.status = status;
            Ok(r)
        });
        assert_eq!(reader.read(0, 0, 8).unwrap_err(), expected);
        assert!(reader.windows.is_empty());
        assert_eq!(reader.requests, 1, "no implicit retry");
    }
    for wrong_header in [true, false] {
        let mut reader = OnlineRangeReader::new(files(), |a, o, n| {
            let mut r = reply(a, o, n);
            if wrong_header {
                r.content_range = "bytes 0-16383/999999".into();
            } else {
                r.bytes.pop();
            }
            Ok(r)
        });
        assert!(reader.read(0, 0, 8).is_err());
        assert!(reader.windows.is_empty());
    }
}

#[test]
fn tablebase_download_http_limits_reserve_before_io_and_do_not_overread_tail() {
    let mut reader = OnlineRangeReader::new(files(), |a, o, n| Ok(reply(a, o, n)));
    assert_eq!(reader.read(2, 131_071, 1).unwrap(), [255]);
    reader.reserved = 64 * 1024 * 1024;
    assert_eq!(
        reader.read(0, 0, 8).unwrap_err(),
        "pc4_online_transfer_limit"
    );
    assert_eq!(reader.requests, 1);
}

#[test]
fn tablebase_download_http_receipt_never_confuses_binary_body_with_status() {
    let body = b"\xff\x00\nCLEARRA-PC4-HTTP\nnot a header";
    let mut output = body.to_vec();
    output.extend_from_slice(b"\nCLEARRA-PC4-HTTP\n206\nbytes 0-31/100\n");
    let r = HttpReply::from_curl_output(output).unwrap();
    assert_eq!(r.status, 206);
    assert_eq!(r.content_range, "bytes 0-31/100");
    assert_eq!(r.bytes, body);
    for invalid in [
        b"missing".as_slice(),
        b"\nCLEARRA-PC4-HTTP\n206\nrange\nextra\n".as_slice(),
    ] {
        assert!(HttpReply::from_curl_output(invalid.to_vec()).is_err());
    }
}
