use super::super::http_range::content_range;
use super::*;
use std::{cell::RefCell, rc::Rc};

type Calls = Rc<RefCell<Vec<(String, u64, u64)>>>;

fn fixture(
    bad_pair: bool,
) -> (
    OnlineRangeReader<impl FnMut(&Artifact, u64, u64) -> Result<HttpReply>>,
    Calls,
) {
    let count = 128;
    let files: Vec<_> = super::super::FILES
        .iter()
        .zip([16 + count * 8, 16 + (count + 1) * 4, count * 12])
        .map(|(path, size)| Artifact {
            path,
            size,
            digest: "1".repeat(64),
        })
        .collect();
    let mut offsets = vec![0_u8; files[1].size as usize];
    for id in 0..=count {
        offsets[(16 + id * 4) as usize..(20 + id * 4) as usize]
            .copy_from_slice(&(id as u32 * 12).to_le_bytes());
    }
    if bad_pair {
        offsets[60..64].fill(0);
    }
    let calls = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&calls);
    let reader = OnlineRangeReader::new(files, move |a, o, n| {
        log.borrow_mut().push((a.path.to_owned(), o, n));
        let bytes = if a.path == super::super::FILES[1] {
            offsets[o as usize..(o + n) as usize].to_vec()
        } else {
            (o..o + n).map(|value| (value % 251) as u8).collect()
        };
        Ok(HttpReply {
            status: 206,
            content_range: content_range(o, n, a.size),
            bytes,
        })
    });
    (reader, calls)
}

#[test]
fn tablebase_download_known_frontier_reuses_two_stages_without_changing_exact_bytes() {
    let (mut serial, serial_calls) = fixture(false);
    let (mut batched, batch_calls) = fixture(false);
    let ids: Vec<_> = (10..42).collect();
    prefetch(&mut batched, 128, 128 * 12, 56, 8, &ids).unwrap();
    for &id in &ids {
        for (role, offset, length) in [(1, 16 + u64::from(id) * 4, 8), (2, u64::from(id) * 12, 12)]
        {
            assert_eq!(
                serial.read(role, offset, length).unwrap(),
                batched.read(role, offset, length).unwrap()
            );
        }
    }
    assert_eq!(serial_calls.borrow().len(), 64);
    assert_eq!(batch_calls.borrow().len(), 2);
    assert_eq!(
        batch_calls.borrow().iter().map(|v| v.2).sum::<u64>(),
        132 + 384
    );
    prefetch(&mut batched, 128, 128 * 12, 56, 8, &ids).unwrap();
    assert_eq!(batch_calls.borrow().len(), 2);
}

#[test]
fn tablebase_download_frontier_bounds_headers_and_malformed_offsets_fail_closed() {
    let (mut reader, calls) = fixture(false);
    for ids in [vec![], vec![10]] {
        prefetch(&mut reader, 128, 1536, 56, 8, &ids).unwrap();
    }
    prefetch(&mut reader, 128, 1536, 0, 16, &[10, 11]).unwrap();
    for ids in [vec![10; 33], vec![10, 128], vec![11, 12]] {
        assert_eq!(
            prefetch(&mut reader, 128, 1536, 56, 8, &ids).unwrap_err(),
            "pc4_online_frontier_invalid"
        );
    }
    assert!(calls.borrow().is_empty());
    let (mut bad, bad_calls) = fixture(true);
    assert_eq!(
        prefetch(&mut bad, 128, 1536, 56, 8, &[10, 11]).unwrap_err(),
        "pc4_online_record_bounds"
    );
    assert_eq!(bad_calls.borrow().len(), 1);
    assert_eq!(bad_calls.borrow()[0].0, super::super::FILES[1]);
}
