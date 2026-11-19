//! Git LFS pointer files, which stand in for large files in a checkout that didn't download them.
//!
//! A [pointer file](https://github.com/git-lfs/git-lfs/blob/main/docs/spec.md) is a few
//! `key value` lines that name the object and its real size. The parser is hand-written: no serde
//! format matches this line-based text, and the format's own rules, such as sorted keys, would need
//! checking by hand anyway. It accepts only version 1 pointers, with their keys in order and every
//! line ended by a newline, so ordinary files don't parse as pointers by accident.

use crate::FileBytes;

/// Every pointer file is smaller than this many bytes, extension lines included.
pub(crate) const POINTER_SIZE_BOUND: u64 = 1024;

/// First lines of a version 1 pointer: the current URL, and the legacy one that Git LFS still reads.
const VERSION_LINES: [&str; 2] = [
    "version https://git-lfs.github.com/spec/v1\n",
    "version https://hawser.github.com/spec/v1\n",
];

/// Prefix of the object ID, the only hash algorithm the format supports.
const OID_PREFIX: &str = "sha256:";

/// Number of lowercase hexadecimal digits in a SHA-256 object ID.
const OID_HEX_DIGITS: usize = 64;

/// Returns the size an LFS pointer declares, or `None` when `content` isn't a valid pointer.
pub(crate) fn declared_size(content: &[u8]) -> Option<FileBytes> {
    let text = std::str::from_utf8(content).ok()?;
    let body = VERSION_LINES
        .iter()
        .find_map(|version| text.strip_prefix(version))?
        .strip_suffix('\n')?;
    let mut previous_key: Option<&str> = None;
    let mut has_oid = false;
    let mut size = None;
    for line in body.split('\n') {
        let (key, value) = line.split_once(' ')?;
        // Keys after `version` are sorted and unique, so `version` itself can't come again.
        let out_of_order = previous_key.is_some_and(|previous| key <= previous);
        if key == "version"
            || out_of_order
            || !is_key(key)
            || value.is_empty()
            || value.contains('\r')
        {
            return None;
        }
        match key {
            "oid" => has_oid = is_oid(value),
            "size" => size = parse_size(value),
            _ => {}
        }
        previous_key = Some(key);
    }
    size.filter(|_| has_oid)
}

/// Returns whether `key` uses only the characters the format allows in keys.
fn is_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b".-".contains(&byte))
}

/// Returns whether `value` is a SHA-256 object ID.
fn is_oid(value: &str) -> bool {
    value.strip_prefix(OID_PREFIX).is_some_and(|hex| {
        hex.len() == OID_HEX_DIGITS
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

/// Parses a size written as decimal digits, which rules out the sign that `u64` parsing accepts.
fn parse_size(value: &str) -> Option<FileBytes> {
    if !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok().map(FileBytes::new)
}

#[cfg(test)]
mod tests;
