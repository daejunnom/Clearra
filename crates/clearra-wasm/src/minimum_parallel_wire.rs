//! SRP rationale: the bounded, integrity-checked binary transport for the
//! portable exact AtMost worker contract. Hosts relay these bytes unchanged;
//! they never infer a minimum cardinality or manufacture a proof decision.

use clearra_coverage::{
    cover::{
        ExactAtMostQuery, ExactAtMostQueryIdentity, ExactAtMostReceipt, ExactAtMostShardOutcome,
        ExactAtMostTask,
    },
    pattern::pattern_bitset::PatternBitSet,
};
use sha2::{Digest, Sha256};

use crate::WasmCommandRuntimeError;

const MAGIC: &[u8; 8] = b"CRATM002";
const QUERY: u32 = 1;
const TASK: u32 = 2;
const RECEIPT: u32 = 3;
const GUARDED_QUERY: u32 = 4;
const DIGEST_BYTES: usize = 32;
const MAX_BYTES: usize = 512 * 1024 * 1024;

fn error(reason: &'static str) -> WasmCommandRuntimeError {
    WasmCommandRuntimeError::new("E_WASM_MINIMUM_PARALLEL_WIRE", reason)
}

struct Writer(Vec<u8>);

impl Writer {
    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn u64(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn u128(&mut self, value: u128) {
        self.0.extend_from_slice(&value.to_le_bytes());
    }

    fn count(&mut self, value: usize) -> Result<(), WasmCommandRuntimeError> {
        self.u32(u32::try_from(value).map_err(|_| error("parallel count exceeds wire range"))?);
        Ok(())
    }

    fn identity(&mut self, identity: ExactAtMostQueryIdentity) {
        self.0.extend_from_slice(&identity.matrix_id);
        self.u64(identity.generation);
        self.u64(identity.query_id);
    }

    fn indices(&mut self, values: &[usize]) -> Result<(), WasmCommandRuntimeError> {
        let bytes = values
            .len()
            .checked_mul(4)
            .and_then(|bytes| bytes.checked_add(4))
            .ok_or_else(|| error("parallel row index projection overflow"))?;
        if bytes > MAX_BYTES.saturating_sub(self.0.len() + DIGEST_BYTES) {
            return Err(error("parallel row indices exceed transfer capacity"));
        }
        self.0
            .try_reserve_exact(bytes)
            .map_err(|_| error("parallel row index allocation failed"))?;
        self.count(values.len())?;
        for &value in values {
            self.count(value)?;
        }
        Ok(())
    }

    fn task(&mut self, task: &ExactAtMostTask) -> Result<(), WasmCommandRuntimeError> {
        self.identity(task.identity());
        self.u64(task.partition_id());
        self.indices(task.forced_rows())?;
        self.indices(task.excluded_rows())
    }

    fn finish(mut self) -> Result<Vec<u8>, WasmCommandRuntimeError> {
        if self.0.len() > MAX_BYTES - DIGEST_BYTES {
            return Err(error("parallel packet exceeds transfer capacity"));
        }
        let digest = Sha256::digest(&self.0);
        self.0.extend_from_slice(&digest);
        Ok(self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WasmMinimumParallelWorker;
    use clearra_coverage::cover::{
        ExactAtMostCoordinator, ExactAtMostParallelDecision, ExactAtMostShardSession,
    };

    fn query(limit: usize) -> ExactAtMostQuery {
        ExactAtMostQuery::new(
            ExactAtMostQueryIdentity {
                matrix_id: [17; 32],
                generation: 8,
                query_id: 9,
            },
            PatternBitSet::from_words(3, vec![7]).unwrap(),
            [3, 5, 6]
                .into_iter()
                .map(|mask| PatternBitSet::from_words(3, vec![mask]).unwrap())
                .collect(),
            limit,
        )
        .unwrap()
    }

    #[test]
    fn exact_parallel_wire_roundtrip_drives_positive_and_all_negative_shards() {
        for limit in [1, 2] {
            let original = query(limit);
            let bytes = encode_query(&original).unwrap();
            let decoded = decode_query(&bytes).unwrap();
            assert_eq!(decoded.identity(), original.identity());
            assert_eq!(decoded.required(), original.required());
            assert_eq!(decoded.rows(), original.rows());
            assert_eq!(decoded.limit(), original.limit());
            let mut coordinator =
                ExactAtMostCoordinator::prepare(original, 4, &mut |_| Ok(()), &mut || false)
                    .unwrap();
            let tasks = coordinator.tasks().to_vec();
            assert!(tasks.len() > 1);
            let mut worker = WasmMinimumParallelWorker::initialize(&bytes).unwrap();
            for task in tasks.into_iter().rev() {
                let task_bytes = encode_task(&task).unwrap();
                assert_eq!(decode_task(&task_bytes).unwrap(), task);
                let mut task_peak = 0;
                decode_task_with_memory_guard(&task_bytes, &mut |owner| {
                    task_peak = task_peak.max(owner);
                    Ok(())
                })
                .unwrap();
                assert!(decode_task_with_memory_guard(&task_bytes, &mut |owner| {
                    if owner >= task_peak {
                        Err(error("synthetic task owner cap"))
                    } else {
                        Ok(())
                    }
                })
                .is_err());
                assert!(encode_task_with_memory_guard(&task, &mut |owner| {
                    if owner >= task_bytes.len() as u128 {
                        Err(error("synthetic task packet cap"))
                    } else {
                        Ok(())
                    }
                })
                .is_err());
                worker.start(&task_bytes).unwrap();
                assert!(worker.advance(0).unwrap().is_none());
                let mut result = None;
                for _ in 0..1_000 {
                    if let Some(receipt) = worker.advance(1).unwrap() {
                        result = Some(receipt);
                        break;
                    }
                }
                let wire = result.expect("bounded tiny exact shard");
                let mut peak = 0;
                let receipt = decode_receipt_with_memory_guard(&wire, &mut |bytes| {
                    peak = peak.max(bytes);
                    Ok(())
                })
                .unwrap();
                assert!(peak >= core::mem::size_of::<ExactAtMostReceipt>() as u128);
                assert!(
                    decode_receipt_with_memory_guard(&wire, &mut |bytes| {
                        if bytes >= peak {
                            Err(error("synthetic receipt owner cap"))
                        } else {
                            Ok(())
                        }
                    })
                    .is_err(),
                    "all receipt vector allocations honor the aggregate owner cap"
                );
                assert_eq!(receipt.task(), &task);
                assert!(
                    wire.len() <= checked_maximum_task_receipt_encoded_bytes(&decoded).unwrap()
                );
                assert!(encode_receipt_with_memory_guard(&receipt, &mut |owner| {
                    if owner >= wire.len() as u128 {
                        Err(error("synthetic receipt packet cap"))
                    } else {
                        Ok(())
                    }
                })
                .is_err());
                coordinator.accept(receipt).unwrap();
            }
            match coordinator.decision() {
                ExactAtMostParallelDecision::ProvedNone => assert_eq!(limit, 1),
                ExactAtMostParallelDecision::Found(rows) => {
                    assert_eq!(limit, 2);
                    assert!(rows.len() <= 2);
                }
                other => panic!("all receipts must complete exact decision: {other:?}"),
            }
        }
    }

    #[test]
    fn exact_parallel_wire_rejects_corruption_truncation_kind_and_padding() {
        let bytes = encode_query(&query(1)).unwrap();
        for length in [0, 1, 12, bytes.len() - 1] {
            assert!(decode_query(&bytes[..length]).is_err());
        }
        let mut corrupted = bytes.clone();
        corrupted[20] ^= 1;
        assert!(decode_query(&corrupted).is_err());
        assert!(decode_task(&bytes).is_err());
        let mut malformed = bytes;
        // First required word follows magic/kind, identity, limit and dimensions.
        let required_word = 12 + 48 + 12;
        malformed[required_word + 7] |= 0x80;
        let body_len = malformed.len() - DIGEST_BYTES;
        let digest = Sha256::digest(&malformed[..body_len]);
        malformed[body_len..].copy_from_slice(&digest);
        assert!(
            decode_query(&malformed).is_err(),
            "valid digest must not normalize invalid tail bits"
        );
    }

    #[test]
    fn exact_parallel_wire_preserves_advisory_hint_and_rejects_foreign_rows() {
        let base = query(2);
        let hinted = ExactAtMostQuery::new_with_witness_hint(
            base.identity(),
            base.required().clone(),
            base.rows().to_vec(),
            base.limit(),
            Some(vec![0, 2]),
        )
        .unwrap();
        let bytes = encode_query(&hinted).unwrap();
        let decoded = decode_query(&bytes).unwrap();
        assert_eq!(decoded.witness_hint(), Some([0, 2].as_slice()));
        assert_eq!(
            decode_query(&encode_query(&base).unwrap())
                .unwrap()
                .witness_hint(),
            None
        );
        let mut foreign = bytes;
        let body_len = foreign.len() - DIGEST_BYTES;
        foreign[body_len - 4..body_len].copy_from_slice(&99u32.to_le_bytes());
        let digest = Sha256::digest(&foreign[..body_len]);
        foreign[body_len..].copy_from_slice(&digest);
        assert!(decode_query(&foreign).is_err());
    }

    #[test]
    fn exact_parallel_wire_rejects_stale_worker_task_and_simultaneous_cursor() {
        let query = query(2);
        let coordinator =
            ExactAtMostCoordinator::prepare(query.clone(), 4, &mut |_| Ok(()), &mut || false)
                .unwrap();
        let task = coordinator.tasks()[0].clone();
        let mut worker =
            WasmMinimumParallelWorker::initialize(&encode_query(&query).unwrap()).unwrap();
        let stale = ExactAtMostTask::from_parts(
            ExactAtMostQueryIdentity {
                query_id: query.identity().query_id + 1,
                ..query.identity()
            },
            task.partition_id(),
            task.forced_rows().to_vec(),
            task.excluded_rows().to_vec(),
        )
        .unwrap();
        assert!(worker.start(&encode_task(&stale).unwrap()).is_err());
        worker.start(&encode_task(&task).unwrap()).unwrap();
        assert!(worker.start(&encode_task(&task).unwrap()).is_err());
    }

    #[test]
    fn completion_carrier_reservation_covers_selector_boundary_and_larger_later_suffix() {
        let identity = query(2).identity();
        let first = ExactAtMostQuery::new(
            identity,
            PatternBitSet::all(64),
            vec![PatternBitSet::all(64)],
            1,
        )
        .unwrap();
        let later = ExactAtMostQuery::new_with_witness_hint(
            identity,
            PatternBitSet::all(65),
            vec![PatternBitSet::all(65), PatternBitSet::all(65)],
            2,
            Some(vec![0, 1]),
        )
        .unwrap();
        let (query_bound, receipt_bound) = checked_completion_query_carrier_bounds(2, 64).unwrap();
        assert!(
            checked_guarded_query_encoded_bytes(&later).unwrap()
                > checked_guarded_query_encoded_bytes(&first).unwrap()
        );
        assert!(encode_guarded_query(&later, 1_048_576).unwrap().len() <= query_bound);
        assert!(checked_maximum_task_receipt_encoded_bytes(&later).unwrap() <= receipt_bound);
        assert!(checked_completion_query_carrier_bounds(usize::MAX, 64).is_err());
        assert!(checked_completion_query_carrier_bounds(2, usize::MAX).is_err());
    }

    #[test]
    fn guarded_query_binds_cap_and_admits_every_decode_and_encode_allocation() {
        let original = query(2);
        let cap = 1_048_576;
        let bytes = encode_guarded_query(&original, cap).unwrap();
        assert_eq!(
            bytes.len(),
            checked_guarded_query_encoded_bytes(&original).unwrap()
        );
        assert_eq!(query_memory_cap(&bytes).unwrap(), Some(cap));
        assert_eq!(
            query_memory_cap(&encode_query(&original).unwrap()).unwrap(),
            None
        );
        let mut peak = 0;
        let decoded = decode_query_with_memory_guard(&bytes, &mut |bytes| {
            peak = peak.max(bytes);
            Ok(())
        })
        .unwrap();
        assert_eq!(decoded.rows(), original.rows());
        assert_eq!(decoded.required(), original.required());
        assert!(peak >= decoded.checked_retained_bytes().unwrap());
        assert!(decode_query_with_memory_guard(&bytes, &mut |bytes| {
            if bytes >= peak {
                Err(error("synthetic decoded owner cap"))
            } else {
                Ok(())
            }
        })
        .is_err());
        let mut corrupted = bytes.clone();
        corrupted[12] ^= 1;
        assert!(
            query_memory_cap(&corrupted).is_err(),
            "cap mutation invalidates the bound packet"
        );
        assert!(encode_guarded_query(&original, 0).is_err());
        assert!(
            encode_guarded_query_with_memory_guard(&original, cap, &mut |owner| {
                if owner >= bytes.len() as u128 {
                    Err(error("synthetic output owner cap"))
                } else {
                    Ok(())
                }
            })
            .is_err()
        );

        let coordinator =
            ExactAtMostCoordinator::prepare(original.clone(), 4, &mut |_| Ok(()), &mut || false)
                .unwrap();
        let bound = checked_maximum_task_receipt_encoded_bytes(&original).unwrap();
        for task in coordinator.tasks() {
            assert!(encode_task(task).unwrap().len() <= bound);
        }
    }

    #[test]
    fn guarded_remote_rejects_query_or_shard_without_inventing_a_receipt() {
        let original = query(2);
        assert!(WasmMinimumParallelWorker::initialize(
            &encode_guarded_query(&original, 1).unwrap()
        )
        .is_err());
        let packet = encode_guarded_query(&original, 1_048_576).unwrap();
        let mut query_peak = 0;
        let decoded_query = decode_query_with_memory_guard(&packet, &mut |bytes| {
            query_peak = query_peak.max(bytes);
            Ok(())
        })
        .unwrap();
        let coordinator =
            ExactAtMostCoordinator::prepare(decoded_query.clone(), 4, &mut |_| Ok(()), &mut || {
                false
            })
            .unwrap();
        let task = encode_task(&coordinator.tasks()[0]).unwrap();
        let query_retained = decoded_query.checked_retained_bytes().unwrap();
        let mut start_peak = 0;
        let decoded_task = decode_task_with_memory_guard(&task, &mut |bytes| {
            start_peak = start_peak.max(query_retained.checked_add(bytes).unwrap());
            Ok(())
        })
        .unwrap();
        let probe = ExactAtMostShardSession::prepare(
            decoded_query,
            decoded_task,
            &mut |bytes| {
                start_peak = start_peak.max(bytes);
                Ok(())
            },
            &mut || false,
        )
        .unwrap();
        start_peak = start_peak.max(probe.checked_retained_bytes().unwrap());
        drop(probe);

        // A query decode's constructor peak can exceed a small shard's peak.
        // Admit initialization, then model real ABI carrier growth so the
        // remaining start allowance is exactly the measured peak (or one less).
        let admitted_peak = query_peak.max(start_peak);
        let cap = (packet.len() + core::mem::size_of::<WasmMinimumParallelWorker>()) as u128
            + admitted_peak;
        let packet = encode_guarded_query(&original, cap).unwrap();
        for allowance in [start_peak.checked_sub(1).unwrap(), start_peak] {
            let mut worker = WasmMinimumParallelWorker::initialize(&packet)
                .expect("query decode and retained owner are admitted");
            let outer = packet.len() as u128 + admitted_peak - allowance;
            let started = worker.start_guarded(&task, outer);
            if allowance == start_peak {
                started.expect("the exact measured start peak must be admitted");
                assert!(worker.has_active_shard());
            } else {
                assert_eq!(
                    started.unwrap_err().code(),
                    "E_WASM_MINIMUM_PARALLEL_RESOURCE",
                    "peak-minus-one must reject through the real resource guard"
                );
                assert!(!worker.has_active_shard());
                assert!(
                    worker.advance(1).is_err(),
                    "a resource decline is not an exact negative receipt"
                );
            }
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8], kind: u32) -> Result<Self, WasmCommandRuntimeError> {
        if !(MAGIC.len() + 4 + DIGEST_BYTES..=MAX_BYTES).contains(&bytes.len()) {
            return Err(error("parallel packet length is invalid"));
        }
        let (body, digest) = bytes.split_at(bytes.len() - DIGEST_BYTES);
        if Sha256::digest(body).as_slice() != digest {
            return Err(error("parallel packet integrity mismatch"));
        }
        let mut reader = Self {
            bytes: body,
            offset: 0,
        };
        if reader.bytes(MAGIC.len())? != MAGIC || reader.u32()? != kind {
            return Err(error("parallel packet kind or version mismatch"));
        }
        Ok(reader)
    }

    fn bytes(&mut self, count: usize) -> Result<&'a [u8], WasmCommandRuntimeError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or_else(|| error("parallel offset overflow"))?;
        let result = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| error("parallel packet truncated"))?;
        self.offset = end;
        Ok(result)
    }

    fn u32(&mut self) -> Result<u32, WasmCommandRuntimeError> {
        Ok(u32::from_le_bytes(
            self.bytes(4)?.try_into().expect("four bytes"),
        ))
    }

    fn u64(&mut self) -> Result<u64, WasmCommandRuntimeError> {
        Ok(u64::from_le_bytes(
            self.bytes(8)?.try_into().expect("eight bytes"),
        ))
    }

    fn u128(&mut self) -> Result<u128, WasmCommandRuntimeError> {
        Ok(u128::from_le_bytes(
            self.bytes(16)?.try_into().expect("sixteen bytes"),
        ))
    }

    fn count(&mut self) -> Result<usize, WasmCommandRuntimeError> {
        usize::try_from(self.u32()?).map_err(|_| error("parallel count exceeds host range"))
    }

    fn identity(&mut self) -> Result<ExactAtMostQueryIdentity, WasmCommandRuntimeError> {
        Ok(ExactAtMostQueryIdentity {
            matrix_id: self.bytes(32)?.try_into().expect("matrix identity bytes"),
            generation: self.u64()?,
            query_id: self.u64()?,
        })
    }

    fn finish(self) -> Result<(), WasmCommandRuntimeError> {
        if self.offset != self.bytes.len() {
            return Err(error("parallel packet has trailing data"));
        }
        Ok(())
    }
}

fn checked_query_encoded_bytes(
    query: &ExactAtMostQuery,
    guarded: bool,
) -> Result<usize, WasmCommandRuntimeError> {
    let matrix_bytes = query
        .rows()
        .len()
        .checked_add(1)
        .and_then(|rows| rows.checked_mul(query.required().word_count()))
        .and_then(|words| words.checked_mul(8))
        .ok_or_else(|| error("parallel matrix byte projection overflow"))?;
    let hint_bytes = match query.witness_hint() {
        Some(rows) => rows
            .len()
            .checked_mul(4)
            .and_then(|bytes| bytes.checked_add(4))
            .ok_or_else(|| error("parallel hint byte projection overflow"))?,
        None => 0,
    };
    let bytes = (12_usize + 48 + 12 + 4 + DIGEST_BYTES)
        .checked_add(usize::from(guarded) * 16)
        .and_then(|bytes| bytes.checked_add(matrix_bytes))
        .and_then(|bytes| bytes.checked_add(hint_bytes))
        .filter(|bytes| *bytes <= MAX_BYTES)
        .ok_or_else(|| error("parallel query exceeds transfer capacity"))?;
    Ok(bytes)
}

pub(crate) fn checked_guarded_query_encoded_bytes(
    query: &ExactAtMostQuery,
) -> Result<usize, WasmCommandRuntimeError> {
    checked_query_encoded_bytes(query, true)
}

/// A checked bound for either one task or one terminal receipt. Task rows are
/// source-bound to the query and a witness has at most `min(limit, rows)` IDs.
pub(crate) fn checked_maximum_task_receipt_encoded_bytes(
    query: &ExactAtMostQuery,
) -> Result<usize, WasmCommandRuntimeError> {
    query
        .rows()
        .len()
        .checked_mul(2)
        .and_then(|rows| rows.checked_add(query.limit().min(query.rows().len())))
        .and_then(|rows| rows.checked_mul(4))
        .and_then(|bytes| bytes.checked_add(12 + 48 + 8 + 8 + 4 + 4 + DIGEST_BYTES))
        .filter(|bytes| *bytes <= MAX_BYTES)
        .ok_or_else(|| error("parallel receipt bound exceeds transfer capacity"))
}

/// Whole-completion transport reservation. A later canonical query may use a
/// different original-row suffix, one extra selector bit, and a witness hint
/// absent from the first query. Only immutable source dimensions are bounds.
pub(crate) fn checked_completion_query_carrier_bounds(
    source_rows: usize,
    source_patterns: usize,
) -> Result<(usize, usize), WasmCommandRuntimeError> {
    let patterns = source_patterns
        .checked_add(1)
        .filter(|patterns| u32::try_from(*patterns).is_ok())
        .ok_or_else(|| error("parallel completion pattern bound overflow"))?;
    if u32::try_from(source_rows).is_err() {
        return Err(error("parallel completion row bound overflow"));
    }
    let query = source_rows
        .checked_add(1)
        .and_then(|rows| rows.checked_mul(patterns.div_ceil(64)))
        .and_then(|words| words.checked_mul(8))
        .and_then(|bytes| bytes.checked_add(source_rows.checked_mul(4)?))
        .and_then(|bytes| bytes.checked_add(12 + 16 + 48 + 12 + 4 + 4 + DIGEST_BYTES))
        .filter(|bytes| *bytes <= MAX_BYTES)
        .ok_or_else(|| error("parallel completion query exceeds transfer capacity"))?;
    let task_receipt = source_rows
        .checked_mul(3)
        .and_then(|rows| rows.checked_mul(4))
        .and_then(|bytes| bytes.checked_add(12 + 48 + 8 + 8 + 4 + 4 + DIGEST_BYTES))
        .filter(|bytes| *bytes <= MAX_BYTES)
        .ok_or_else(|| error("parallel completion receipt exceeds transfer capacity"))?;
    Ok((query, task_receipt))
}

#[cfg(test)]
pub(crate) fn encode_query(query: &ExactAtMostQuery) -> Result<Vec<u8>, WasmCommandRuntimeError> {
    encode_query_with_memory_guard(query, None, &mut |_| Ok(()))
}

#[cfg(test)]
pub(crate) fn encode_guarded_query(
    query: &ExactAtMostQuery,
    worker_memory_cap: u128,
) -> Result<Vec<u8>, WasmCommandRuntimeError> {
    encode_guarded_query_with_memory_guard(query, worker_memory_cap, &mut |_| Ok(()))
}

pub(crate) fn encode_guarded_query_with_memory_guard(
    query: &ExactAtMostQuery,
    worker_memory_cap: u128,
    guard: &mut impl FnMut(u128) -> Result<(), WasmCommandRuntimeError>,
) -> Result<Vec<u8>, WasmCommandRuntimeError> {
    if worker_memory_cap == 0 {
        return Err(error("parallel worker memory cap is zero"));
    }
    encode_query_with_memory_guard(query, Some(worker_memory_cap), guard)
}

pub(crate) fn encode_query_with_memory_guard(
    query: &ExactAtMostQuery,
    worker_memory_cap: Option<u128>,
    guard: &mut impl FnMut(u128) -> Result<(), WasmCommandRuntimeError>,
) -> Result<Vec<u8>, WasmCommandRuntimeError> {
    let requested = checked_query_encoded_bytes(query, worker_memory_cap.is_some())?;
    guard(requested as u128)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(requested)
        .map_err(|_| error("parallel query allocation failed"))?;
    guard(bytes.capacity() as u128)?;
    let mut writer = Writer(bytes);
    writer.0.extend_from_slice(MAGIC);
    writer.u32(if worker_memory_cap.is_some() {
        GUARDED_QUERY
    } else {
        QUERY
    });
    if let Some(cap) = worker_memory_cap {
        writer.u128(cap);
    }
    writer.identity(query.identity());
    writer.count(query.limit())?;
    writer.count(query.required().pattern_count())?;
    writer.count(query.rows().len())?;
    let matrix_bytes = query
        .rows()
        .len()
        .checked_add(1)
        .and_then(|count| count.checked_mul(query.required().word_count()))
        .and_then(|count| count.checked_mul(8))
        .ok_or_else(|| error("parallel matrix byte projection overflow"))?;
    if matrix_bytes > MAX_BYTES - writer.0.len() - DIGEST_BYTES {
        return Err(error("parallel matrix exceeds transfer capacity"));
    }
    writer
        .0
        .try_reserve_exact(matrix_bytes + DIGEST_BYTES)
        .map_err(|_| error("parallel matrix allocation failed"))?;
    for row in core::iter::once(query.required()).chain(query.rows()) {
        for word in 0..row.word_count() {
            writer.u64(row.word_at(word));
        }
    }
    match query.witness_hint() {
        Some(hint) => {
            writer.u32(1);
            writer.indices(hint)?;
        }
        None => writer.u32(0),
    }
    let bytes = writer.finish()?;
    guard(bytes.capacity() as u128)?;
    debug_assert_eq!(bytes.len(), requested);
    Ok(bytes)
}

fn query_reader(bytes: &[u8]) -> Result<(Reader<'_>, Option<u128>), WasmCommandRuntimeError> {
    let kind = bytes
        .get(8..12)
        .and_then(|kind| <[u8; 4]>::try_from(kind).ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| error("parallel query header is truncated"))?;
    match kind {
        QUERY => Ok((Reader::new(bytes, QUERY)?, None)),
        GUARDED_QUERY => {
            let mut reader = Reader::new(bytes, GUARDED_QUERY)?;
            let cap = reader.u128()?;
            if cap == 0 {
                return Err(error("parallel worker memory cap is zero"));
            }
            Ok((reader, Some(cap)))
        }
        _ => Err(error("parallel packet is not a query")),
    }
}

/// Authentication and cap parsing are allocation-free. The cap and matrix
/// identity are covered by the same digest, never supplied as separate advice.
pub(crate) fn query_memory_cap(bytes: &[u8]) -> Result<Option<u128>, WasmCommandRuntimeError> {
    query_reader(bytes).map(|(_, cap)| cap)
}

#[cfg(test)]
pub(crate) fn decode_query(bytes: &[u8]) -> Result<ExactAtMostQuery, WasmCommandRuntimeError> {
    decode_query_with_memory_guard(bytes, &mut |_| Ok(()))
}

pub(crate) fn decode_query_with_memory_guard(
    bytes: &[u8],
    guard: &mut impl FnMut(u128) -> Result<(), WasmCommandRuntimeError>,
) -> Result<ExactAtMostQuery, WasmCommandRuntimeError> {
    let (mut reader, _) = query_reader(bytes)?;
    let identity = reader.identity()?;
    let limit = reader.count()?;
    let pattern_count = reader.count()?;
    let row_count = reader.count()?;
    let word_count = pattern_count.div_ceil(64);
    let expected_bytes = row_count
        .checked_add(1)
        .and_then(|count| count.checked_mul(word_count))
        .and_then(|count| count.checked_mul(8))
        .ok_or_else(|| error("parallel matrix byte projection overflow"))?;
    if reader.bytes.len() - reader.offset < expected_bytes.saturating_add(4)
        || row_count > MAX_BYTES / core::mem::size_of::<PatternBitSet>()
    {
        return Err(error("parallel matrix dimensions mismatch"));
    }
    let mut retained = ExactAtMostQuery::checked_constructor_owner_bytes()
        .ok_or_else(|| error("parallel query owner overflow"))?;
    guard(retained)?;
    let inline_rows = (row_count as u128)
        .checked_mul(core::mem::size_of::<PatternBitSet>() as u128)
        .ok_or_else(|| error("parallel matrix owner overflow"))?;
    guard(
        retained
            .checked_add(inline_rows)
            .ok_or_else(|| error("parallel matrix owner overflow"))?,
    )?;
    let mut rows = Vec::new();
    rows.try_reserve_exact(row_count)
        .map_err(|_| error("parallel matrix allocation failed"))?;
    retained = retained
        .checked_add(
            (rows.capacity() as u128)
                .checked_mul(core::mem::size_of::<PatternBitSet>() as u128)
                .ok_or_else(|| error("parallel matrix owner overflow"))?,
        )
        .ok_or_else(|| error("parallel matrix owner overflow"))?;
    guard(retained)?;
    let read_row = |reader: &mut Reader<'_>,
                    retained: &mut u128,
                    guard: &mut dyn FnMut(u128) -> Result<(), WasmCommandRuntimeError>|
     -> Result<PatternBitSet, WasmCommandRuntimeError> {
        // The existing constructor keeps compact sparse/dense representation.
        // Its conservative branch bound also covers the zero-word sparse owner
        // by using the one-word bound in that degenerate case.
        let constructor = PatternBitSet::checked_external_words_materialize_union_future_bytes(
            pattern_count.max(1),
        )
        .ok_or_else(|| error("parallel bitset constructor overflow"))?;
        guard(
            retained
                .checked_add(constructor)
                .ok_or_else(|| error("parallel bitset owner overflow"))?,
        )?;
        let mut words = Vec::new();
        words
            .try_reserve_exact(word_count)
            .map_err(|_| error("parallel bitset allocation failed"))?;
        let spare = (words.capacity().saturating_sub(word_count) as u128)
            .checked_mul(8)
            .ok_or_else(|| error("parallel bitset owner overflow"))?;
        guard(
            retained
                .checked_add(constructor)
                .and_then(|bytes| bytes.checked_add(spare))
                .ok_or_else(|| error("parallel bitset owner overflow"))?,
        )?;
        for _ in 0..word_count {
            words.push(reader.u64()?);
        }
        if pattern_count % 64 != 0
            && words
                .last()
                .is_some_and(|last| last >> (pattern_count % 64) != 0)
        {
            return Err(error("parallel bitset has noncanonical padding"));
        }
        let row = PatternBitSet::from_words(pattern_count, words)
            .map_err(|_| error("parallel bitset is invalid"))?;
        *retained = retained
            .checked_add(
                row.checked_storage_retained_bytes()
                    .ok_or_else(|| error("parallel bitset owner overflow"))?,
            )
            .ok_or_else(|| error("parallel bitset owner overflow"))?;
        guard(*retained)?;
        Ok(row)
    };
    let required = read_row(&mut reader, &mut retained, guard)?;
    for _ in 0..row_count {
        rows.push(read_row(&mut reader, &mut retained, guard)?);
    }
    let witness_hint = match reader.u32()? {
        0 => None,
        1 => Some(read_indices_guarded(&mut reader, &mut retained, guard)?),
        _ => return Err(error("parallel witness hint tag is invalid")),
    };
    reader.finish()?;
    guard(retained)?;
    let query =
        ExactAtMostQuery::new_with_witness_hint(identity, required, rows, limit, witness_hint)
            .map_err(|_| error("parallel query is invalid"))?;
    guard(
        query
            .checked_retained_bytes()
            .ok_or_else(|| error("parallel query owner overflow"))?,
    )?;
    Ok(query)
}

#[cfg(test)]
pub(crate) fn encode_task(task: &ExactAtMostTask) -> Result<Vec<u8>, WasmCommandRuntimeError> {
    encode_task_with_memory_guard(task, &mut |_| Ok(()))
}

pub(crate) fn encode_task_with_memory_guard(
    task: &ExactAtMostTask,
    guard: &mut impl FnMut(u128) -> Result<(), WasmCommandRuntimeError>,
) -> Result<Vec<u8>, WasmCommandRuntimeError> {
    let requested = task
        .forced_rows()
        .len()
        .checked_add(task.excluded_rows().len())
        .and_then(|rows| rows.checked_mul(4))
        .and_then(|bytes| bytes.checked_add(12 + 48 + 8 + 8 + DIGEST_BYTES))
        .filter(|bytes| *bytes <= MAX_BYTES)
        .ok_or_else(|| error("parallel task exceeds transfer capacity"))?;
    guard(requested as u128)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(requested)
        .map_err(|_| error("parallel task allocation failed"))?;
    guard(bytes.capacity() as u128)?;
    let mut writer = Writer(bytes);
    writer.0.extend_from_slice(MAGIC);
    writer.u32(TASK);
    writer.task(task)?;
    let bytes = writer.finish()?;
    guard(bytes.capacity() as u128)?;
    debug_assert_eq!(bytes.len(), requested);
    Ok(bytes)
}

#[cfg(test)]
pub(crate) fn decode_task(bytes: &[u8]) -> Result<ExactAtMostTask, WasmCommandRuntimeError> {
    decode_task_with_memory_guard(bytes, &mut |_| Ok(()))
}

fn read_indices_guarded(
    reader: &mut Reader<'_>,
    retained: &mut u128,
    guard: &mut impl FnMut(u128) -> Result<(), WasmCommandRuntimeError>,
) -> Result<Vec<usize>, WasmCommandRuntimeError> {
    let count = reader.count()?;
    if count > (reader.bytes.len() - reader.offset) / 4 {
        return Err(error("parallel row index count exceeds packet"));
    }
    let projected = |capacity: usize| {
        (capacity as u128)
            .checked_mul(core::mem::size_of::<usize>() as u128)
            .and_then(|bytes| retained.checked_add(bytes))
            .ok_or_else(|| error("parallel vector owner overflow"))
    };
    guard(projected(count)?)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| error("parallel row allocation failed"))?;
    let actual = projected(values.capacity())?;
    guard(actual)?;
    *retained = actual;
    for _ in 0..count {
        values.push(reader.count()?);
    }
    Ok(values)
}

pub(crate) fn decode_task_with_memory_guard(
    bytes: &[u8],
    guard: &mut impl FnMut(u128) -> Result<(), WasmCommandRuntimeError>,
) -> Result<ExactAtMostTask, WasmCommandRuntimeError> {
    let mut reader = Reader::new(bytes, TASK)?;
    let mut retained = core::mem::size_of::<ExactAtMostTask>() as u128;
    guard(retained)?;
    let identity = reader.identity()?;
    let partition = reader.u64()?;
    let forced = read_indices_guarded(&mut reader, &mut retained, guard)?;
    let excluded = read_indices_guarded(&mut reader, &mut retained, guard)?;
    let task = ExactAtMostTask::from_parts(identity, partition, forced, excluded)
        .map_err(|_| error("parallel partition descriptor is invalid"))?;
    reader.finish()?;
    Ok(task)
}

pub(crate) fn encode_receipt_with_memory_guard(
    receipt: &ExactAtMostReceipt,
    guard: &mut impl FnMut(u128) -> Result<(), WasmCommandRuntimeError>,
) -> Result<Vec<u8>, WasmCommandRuntimeError> {
    let task = receipt.task();
    let outcome_bytes = match receipt.outcome() {
        ExactAtMostShardOutcome::Found(rows) => rows
            .len()
            .checked_mul(4)
            .and_then(|bytes| bytes.checked_add(4)),
        _ => Some(0),
    }
    .ok_or_else(|| error("parallel receipt byte projection overflow"))?;
    let requested = task
        .forced_rows()
        .len()
        .checked_add(task.excluded_rows().len())
        .and_then(|rows| rows.checked_mul(4))
        .and_then(|bytes| bytes.checked_add(12 + 48 + 8 + 8 + 4 + DIGEST_BYTES))
        .and_then(|bytes| bytes.checked_add(outcome_bytes))
        .filter(|bytes| *bytes <= MAX_BYTES)
        .ok_or_else(|| error("parallel receipt exceeds transfer capacity"))?;
    guard(requested as u128)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(requested)
        .map_err(|_| error("parallel receipt allocation failed"))?;
    guard(bytes.capacity() as u128)?;
    let mut writer = Writer(bytes);
    writer.0.extend_from_slice(MAGIC);
    writer.u32(RECEIPT);
    writer.task(receipt.task())?;
    match receipt.outcome() {
        ExactAtMostShardOutcome::Found(rows) => {
            writer.u32(0);
            writer.indices(rows)?;
        }
        ExactAtMostShardOutcome::ProvedNone => writer.u32(1),
        ExactAtMostShardOutcome::Cancelled => writer.u32(2),
    }
    let bytes = writer.finish()?;
    guard(bytes.capacity() as u128)?;
    debug_assert_eq!(bytes.len(), requested);
    Ok(bytes)
}

pub(crate) fn decode_receipt_with_memory_guard(
    bytes: &[u8],
    guard: &mut impl FnMut(u128) -> Result<(), WasmCommandRuntimeError>,
) -> Result<ExactAtMostReceipt, WasmCommandRuntimeError> {
    let mut reader = Reader::new(bytes, RECEIPT)?;
    let mut retained = core::mem::size_of::<ExactAtMostReceipt>() as u128;
    guard(retained)?;
    let identity = reader.identity()?;
    let partition = reader.u64()?;
    let forced = read_indices_guarded(&mut reader, &mut retained, guard)?;
    let excluded = read_indices_guarded(&mut reader, &mut retained, guard)?;
    let task = ExactAtMostTask::from_parts(identity, partition, forced, excluded)
        .map_err(|_| error("parallel partition descriptor is invalid"))?;
    let outcome = match reader.u32()? {
        0 => {
            ExactAtMostShardOutcome::Found(read_indices_guarded(&mut reader, &mut retained, guard)?)
        }
        1 => ExactAtMostShardOutcome::ProvedNone,
        2 => ExactAtMostShardOutcome::Cancelled,
        _ => return Err(error("parallel receipt outcome is invalid")),
    };
    reader.finish()?;
    ExactAtMostReceipt::from_parts(task, outcome)
        .map_err(|_| error("parallel receipt shape is invalid"))
}
