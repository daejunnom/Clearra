use crate::{GraphTargetEncoding, Pc4GraphPiece};

const HYDRA_GRAPH_FIELD_HASH_BYTES: usize = 5;
const HYDRA_GRAPH_PIECE_COUNT: usize = 7;
const HYDRA_GRAPH_MAX_CUMULATIVE_DEGREE: usize = u8::MAX as usize;

/// One complete record from the qualified Hydra-compatible graph layout.
///
/// This type only proves that the bytes obey the declared record format. It
/// does not prove that a graph is complete for a rule profile or terminal
/// target; that authority remains in `QualifiedPc4TargetIdentity`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedHydraGraphRecordV1 {
    source_field_hash: u64,
    targets: [Vec<u32>; HYDRA_GRAPH_PIECE_COUNT],
}

impl DecodedHydraGraphRecordV1 {
    pub const fn source_field_hash(&self) -> u64 {
        self.source_field_hash
    }

    pub fn targets(&self, piece: Pc4GraphPiece) -> &[u32] {
        &self.targets[hydra_piece_index(piece)]
    }

    pub fn total_target_count(&self) -> usize {
        self.targets.iter().map(Vec::len).sum()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HydraGraphRecordDecodeError {
    TruncatedSourceFieldHash {
        byte_len: usize,
    },
    SourceFieldHashMismatch {
        expected: u64,
        actual: u64,
    },
    TruncatedPieceDegree {
        piece: Pc4GraphPiece,
    },
    TruncatedPieceTargets {
        piece: Pc4GraphPiece,
        expected_bytes: usize,
        remaining_bytes: usize,
    },
    CumulativeDegreeOutsideCanonicalDomain {
        attempted: usize,
    },
    TargetOutsideFieldDomain {
        piece: Pc4GraphPiece,
        target: u32,
        field_count: u32,
    },
    TrailingBytes {
        byte_len: usize,
    },
}

impl HydraGraphRecordDecodeError {
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::TruncatedSourceFieldHash { .. } => "pc4_hydra_graph_record_source_hash_truncated",
            Self::SourceFieldHashMismatch { .. } => "pc4_hydra_graph_record_source_hash_mismatch",
            Self::TruncatedPieceDegree { .. } => "pc4_hydra_graph_record_piece_degree_truncated",
            Self::TruncatedPieceTargets { .. } => "pc4_hydra_graph_record_piece_targets_truncated",
            Self::CumulativeDegreeOutsideCanonicalDomain { .. } => {
                "pc4_hydra_graph_record_cumulative_degree_outside_domain"
            }
            Self::TargetOutsideFieldDomain { .. } => {
                "pc4_hydra_graph_record_target_outside_field_domain"
            }
            Self::TrailingBytes { .. } => "pc4_hydra_graph_record_trailing_bytes",
        }
    }
}

/// Decodes every outgoing edge in one Hydra-compatible graph record.
///
/// The on-disk piece order is `I, J, L, O, S, T, Z`. No edge is selected,
/// ranked, or pruned here. Duplicate target IDs are intentionally retained so
/// the separately qualified adjacency boundary can apply its declared
/// transition canonicalization policy.
pub fn decode_hydra_graph_record_v1(
    bytes: &[u8],
    expected_source_field_hash: u64,
    target_encoding: GraphTargetEncoding,
    field_count: u32,
) -> Result<DecodedHydraGraphRecordV1, HydraGraphRecordDecodeError> {
    if bytes.len() < HYDRA_GRAPH_FIELD_HASH_BYTES {
        return Err(HydraGraphRecordDecodeError::TruncatedSourceFieldHash {
            byte_len: bytes.len(),
        });
    }
    let source_field_hash = read_u40_big_endian(bytes);
    if source_field_hash != expected_source_field_hash {
        return Err(HydraGraphRecordDecodeError::SourceFieldHashMismatch {
            expected: expected_source_field_hash,
            actual: source_field_hash,
        });
    }

    let target_width = target_encoding.byte_width();
    let mut cursor = HYDRA_GRAPH_FIELD_HASH_BYTES;
    let mut cumulative_degree = 0usize;
    let mut targets: [Vec<u32>; HYDRA_GRAPH_PIECE_COUNT] = core::array::from_fn(|_| Vec::new());
    for piece in HYDRA_GRAPH_PIECES {
        let degree = usize::from(
            *bytes
                .get(cursor)
                .ok_or(HydraGraphRecordDecodeError::TruncatedPieceDegree { piece })?,
        );
        cursor += 1;
        cumulative_degree = cumulative_degree.checked_add(degree).ok_or(
            HydraGraphRecordDecodeError::CumulativeDegreeOutsideCanonicalDomain {
                attempted: usize::MAX,
            },
        )?;
        if cumulative_degree > HYDRA_GRAPH_MAX_CUMULATIVE_DEGREE {
            return Err(
                HydraGraphRecordDecodeError::CumulativeDegreeOutsideCanonicalDomain {
                    attempted: cumulative_degree,
                },
            );
        }
        let target_bytes = degree.checked_mul(target_width).ok_or(
            HydraGraphRecordDecodeError::CumulativeDegreeOutsideCanonicalDomain {
                attempted: cumulative_degree,
            },
        )?;
        let end = cursor.checked_add(target_bytes).ok_or(
            HydraGraphRecordDecodeError::TruncatedPieceTargets {
                piece,
                expected_bytes: target_bytes,
                remaining_bytes: bytes.len().saturating_sub(cursor),
            },
        )?;
        let encoded =
            bytes
                .get(cursor..end)
                .ok_or(HydraGraphRecordDecodeError::TruncatedPieceTargets {
                    piece,
                    expected_bytes: target_bytes,
                    remaining_bytes: bytes.len().saturating_sub(cursor),
                })?;
        let decoded = decode_graph_target_sequence(encoded, target_encoding, field_count).map_err(
            |error| match error {
                GraphTargetDecodeError::TruncatedTarget { .. } => {
                    HydraGraphRecordDecodeError::TruncatedPieceTargets {
                        piece,
                        expected_bytes: target_bytes,
                        remaining_bytes: bytes.len().saturating_sub(cursor),
                    }
                }
                GraphTargetDecodeError::TargetOutsideFieldDomain {
                    target,
                    field_count,
                } => HydraGraphRecordDecodeError::TargetOutsideFieldDomain {
                    piece,
                    target,
                    field_count,
                },
            },
        )?;
        targets[hydra_piece_index(piece)] = decoded;
        cursor = end;
    }
    if cursor != bytes.len() {
        return Err(HydraGraphRecordDecodeError::TrailingBytes {
            byte_len: bytes.len() - cursor,
        });
    }
    Ok(DecodedHydraGraphRecordV1 {
        source_field_hash,
        targets,
    })
}

const HYDRA_GRAPH_PIECES: [Pc4GraphPiece; HYDRA_GRAPH_PIECE_COUNT] = [
    Pc4GraphPiece::I,
    Pc4GraphPiece::J,
    Pc4GraphPiece::L,
    Pc4GraphPiece::O,
    Pc4GraphPiece::S,
    Pc4GraphPiece::T,
    Pc4GraphPiece::Z,
];

const fn hydra_piece_index(piece: Pc4GraphPiece) -> usize {
    match piece {
        Pc4GraphPiece::I => 0,
        Pc4GraphPiece::J => 1,
        Pc4GraphPiece::L => 2,
        Pc4GraphPiece::O => 3,
        Pc4GraphPiece::S => 4,
        Pc4GraphPiece::T => 5,
        Pc4GraphPiece::Z => 6,
    }
}

fn read_u40_big_endian(bytes: &[u8]) -> u64 {
    (u64::from(bytes[0]) << 32)
        | (u64::from(bytes[1]) << 24)
        | (u64::from(bytes[2]) << 16)
        | (u64::from(bytes[3]) << 8)
        | u64::from(bytes[4])
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GraphTargetDecodeError {
    TruncatedTarget {
        byte_len: usize,
        target_width: usize,
    },
    TargetOutsideFieldDomain {
        target: u32,
        field_count: u32,
    },
}

/// Decodes an already-delimited sequence of graph target words.
///
/// The online lookup machine deliberately returns an opaque graph record. An
/// upstream-qualified record-layout materializer must delimit target sequences
/// before calling this function; this function must not be used to guess an
/// undocumented graph record layout.
pub fn decode_graph_target_sequence(
    bytes: &[u8],
    encoding: GraphTargetEncoding,
    field_count: u32,
) -> Result<Vec<u32>, GraphTargetDecodeError> {
    let width = encoding.byte_width();
    if !bytes.len().is_multiple_of(width) {
        return Err(GraphTargetDecodeError::TruncatedTarget {
            byte_len: bytes.len(),
            target_width: width,
        });
    }
    let mut targets = Vec::with_capacity(bytes.len() / width);
    for encoded in bytes.chunks_exact(width) {
        let target = match encoding {
            GraphTargetEncoding::U24LittleEndian => {
                u32::from(encoded[0]) | (u32::from(encoded[1]) << 8) | (u32::from(encoded[2]) << 16)
            }
            GraphTargetEncoding::U32LittleEndian => {
                u32::from_le_bytes([encoded[0], encoded[1], encoded[2], encoded[3]])
            }
        };
        if target >= field_count {
            return Err(GraphTargetDecodeError::TargetOutsideFieldDomain {
                target,
                field_count,
            });
        }
        targets.push(target);
    }
    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hydra_record(
        source_hash: u64,
        per_piece_targets: [&[u32]; HYDRA_GRAPH_PIECE_COUNT],
    ) -> Vec<u8> {
        let mut bytes = source_hash.to_be_bytes()[3..].to_vec();
        for targets in per_piece_targets {
            bytes.push(u8::try_from(targets.len()).expect("small test degree"));
            for target in targets {
                bytes.extend_from_slice(&target.to_le_bytes()[..3]);
            }
        }
        bytes
    }

    #[test]
    fn target_words_decode_for_both_qualified_encodings() {
        assert_eq!(
            decode_graph_target_sequence(
                &[0x01, 0x00, 0x00, 0xff, 0x00, 0x00],
                GraphTargetEncoding::U24LittleEndian,
                256,
            ),
            Ok(vec![1, 255])
        );
        assert_eq!(
            decode_graph_target_sequence(
                &[0x01, 0x00, 0x00, 0x00, 0xff, 0x00, 0x00, 0x00],
                GraphTargetEncoding::U32LittleEndian,
                256,
            ),
            Ok(vec![1, 255])
        );
    }

    #[test]
    fn target_words_fail_closed_on_truncation_or_out_of_domain_ids() {
        assert_eq!(
            decode_graph_target_sequence(&[0, 0], GraphTargetEncoding::U24LittleEndian, 1,),
            Err(GraphTargetDecodeError::TruncatedTarget {
                byte_len: 2,
                target_width: 3,
            })
        );
        assert_eq!(
            decode_graph_target_sequence(&[2, 0, 0], GraphTargetEncoding::U24LittleEndian, 2,),
            Err(GraphTargetDecodeError::TargetOutsideFieldDomain {
                target: 2,
                field_count: 2,
            })
        );
    }

    #[test]
    fn hydra_record_decodes_every_piece_group_without_selecting_an_edge() {
        let record = hydra_record(
            0x01_0203_0405,
            [&[4, 2, 4], &[1], &[], &[0, 3], &[2], &[4], &[1, 0]],
        );
        let decoded = decode_hydra_graph_record_v1(
            &record,
            0x01_0203_0405,
            GraphTargetEncoding::U24LittleEndian,
            5,
        )
        .expect("complete record");

        assert_eq!(decoded.source_field_hash(), 0x01_0203_0405);
        assert_eq!(decoded.targets(Pc4GraphPiece::I), &[4, 2, 4]);
        assert_eq!(decoded.targets(Pc4GraphPiece::J), &[1]);
        assert_eq!(decoded.targets(Pc4GraphPiece::L), &[]);
        assert_eq!(decoded.targets(Pc4GraphPiece::O), &[0, 3]);
        assert_eq!(decoded.targets(Pc4GraphPiece::S), &[2]);
        assert_eq!(decoded.targets(Pc4GraphPiece::T), &[4]);
        assert_eq!(decoded.targets(Pc4GraphPiece::Z), &[1, 0]);
        assert_eq!(decoded.total_target_count(), 10);
    }

    #[test]
    fn hydra_record_rejects_binding_truncation_domain_and_trailing_drift() {
        assert_eq!(
            decode_hydra_graph_record_v1(&[0, 1, 2, 3], 0, GraphTargetEncoding::U24LittleEndian, 1,),
            Err(HydraGraphRecordDecodeError::TruncatedSourceFieldHash { byte_len: 4 })
        );

        let complete = hydra_record(7, [&[0], &[], &[], &[], &[], &[], &[]]);
        assert_eq!(
            decode_hydra_graph_record_v1(&complete, 8, GraphTargetEncoding::U24LittleEndian, 1,),
            Err(HydraGraphRecordDecodeError::SourceFieldHashMismatch {
                expected: 8,
                actual: 7,
            })
        );

        let mut truncated = complete.clone();
        truncated.pop();
        assert!(matches!(
            decode_hydra_graph_record_v1(&truncated, 7, GraphTargetEncoding::U24LittleEndian, 1,),
            Err(HydraGraphRecordDecodeError::TruncatedPieceDegree { .. })
                | Err(HydraGraphRecordDecodeError::TruncatedPieceTargets { .. })
        ));

        let outside = hydra_record(7, [&[1], &[], &[], &[], &[], &[], &[]]);
        assert_eq!(
            decode_hydra_graph_record_v1(&outside, 7, GraphTargetEncoding::U24LittleEndian, 1,),
            Err(HydraGraphRecordDecodeError::TargetOutsideFieldDomain {
                piece: Pc4GraphPiece::I,
                target: 1,
                field_count: 1,
            })
        );

        let mut trailing = complete;
        trailing.push(0);
        assert_eq!(
            decode_hydra_graph_record_v1(&trailing, 7, GraphTargetEncoding::U24LittleEndian, 1,),
            Err(HydraGraphRecordDecodeError::TrailingBytes { byte_len: 1 })
        );
    }

    #[test]
    fn hydra_record_rejects_noncanonical_cumulative_degree() {
        let many = vec![0_u32; 43];
        let record = hydra_record(0, [&many, &many, &many, &many, &many, &many, &many]);
        assert_eq!(
            decode_hydra_graph_record_v1(&record, 0, GraphTargetEncoding::U24LittleEndian, 1,),
            Err(
                HydraGraphRecordDecodeError::CumulativeDegreeOutsideCanonicalDomain {
                    attempted: 258,
                }
            )
        );
    }
}
