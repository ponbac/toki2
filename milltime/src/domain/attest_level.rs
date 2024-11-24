use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Default, PartialEq)]
#[serde(from = "i64", into = "i64")]
pub enum AttestLevel {
    #[default]
    None = 0,
    Week = 1,
    Month = 2,
}

impl From<i64> for AttestLevel {
    fn from(value: i64) -> Self {
        match value {
            0 => AttestLevel::None,
            1 => AttestLevel::Week,
            2 => AttestLevel::Month,
            _ => AttestLevel::None,
        }
    }
}

impl From<AttestLevel> for i64 {
    fn from(val: AttestLevel) -> Self {
        val as i64
    }
}
