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
    for offset in [0, 8, 16, 1_024, 3_333, 4_088] {
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
fn tablebase_download_graph_records_do_not_evict_reused_index_pages_or_overread() {
    let mut reader = OnlineRangeReader::new(files(), |a, offset, length| {
        if a.path == super::super::FILES[2] {
            assert_eq!(length, 12);
        }
        Ok(reply(a, offset, length))
    });
    reader.read(1, 0, 8).unwrap();
    for id in 0..3_000 {
        reader.read(2, id * 12, 12).unwrap();
    }
    assert_eq!(reader.windows.len(), 1);
    assert_eq!(reader.retained, 4_096);
    reader.read(1, 8, 8).unwrap();
    assert_eq!(
        reader.requests, 3_001,
        "graph bytes must not evict a reusable offset page"
    );
    assert_eq!(reader.reserved, 4_096 + 3_000 * 12);
}

#[test]
fn tablebase_download_small_indexes_cache_exact_ranges_without_whole_file_expansion() {
    let mut small = files();
    small[0].size = 32;
    let mut reader = OnlineRangeReader::new(small, |a, offset, length| {
        assert_eq!((offset, length), (0, 16));
        Ok(reply(a, offset, length))
    });
    reader.read(0, 0, 16).unwrap();
    assert_eq!(reader.read(0, 8, 8).unwrap(), (8..16).collect::<Vec<u8>>());
    reader.read(0, 0, 16).unwrap();
    assert_eq!(reader.requests, 1);
    assert_eq!(reader.retained, 16);
    assert_eq!(reader.reserved, 16);
}

#[test]
fn tablebase_download_explicit_batch_validates_before_io_and_excludes_cached_subranges() {
    let calls = Rc::new(std::cell::RefCell::new(Vec::new()));
    let log = Rc::clone(&calls);
    let mut reader = OnlineRangeReader::new(files(), move |a, o, n| {
        log.borrow_mut().push((o, n));
        Ok(reply(a, o, n))
    });
    reader.read_many(2, &[(100, 12)], 1024).unwrap();
    let demands = [(108, 4), (120, 12), (100, 12)];
    let bytes = reader.read_many(2, &demands, 1024).unwrap();
    for ((offset, length), value) in demands.into_iter().zip(bytes) {
        assert_eq!(
            value,
            (offset..offset + length)
                .map(|v| v as u8)
                .collect::<Vec<_>>()
        );
    }
    assert_eq!(*calls.borrow(), [(100, 12), (120, 12)]);
    assert!(reader
        .read_many(2, &[(200, 12), (u64::MAX, 12)], 1024)
        .is_err());
    assert!(reader.read_many(2, &[(200, 12); 513], 1024).is_err());
    assert!(reader.read_many(2, &[(200, 12)], 4097).is_err());
    assert_eq!(calls.borrow().len(), 2);
    reader
        .read_many(2, &[(0, 65_536), (65_536, 12)], 1024)
        .unwrap();
    assert_eq!(&calls.borrow()[2..], [(0, 65_536), (65_536, 12)]);
}

#[test]
fn tablebase_download_external_batch_reserves_every_span_before_parallel_io() {
    let calls = Rc::new(Cell::new(0));
    let count = Rc::clone(&calls);
    let mut reader = OnlineRangeReader::new(files(), move |a, o, n| {
        count.set(count.get() + 1);
        Ok(reply(a, o, n))
    });
    reader
        .reserve_external(&[(2, 100, 12), (2, 120, 24)])
        .unwrap();
    assert_eq!(reader.requests, 2);
    assert_eq!(reader.reserved, 36);
    assert_eq!(calls.get(), 0, "reservation never performs transport");

    let requests = reader.requests;
    let reserved = reader.reserved;
    assert_eq!(
        reader
            .reserve_external(&[(2, 200, 12), (3, 0, 1)])
            .unwrap_err(),
        "pc4_online_artifact_invalid"
    );
    assert_eq!((reader.requests, reader.reserved), (requests, reserved));
    assert_eq!(
        reader.reserve_external(&[(2, 0, 12); 17]).unwrap_err(),
        "pc4_online_batch_invalid"
    );
    assert_eq!((reader.requests, reader.reserved), (requests, reserved));

    reader.reserved = 64 * 1024 * 1024 - 8;
    assert_eq!(
        reader.reserve_external(&[(2, 0, 12)]).unwrap_err(),
        "pc4_online_transfer_limit"
    );
    assert_eq!(reader.requests, requests);
}

#[test]
fn tablebase_download_consumed_graph_prefetches_release_index_cache_capacity() {
    let mut reader = OnlineRangeReader::new(files(), |a, o, n| Ok(reply(a, o, n)));
    reader.read(1, 0, 8).unwrap();
    let demands: Vec<_> = (0..32).map(|i| (i * 4096, 12)).collect();
    reader.read_many(2, &demands, 1024).unwrap();
    for &(offset, length) in &demands {
        reader.read(2, offset, length).unwrap();
    }
    assert_eq!(reader.retained, 4096);
    assert_eq!(reader.windows.len(), 1);
    assert_eq!(reader.requests, 33);
    reader.read(1, 16, 8).unwrap();
    assert_eq!(reader.requests, 33);
}

#[test]
fn tablebase_download_http_requires_real_partial_content_before_cache_admission() {
    for (status, expected) in [
        (200, "pc4_online_whole_content_rejected"),
        (429, "pc4_online_rate_limited"),
        (416, "pc4_online_range_unsatisfiable"),
        (408, "pc4_online_dataset_unavailable"),
        (425, "pc4_online_dataset_unavailable"),
        (500, "pc4_online_dataset_unavailable"),
        (502, "pc4_online_dataset_unavailable"),
        (503, "pc4_online_dataset_unavailable"),
        (504, "pc4_online_dataset_unavailable"),
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
