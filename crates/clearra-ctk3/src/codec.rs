// SRP rationale: this module has one change reason: deterministic CTK3 document encoding and decoding.
use std::collections::HashMap;
use std::io::Write;

use crate::bitstream::BitWriter;
use crate::cell::{bits_for_choices, write_best_cell_encoding, write_best_predicted_cell_encoding};
use crate::decoder::decode_ctk3_segment;
use crate::geometry::{operation_cells, operation_rotations};
use crate::transport::{crc16, encode_ctk64, extract_exact_payload, Transport};
use crate::{
    normalize_document, Ctk3CodecError, Ctk3Document, Ctk3Operation, Ctk3PageFlags, Ctk3WriteError,
    NormalizedPage, COMPACT_SCHEMA_REVISION, CTK3_BUNDLE_PREFIX, CTK3_MAX_BUNDLE_PAGES,
    CTK3_PREFIX, MAGIC, MAX_HEIGHT, MAX_PAYLOAD_BYTES, SHARED_FIELD_SCHEMA_REVISION,
    TEMPORAL_SCHEMA_REVISION,
};

const TEMPORAL_REFERENCE_WINDOW: usize = 16;

#[derive(Clone, Debug)]
struct SharedFieldPredictor {
    height: usize,
    codes: Vec<u8>,
}

struct TemporalContext<'a> {
    pages: &'a [NormalizedPage],
    shared_field: Option<&'a SharedFieldPredictor>,
    latest_field: HashMap<(usize, Vec<u8>), usize>,
    latest_page: HashMap<NormalizedPage, usize>,
    latest_comment: HashMap<String, usize>,
}

/// Encodes the shortest canonical CTK3 revision accepted by the shared codec.
///
/// Revision 1, revision 2, and (when applicable) revision 3 are encoded in
/// full. The shortest byte payload wins; equal lengths prefer the lower
/// revision exactly as `packages/ctk3` does.
pub fn encode_ctk3(document: &Ctk3Document) -> Result<String, Ctk3CodecError> {
    let pages = normalize_document(document)?;
    let mut candidates = vec![
        encode_normalized(document.width, &pages, COMPACT_SCHEMA_REVISION, None)?,
        encode_normalized(document.width, &pages, TEMPORAL_SCHEMA_REVISION, None)?,
    ];
    let shared = build_shared_field(&pages, document.width);
    if let Some(shared) = &shared {
        candidates.push(encode_normalized(
            document.width,
            &pages,
            SHARED_FIELD_SCHEMA_REVISION,
            Some(shared),
        )?);
    }
    let mut selected = candidates.remove(0);
    for candidate in candidates {
        if candidate.1.len() < selected.1.len()
            || (candidate.1.len() == selected.1.len() && candidate.0 < selected.0)
        {
            selected = candidate;
        }
    }
    Ok(format!("{CTK3_PREFIX}{}", encode_ctk64(&selected.1)))
}

pub fn encode_ctk3_into(
    document: &Ctk3Document,
    destination: &mut impl Write,
) -> Result<(), Ctk3WriteError> {
    destination.write_all(encode_ctk3(document)?.as_bytes())?;
    Ok(())
}

/// Encodes the deterministic revision-1 CTK3 streaming segment.
pub fn encode_ctk3_compact(document: &Ctk3Document) -> Result<String, Ctk3CodecError> {
    let pages = normalize_document(document)?;
    let (_, payload) = encode_normalized(document.width, &pages, COMPACT_SCHEMA_REVISION, None)?;
    Ok(format!("{CTK3_PREFIX}{}", encode_ctk64(&payload)))
}

pub fn encode_ctk3_compact_into(
    document: &Ctk3Document,
    destination: &mut impl Write,
) -> Result<(), Ctk3WriteError> {
    destination.write_all(encode_ctk3_compact(document)?.as_bytes())?;
    Ok(())
}

/// Creates a standard `ctk3b_` envelope from strict, fully valid CTK64 segments.
pub fn encode_ctk3_bundle(segments: &[String]) -> Result<String, Ctk3CodecError> {
    if segments.is_empty() || segments.len() > CTK3_MAX_BUNDLE_PAGES {
        return Err(Ctk3CodecError::InvalidBundleSegmentCount {
            count: segments.len(),
        });
    }
    let mut width = None;
    let mut page_count = 0usize;
    let mut payloads = Vec::with_capacity(segments.len());
    for (index, segment) in segments.iter().enumerate() {
        let (transport, payload) = extract_exact_payload(segment)
            .map_err(|_| Ctk3CodecError::InvalidBundleSegment { index })?;
        if transport != Transport::Ctk64 {
            return Err(Ctk3CodecError::InvalidBundleSegment { index });
        }
        let document = decode_ctk3_segment(segment)
            .map_err(|_| Ctk3CodecError::InvalidBundleSegment { index })?;
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
        payloads.push(payload);
    }
    if payloads.len() == 1 {
        return Ok(format!("{CTK3_PREFIX}{}", payloads[0]));
    }
    Ok(format!("{CTK3_BUNDLE_PREFIX}{}", payloads.join(".")))
}

/// Validates all segments before emitting any bytes, then streams the bundle
/// envelope without allocating the aggregate string.
pub fn encode_ctk3_bundle_into(
    segments: &[String],
    destination: &mut impl Write,
) -> Result<(), Ctk3WriteError> {
    if segments.is_empty() || segments.len() > CTK3_MAX_BUNDLE_PAGES {
        return Err(Ctk3CodecError::InvalidBundleSegmentCount {
            count: segments.len(),
        }
        .into());
    }
    let mut width = None;
    let mut page_count = 0usize;
    let mut payloads = Vec::with_capacity(segments.len());
    for (index, segment) in segments.iter().enumerate() {
        let (transport, payload) = extract_exact_payload(segment)
            .map_err(|_| Ctk3CodecError::InvalidBundleSegment { index })?;
        if transport != Transport::Ctk64 {
            return Err(Ctk3CodecError::InvalidBundleSegment { index }.into());
        }
        let document = decode_ctk3_segment(segment)
            .map_err(|_| Ctk3CodecError::InvalidBundleSegment { index })?;
        if let Some(expected) = width {
            if document.width != expected {
                return Err(Ctk3CodecError::BundleWidthMismatch { index }.into());
            }
        } else {
            width = Some(document.width);
        }
        page_count = page_count
            .checked_add(document.pages.len())
            .ok_or(Ctk3CodecError::IntegerOverflow)?;
        if page_count > CTK3_MAX_BUNDLE_PAGES {
            return Err(Ctk3CodecError::BundlePageLimitExceeded.into());
        }
        payloads.push(payload);
    }
    if payloads.len() == 1 {
        destination.write_all(CTK3_PREFIX.as_bytes())?;
        destination.write_all(payloads[0].as_bytes())?;
        return Ok(());
    }
    destination.write_all(CTK3_BUNDLE_PREFIX.as_bytes())?;
    for (index, payload) in payloads.into_iter().enumerate() {
        if index != 0 {
            destination.write_all(b".")?;
        }
        destination.write_all(payload.as_bytes())?;
    }
    Ok(())
}

/// Encodes a large logical document as bounded revision-1 segments and writes
/// the CTK3 framing incrementally.
///
/// Only one encoded segment is held at a time. A single input document is
/// emitted as a normal `ctk3_` value; two or more are emitted as `ctk3b_`.
/// Callers that require all-or-nothing publication should provide an atomic
/// temporary-file sink and commit it only after this function succeeds.
pub fn encode_ctk3_segmented_documents_into(
    documents: &[Ctk3Document],
    destination: &mut impl Write,
) -> Result<(), Ctk3WriteError> {
    encode_ctk3_segmented_documents_iter_into(
        documents.iter().cloned(),
        documents.len(),
        destination,
    )
}

/// Owned, exact-size counterpart of [`encode_ctk3_segmented_documents_into`].
///
/// The iterator is consumed one document at a time. Once its segment has been
/// written, that document is dropped before the next item is requested. This
/// keeps live page/cell storage segment-bounded even at the 1,048,576-page
/// logical-document limit. A late validation or I/O failure can leave bytes in
/// `destination`; callers must discard or atomically abort that sink.
pub fn encode_ctk3_segmented_documents_iter_into<I, W>(
    documents: I,
    segment_count: usize,
    destination: &mut W,
) -> Result<(), Ctk3WriteError>
where
    I: IntoIterator<Item = Ctk3Document>,
    I::IntoIter: ExactSizeIterator,
    W: Write + ?Sized,
{
    let mut documents = documents.into_iter();
    let actual_count = documents.len();
    if actual_count != segment_count {
        return Err(Ctk3CodecError::BundleSegmentCountMismatch {
            expected: segment_count,
            actual: actual_count,
        }
        .into());
    }
    if segment_count == 0 || segment_count > CTK3_MAX_BUNDLE_PAGES {
        return Err(Ctk3CodecError::InvalidBundleSegmentCount {
            count: segment_count,
        }
        .into());
    }

    let first = documents
        .next()
        .ok_or(Ctk3CodecError::InvalidBundleSegmentCount { count: 0 })?;
    let expected_width = first.width;
    let mut page_count = first.pages.len();
    if page_count > CTK3_MAX_BUNDLE_PAGES {
        return Err(Ctk3CodecError::BundlePageLimitExceeded.into());
    }
    let first_segment = encode_ctk3_compact(&first)?;
    if segment_count == 1 {
        destination.write_all(first_segment.as_bytes())?;
        return Ok(());
    }
    destination.write_all(CTK3_BUNDLE_PREFIX.as_bytes())?;
    write_segment_payload(&first_segment, destination)?;
    drop(first_segment);
    drop(first);

    for (offset, document) in documents.enumerate() {
        let index = offset + 1;
        if document.width != expected_width {
            return Err(Ctk3CodecError::BundleWidthMismatch { index }.into());
        }
        page_count = page_count
            .checked_add(document.pages.len())
            .ok_or(Ctk3CodecError::IntegerOverflow)?;
        if page_count > CTK3_MAX_BUNDLE_PAGES {
            return Err(Ctk3CodecError::BundlePageLimitExceeded.into());
        }
        let segment = encode_ctk3_compact(&document)?;
        destination.write_all(b".")?;
        write_segment_payload(&segment, destination)?;
    }
    Ok(())
}

fn write_segment_payload(
    segment: &str,
    destination: &mut (impl Write + ?Sized),
) -> Result<(), Ctk3WriteError> {
    let payload = segment
        .strip_prefix(CTK3_PREFIX)
        .ok_or_else(|| Ctk3CodecError::invalid("native segment prefix is invalid"))?;
    destination.write_all(payload.as_bytes())?;
    Ok(())
}

fn encode_normalized(
    width: usize,
    pages: &[NormalizedPage],
    revision: u32,
    shared_field: Option<&SharedFieldPredictor>,
) -> Result<(u32, Vec<u8>), Ctk3CodecError> {
    let mut writer = BitWriter::default();
    writer.write_bits(MAGIC, 8)?;
    writer.write_bits(revision, 3)?;
    writer.write_bits((width - 1) as u32, 5)?;
    writer.write_var_uint(pages.len() as u64)?;
    writer.write_bit(false);
    match revision {
        COMPACT_SCHEMA_REVISION => {
            for (index, page) in pages.iter().enumerate() {
                write_compact_page(
                    &mut writer,
                    page,
                    index.checked_sub(1).map(|prior| &pages[prior]),
                )?;
            }
        }
        TEMPORAL_SCHEMA_REVISION => write_temporal_pages(&mut writer, pages, width, None)?,
        SHARED_FIELD_SCHEMA_REVISION => {
            let shared =
                shared_field.ok_or_else(|| Ctk3CodecError::invalid("shared field is missing"))?;
            writer.write_var_uint(shared.height as u64)?;
            write_best_cell_encoding(&mut writer, &shared.codes, None, 4, true)?;
            write_temporal_pages(&mut writer, pages, width, Some(shared))?;
        }
        _ => return Err(Ctk3CodecError::invalid("schema revision is unsupported")),
    }
    let body = writer.into_bytes();
    let payload_size = body
        .len()
        .checked_add(2)
        .ok_or(Ctk3CodecError::IntegerOverflow)?;
    if payload_size > MAX_PAYLOAD_BYTES {
        return Err(Ctk3CodecError::PayloadTooLarge {
            bytes: payload_size,
        });
    }
    let checksum = crc16(&body);
    let mut payload = body;
    payload.extend_from_slice(&checksum.to_be_bytes());
    Ok((revision, payload))
}

fn write_compact_page(
    writer: &mut BitWriter,
    page: &NormalizedPage,
    previous: Option<&NormalizedPage>,
) -> Result<(), Ctk3CodecError> {
    let flag_mode = if page.flags == Ctk3PageFlags::default() {
        0
    } else if previous.is_some_and(|previous| previous.flags == page.flags) {
        1
    } else {
        2
    };
    writer.write_bits(flag_mode, 2)?;
    if flag_mode == 2 {
        writer.write_bits(page.flags.bits(), 5)?;
    }
    writer.write_bit(!page.comment.is_empty());
    writer.write_bit(page.operation.is_some());
    writer.write_bit(page.garbage_codes.is_some());
    if let Some(previous) = previous {
        let same_height = page.height == previous.height;
        writer.write_bit(same_height);
        if !same_height {
            writer.write_var_uint(page.height as u64)?;
        }
    } else {
        writer.write_var_uint(page.height as u64)?;
    }
    write_best_cell_encoding(
        writer,
        &page.codes,
        previous.map(|page| page.codes.as_slice()),
        3,
        true,
    )?;
    if let Some(garbage) = &page.garbage_codes {
        write_best_cell_encoding(writer, garbage, None, 3, true)?;
    }
    if !page.comment.is_empty() {
        writer.write_var_uint(page.comment.len() as u64)?;
        writer.write_bytes(page.comment.as_bytes())?;
    }
    if let Some(operation) = page.operation {
        write_operation_body(writer, operation)?;
    }
    Ok(())
}

fn build_shared_field(pages: &[NormalizedPage], width: usize) -> Option<SharedFieldPredictor> {
    if pages.len() < 2 || pages[0].codes.is_empty() {
        return None;
    }
    let mut common = pages[0].codes.clone();
    for page in &pages[1..] {
        for (index, code) in common.iter_mut().enumerate() {
            if *code != 0 && *code != page.codes.get(index).copied().unwrap_or(0) {
                *code = 0;
            }
        }
    }
    let mut height = common.len().div_ceil(width);
    while height > 0
        && common[(height - 1) * width..height * width]
            .iter()
            .all(|code| *code == 0)
    {
        height -= 1;
    }
    if height == 0 {
        None
    } else {
        common.truncate(height * width);
        Some(SharedFieldPredictor {
            height,
            codes: common,
        })
    }
}

fn write_temporal_pages(
    writer: &mut BitWriter,
    pages: &[NormalizedPage],
    width: usize,
    shared_field: Option<&SharedFieldPredictor>,
) -> Result<(), Ctk3CodecError> {
    let mut context = TemporalContext {
        pages,
        shared_field,
        latest_field: HashMap::new(),
        latest_page: HashMap::new(),
        latest_comment: HashMap::new(),
    };
    let mut index = 0usize;
    while index < pages.len() {
        let page = &pages[index];
        let previous = index.checked_sub(1).map(|prior| &pages[prior]);
        if previous == Some(page) {
            let mut repeat_count = 1usize;
            while index + repeat_count < pages.len()
                && &pages[index + repeat_count] == previous.expect("known previous")
            {
                repeat_count += 1;
            }
            if repeat_count >= 2 {
                let mut run = BitWriter::default();
                run.write_bits(3, 2)?;
                run.write_var_uint((repeat_count - 2) as u64)?;
                if run.bit_len < repeat_count * 2 {
                    writer.append(&run);
                    for offset in 0..repeat_count {
                        record_temporal_page(&mut context, index + offset);
                    }
                    index += repeat_count;
                    continue;
                }
            }
        }
        let mut candidates = Vec::new();
        let mut normal = BitWriter::default();
        normal.write_bits(0, 2)?;
        write_temporal_page(&mut normal, page, &context, index, width)?;
        candidates.push(normal);
        if previous == Some(page) {
            let mut copied = BitWriter::default();
            copied.write_bits(1, 2)?;
            candidates.push(copied);
        }
        if let Some(prior) = context
            .latest_page
            .get(page)
            .copied()
            .filter(|prior| *prior + 1 != index)
        {
            let mut referenced = BitWriter::default();
            referenced.write_bits(2, 2)?;
            referenced.write_var_uint((index - prior - 1) as u64)?;
            candidates.push(referenced);
        }
        append_shortest(writer, candidates);
        record_temporal_page(&mut context, index);
        index += 1;
    }
    Ok(())
}

fn write_temporal_page(
    writer: &mut BitWriter,
    page: &NormalizedPage,
    context: &TemporalContext<'_>,
    index: usize,
    width: usize,
) -> Result<(), Ctk3CodecError> {
    let previous = index.checked_sub(1).map(|prior| &context.pages[prior]);
    write_temporal_flags(writer, page.flags, previous.map(|page| page.flags))?;
    if let Some(previous) = previous {
        let same_height = page.height == previous.height;
        writer.write_bit(!same_height);
        if !same_height {
            writer.write_var_uint(page.height as u64)?;
        }
    } else {
        writer.write_var_uint(page.height as u64)?;
    }
    write_temporal_field(writer, page, context, index, width)?;
    write_temporal_garbage(
        writer,
        page.garbage_codes.as_deref(),
        previous.and_then(|page| page.garbage_codes.as_deref()),
    )?;
    write_temporal_comment(writer, &page.comment, context, index)?;
    write_temporal_operation(
        writer,
        page.operation,
        previous.and_then(|page| page.operation),
    )?;
    Ok(())
}

fn write_temporal_flags(
    writer: &mut BitWriter,
    flags: Ctk3PageFlags,
    previous: Option<Ctk3PageFlags>,
) -> Result<(), Ctk3CodecError> {
    if flags == Ctk3PageFlags::default() {
        writer.write_bit(false);
    } else if previous == Some(flags) {
        writer.write_bits(1, 2)?;
    } else {
        writer.write_bits(3, 2)?;
        writer.write_bits(flags.bits(), 5)?;
    }
    Ok(())
}

fn write_temporal_garbage(
    writer: &mut BitWriter,
    garbage: Option<&[u8]>,
    previous: Option<&[u8]>,
) -> Result<(), Ctk3CodecError> {
    let Some(garbage) = garbage else {
        writer.write_bit(false);
        return Ok(());
    };
    if previous == Some(garbage) {
        writer.write_bits(1, 2)?;
        return Ok(());
    }
    writer.write_bits(3, 2)?;
    write_best_cell_encoding(writer, garbage, previous, 4, true)
}

fn write_temporal_comment(
    writer: &mut BitWriter,
    comment: &str,
    context: &TemporalContext<'_>,
    index: usize,
) -> Result<(), Ctk3CodecError> {
    if comment.is_empty() {
        writer.write_bit(false);
        return Ok(());
    }
    let mut candidates = Vec::new();
    let mut literal = BitWriter::default();
    literal.write_bits(7, 3)?;
    literal.write_var_uint(comment.len() as u64)?;
    literal.write_bytes(comment.as_bytes())?;
    candidates.push(literal);
    if index > 0 && context.pages[index - 1].comment == comment {
        let mut copied = BitWriter::default();
        copied.write_bits(1, 2)?;
        candidates.push(copied);
    }
    if let Some(prior) = context
        .latest_comment
        .get(comment)
        .copied()
        .filter(|prior| *prior + 1 != index)
    {
        let mut referenced = BitWriter::default();
        referenced.write_bits(3, 3)?;
        referenced.write_var_uint((index - prior - 1) as u64)?;
        candidates.push(referenced);
    }
    append_shortest(writer, candidates);
    Ok(())
}

fn write_temporal_operation(
    writer: &mut BitWriter,
    operation: Option<Ctk3Operation>,
    previous: Option<Ctk3Operation>,
) -> Result<(), Ctk3CodecError> {
    let Some(operation) = operation else {
        writer.write_bit(false);
        return Ok(());
    };
    if previous == Some(operation) {
        writer.write_bits(1, 2)?;
        return Ok(());
    }
    let mut candidates = Vec::new();
    let mut literal = BitWriter::default();
    literal.write_bits(7, 3)?;
    write_operation_body(&mut literal, operation)?;
    candidates.push(literal);
    if let Some(previous) = previous.filter(|previous| {
        previous.piece == operation.piece && previous.rotation == operation.rotation
    }) {
        let mut delta = BitWriter::default();
        delta.write_bits(3, 3)?;
        delta.write_signed_var_int(i64::from(operation.x) - i64::from(previous.x))?;
        delta.write_signed_var_int(i64::from(operation.y) - i64::from(previous.y))?;
        candidates.push(delta);
    }
    append_shortest(writer, candidates);
    Ok(())
}

fn write_operation_body(
    writer: &mut BitWriter,
    operation: Ctk3Operation,
) -> Result<(), Ctk3CodecError> {
    writer.write_bits(operation.piece.wire_index(), 3)?;
    let rotations = operation_rotations(operation.piece);
    let rotation = rotations
        .iter()
        .position(|candidate| *candidate == operation.rotation)
        .ok_or_else(|| Ctk3CodecError::invalid("operation rotation is not canonical"))?;
    writer.write_bits(rotation as u32, bits_for_choices(rotations.len()))?;
    writer.write_signed_var_int(i64::from(operation.x))?;
    writer.write_signed_var_int(i64::from(operation.y))?;
    Ok(())
}

fn write_temporal_field(
    writer: &mut BitWriter,
    page: &NormalizedPage,
    context: &TemporalContext<'_>,
    index: usize,
    width: usize,
) -> Result<(), Ctk3CodecError> {
    let mut candidates = Vec::new();
    add_field_candidate(&mut candidates, 0, &page.codes, None, None)?;
    let target_len = page.height * width;
    if index > 0 {
        let previous = &context.pages[index - 1];
        add_field_candidate(
            &mut candidates,
            1,
            &page.codes,
            Some(&fit_codes(&previous.codes, target_len)),
            None,
        )?;
        add_field_candidate(
            &mut candidates,
            2,
            &page.codes,
            Some(&grayscale_codes(&previous.codes, target_len)),
            None,
        )?;
        add_field_candidate(
            &mut candidates,
            3,
            &page.codes,
            Some(&mirror_codes(
                &previous.codes,
                previous.height,
                width,
                page.height,
            )),
            None,
        )?;
        for (mode, clear_rows, grayscale) in [
            (4, false, false),
            (5, false, true),
            (6, true, false),
            (7, true, true),
        ] {
            add_field_candidate(
                &mut candidates,
                mode,
                &page.codes,
                Some(&predict_locked_codes(
                    previous,
                    width,
                    page.height,
                    clear_rows,
                    grayscale,
                )),
                None,
            )?;
        }
    }
    let mut references = Vec::new();
    if let Some(prior) = context
        .latest_field
        .get(&(page.height, page.codes.clone()))
        .copied()
        .filter(|prior| *prior + 1 != index)
    {
        references.push(prior);
    }
    let start = index.saturating_sub(TEMPORAL_REFERENCE_WINDOW);
    for prior in start..index.saturating_sub(1) {
        if !references.contains(&prior) {
            references.push(prior);
        }
    }
    for prior in references {
        let reference = &context.pages[prior];
        let distance = index - prior;
        add_field_candidate(
            &mut candidates,
            8,
            &page.codes,
            Some(&fit_codes(&reference.codes, target_len)),
            Some(distance),
        )?;
        add_field_candidate(
            &mut candidates,
            9,
            &page.codes,
            Some(&mirror_codes(
                &reference.codes,
                reference.height,
                width,
                page.height,
            )),
            Some(distance),
        )?;
    }
    if let Some(shared) = context.shared_field {
        add_field_candidate(
            &mut candidates,
            10,
            &page.codes,
            Some(&fit_codes(&shared.codes, target_len)),
            None,
        )?;
    }
    append_shortest(writer, candidates);
    Ok(())
}

fn add_field_candidate(
    candidates: &mut Vec<BitWriter>,
    mode: u32,
    codes: &[u8],
    predictor: Option<&[u8]>,
    reference_distance: Option<usize>,
) -> Result<(), Ctk3CodecError> {
    let mut candidate = BitWriter::default();
    candidate.write_bits(mode, 4)?;
    if let Some(distance) = reference_distance {
        candidate.write_var_uint((distance - 1) as u64)?;
    }
    if let Some(predictor) = predictor {
        write_best_predicted_cell_encoding(&mut candidate, codes, predictor, 4)?;
    } else {
        write_best_cell_encoding(&mut candidate, codes, None, 4, true)?;
    }
    candidates.push(candidate);
    Ok(())
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
        let valid = occupied[..occupied_len].iter().all(|(x, y)| {
            *x >= 0
                && (*x as usize) < width
                && *y >= 0
                && (*y as usize) < source_height
                && cells[*y as usize * width + *x as usize] == 0
        });
        if valid {
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
                mirror_color(source.get(y * width + x).copied().unwrap_or(0));
        }
    }
    mirrored
}

fn mirror_color(code: u8) -> u8 {
    match code {
        5 => 6,
        6 => 5,
        7 => 8,
        8 => 7,
        _ => code,
    }
}

fn record_temporal_page(context: &mut TemporalContext<'_>, index: usize) {
    let page = &context.pages[index];
    context
        .latest_field
        .insert((page.height, page.codes.clone()), index);
    context.latest_page.insert(page.clone(), index);
    if !page.comment.is_empty() {
        context.latest_comment.insert(page.comment.clone(), index);
    }
}

fn append_shortest(writer: &mut BitWriter, candidates: Vec<BitWriter>) {
    let mut candidates = candidates.into_iter();
    let mut shortest = candidates
        .next()
        .expect("CTK3 always has a temporal candidate");
    for candidate in candidates {
        if candidate.bit_len < shortest.bit_len {
            shortest = candidate;
        }
    }
    writer.append(&shortest);
}

#[cfg(test)]
mod tests {
    use crate::{Ctk3Color, Ctk3Operation, Ctk3Page, Ctk3Piece, Ctk3Rotation};

    use super::*;

    #[test]
    fn compact_encoder_matches_original_typescript_kats() {
        let empty = Ctk3Document::new(10, vec![Ctk3Page::new(0, vec![])]);
        assert_eq!(
            encode_ctk3_compact(&empty),
            Ok("ctk3_w0kCAAdPCg".to_owned())
        );

        let colored = Ctk3Document::new(
            10,
            vec![Ctk3Page::new(
                1,
                vec![
                    Ctk3Color::Gray,
                    Ctk3Color::Empty,
                    Ctk3Color::Empty,
                    Ctk3Color::Empty,
                    Ctk3Color::Piece(Ctk3Piece::I),
                    Ctk3Color::Piece(Ctk3Piece::I),
                    Ctk3Color::Piece(Ctk3Piece::I),
                    Ctk3Color::Piece(Ctk3Piece::I),
                    Ctk3Color::Empty,
                    Ctk3Color::Empty,
                ],
            )
            .with_comment("native")],
        );
        assert_eq!(
            encode_ctk3_compact(&colored),
            Ok("ctk3_w0kCERPPgGduYXRpdmWycg".to_owned())
        );
    }

    #[test]
    fn canonical_encoder_matches_shared_typescript_revision_kats() {
        let fixture = include_str!("../../../tests/fixtures/contracts/ctk3_native_interop.v1.tsv");
        for line in fixture.lines().skip(1).filter(|line| !line.is_empty()) {
            let columns = line.split('\t').collect::<Vec<_>>();
            assert_eq!(columns.len(), 4, "malformed KAT row: {line}");
            let document = interoperability_document(columns[0]);
            assert_eq!(
                encode_ctk3(&document).as_deref(),
                Ok(columns[2]),
                "{}",
                columns[0]
            );
            assert_eq!(
                encode_ctk3_compact(&document).as_deref(),
                Ok(columns[3]),
                "{} compact",
                columns[0]
            );
            assert_eq!(
                crate::decode_ctk3_exact(columns[2]).map(|value| value.width),
                Ok(10)
            );
        }
    }

    #[test]
    fn segmented_writer_matches_validated_bundle_without_aggregate_encoding() {
        let first = Ctk3Document::new(
            10,
            vec![Ctk3Page::new(1, row(&[Ctk3Color::Piece(Ctk3Piece::I); 4]))],
        );
        let second = Ctk3Document::new(
            10,
            vec![Ctk3Page::new(1, row(&[Ctk3Color::Piece(Ctk3Piece::T); 3]))],
        );
        let segments = [&first, &second]
            .into_iter()
            .map(|document| encode_ctk3_compact(document).expect("segment"))
            .collect::<Vec<_>>();
        let expected = encode_ctk3_bundle(&segments).expect("bundle");
        let mut streamed = Vec::new();
        encode_ctk3_segmented_documents_into(&[first, second], &mut streamed)
            .expect("streamed bundle");
        assert_eq!(
            String::from_utf8(streamed).as_deref(),
            Ok(expected.as_str())
        );
        assert_eq!(
            crate::decode_ctk3_exact(&expected).map(|value| value.pages.len()),
            Ok(2)
        );
    }

    #[test]
    fn segmented_writer_returns_typed_late_error_for_atomic_sink_abort() {
        let documents = [
            Ctk3Document::new(10, vec![Ctk3Page::new(0, vec![])]),
            Ctk3Document::new(4, vec![Ctk3Page::new(0, vec![])]),
        ];
        let mut output = Vec::new();
        assert!(matches!(
            encode_ctk3_segmented_documents_into(&documents, &mut output),
            Err(crate::Ctk3WriteError::Codec(
                Ctk3CodecError::BundleWidthMismatch { index: 1 }
            ))
        ));
        assert!(output.starts_with(CTK3_BUNDLE_PREFIX.as_bytes()));
    }

    #[test]
    fn owned_segment_iterator_rejects_declared_count_before_writing() {
        let documents = vec![Ctk3Document::new(10, vec![Ctk3Page::new(0, vec![])])];
        let mut output = Vec::new();
        assert!(matches!(
            encode_ctk3_segmented_documents_iter_into(documents, 2, &mut output),
            Err(crate::Ctk3WriteError::Codec(
                Ctk3CodecError::BundleSegmentCountMismatch {
                    expected: 2,
                    actual: 1
                }
            ))
        ));
        assert!(output.is_empty());
    }

    fn interoperability_document(name: &str) -> Ctk3Document {
        match name {
            "empty" => Ctk3Document::new(10, vec![Ctk3Page::new(0, vec![])]),
            "unicode_operation" => {
                let mut page = Ctk3Page::new(
                    1,
                    row(&[
                        Ctk3Color::Gray,
                        Ctk3Color::Empty,
                        Ctk3Color::Empty,
                        Ctk3Color::Empty,
                        Ctk3Color::Piece(Ctk3Piece::I),
                        Ctk3Color::Piece(Ctk3Piece::I),
                        Ctk3Color::Piece(Ctk3Piece::I),
                        Ctk3Color::Piece(Ctk3Piece::I),
                    ]),
                )
                .with_comment("주석 😀");
                page.operation = Some(Ctk3Operation {
                    piece: Ctk3Piece::I,
                    rotation: Ctk3Rotation::Right,
                    x: 4,
                    y: 2,
                });
                page.flags.mirror = true;
                page.garbage = Some(row(&[Ctk3Color::Empty, Ctk3Color::Gray]));
                Ctk3Document::new(10, vec![page])
            }
            "temporal_repeat" => Ctk3Document::new(
                10,
                (0..8)
                    .map(|_| {
                        Ctk3Page::new(
                            1,
                            row(&[
                                Ctk3Color::Piece(Ctk3Piece::T),
                                Ctk3Color::Piece(Ctk3Piece::T),
                                Ctk3Color::Piece(Ctk3Piece::T),
                            ]),
                        )
                        .with_comment("same")
                    })
                    .collect(),
            ),
            "temporal_moving" => Ctk3Document::new(
                10,
                (0..20)
                    .map(|index| {
                        let mut first = vec![Ctk3Color::Empty; index % 10];
                        first.push(Ctk3Color::Piece(Ctk3Piece::T));
                        let mut second = vec![Ctk3Color::Empty; (index * 3) % 10];
                        second.push(Ctk3Color::Piece(Ctk3Piece::I));
                        let mut cells = row(&first);
                        cells.extend(row(&second));
                        let mut page =
                            Ctk3Page::new(2, cells).with_comment(format!("p{}", index % 4));
                        page.operation = Some(Ctk3Operation {
                            piece: Ctk3Piece::T,
                            rotation: Ctk3Rotation::Spawn,
                            x: (index % 8) as i32,
                            y: 3,
                        });
                        page
                    })
                    .collect(),
            ),
            "temporal_delta" => Ctk3Document::new(
                10,
                (0..12)
                    .map(|index| {
                        let mut cells = row(&[
                            Ctk3Color::Piece(Ctk3Piece::J),
                            Ctk3Color::Piece(Ctk3Piece::J),
                            Ctk3Color::Piece(Ctk3Piece::J),
                            Ctk3Color::Piece(Ctk3Piece::J),
                        ]);
                        let mut second = vec![Ctk3Color::Empty; index % 10];
                        second.push(Ctk3Color::Piece(Ctk3Piece::T));
                        cells.extend(row(&second));
                        Ctk3Page::new(2, cells).with_comment(if index % 3 == 0 { "A" } else { "B" })
                    })
                    .collect(),
            ),
            "shared_field" => Ctk3Document::new(
                10,
                (0..10)
                    .map(|index| {
                        let mut cells = vec![Ctk3Color::Gray; 10];
                        let mut second = vec![Ctk3Color::Empty; index % 7];
                        second.push(Ctk3Color::Piece(if index % 2 == 1 {
                            Ctk3Piece::S
                        } else {
                            Ctk3Piece::Z
                        }));
                        cells.extend(row(&second));
                        Ctk3Page::new(2, cells)
                    })
                    .collect(),
            ),
            _ => panic!("unknown CTK3 KAT: {name}"),
        }
    }

    fn row(prefix: &[Ctk3Color]) -> Vec<Ctk3Color> {
        let mut row = prefix.to_vec();
        row.resize(10, Ctk3Color::Empty);
        row
    }
}
