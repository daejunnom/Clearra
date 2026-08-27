use crate::big_nat::BigNat;
use crate::Ctk3CodecError;

#[derive(Clone, Debug, Default)]
pub(crate) struct BitWriter {
    bytes: Vec<u8>,
    pub(crate) bit_len: usize,
}

impl BitWriter {
    pub(crate) fn write_bit(&mut self, value: bool) {
        let byte_index = self.bit_len / 8;
        let bit_index = self.bit_len % 8;
        if byte_index == self.bytes.len() {
            self.bytes.push(0);
        }
        if value {
            self.bytes[byte_index] |= 1 << bit_index;
        }
        self.bit_len += 1;
    }

    pub(crate) fn write_bits(&mut self, value: u32, width: usize) -> Result<(), Ctk3CodecError> {
        if width > 32 || (width < 32 && u64::from(value) >= (1u64 << width)) {
            return Err(Ctk3CodecError::IntegerOverflow);
        }
        for bit in 0..width {
            self.write_bit(value & (1u32 << bit) != 0);
        }
        Ok(())
    }

    pub(crate) fn write_big_bits(
        &mut self,
        value: &BigNat,
        width: usize,
    ) -> Result<(), Ctk3CodecError> {
        if value.bit_len() > width {
            return Err(Ctk3CodecError::IntegerOverflow);
        }
        for bit in 0..width {
            self.write_bit(value.bit(bit));
        }
        Ok(())
    }

    pub(crate) fn write_var_uint(&mut self, value: u64) -> Result<(), Ctk3CodecError> {
        if value > u64::from(u32::MAX) {
            return Err(Ctk3CodecError::IntegerOverflow);
        }
        if value < 16 {
            self.write_bit(false);
            self.write_bits(value as u32, 4)
        } else if value < 256 {
            self.write_bits(1, 2)?;
            self.write_bits(value as u32, 8)
        } else if value < 65_536 {
            self.write_bits(3, 3)?;
            self.write_bits(value as u32, 16)
        } else {
            self.write_bits(7, 3)?;
            self.write_bits(value as u32, 32)
        }
    }

    pub(crate) fn write_signed_var_int(&mut self, value: i64) -> Result<(), Ctk3CodecError> {
        let encoded = if value >= 0 {
            value.checked_mul(2)
        } else {
            value.checked_mul(-2).and_then(|value| value.checked_sub(1))
        }
        .ok_or(Ctk3CodecError::IntegerOverflow)?;
        self.write_var_uint(encoded as u64)
    }

    pub(crate) fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), Ctk3CodecError> {
        for byte in bytes {
            self.write_bits(u32::from(*byte), 8)?;
        }
        Ok(())
    }

    pub(crate) fn append(&mut self, other: &Self) {
        for bit in 0..other.bit_len {
            self.write_bit(other.bytes[bit / 8] & (1 << (bit % 8)) != 0);
        }
    }

    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

pub(crate) struct BitReader<'a> {
    bytes: &'a [u8],
    bit_offset: usize,
}

impl<'a> BitReader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self {
            bytes,
            bit_offset: 0,
        }
    }

    pub(crate) fn read_bit(&mut self) -> Result<bool, Ctk3CodecError> {
        if self.bit_offset >= self.bytes.len().saturating_mul(8) {
            return Err(Ctk3CodecError::invalid("payload ended unexpectedly"));
        }
        let value = self.bytes[self.bit_offset / 8] & (1 << (self.bit_offset % 8)) != 0;
        self.bit_offset += 1;
        Ok(value)
    }

    pub(crate) fn read_bits(&mut self, width: usize) -> Result<u32, Ctk3CodecError> {
        if width > 32 {
            return Err(Ctk3CodecError::invalid("bit width is invalid"));
        }
        let mut value = 0u32;
        for bit in 0..width {
            if self.read_bit()? {
                value |= 1u32 << bit;
            }
        }
        Ok(value)
    }

    pub(crate) fn read_big_bits(&mut self, width: usize) -> Result<BigNat, Ctk3CodecError> {
        let mut value = BigNat::zero();
        for bit in 0..width {
            if self.read_bit()? {
                value.set_bit(bit);
            }
        }
        Ok(value)
    }

    pub(crate) fn read_var_uint(&mut self) -> Result<u32, Ctk3CodecError> {
        if !self.read_bit()? {
            self.read_bits(4)
        } else if !self.read_bit()? {
            self.read_bits(8)
        } else if !self.read_bit()? {
            self.read_bits(16)
        } else {
            self.read_bits(32)
        }
    }

    pub(crate) fn read_signed_var_int(&mut self) -> Result<i64, Ctk3CodecError> {
        let value = i64::from(self.read_var_uint()?);
        Ok(if value & 1 == 1 {
            -(value / 2 + 1)
        } else {
            value / 2
        })
    }

    pub(crate) fn read_bytes(&mut self, length: usize) -> Result<Vec<u8>, Ctk3CodecError> {
        let required_bits = length
            .checked_mul(8)
            .ok_or(Ctk3CodecError::IntegerOverflow)?;
        let remaining_bits = self
            .bytes
            .len()
            .saturating_mul(8)
            .saturating_sub(self.bit_offset);
        if required_bits > remaining_bits {
            return Err(Ctk3CodecError::invalid("payload ended unexpectedly"));
        }
        let mut bytes = Vec::with_capacity(length);
        for _ in 0..length {
            bytes.push(self.read_bits(8)? as u8);
        }
        Ok(bytes)
    }

    pub(crate) fn assert_zero_padding(&mut self) -> Result<(), Ctk3CodecError> {
        while self.bit_offset < self.bytes.len().saturating_mul(8) {
            if self.read_bit()? {
                return Err(Ctk3CodecError::invalid("payload has trailing data"));
            }
        }
        Ok(())
    }
}
