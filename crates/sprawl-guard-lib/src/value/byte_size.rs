//! Byte sizes in configuration, written as an integer or as a number with a unit, such as `"10 MB"`.
//!
//! The parser is written by hand rather than taken from a size-parsing crate. Those crates accept
//! `10M`, `10mb` and `10 KB` and pick a meaning for them; this parser rejects every spelling whose
//! meaning a reader might mistake and names the units it accepts instead. It also computes exactly
//! in integers, so a fraction that doesn't come to a whole number of bytes is an error rather than
//! being rounded.

use std::fmt;

use schemars::{Schema, json_schema};
use serde::Deserializer;
use serde::de::{self, Unexpected, Visitor};

/// Units a size may end with, and how many bytes each stands for.
///
/// Decimal units are powers of 1000 and binary units are powers of 1024, as in IEC 80000-13.
const UNITS: [(&str, u64); 9] = [
    ("B", 1),
    ("kB", 1_000),
    ("MB", 1_000_000),
    ("GB", 1_000_000_000),
    ("TB", 1_000_000_000_000),
    ("KiB", 1 << 10),
    ("MiB", 1 << 20),
    ("GiB", 1 << 30),
    ("TiB", 1 << 40),
];

/// Keyword that turns a byte limit off.
pub(crate) const UNLIMITED: &str = "unlimited";

/// Separator between the whole and fractional parts of a size's number.
const DECIMAL_POINT: char = '.';

/// Separator allowed between a size's number and its unit.
const UNIT_SEPARATOR: char = ' ';

/// Digit that can end a fraction without changing its value.
const TRAILING_ZERO: char = '0';

/// Most digits a size's fraction may have, not counting trailing zeros.
///
/// Twelve digits express any whole number of bytes in a decimal unit, and with them every size that
/// fits in 64 bits is computed exactly within 128 bits.
const MAX_FRACTION_DIGITS: usize = 12;

/// Largest size, in bytes: the largest TOML integer, so the effective configuration can print it.
const MAX_BYTE_SIZE: u64 = i64::MAX.unsigned_abs();

/// Error returned when a size string can't be read as a whole number of bytes.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub(crate) enum ByteSizeError {
    /// The string isn't a number followed by a unit.
    #[error("{0:?} isn't a size; write a number and a unit, such as \"10 MB\" or \"10 MiB\"")]
    Malformed(String),
    /// The unit isn't one of the accepted units, which are case-sensitive.
    #[error(
        "{0:?} isn't a unit; use B, kB, MB, GB or TB for powers of 1000, or KiB, MiB, GiB or TiB \
         for powers of 1024"
    )]
    UnknownUnit(String),
    /// The number and unit come to a fraction of a byte.
    #[error("{0:?} isn't a whole number of bytes")]
    FractionalBytes(String),
    /// The fraction has more digits than [`MAX_FRACTION_DIGITS`].
    #[error("{0:?} has more than {MAX_FRACTION_DIGITS} digits after the decimal point")]
    TooPrecise(String),
    /// The size is larger than [`MAX_BYTE_SIZE`].
    #[error("{0:?} is larger than {MAX_BYTE_SIZE} bytes")]
    TooLarge(String),
}

/// Returns the number of bytes that `text`, such as `"10 MB"` or `"1.5 GiB"`, stands for.
///
/// The text is a number, optionally with a fractional part, then optionally one space, then one of
/// the [`UNITS`].
pub(crate) fn parse_byte_size(text: &str) -> Result<u64, ByteSizeError> {
    let number_end = text
        .find(|character: char| !character.is_ascii_digit() && character != DECIMAL_POINT)
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(number_end);
    let unit = unit.strip_prefix(UNIT_SEPARATOR).unwrap_or(unit);
    let (whole, fraction) = number.split_once(DECIMAL_POINT).unwrap_or((number, ""));
    let is_digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    if whole.is_empty()
        || !is_digits(whole)
        || !is_digits(fraction)
        || (number.contains(DECIMAL_POINT) && fraction.is_empty())
        || unit.is_empty()
        || !unit.bytes().all(|byte| byte.is_ascii_alphabetic())
    {
        return Err(ByteSizeError::Malformed(text.to_owned()));
    }
    let Some(&(_, unit_bytes)) = UNITS.iter().find(|(name, _)| *name == unit) else {
        return Err(ByteSizeError::UnknownUnit(unit.to_owned()));
    };
    let fraction = fraction.trim_end_matches(TRAILING_ZERO);
    if fraction.len() > MAX_FRACTION_DIGITS {
        return Err(ByteSizeError::TooPrecise(text.to_owned()));
    }
    let too_large = || ByteSizeError::TooLarge(text.to_owned());
    let digits = |part: &str| part.parse::<u128>().map_err(|_| too_large());
    let scale = 10_u128.pow(
        u32::try_from(fraction.len()).expect("a fraction has at most MAX_FRACTION_DIGITS digits"),
    );
    let fraction_value = if fraction.is_empty() {
        0
    } else {
        digits(fraction)?
    };
    let scaled_bytes = digits(whole)?
        .checked_mul(scale)
        .and_then(|value| value.checked_add(fraction_value))
        .and_then(|value| value.checked_mul(u128::from(unit_bytes)))
        .ok_or_else(too_large)?;
    if scaled_bytes % scale != 0 {
        return Err(ByteSizeError::FractionalBytes(text.to_owned()));
    }
    u64::try_from(scaled_bytes / scale)
        .ok()
        .filter(|&bytes| bytes <= MAX_BYTE_SIZE)
        .ok_or_else(too_large)
}

/// Regular expression for the size strings that [`parse_byte_size`] accepts, for the published
/// schema.
///
/// It doesn't rule out sizes that are zero, too large, a fraction of a byte, or written with too
/// many digits after the decimal point.
fn byte_size_pattern() -> String {
    let units = UNITS.map(|(name, _)| name).join("|");
    format!("^[0-9]+(\\.[0-9]+)? ?({units})$")
}

/// Byte limit as configuration writes it, before it is checked to be positive.
pub(crate) enum ConfiguredByteLimit {
    /// A number of bytes.
    Bytes(u64),
    /// No limit.
    Unlimited,
}

/// Reads an integer, a size string, or `"unlimited"`.
struct ConfiguredByteLimitVisitor;

impl Visitor<'_> for ConfiguredByteLimitVisitor {
    type Value = ConfiguredByteLimit;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a positive integer or a size such as \"10 MB\"")
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        Ok(ConfiguredByteLimit::Bytes(value))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        u64::try_from(value)
            .map(ConfiguredByteLimit::Bytes)
            .map_err(|_| E::invalid_value(Unexpected::Signed(value), &self))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        if value == UNLIMITED {
            return Ok(ConfiguredByteLimit::Unlimited);
        }
        parse_byte_size(value)
            .map(ConfiguredByteLimit::Bytes)
            .map_err(E::custom)
    }
}

/// Reads a byte limit from configuration: a positive integer, a size string, or `"unlimited"`.
///
/// The caller checks that a number of bytes is positive.
pub(crate) fn deserialize_byte_limit<'de, D>(
    deserializer: D,
) -> Result<ConfiguredByteLimit, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(ConfiguredByteLimitVisitor)
}

/// Reads a byte count from configuration: a positive integer or a size string.
///
/// The caller checks that the count is positive.
pub(crate) fn deserialize_byte_count<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    match deserialize_byte_limit(deserializer)? {
        ConfiguredByteLimit::Bytes(bytes) => Ok(bytes),
        ConfiguredByteLimit::Unlimited => Err(de::Error::invalid_value(
            Unexpected::Str(UNLIMITED),
            &ConfiguredByteLimitVisitor,
        )),
    }
}

/// Schemas of the two ways to write a byte count in configuration: a positive integer, and a size
/// string.
pub(crate) fn byte_count_schemas() -> [Schema; 2] {
    [
        json_schema!({ "type": "integer", "minimum": 1 }),
        json_schema!({ "type": "string", "pattern": byte_size_pattern() }),
    ]
}

#[cfg(test)]
mod tests;
