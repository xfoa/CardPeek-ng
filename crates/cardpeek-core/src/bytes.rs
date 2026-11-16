use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::CoreError;

/// A sequence of 1-bit, 4-bit, or 8-bit elements.
///
/// Conversion to a wider width is left aligned, matching legacy `CardPeek`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ByteString {
    width: u8,
    elements: Vec<u8>,
}

impl ByteString {
    /// Creates an empty byte string with the selected element width.
    #[must_use]
    pub const fn new(width: u8) -> Self {
        Self {
            width,
            elements: Vec::new(),
        }
    }

    /// Creates a byte string after validating every element.
    ///
    /// # Errors
    ///
    /// Returns an error for an unsupported width or an element that exceeds it.
    pub fn from_elements(width: u8, elements: Vec<u8>) -> Result<Self, CoreError> {
        if !matches!(width, 1 | 4 | 8) {
            return Err(CoreError::InvalidDocument(format!(
                "unsupported element width {width}"
            )));
        }
        let mask = width_mask(width);
        if let Some(value) = elements.iter().copied().find(|value| *value > mask) {
            return Err(CoreError::ElementOverflow { width, value });
        }
        Ok(Self { width, elements })
    }

    /// Creates an 8-bit byte string from bytes.
    #[must_use]
    pub const fn from_bytes(bytes: Vec<u8>) -> Self {
        Self {
            width: 8,
            elements: bytes,
        }
    }

    /// Parses hexadecimal digits with optional whitespace, colons, and hyphens.
    /// An optional legacy prefix such as `8:` is accepted.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid digits, invalid widths, or incomplete bytes.
    pub fn from_hex(width: u8, input: &str) -> Result<Self, CoreError> {
        let input = input.strip_prefix(&format!("{width}:")).unwrap_or(input);
        let digits: Vec<(usize, char)> = input
            .char_indices()
            .filter(|(_, character)| {
                !character.is_ascii_whitespace() && !matches!(character, ':' | '-' | '_')
            })
            .collect();
        if width == 8 && !digits.len().is_multiple_of(2) {
            return Err(CoreError::OddHexLength);
        }
        let mut values = Vec::new();
        for (index, digit) in digits {
            let Some(value) = digit.to_digit(16) else {
                return Err(CoreError::InvalidHexDigit { index, digit });
            };
            let value =
                u8::try_from(value).map_err(|_| CoreError::InvalidHexDigit { index, digit })?;
            values.push(value);
        }
        if width == 8 {
            let (pairs, _) = values.as_chunks::<2>();
            return Ok(Self::from_bytes(
                pairs.iter().map(|pair| (pair[0] << 4) | pair[1]).collect(),
            ));
        }
        Self::from_elements(width, values)
    }

    /// Returns the width of each element in bits.
    #[must_use]
    pub const fn width(&self) -> u8 {
        self.width
    }

    /// Returns the elements.
    #[must_use]
    pub fn elements(&self) -> &[u8] {
        &self.elements
    }

    /// Returns the number of elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// Reports whether the sequence is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }

    /// Returns the exact value size in bits.
    #[must_use]
    pub fn bit_len(&self) -> usize {
        self.elements.len() * usize::from(self.width)
    }

    /// Returns bytes for an 8-bit string.
    #[must_use]
    pub fn as_bytes(&self) -> Option<&[u8]> {
        (self.width == 8).then_some(&self.elements)
    }

    /// Appends another byte string with the same width.
    ///
    /// # Errors
    ///
    /// Returns an error when the element widths differ.
    pub fn append(&mut self, other: &Self) -> Result<(), CoreError> {
        if self.width != other.width {
            return Err(CoreError::InvalidDocument(format!(
                "cannot concatenate {}-bit and {}-bit values",
                self.width, other.width
            )));
        }
        self.elements.extend_from_slice(&other.elements);
        Ok(())
    }

    /// Converts the sequence to another supported width.
    #[must_use]
    pub fn convert(&self, width: u8) -> Self {
        if width == self.width {
            return self.clone();
        }
        let bits: Vec<u8> = self
            .elements
            .iter()
            .flat_map(|value| (0..self.width).rev().map(move |shift| (value >> shift) & 1))
            .collect();
        let mut elements = Vec::with_capacity(bits.len().div_ceil(usize::from(width)));
        for chunk in bits.chunks(usize::from(width)) {
            let mut value = 0;
            for bit in chunk {
                value = (value << 1) | bit;
            }
            let chunk_width = u8::try_from(chunk.len()).unwrap_or(width);
            value <<= width - chunk_width;
            elements.push(value);
        }
        Self { width, elements }
    }

    /// Returns a range of elements, clamped to the available sequence.
    #[must_use]
    pub fn slice(&self, start: usize, length: Option<usize>) -> Self {
        if start >= self.len() {
            return Self::new(self.width);
        }
        let end = length
            .and_then(|length| start.checked_add(length))
            .unwrap_or(self.len())
            .min(self.len());
        Self {
            width: self.width,
            elements: self.elements[start..end].to_vec(),
        }
    }

    /// Formats the sequence as contiguous uppercase hexadecimal digits.
    #[must_use]
    pub fn to_hex(&self) -> String {
        self.elements
            .iter()
            .map(|value| {
                if self.width == 8 {
                    format!("{value:02X}")
                } else {
                    format!("{value:X}")
                }
            })
            .collect()
    }
}

impl fmt::Display for ByteString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_hex())
    }
}

impl From<Vec<u8>> for ByteString {
    fn from(value: Vec<u8>) -> Self {
        Self::from_bytes(value)
    }
}

impl FromStr for ByteString {
    type Err = CoreError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::from_hex(8, value)
    }
}

const fn width_mask(width: u8) -> u8 {
    match width {
        1 => 1,
        4 => 0x0f,
        _ => 0xff,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats_bytes() {
        let value = ByteString::from_hex(8, "3b 8f-80").unwrap();
        assert_eq!(value.elements(), &[0x3b, 0x8f, 0x80]);
        assert_eq!(value.to_hex(), "3B8F80");
        assert_eq!(value.bit_len(), 24);
    }

    #[test]
    fn conversion_left_aligns_incomplete_output_elements() {
        let bits = ByteString::from_elements(1, vec![1, 0, 1]).unwrap();
        assert_eq!(bits.convert(4).elements(), &[0x0a]);
        assert_eq!(bits.convert(8).elements(), &[0xa0]);
        assert_eq!(
            bits.convert(4).convert(1),
            ByteString::from_elements(1, vec![1, 0, 1, 0]).unwrap()
        );
    }

    #[test]
    fn rejects_invalid_values() {
        assert!(ByteString::from_elements(1, vec![2]).is_err());
        assert!(ByteString::from_hex(8, "ABC").is_err());
        assert!(ByteString::from_hex(8, "GG").is_err());
    }
}
