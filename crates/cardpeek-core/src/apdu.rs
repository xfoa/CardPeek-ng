use serde::{Deserialize, Serialize};

use crate::{ByteString, CoreError, StatusWord};

/// An ISO 7816 command header and body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApduCommand {
    /// Class byte.
    pub cla: u8,
    /// Instruction byte.
    pub ins: u8,
    /// First parameter byte.
    pub p1: u8,
    /// Second parameter byte.
    pub p2: u8,
    /// Command data.
    pub data: Vec<u8>,
    /// Expected response length, when present.
    pub le: Option<u32>,
}

impl ApduCommand {
    /// Parses a short or extended APDU without adding implicit retries.
    ///
    /// # Errors
    ///
    /// Returns an error when the command length does not match its encoding.
    pub fn parse(bytes: &[u8]) -> Result<Self, CoreError> {
        if bytes.len() < 4 {
            return Err(CoreError::InvalidApdu(
                "a command needs at least four header bytes".into(),
            ));
        }
        let header = ApduCommand {
            cla: bytes[0],
            ins: bytes[1],
            p1: bytes[2],
            p2: bytes[3],
            data: Vec::new(),
            le: None,
        };
        if bytes.len() == 4 {
            return Ok(header);
        }
        if bytes.len() == 5 {
            return Ok(Self {
                le: Some(decode_le(u32::from(bytes[4]), 256)),
                ..header
            });
        }
        if bytes[4] != 0 {
            let lc = usize::from(bytes[4]);
            if bytes.len() == 6 + lc {
                return Ok(Self {
                    data: bytes[5..5 + lc].to_vec(),
                    le: Some(decode_le(u32::from(bytes[5 + lc]), 256)),
                    ..header
                });
            }
            if bytes.len() == 5 + lc {
                return Ok(Self {
                    data: bytes[5..].to_vec(),
                    ..header
                });
            }
            return Err(CoreError::InvalidApdu(format!(
                "short Lc {lc} does not match command length {}",
                bytes.len()
            )));
        }
        if bytes.len() == 7 {
            let le = u16::from_be_bytes([bytes[5], bytes[6]]);
            return Ok(Self {
                le: Some(decode_le(u32::from(le), 65_536)),
                ..header
            });
        }
        if bytes.len() < 7 {
            return Err(CoreError::InvalidApdu(
                "extended command has a truncated Lc".into(),
            ));
        }
        let lc = usize::from(u16::from_be_bytes([bytes[5], bytes[6]]));
        if lc == 0 {
            return Err(CoreError::InvalidApdu(
                "extended Lc must not be zero".into(),
            ));
        }
        if bytes.len() == 7 + lc {
            return Ok(Self {
                data: bytes[7..].to_vec(),
                ..header
            });
        }
        if bytes.len() == 8 + lc {
            return Err(CoreError::InvalidApdu("extended Le needs two bytes".into()));
        }
        if bytes.len() == 9 + lc {
            let le = u16::from_be_bytes([bytes[7 + lc], bytes[8 + lc]]);
            return Ok(Self {
                data: bytes[7..7 + lc].to_vec(),
                le: Some(decode_le(u32::from(le), 65_536)),
                ..header
            });
        }
        Err(CoreError::InvalidApdu(format!(
            "extended Lc {lc} does not match command length {}",
            bytes.len()
        )))
    }

    /// Returns the shortest command encoding supported by its fields.
    ///
    /// # Errors
    ///
    /// Returns an error when data or expected length exceeds APDU limits.
    pub fn encode(&self) -> Result<Vec<u8>, CoreError> {
        if self.le == Some(0) {
            return Err(CoreError::InvalidApdu("Le must be at least one".into()));
        }
        let mut bytes = vec![self.cla, self.ins, self.p1, self.p2];
        if self.data.is_empty() {
            match self.le {
                None => {}
                Some(le @ 1..=256) => bytes.push((le & 0xff) as u8),
                Some(le) => {
                    return Err(CoreError::InvalidApdu(format!(
                        "case 2 Le {le} requires extended encoding; use explicit data with encode_extended"
                    )));
                }
            }
            return Ok(bytes);
        }
        if self.data.len() <= 255 && self.le.is_none_or(|le| le <= 256) {
            let lc = u8::try_from(self.data.len())
                .map_err(|_| CoreError::InvalidApdu("short Lc exceeds 255".into()))?;
            bytes.push(lc);
            bytes.extend_from_slice(&self.data);
            if let Some(le) = self.le {
                bytes.push((le & 0xff) as u8);
            }
        } else if self.data.len() <= 65_535 && self.le.is_none_or(|le| le <= 65_536) {
            let lc = u16::try_from(self.data.len())
                .map_err(|_| CoreError::InvalidApdu("extended Lc exceeds 65535".into()))?;
            bytes.push(0);
            bytes.extend_from_slice(&lc.to_be_bytes());
            bytes.extend_from_slice(&self.data);
            if let Some(le) = self.le {
                bytes.extend_from_slice(&((le & 0xffff) as u16).to_be_bytes());
            }
        } else {
            return Err(CoreError::InvalidApdu(
                "command data or expected length exceeds extended APDU limits".into(),
            ));
        }
        Ok(bytes)
    }

    /// Encodes a case 2 command using extended form.
    ///
    /// Data-bearing commands use [`Self::encode`].
    ///
    /// # Errors
    ///
    /// Returns an error for case 1 or an expected length outside the APDU limits.
    pub fn encode_extended(&self) -> Result<Vec<u8>, CoreError> {
        if !self.data.is_empty() {
            return self.encode();
        }
        let Some(le) = self.le else {
            return Err(CoreError::InvalidApdu(
                "extended case 1 commands are not represented".into(),
            ));
        };
        if !(1..=65_536).contains(&le) {
            return Err(CoreError::InvalidApdu(format!("invalid Le {le}")));
        }
        let mut bytes = vec![self.cla, self.ins, self.p1, self.p2, 0];
        bytes.extend_from_slice(&((le & 0xffff) as u16).to_be_bytes());
        Ok(bytes)
    }

    /// Classifies the command by its wire encoding.
    #[must_use]
    pub fn case(&self) -> ApduCase {
        match (self.data.is_empty(), self.le) {
            (true, None) => ApduCase::One,
            (true, Some(le)) if le <= 256 => ApduCase::TwoShort,
            (true, Some(_)) => ApduCase::TwoExtended,
            (false, None) if self.data.len() <= 255 => ApduCase::ThreeShort,
            (false, Some(le)) if self.data.len() <= 255 && le <= 256 => ApduCase::FourShort,
            (false, None) => ApduCase::ThreeExtended,
            (false, Some(_)) => ApduCase::FourExtended,
        }
    }
}

impl TryFrom<&[u8]> for ApduCommand {
    type Error = CoreError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

/// ISO 7816 command encoding cases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApduCase {
    /// No data and no expected length.
    One,
    /// No data with a short expected length.
    TwoShort,
    /// No data with an extended expected length.
    TwoExtended,
    /// Short command data without an expected length.
    ThreeShort,
    /// Extended command data without an expected length.
    ThreeExtended,
    /// Short command data with a short expected length.
    FourShort,
    /// Extended command data with an extended expected length.
    FourExtended,
}

/// A card response with data and a status word.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApduResponse {
    /// Response data, excluding SW1 and SW2.
    pub data: Vec<u8>,
    /// Card status word.
    pub status: StatusWord,
}

impl ApduResponse {
    /// Creates a response from its components.
    #[must_use]
    pub const fn new(data: Vec<u8>, status: StatusWord) -> Self {
        Self { data, status }
    }

    /// Parses a response that ends in SW1 and SW2.
    ///
    /// # Errors
    ///
    /// Returns an error when fewer than two bytes are supplied.
    pub fn parse(bytes: &[u8]) -> Result<Self, CoreError> {
        if bytes.len() < 2 {
            return Err(CoreError::InvalidApdu(
                "a response needs at least SW1 and SW2".into(),
            ));
        }
        let status_offset = bytes.len() - 2;
        Ok(Self {
            data: bytes[..status_offset].to_vec(),
            status: StatusWord::new(bytes[status_offset], bytes[status_offset + 1]),
        })
    }

    /// Encodes response data followed by SW1 and SW2.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = self.data.clone();
        bytes.extend_from_slice(&self.status.to_be_bytes());
        bytes
    }

    /// Returns the response as an 8-bit byte string.
    #[must_use]
    pub fn to_byte_string(&self) -> ByteString {
        ByteString::from_bytes(self.encode())
    }
}

const fn decode_le(value: u32, maximum: u32) -> u32 {
    if value == 0 { maximum } else { value }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_all_short_cases() {
        assert_eq!(
            ApduCommand::parse(&[0, 0xa4, 4, 0]).unwrap().case(),
            ApduCase::One
        );
        assert_eq!(
            ApduCommand::parse(&[0, 0xc0, 0, 0, 0]).unwrap().le,
            Some(256)
        );
        let command = ApduCommand::parse(&[0, 0xa4, 4, 0, 2, 0x3f, 0, 0]).unwrap();
        assert_eq!(command.data, [0x3f, 0]);
        assert_eq!(command.le, Some(256));
        assert_eq!(command.case(), ApduCase::FourShort);
        assert_eq!(command.encode().unwrap(), [0, 0xa4, 4, 0, 2, 0x3f, 0, 0]);
    }

    #[test]
    fn parses_extended_commands() {
        let case_two = ApduCommand::parse(&[0, 0xca, 0, 0, 0, 0, 0]).unwrap();
        assert_eq!(case_two.le, Some(65_536));
        assert_eq!(case_two.case(), ApduCase::TwoExtended);
        assert_eq!(
            case_two.encode_extended().unwrap(),
            [0, 0xca, 0, 0, 0, 0, 0]
        );

        let command = ApduCommand::parse(&[0, 0xca, 0, 0, 0, 0, 1, 0xaa, 0, 0]).unwrap();
        assert_eq!(command.data, [0xaa]);
        assert_eq!(command.le, Some(65_536));
        assert_eq!(command.case(), ApduCase::FourExtended);
        assert_eq!(
            command.encode().unwrap(),
            [0, 0xca, 0, 0, 0, 0, 1, 0xaa, 0, 0]
        );
    }

    #[test]
    fn response_round_trip() {
        let response = ApduResponse::parse(&[0x6f, 0, 0x90, 0]).unwrap();
        assert_eq!(response.status, StatusWord::NORMAL_PROCESSING);
        assert_eq!(response.encode(), [0x6f, 0, 0x90, 0]);
    }
}
