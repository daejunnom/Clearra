use crate::{
    Ctk3CodecError, CTK3_BUNDLE_PREFIX, CTK3_LEGACY_PREFIX, CTK3_PREFIX, MAX_PAYLOAD_BYTES,
};

const CTK64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const CTK85_ALPHABET: &[u8; 85] =
    b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-:+=^!/*?&<>()[]{}@%$#";
const MAX_CTK64_CHARACTERS: usize = (MAX_PAYLOAD_BYTES * 8).div_ceil(6);
const MAX_CTK85_CHARACTERS: usize = MAX_PAYLOAD_BYTES.div_ceil(4) * 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Transport {
    Ctk64,
    Ctk85,
}

pub(crate) fn encode_ctk64(bytes: &[u8]) -> String {
    let mut output = String::with_capacity((bytes.len() * 8).div_ceil(6));
    let mut value = 0u32;
    let mut bit_count = 0usize;
    for byte in bytes {
        value = value * 256 + u32::from(*byte);
        bit_count += 8;
        while bit_count >= 6 {
            bit_count -= 6;
            let digit = value >> bit_count;
            output.push(CTK64_ALPHABET[digit as usize] as char);
            value &= (1u32 << bit_count).wrapping_sub(1);
        }
    }
    if bit_count > 0 {
        output.push(CTK64_ALPHABET[(value << (6 - bit_count)) as usize] as char);
    }
    output
}

pub(crate) fn decode_payload(
    transport: Transport,
    encoded: &str,
) -> Result<Vec<u8>, Ctk3CodecError> {
    match transport {
        Transport::Ctk64 => decode_ctk64(encoded),
        Transport::Ctk85 => decode_ctk85(encoded),
    }
}

pub(crate) fn extract_exact_payload(input: &str) -> Result<(Transport, &str), Ctk3CodecError> {
    let source = input.trim();
    if starts_with_ignore_ascii_case(source, CTK3_PREFIX) {
        let payload = &source[CTK3_PREFIX.len()..];
        validate_exact_alphabet(payload, Transport::Ctk64)?;
        Ok((Transport::Ctk64, payload))
    } else if starts_with_ignore_ascii_case(source, CTK3_LEGACY_PREFIX) {
        let payload = &source[CTK3_LEGACY_PREFIX.len()..];
        validate_exact_alphabet(payload, Transport::Ctk85)?;
        Ok((Transport::Ctk85, payload))
    } else {
        Err(Ctk3CodecError::invalid("no CTK3 header was found"))
    }
}

pub(crate) fn extract_compatible_payload(input: &str) -> Result<(Transport, &str), Ctk3CodecError> {
    let source = input.trim();
    let ctk64 = find_ignore_ascii_case(source, CTK3_PREFIX);
    let ctk85 = find_ignore_ascii_case(source, CTK3_LEGACY_PREFIX);
    let (transport, start) = match (ctk64, ctk85) {
        (Some(left), Some(right)) if left < right => (Transport::Ctk64, left + CTK3_PREFIX.len()),
        (Some(_), Some(right)) => (Transport::Ctk85, right + CTK3_LEGACY_PREFIX.len()),
        (Some(left), None) => (Transport::Ctk64, left + CTK3_PREFIX.len()),
        (None, Some(right)) => (Transport::Ctk85, right + CTK3_LEGACY_PREFIX.len()),
        (None, None) => return Err(Ctk3CodecError::invalid("no CTK3 header was found")),
    };
    let alphabet = match transport {
        Transport::Ctk64 => CTK64_ALPHABET.as_slice(),
        Transport::Ctk85 => CTK85_ALPHABET.as_slice(),
    };
    let end = source[start..]
        .bytes()
        .position(|byte| !alphabet.contains(&byte))
        .map_or(source.len(), |offset| start + offset);
    if end == start {
        return Err(Ctk3CodecError::invalid("payload is empty"));
    }
    Ok((transport, &source[start..end]))
}

pub(crate) fn extract_exact_bundle(input: &str) -> Result<Option<Vec<&str>>, Ctk3CodecError> {
    let source = input.trim();
    if !starts_with_ignore_ascii_case(source, CTK3_BUNDLE_PREFIX) {
        return Ok(None);
    }
    let payloads = source[CTK3_BUNDLE_PREFIX.len()..]
        .split('.')
        .collect::<Vec<_>>();
    if payloads.len() < 2 {
        return Err(Ctk3CodecError::invalid("bundle payload is invalid"));
    }
    for payload in &payloads {
        validate_exact_alphabet(payload, Transport::Ctk64)?;
    }
    Ok(Some(payloads))
}

pub(crate) fn extract_compatible_bundle(input: &str) -> Result<Option<Vec<&str>>, Ctk3CodecError> {
    let source = input.trim();
    let Some(prefix) = find_ignore_ascii_case(source, CTK3_BUNDLE_PREFIX) else {
        return Ok(None);
    };
    let start = prefix + CTK3_BUNDLE_PREFIX.len();
    let end = source[start..]
        .bytes()
        .position(|byte| byte != b'.' && !CTK64_ALPHABET.contains(&byte))
        .map_or(source.len(), |offset| start + offset);
    let payloads = source[start..end].split('.').collect::<Vec<_>>();
    if payloads.len() < 2 {
        return Err(Ctk3CodecError::invalid("bundle payload is invalid"));
    }
    for payload in &payloads {
        validate_exact_alphabet(payload, Transport::Ctk64)?;
    }
    Ok(Some(payloads))
}

fn validate_exact_alphabet(payload: &str, transport: Transport) -> Result<(), Ctk3CodecError> {
    let (alphabet, maximum, invalid_remainder) = match transport {
        Transport::Ctk64 => (CTK64_ALPHABET.as_slice(), MAX_CTK64_CHARACTERS, 1),
        Transport::Ctk85 => (CTK85_ALPHABET.as_slice(), MAX_CTK85_CHARACTERS, 1),
    };
    let modulus = match transport {
        Transport::Ctk64 => 4,
        Transport::Ctk85 => 5,
    };
    if payload.is_empty()
        || payload.len() > maximum
        || payload.len() % modulus == invalid_remainder
        || payload.bytes().any(|byte| !alphabet.contains(&byte))
    {
        return Err(Ctk3CodecError::invalid("transport payload is invalid"));
    }
    Ok(())
}

fn decode_ctk64(encoded: &str) -> Result<Vec<u8>, Ctk3CodecError> {
    validate_exact_alphabet(encoded, Transport::Ctk64)?;
    let mut bytes = Vec::with_capacity(encoded.len() * 6 / 8);
    let mut value = 0u32;
    let mut bit_count = 0usize;
    for byte in encoded.bytes() {
        let digit = CTK64_ALPHABET
            .iter()
            .position(|candidate| *candidate == byte)
            .ok_or_else(|| Ctk3CodecError::invalid("CTK64 character is invalid"))?
            as u32;
        value = value * 64 + digit;
        bit_count += 6;
        while bit_count >= 8 {
            bit_count -= 8;
            bytes.push((value >> bit_count) as u8);
            value &= (1u32 << bit_count).wrapping_sub(1);
        }
    }
    if value != 0 {
        return Err(Ctk3CodecError::invalid("CTK64 trailing bits are non-zero"));
    }
    if encode_ctk64(&bytes) != encoded {
        return Err(Ctk3CodecError::invalid("CTK64 payload is not canonical"));
    }
    Ok(bytes)
}

fn decode_ctk85(encoded: &str) -> Result<Vec<u8>, Ctk3CodecError> {
    validate_exact_alphabet(encoded, Transport::Ctk85)?;
    let complete_length = encoded.len() - encoded.len() % 5;
    let mut bytes = Vec::with_capacity(encoded.len() * 4 / 5);
    for block in encoded.as_bytes()[..complete_length].chunks_exact(5) {
        let value = decode_ctk85_value(block)?;
        if value > u64::from(u32::MAX) {
            return Err(Ctk3CodecError::invalid("CTK85 block overflows 32 bits"));
        }
        let value = value as u32;
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    let remaining = encoded.len() - complete_length;
    if remaining > 0 {
        let byte_count = remaining - 1;
        let value = decode_ctk85_value(&encoded.as_bytes()[complete_length..])?;
        let limit = 1u64 << (byte_count * 8);
        if value >= limit {
            return Err(Ctk3CodecError::invalid("CTK85 partial block overflows"));
        }
        for shift in (0..byte_count).rev() {
            bytes.push(((value >> (shift * 8)) & 0xff) as u8);
        }
    }
    Ok(bytes)
}

fn decode_ctk85_value(encoded: &[u8]) -> Result<u64, Ctk3CodecError> {
    let mut value = 0u64;
    for byte in encoded {
        let digit = CTK85_ALPHABET
            .iter()
            .position(|candidate| candidate == byte)
            .ok_or_else(|| Ctk3CodecError::invalid("CTK85 character is invalid"))?
            as u64;
        value = value
            .checked_mul(85)
            .and_then(|value| value.checked_add(digit))
            .ok_or(Ctk3CodecError::IntegerOverflow)?;
    }
    Ok(value)
}

pub(crate) fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0xffffu16;
    for byte in bytes {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn starts_with_ignore_ascii_case(source: &str, prefix: &str) -> bool {
    source
        .get(..prefix.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix))
}

fn find_ignore_ascii_case(source: &str, needle: &str) -> Option<usize> {
    if source.len() < needle.len() {
        return None;
    }
    source
        .as_bytes()
        .windows(needle.len())
        .position(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctk64_round_trips_and_rejects_aliases() {
        for bytes in [vec![0], vec![0xc3, 0x49, 0x02], (0..=255).collect()] {
            let encoded = encode_ctk64(&bytes);
            assert_eq!(decode_ctk64(&encoded), Ok(bytes));
        }
        assert!(decode_ctk64("B").is_err());
        assert!(decode_ctk64("AB").is_err());
    }
}
