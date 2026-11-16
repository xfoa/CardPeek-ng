use serde::{Deserialize, Serialize};

/// The SW1 and SW2 bytes returned by a card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StatusWord(u16);

impl StatusWord {
    /// Normal processing with no additional data.
    pub const NORMAL_PROCESSING: Self = Self(0x9000);

    /// Creates a status word from SW1 and SW2.
    #[must_use]
    pub const fn new(sw1: u8, sw2: u8) -> Self {
        Self(((sw1 as u16) << 8) | sw2 as u16)
    }

    /// Creates a status word from its numeric representation.
    #[must_use]
    pub const fn from_u16(value: u16) -> Self {
        Self(value)
    }

    /// Returns the numeric representation.
    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }

    /// Returns SW1.
    #[must_use]
    pub const fn sw1(self) -> u8 {
        self.0.to_be_bytes()[0]
    }

    /// Returns SW2.
    #[must_use]
    pub const fn sw2(self) -> u8 {
        self.0.to_be_bytes()[1]
    }

    /// Returns SW1 and SW2 in wire order.
    #[must_use]
    pub const fn to_be_bytes(self) -> [u8; 2] {
        [self.sw1(), self.sw2()]
    }

    /// Returns the number of response bytes announced by a 61xx status.
    #[must_use]
    pub const fn available_bytes(self) -> Option<u8> {
        if self.sw1() == 0x61 {
            Some(self.sw2())
        } else {
            None
        }
    }

    /// Returns the corrected Le announced by a 6Cxx status.
    #[must_use]
    pub const fn corrected_length(self) -> Option<u8> {
        if self.sw1() == 0x6c {
            Some(self.sw2())
        } else {
            None
        }
    }

    /// Returns a general status description without interpreting native errors.
    #[must_use]
    pub fn description(self) -> &'static str {
        match self.0 {
            0x9000 => "Normal processing",
            0x6100..=0x61ff => "Response data available",
            0x6200..=0x62ff => "Warning: nonvolatile memory state",
            0x6300..=0x63ff => "Warning: card state or verification changed",
            0x6400..=0x64ff => "Execution error: nonvolatile memory state",
            0x6500..=0x65ff => "Execution error: memory failure",
            0x6600 => "Security-related issue",
            0x6700 => "Wrong length",
            0x6800..=0x68ff => "Functions in CLA not supported",
            0x6900..=0x69ff => "Command not allowed",
            0x6a00..=0x6aff => "Wrong parameters",
            0x6b00 => "Wrong P1 or P2",
            0x6c00..=0x6cff => "Wrong Le; corrected length supplied",
            0x6d00 => "Instruction not supported",
            0x6e00 => "Class not supported",
            0x6f00 => "No precise diagnosis",
            _ => "Unspecified status",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_status_components() {
        let status = StatusWord::new(0x61, 0x12);
        assert_eq!(status.available_bytes(), Some(0x12));
        assert_eq!(status.corrected_length(), None);
        assert_eq!(status.to_be_bytes(), [0x61, 0x12]);
    }
}
