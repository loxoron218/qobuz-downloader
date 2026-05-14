//! Shared application types used across multiple modules.

use std::fmt::{Display, Formatter, Result as FmtResult};

use {
    qobuz_api_rust_refactor::models::file_url::quality::{
        FLAC_16_44, FLAC_24_96, FLAC_24_192, MP3_320,
    },
    serde::{Deserialize, Serialize},
};

/// Audio quality selection wrapping API library constants.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum Quality {
    /// MP3 320kbps (`format_id` = 5).
    Mp3_320,
    /// FLAC 16-bit / 44.1kHz (`format_id` = 6).
    #[default]
    Flac16_44,
    /// FLAC 24-bit / 96kHz (`format_id` = 7).
    Flac24_96,
    /// FLAC 24-bit / 192kHz (`format_id` = 27).
    Flac24_192,
}

impl Quality {
    /// Returns the file extension for this quality level.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp3_320 => "mp3",
            Self::Flac16_44 | Self::Flac24_96 | Self::Flac24_192 => "flac",
        }
    }
}

impl Display for Quality {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            Self::Mp3_320 => write!(f, "MP3 320kbps"),
            Self::Flac16_44 => write!(f, "FLAC 16-bit / 44.1kHz"),
            Self::Flac24_96 => write!(f, "FLAC 24-bit / 96kHz"),
            Self::Flac24_192 => write!(f, "FLAC 24-bit / 192kHz"),
        }
    }
}

impl TryFrom<i32> for Quality {
    type Error = String;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            v if v == MP3_320 => Ok(Self::Mp3_320),
            v if v == FLAC_16_44 => Ok(Self::Flac16_44),
            v if v == FLAC_24_96 => Ok(Self::Flac24_96),
            v if v == FLAC_24_192 => Ok(Self::Flac24_192),
            _ => Err(format!("Unknown quality format_id: {value}")),
        }
    }
}

impl From<Quality> for i32 {
    fn from(quality: Quality) -> Self {
        match quality {
            Quality::Mp3_320 => MP3_320,
            Quality::Flac16_44 => FLAC_16_44,
            Quality::Flac24_96 => FLAC_24_96,
            Quality::Flac24_192 => FLAC_24_192,
        }
    }
}
