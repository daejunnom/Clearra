use sha2::{Digest, Sha256};

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use core::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

pub(crate) fn base64_standard(bytes: &[u8]) -> Result<String, DocumentEncodingError> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let encoded_length = bytes
        .len()
        .checked_add(2)
        .and_then(|length| length.checked_div(3))
        .and_then(|groups| groups.checked_mul(4))
        .ok_or(DocumentEncodingError::CapacityExceeded)?;
    let mut output = String::new();
    output
        .try_reserve_exact(encoded_length)
        .map_err(|_| DocumentEncodingError::CapacityExceeded)?;
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or_default();
        let third = chunk.get(2).copied().unwrap_or_default();
        let packed = (u32::from(first) << 16) | (u32::from(second) << 8) | u32::from(third);
        output.push(ALPHABET[((packed >> 18) & 0x3f) as usize] as char);
        output.push(ALPHABET[((packed >> 12) & 0x3f) as usize] as char);
        output.push(if chunk.len() > 1 {
            ALPHABET[((packed >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        output.push(if chunk.len() > 2 {
            ALPHABET[(packed & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    debug_assert_eq!(output.len(), encoded_length);
    Ok(output)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DocumentEncodingError {
    CapacityExceeded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_base64_and_sha256_are_deterministic() {
        assert_eq!(base64_standard(b"Clearra").unwrap(), "Q2xlYXJyYQ==");
        assert_eq!(
            sha256_hex(b"Clearra"),
            "0de7d542a1a5cba042eef7db19eb30411acb3f04a24aab8739f71b2989781b60"
        );
    }
}
