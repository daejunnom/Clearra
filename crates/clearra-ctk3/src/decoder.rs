use crate::bitstream::BitReader;
use crate::cell::{bits_for_choices, read_cell_encoding};
use crate::geometry::{canonicalize_operation, operation_cells, operation_rotations};
use crate::transport::{
    crc16, decode_payload, extract_compatible_bundle, extract_compatible_payload,
    extract_exact_bundle, extract_exact_payload, Transport,
};
use crate::{
    normalized_to_page, Ctk3CodecError, Ctk3Document, Ctk3DocumentInfo, Ctk3Operation,
    Ctk3PageFlags, Ctk3Piece, Ctk3Rotation, NormalizedPage, COMPACT_SCHEMA_REVISION,
    CTK3_MAX_BUNDLE_PAGES, CTK3_MAX_SEGMENT_PAGES, CTK3_PREFIX, LEGACY_SCHEMA_REVISION, MAGIC,
    MAX_COMMENT_BYTES, MAX_HEIGHT, MAX_OPERATION_COORDINATE, MAX_PAYLOAD_BYTES, MAX_WIDTH,
    SHARED_FIELD_SCHEMA_REVISION, TEMPORAL_SCHEMA_REVISION,
};

#[derive(Clone, Debug)]
struct SharedFieldPredictor {
    codes: Vec<u8>,
}

/// Strictly decodes one exact CTK3 document or bundle value.
pub fn decode_ctk3_exact(input: &str) -> Result<Ctk3Document, Ctk3CodecError> {
    if let Some(payloads) = extract_exact_bundle(input)? {
        decode_bundle_payloads(&payloads)
    } else {
        let (transport, payload) = extract_exact_payload(input)?;
        decode_single_payload(transport, payload)
    }
}

/// Compatibility decoder that can locate an unescaped CTK3 value in an envelope.
/// Exact native files and CLI arguments should prefer [`decode_ctk3_exact`].
pub fn decode_ctk3(input: &str) -> Result<Ctk3Document, Ctk3CodecError> {
    if let Some(payloads) = extract_compatible_bundle(input)? {
        decode_bundle_payloads(&payloads)
    } else {
        let (transport, payload) = extract_compatible_payload(input)?;
        decode_single_payload(transport, payload)
    }
}

/// Decodes one segment and rejects a bundle envelope.
pub fn decode_ctk3_segment(input: &str) -> Result<Ctk3Document, Ctk3CodecError> {
    if extract_exact_bundle(input)?.is_some() || extract_compatible_bundle(input)?.is_some() {
        return Err(Ctk3CodecError::invalid("a segment cannot contain a bundle"));
    }
    let (transport, payload) = extract_exact_payload(input)?;
    decode_single_payload(transport, payload)
}

/// Returns strict aggregate metadata after fully validating every segment.
pub fn inspect_ctk3_exact(input: &str) -> Result<Ctk3DocumentInfo, Ctk3CodecError> {
    if let Some(payloads) = extract_exact_bundle(input)? {
        let mut width = None;
        let mut page_count = 0usize;
        let mut segment_page_counts = Vec::with_capacity(payloads.len());
        for (index, payload) in payloads.iter().enumerate() {
            let document = decode_single_payload(Transport::Ctk64, payload)?;
            if let Some(expected) = width {
                if document.width != expected {
                    return Err(Ctk3CodecError::BundleWidthMismatch { index });
                }
            } else {
                width = Some(document.width);
            }
            page_count = page_count
                .checked_add(document.pages.len())
                .ok_or(Ctk3CodecError::IntegerOverflow)?;
            if page_count > CTK3_MAX_BUNDLE_PAGES {
                return Err(Ctk3CodecError::BundlePageLimitExceeded);
            }
            segment_page_counts.push(document.pages.len());
        }
        Ok(Ctk3DocumentInfo {
            width: width.ok_or_else(|| Ctk3CodecError::invalid("bundle is empty"))?,
            page_count,
            segment_count: payloads.len(),
            segment_page_counts,
            bundled: true,
        })
    } else {
        let document = decode_ctk3_exact(input)?;
        Ok(Ctk3DocumentInfo {
            width: document.width,
            page_count: document.pages.len(),
            segment_count: 1,
            segment_page_counts: vec![document.pages.len()],
            bundled: false,
        })
    }
}

pub fn split_ctk3_segments(input: &str) -> Result<Vec<String>, Ctk3CodecError> {
    if let Some(payloads) = extract_exact_bundle(input)? {
        Ok(payloads
            .into_iter()
            .map(|payload| format!("{CTK3_PREFIX}{payload}"))
            .collect())
    } else {
        let source = input.trim();
        decode_ctk3_segment(source)?;
        Ok(vec![source.to_owned()])
    }
}

fn decode_bundle_payloads(payloads: &[&str]) -> Result<Ctk3Document, Ctk3CodecError> {
    if payloads.len() > CTK3_MAX_BUNDLE_PAGES {
        return Err(Ctk3CodecError::BundlePageLimitExceeded);
    }
    let mut width = None;
    let mut pages = Vec::new();
    for (index, payload) in payloads.iter().enumerate() {
        let document = decode_single_payload(Transport::Ctk64, payload)?;
        if let Some(expected) = width {
            if document.width != expected {
                return Err(Ctk3CodecError::BundleWidthMismatch { index });
            }
        } else {
            width = Some(document.width);
        }
        if document.pages.len() > CTK3_MAX_BUNDLE_PAGES - pages.len() {
            return Err(Ctk3CodecError::BundlePageLimitExceeded);
        }
        pages.extend(document.pages);
    }
    Ok(Ctk3Document {
        width: width.ok_or_else(|| Ctk3CodecError::invalid("bundle is empty"))?,
        pages,
    })
}

fn decode_single_payload(
    transport: Transport,
    encoded: &str,
) -> Result<Ctk3Document, Ctk3CodecError> {
    let payload = decode_payload(transport, encoded)?;
    if payload.len() < 4 || payload.len() > MAX_PAYLOAD_BYTES {
        return Err(Ctk3CodecError::invalid("payload length is invalid"));
    }
    let body_len = payload.len() - 2;
    let body = &payload[..body_len];
    let expected = u16::from_be_bytes([payload[body_len], payload[body_len + 1]]);
    if crc16(body) != expected {
        return Err(Ctk3CodecError::invalid("checksum does not match"));
    }
    let mut reader = BitReader::new(body);
    if reader.read_bits(8)? != MAGIC {
        return Err(Ctk3CodecError::invalid("payload header is invalid"));
    }
    let revision = reader.read_bits(3)?;
    if ![
        LEGACY_SCHEMA_REVISION,
        COMPACT_SCHEMA_REVISION,
        TEMPORAL_SCHEMA_REVISION,
        SHARED_FIELD_SCHEMA_REVISION,
    ]
    .contains(&revision)
    {
        return Err(Ctk3CodecError::invalid("schema revision is unsupported"));
    }
    let width = reader.read_bits(5)? as usize + 1;
    if width == 0 || width > MAX_WIDTH {
        return Err(Ctk3CodecError::InvalidWidth { width });
    }
    let page_count = reader.read_var_uint()? as usize;
    if page_count == 0 || page_count > CTK3_MAX_SEGMENT_PAGES {
        return Err(Ctk3CodecError::InvalidPageCount { count: page_count });
    }
    if reader.read_bit()? {
        return Err(Ctk3CodecError::invalid("extension block is unsupported"));
    }
    let normalized =
        if revision == TEMPORAL_SCHEMA_REVISION || revision == SHARED_FIELD_SCHEMA_REVISION {
            let shared = if revision == SHARED_FIELD_SCHEMA_REVISION {
                let height = reader.read_var_uint()? as usize;
                if height == 0 || height > MAX_HEIGHT {
                    return Err(Ctk3CodecError::invalid("shared field height is invalid"));
                }
                let count = width
                    .checked_mul(height)
                    .ok_or(Ctk3CodecError::IntegerOverflow)?;
                Some(SharedFieldPredictor {
                    codes: read_cell_encoding(&mut reader, count, None, 4)?,
                })
            } else {
                None
            };
            read_temporal_pages(&mut reader, width, page_count, shared.as_ref())?
        } else {
            read_linear_pages(&mut reader, width, page_count, revision)?
        };
    reader.assert_zero_padding()?;
    Ok(Ctk3Document {
        width,
        pages: normalized
            .iter()
            .map(normalized_to_page)
            .collect::<Result<_, _>>()?,
    })
}

fn read_linear_pages(
    reader: &mut BitReader<'_>,
    width: usize,
    page_count: usize,
    revision: u32,
) -> Result<Vec<NormalizedPage>, Ctk3CodecError> {
    let mut pages = Vec::with_capacity(page_count);
    for _ in 0..page_count {
        let page = if revision == LEGACY_SCHEMA_REVISION {
            read_legacy_page(reader, width, pages.last())?
        } else {
            read_compact_page(reader, width, pages.last())?
        };
        pages.push(page);
    }
    Ok(pages)
}

fn read_compact_page(
    reader: &mut BitReader<'_>,
    width: usize,
    previous: Option<&NormalizedPage>,
) -> Result<NormalizedPage, Ctk3CodecError> {
    let flags = match reader.read_bits(2)? {
        0 => Ctk3PageFlags::default(),
        1 => previous
            .map(|page| page.flags)
            .ok_or_else(|| Ctk3CodecError::invalid("page flag reference is invalid"))?,
        2 => Ctk3PageFlags::from_bits(reader.read_bits(5)?),
        _ => return Err(Ctk3CodecError::invalid("page flag mode is invalid")),
    };
    let has_comment = reader.read_bit()?;
    let has_operation = reader.read_bit()?;
    let has_garbage = reader.read_bit()?;
    let height = if let Some(previous) = previous {
        if reader.read_bit()? {
            previous.height
        } else {
            reader.read_var_uint()? as usize
        }
    } else {
        reader.read_var_uint()? as usize
    };
    if height > MAX_HEIGHT {
        return Err(Ctk3CodecError::invalid("page height is invalid"));
    }
    let cell_count = width
        .checked_mul(height)
        .ok_or(Ctk3CodecError::IntegerOverflow)?;
    let codes = read_cell_encoding(
        reader,
        cell_count,
        previous.map(|page| page.codes.as_slice()),
        3,
    )?;
    let garbage_codes = if has_garbage {
        Some(read_cell_encoding(reader, width, None, 3)?)
    } else {
        None
    };
    let comment = if has_comment {
        read_comment_literal(reader)?
    } else {
        String::new()
    };
    let operation = if has_operation {
        Some(read_operation_body(reader)?)
    } else {
        None
    };
    Ok(NormalizedPage {
        height,
        codes,
        comment,
        operation,
        flags,
        garbage_codes,
    })
}

fn read_legacy_page(
    reader: &mut BitReader<'_>,
    width: usize,
    previous: Option<&NormalizedPage>,
) -> Result<NormalizedPage, Ctk3CodecError> {
    let metadata = reader.read_bits(8)?;
    let flags = Ctk3PageFlags::from_bits(metadata & 0x1f);
    let height = reader.read_var_uint()? as usize;
    if height > MAX_HEIGHT {
        return Err(Ctk3CodecError::invalid("page height is invalid"));
    }
    let count = width
        .checked_mul(height)
        .ok_or(Ctk3CodecError::IntegerOverflow)?;
    let codes = read_cell_encoding(reader, count, previous.map(|page| page.codes.as_slice()), 2)?;
    let garbage_codes = if metadata & 0x80 != 0 {
        Some(read_cell_encoding(reader, width, None, 2)?)
    } else {
        None
    };
    let comment = if metadata & 0x20 != 0 {
        read_comment_literal(reader)?
    } else {
        String::new()
    };
    let operation = if metadata & 0x40 != 0 {
        let piece = Ctk3Piece::from_wire_index(reader.read_bits(3)?)
            .ok_or_else(|| Ctk3CodecError::invalid("operation piece is invalid"))?;
        let rotation = Ctk3Rotation::from_quarter_turns(reader.read_bits(2)?)
            .ok_or_else(|| Ctk3CodecError::invalid("operation rotation is invalid"))?;
        Some(normalize_decoded_operation(Ctk3Operation {
            piece,
            rotation,
            x: read_coordinate(reader)?,
            y: read_coordinate(reader)?,
        })?)
    } else {
        None
    };
    Ok(NormalizedPage {
        height,
        codes,
        comment,
        operation,
        flags,
        garbage_codes,
    })
}

fn read_temporal_pages(
    reader: &mut BitReader<'_>,
    width: usize,
    page_count: usize,
    shared: Option<&SharedFieldPredictor>,
) -> Result<Vec<NormalizedPage>, Ctk3CodecError> {
    let mut pages = Vec::with_capacity(page_count);
    while pages.len() < page_count {
        match reader.read_bits(2)? {
            0 => {
                let page = read_temporal_page(reader, width, &pages, shared)?;
                pages.push(page);
            }
            1 => {
                let previous = pages
                    .last()
                    .cloned()
                    .ok_or_else(|| Ctk3CodecError::invalid("repeated page has no reference"))?;
                pages.push(previous);
            }
            2 => {
                let distance = reader.read_var_uint()? as usize + 1;
                let index = pages
                    .len()
                    .checked_sub(distance)
                    .ok_or_else(|| Ctk3CodecError::invalid("page reference is invalid"))?;
                pages.push(pages[index].clone());
            }
            _ => {
                let previous = pages
                    .last()
                    .cloned()
                    .ok_or_else(|| Ctk3CodecError::invalid("repeated page run has no reference"))?;
                let repeat = reader.read_var_uint()? as usize + 2;
                if repeat > page_count - pages.len() {
                    return Err(Ctk3CodecError::invalid("repeated page run is invalid"));
                }
                pages.extend(core::iter::repeat_n(previous, repeat));
            }
        }
    }
    Ok(pages)
}

fn read_temporal_page(
    reader: &mut BitReader<'_>,
    width: usize,
    history: &[NormalizedPage],
    shared: Option<&SharedFieldPredictor>,
) -> Result<NormalizedPage, Ctk3CodecError> {
    let previous = history.last();
    let flags = read_temporal_flags(reader, previous.map(|page| page.flags))?;
    let height = if let Some(previous) = previous {
        if reader.read_bit()? {
            reader.read_var_uint()? as usize
        } else {
            previous.height
        }
    } else {
        reader.read_var_uint()? as usize
    };
    if height > MAX_HEIGHT {
        return Err(Ctk3CodecError::invalid("page height is invalid"));
    }
    let codes = read_temporal_field(reader, width, height, history, shared)?;
    let garbage_codes = read_temporal_garbage(
        reader,
        width,
        previous.and_then(|page| page.garbage_codes.as_deref()),
    )?;
    let comment = read_temporal_comment(reader, history)?;
    let operation = read_temporal_operation(reader, previous.and_then(|page| page.operation))?;
    Ok(NormalizedPage {
        height,
        codes,
        comment,
        operation,
        flags,
        garbage_codes,
    })
}

fn read_temporal_flags(
    reader: &mut BitReader<'_>,
    previous: Option<Ctk3PageFlags>,
) -> Result<Ctk3PageFlags, Ctk3CodecError> {
    if !reader.read_bit()? {
        return Ok(Ctk3PageFlags::default());
    }
    if !reader.read_bit()? {
        return previous.ok_or_else(|| Ctk3CodecError::invalid("page flag reference is invalid"));
    }
    Ok(Ctk3PageFlags::from_bits(reader.read_bits(5)?))
}

fn read_temporal_garbage(
    reader: &mut BitReader<'_>,
    width: usize,
    previous: Option<&[u8]>,
) -> Result<Option<Vec<u8>>, Ctk3CodecError> {
    if !reader.read_bit()? {
        return Ok(None);
    }
    if !reader.read_bit()? {
        return previous
            .map(|codes| Some(codes.to_vec()))
            .ok_or_else(|| Ctk3CodecError::invalid("garbage reference is invalid"));
    }
    Ok(Some(read_cell_encoding(reader, width, previous, 4)?))
}

fn read_temporal_comment(
    reader: &mut BitReader<'_>,
    history: &[NormalizedPage],
) -> Result<String, Ctk3CodecError> {
    if !reader.read_bit()? {
        return Ok(String::new());
    }
    if !reader.read_bit()? {
        return history
            .last()
            .map(|page| page.comment.clone())
            .ok_or_else(|| Ctk3CodecError::invalid("comment reference is invalid"));
    }
    if !reader.read_bit()? {
        let distance = reader.read_var_uint()? as usize + 1;
        let index = history
            .len()
            .checked_sub(distance)
            .ok_or_else(|| Ctk3CodecError::invalid("comment reference is invalid"))?;
        if history[index].comment.is_empty() {
            return Err(Ctk3CodecError::invalid("comment reference is empty"));
        }
        return Ok(history[index].comment.clone());
    }
    read_comment_literal(reader)
}

fn read_temporal_operation(
    reader: &mut BitReader<'_>,
    previous: Option<Ctk3Operation>,
) -> Result<Option<Ctk3Operation>, Ctk3CodecError> {
    if !reader.read_bit()? {
        return Ok(None);
    }
    if !reader.read_bit()? {
        return previous
            .map(Some)
            .ok_or_else(|| Ctk3CodecError::invalid("operation reference is invalid"));
    }
    if !reader.read_bit()? {
        let previous =
            previous.ok_or_else(|| Ctk3CodecError::invalid("operation delta has no reference"))?;
        let x = i64::from(previous.x)
            .checked_add(reader.read_signed_var_int()?)
            .ok_or(Ctk3CodecError::IntegerOverflow)?;
        let y = i64::from(previous.y)
            .checked_add(reader.read_signed_var_int()?)
            .ok_or(Ctk3CodecError::IntegerOverflow)?;
        return Ok(Some(normalize_decoded_operation(Ctk3Operation {
            x: i32::try_from(x).map_err(|_| Ctk3CodecError::invalid("operation is invalid"))?,
            y: i32::try_from(y).map_err(|_| Ctk3CodecError::invalid("operation is invalid"))?,
            ..previous
        })?));
    }
    Ok(Some(read_operation_body(reader)?))
}

fn read_operation_body(reader: &mut BitReader<'_>) -> Result<Ctk3Operation, Ctk3CodecError> {
    let piece = Ctk3Piece::from_wire_index(reader.read_bits(3)?)
        .ok_or_else(|| Ctk3CodecError::invalid("operation piece is invalid"))?;
    let rotations = operation_rotations(piece);
    let rotation = rotations
        .get(reader.read_bits(bits_for_choices(rotations.len()))? as usize)
        .copied()
        .ok_or_else(|| Ctk3CodecError::invalid("operation rotation is invalid"))?;
    normalize_decoded_operation(Ctk3Operation {
        piece,
        rotation,
        x: read_coordinate(reader)?,
        y: read_coordinate(reader)?,
    })
}

fn read_coordinate(reader: &mut BitReader<'_>) -> Result<i32, Ctk3CodecError> {
    let value = reader.read_signed_var_int()?;
    if value.unsigned_abs() > MAX_OPERATION_COORDINATE as u64 {
        return Err(Ctk3CodecError::invalid("operation coordinate is invalid"));
    }
    i32::try_from(value).map_err(|_| Ctk3CodecError::invalid("operation coordinate is invalid"))
}

fn normalize_decoded_operation(operation: Ctk3Operation) -> Result<Ctk3Operation, Ctk3CodecError> {
    if operation.x.unsigned_abs() > MAX_OPERATION_COORDINATE as u32
        || operation.y.unsigned_abs() > MAX_OPERATION_COORDINATE as u32
    {
        return Err(Ctk3CodecError::invalid("operation is invalid"));
    }
    let canonical = canonicalize_operation(operation)
        .ok_or_else(|| Ctk3CodecError::invalid("operation is invalid"))?;
    if canonical.x.unsigned_abs() > MAX_OPERATION_COORDINATE as u32
        || canonical.y.unsigned_abs() > MAX_OPERATION_COORDINATE as u32
    {
        return Err(Ctk3CodecError::invalid("operation is invalid"));
    }
    Ok(canonical)
}

fn read_comment_literal(reader: &mut BitReader<'_>) -> Result<String, Ctk3CodecError> {
    let length = reader.read_var_uint()? as usize;
    if length > MAX_COMMENT_BYTES {
        return Err(Ctk3CodecError::invalid("page comment is too long"));
    }
    String::from_utf8(reader.read_bytes(length)?).map_err(|_| Ctk3CodecError::InvalidUtf8)
}

fn read_temporal_field(
    reader: &mut BitReader<'_>,
    width: usize,
    height: usize,
    history: &[NormalizedPage],
    shared: Option<&SharedFieldPredictor>,
) -> Result<Vec<u8>, Ctk3CodecError> {
    let mode = reader.read_bits(4)?;
    let target_len = width
        .checked_mul(height)
        .ok_or(Ctk3CodecError::IntegerOverflow)?;
    let previous = history.last();
    let predictor = match mode {
        0 => None,
        1..=7 => {
            let previous = previous
                .ok_or_else(|| Ctk3CodecError::invalid("temporal field has no previous page"))?;
            Some(match mode {
                1 => fit_codes(&previous.codes, target_len),
                2 => grayscale_codes(&previous.codes, target_len),
                3 => mirror_codes(&previous.codes, previous.height, width, height),
                4 => predict_locked_codes(previous, width, height, false, false),
                5 => predict_locked_codes(previous, width, height, false, true),
                6 => predict_locked_codes(previous, width, height, true, false),
                7 => predict_locked_codes(previous, width, height, true, true),
                _ => unreachable!(),
            })
        }
        8 | 9 => {
            let distance = reader.read_var_uint()? as usize + 1;
            let index = history
                .len()
                .checked_sub(distance)
                .ok_or_else(|| Ctk3CodecError::invalid("temporal field reference is invalid"))?;
            let reference = &history[index];
            Some(if mode == 8 {
                fit_codes(&reference.codes, target_len)
            } else {
                mirror_codes(&reference.codes, reference.height, width, height)
            })
        }
        10 => Some(fit_codes(
            &shared
                .ok_or_else(|| Ctk3CodecError::invalid("shared field reference is invalid"))?
                .codes,
            target_len,
        )),
        _ => return Err(Ctk3CodecError::invalid("temporal field mode is invalid")),
    };
    read_cell_encoding(reader, target_len, predictor.as_deref(), 4)
}

fn predict_locked_codes(
    previous: &NormalizedPage,
    width: usize,
    target_height: usize,
    clear_rows: bool,
    grayscale: bool,
) -> Vec<u8> {
    let occupied = previous
        .operation
        .map(operation_cells)
        .unwrap_or([(0, 0); 4]);
    let occupied_len = if previous.operation.is_some() { 4 } else { 0 };
    let operation_height = occupied[..occupied_len]
        .iter()
        .fold(0i64, |height, (_, y)| height.max(y + 1));
    let source_height = if operation_height >= 0 && operation_height as usize <= MAX_HEIGHT {
        previous.height.max(operation_height as usize)
    } else {
        previous.height
    };
    let mut cells = fit_codes(&previous.codes, source_height * width);
    if let Some(operation) = previous.operation.filter(|_| previous.flags.lock) {
        if occupied[..occupied_len].iter().all(|(x, y)| {
            *x >= 0
                && (*x as usize) < width
                && *y >= 0
                && (*y as usize) < source_height
                && cells[*y as usize * width + *x as usize] == 0
        }) {
            let code = if previous.flags.colorize {
                operation.piece.wire_index() as u8 + 2
            } else {
                1
            };
            for (x, y) in &occupied[..occupied_len] {
                cells[*y as usize * width + *x as usize] = code;
            }
        }
    }
    let mut transformed = Vec::with_capacity(cells.len());
    for row in cells.chunks_exact(width) {
        if clear_rows && row.iter().all(|code| *code != 0) {
            continue;
        }
        transformed.extend(
            row.iter()
                .map(|code| if grayscale && *code != 0 { 1 } else { *code }),
        );
    }
    fit_codes(&transformed, target_height * width)
}

fn fit_codes(codes: &[u8], target_len: usize) -> Vec<u8> {
    (0..target_len)
        .map(|index| codes.get(index).copied().unwrap_or(0))
        .collect()
}

fn grayscale_codes(codes: &[u8], target_len: usize) -> Vec<u8> {
    (0..target_len)
        .map(|index| u8::from(codes.get(index).copied().unwrap_or(0) != 0))
        .collect()
}

fn mirror_codes(
    source: &[u8],
    source_height: usize,
    width: usize,
    target_height: usize,
) -> Vec<u8> {
    let mut mirrored = vec![0; target_height * width];
    for y in 0..source_height.min(target_height) {
        for x in 0..width {
            mirrored[y * width + (width - x - 1)] =
                match source.get(y * width + x).copied().unwrap_or(0) {
                    5 => 6,
                    6 => 5,
                    7 => 8,
                    8 => 7,
                    code => code,
                };
        }
    }
    mirrored
}

#[cfg(test)]
mod tests {
    use crate::{encode_ctk3, Ctk3Color, Ctk3Page, Ctk3Piece};

    use super::*;

    #[test]
    fn native_round_trip_preserves_normalized_semantics() {
        let document = Ctk3Document::new(
            10,
            vec![
                Ctk3Page::new(1, vec![Ctk3Color::Piece(Ctk3Piece::T); 10]).with_comment("주석 😀"),
                Ctk3Page::new(1, vec![Ctk3Color::Piece(Ctk3Piece::T); 10]).with_comment("주석 😀"),
            ],
        );
        let encoded = encode_ctk3(&document).expect("encode");
        assert_eq!(decode_ctk3_exact(&encoded), Ok(document));
    }

    #[test]
    fn checksum_and_noncanonical_transport_fail_closed() {
        assert!(decode_ctk3_exact("ctk3_w0kCAAdPCA").is_err());
        assert!(decode_ctk3_exact("ctk3_AB").is_err());
    }

    #[test]
    fn legacy_ctk85_remains_interoperable() {
        let legacy = "ctk3@.:)aB*t&hPEXlYu:YoUl/4cH8Ga[PB0z";
        let document = decode_ctk3_exact(legacy).expect("legacy fixture");
        assert_eq!(document.width, 10);
        assert!(!document.pages.is_empty());
    }
}
