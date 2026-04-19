use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Copy, Clone)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[derive(Debug, Error)]
pub enum RgbParseError {
    #[error(transparent)]
    IntParseError(#[from] std::num::ParseIntError),
    #[error("invalid RGB hex length, expected exactly 6 hex digits")]
    InvalidLength,
}

impl FromStr for Rgb {
    type Err = RgbParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 6 {
            return Err(RgbParseError::InvalidLength);
        }

        let get_int = move |start, end| {
            s.get(start..end)
                .ok_or(const {
                    match u8::from_str_radix("😨", 16) {
                        Ok(_) => unreachable!(),
                        Err(err) => err,
                    }
                })
                .and_then(|s| u8::from_str_radix(s, 16))
        };

        Ok(Self {
            r: get_int(0, 2)?,
            g: get_int(2, 4)?,
            b: get_int(4, 6)?,
        })
    }
}
