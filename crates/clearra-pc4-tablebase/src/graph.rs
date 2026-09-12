use crate::GraphTargetEncoding;

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
}
