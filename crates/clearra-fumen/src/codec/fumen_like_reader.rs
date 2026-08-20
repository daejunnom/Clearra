use super::fumen_like_trace::{FumenLikeTrace, FumenLikeTraceError};

use clearra_core_domain::field::occupancy_field::{OccupancyField, OccupancyFieldError};

const FIELD_WIDTH: usize = 10;
const FIELD_TOP: usize = 23;
const GARBAGE_LINES: usize = 1;
const FIELD_BLOCKS: usize = FIELD_WIDTH * (FIELD_TOP + GARBAGE_LINES);
const ENCODE_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const COMMENT_TABLE: &[u8] =
    b" !\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~";
const COMMENT_BASE: usize = COMMENT_TABLE.len() + 1;
pub const FUMEN_MAX_INPUT_BYTES: usize = 16 << 20;
pub const FUMEN_MAX_PAGES: usize = 4_096;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FumenLikeReader;

impl FumenLikeReader {
    pub fn read(input: &str) -> Result<FumenLikeTrace, FumenLikeReadError> {
        let data = extract_v115_data(input)?;
        let mut buffer = FumenValueReader::new(&data)?;
        let mut pages = Vec::new();
        let mut repeat_count = 0usize;
        let mut page_index = 0usize;
        let mut last_comment = String::new();

        while !buffer.is_empty() {
            if pages.len() == FUMEN_MAX_PAGES {
                return Err(FumenLikeReadError::TooManyPages {
                    maximum: FUMEN_MAX_PAGES,
                });
            }
            if repeat_count > 0 {
                repeat_count -= 1;
            } else {
                let changed = read_field_diff(&mut buffer)?;
                if !changed {
                    repeat_count = buffer.poll(1)?;
                }
            }

            let action = decode_action(buffer.poll(3)?);
            let page = if action.comment {
                let comment = read_comment(&mut buffer)?;
                last_comment = comment.clone();
                comment
            } else if page_index == 0 {
                String::new()
            } else {
                last_comment.clone()
            };

            pages.push(page);
            page_index += 1;
        }

        FumenLikeTrace::try_new(pages).map_err(FumenLikeReadError::InvalidTrace)
    }
}
impl FumenLikeReader {
    pub fn read_occupancy_field(
        input: &str,
        width: u8,
        height: u8,
    ) -> Result<OccupancyField, FumenLikeReadError> {
        let trace = Self::read(input)?;
        let mask = trace
            .pages()
            .iter()
            .find_map(|page| field_value(page, "initial_board_mask"))
            .ok_or(FumenLikeReadError::MissingInitialBoardMask)?;
        let mask = parse_mask(mask)?;
        OccupancyField::new(width, height, mask).map_err(FumenLikeReadError::InvalidOccupancyField)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FumenLikeReadError {
    UnsupportedVersion,
    EmptyData,
    InvalidCharacter { index: usize, value: char },
    UnexpectedEnd,
    InvalidFieldRun,
    InvalidFieldDiff { diff: usize },
    InvalidCommentCharacter { index: usize },
    InvalidEscape,
    InputTooLong { length: usize, maximum: usize },
    TooManyPages { maximum: usize },
    InvalidTrace(FumenLikeTraceError),
    MissingInitialBoardMask,
    InvalidInitialBoardMask { value: String },
    InvalidOccupancyField(OccupancyFieldError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DecodedAction {
    comment: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FumenValueReader {
    values: Vec<usize>,
    cursor: usize,
}

impl FumenValueReader {
    fn new(data: &str) -> Result<Self, FumenLikeReadError> {
        if data.is_empty() {
            return Err(FumenLikeReadError::EmptyData);
        }

        let mut values = Vec::with_capacity(data.len());
        for (index, byte) in data.bytes().enumerate() {
            let Some(value) = ENCODE_TABLE.iter().position(|candidate| *candidate == byte) else {
                return Err(FumenLikeReadError::InvalidCharacter {
                    index,
                    value: byte as char,
                });
            };
            values.push(value);
        }

        Ok(Self { values, cursor: 0 })
    }
}
impl FumenValueReader {
    fn poll(&mut self, count: usize) -> Result<usize, FumenLikeReadError> {
        let mut value = 0;
        for digit in 0..count {
            let Some(next) = self.values.get(self.cursor).copied() else {
                return Err(FumenLikeReadError::UnexpectedEnd);
            };
            self.cursor += 1;
            value += next * ENCODE_TABLE.len().pow(digit as u32);
        }
        Ok(value)
    }
}
impl FumenValueReader {
    fn is_empty(&self) -> bool {
        self.cursor >= self.values.len()
    }
}

fn extract_v115_data(input: &str) -> Result<String, FumenLikeReadError> {
    if input.len() > FUMEN_MAX_INPUT_BYTES {
        return Err(FumenLikeReadError::InputTooLong {
            length: input.len(),
            maximum: FUMEN_MAX_INPUT_BYTES,
        });
    }
    let without_params = input.split('&').next().unwrap_or(input);
    let Some(marker_index) = find_v115_marker(without_params) else {
        return Err(FumenLikeReadError::UnsupportedVersion);
    };
    let data = without_params[marker_index + 5..]
        .chars()
        .filter(|ch| *ch != '?' && !ch.is_whitespace())
        .collect::<String>();
    if data.is_empty() {
        return Err(FumenLikeReadError::EmptyData);
    }
    Ok(data)
}

fn find_v115_marker(input: &str) -> Option<usize> {
    ["v115@", "m115@", "d115@"]
        .iter()
        .filter_map(|marker| input.find(marker))
        .min()
}

fn read_field_diff(buffer: &mut FumenValueReader) -> Result<bool, FumenLikeReadError> {
    let mut index = 0;
    let mut changed = true;

    while index < FIELD_BLOCKS {
        let diff_block = buffer.poll(2)?;
        let diff = diff_block / FIELD_BLOCKS;
        let block_count = diff_block % FIELD_BLOCKS;
        if diff > 16 {
            return Err(FumenLikeReadError::InvalidFieldDiff { diff });
        }
        if diff == 8 && block_count == FIELD_BLOCKS - 1 {
            changed = false;
        }

        index += block_count + 1;
        if index > FIELD_BLOCKS {
            return Err(FumenLikeReadError::InvalidFieldRun);
        }
    }

    Ok(changed)
}

fn decode_action(value: usize) -> DecodedAction {
    let mut value = value;
    value /= 8;
    value /= 4;
    value /= FIELD_BLOCKS;
    value /= 2;
    value /= 2;
    value /= 2;
    let comment = value % 2 != 0;
    DecodedAction { comment }
}

fn read_comment(buffer: &mut FumenValueReader) -> Result<String, FumenLikeReadError> {
    let comment_length = buffer.poll(2)?;
    let mut escaped = String::new();

    for _ in 0..comment_length.div_ceil(4) {
        escaped.push_str(&decode_comment_chunk(buffer.poll(5)?)?);
    }

    escaped.truncate(comment_length);
    unescape_comment(&escaped)
}

fn decode_comment_chunk(mut value: usize) -> Result<String, FumenLikeReadError> {
    let mut chunk = String::new();
    for _ in 0..4 {
        let index = value % COMMENT_BASE;
        let Some(byte) = COMMENT_TABLE.get(index).copied() else {
            return Err(FumenLikeReadError::InvalidCommentCharacter { index });
        };
        chunk.push(byte as char);
        value /= COMMENT_BASE;
    }
    Ok(chunk)
}

fn unescape_comment(escaped: &str) -> Result<String, FumenLikeReadError> {
    let bytes = escaped.as_bytes();
    let mut code_units = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] != b'%' {
            code_units.push(u16::from(bytes[index]));
            index += 1;
            continue;
        }

        if bytes.get(index + 1) == Some(&b'u') {
            let hex = bytes
                .get(index + 2..index + 6)
                .ok_or(FumenLikeReadError::InvalidEscape)?;
            code_units.push(parse_hex(hex)?);
            index += 6;
        } else {
            let hex = bytes
                .get(index + 1..index + 3)
                .ok_or(FumenLikeReadError::InvalidEscape)?;
            code_units.push(parse_hex(hex)? & 0xff);
            index += 3;
        }
    }

    String::from_utf16(&code_units).map_err(|_| FumenLikeReadError::InvalidEscape)
}

fn parse_hex(hex: &[u8]) -> Result<u16, FumenLikeReadError> {
    let text = std::str::from_utf8(hex).map_err(|_| FumenLikeReadError::InvalidEscape)?;
    u16::from_str_radix(text, 16).map_err(|_| FumenLikeReadError::InvalidEscape)
}

fn field_value<'a>(page: &'a str, key: &str) -> Option<&'a str> {
    page.lines().find_map(|line| {
        let (line_key, value) = line.split_once('=')?;
        (line_key.trim() == key).then_some(value.trim())
    })
}

fn parse_mask(value: &str) -> Result<u64, FumenLikeReadError> {
    let trimmed = value.trim();
    let digits = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    u64::from_str_radix(digits, 16).map_err(|_| FumenLikeReadError::InvalidInitialBoardMask {
        value: value.to_owned(),
    })
}

#[cfg(test)]
#[path = "fumen_like_reader_tests.rs"]
mod tests;
