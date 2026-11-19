//! Human-readable descriptions of file sizes.

use sprawl_guard_lib::FileSizeSource;
use sprawl_guard_lib::check::FileBytesViolation;

use super::display_relative;

/// Ratio between successive decimal byte units.
const BYTE_UNIT_STEP: f64 = 1000.0;
/// Decimal byte units, from smallest to largest.
const BYTE_UNITS: [&str; 5] = ["kB", "MB", "GB", "TB", "PB"];

/// Formats an exact byte count, followed by a rounded decimal size from one kilobyte up.
pub(in crate::cli) fn byte_size(bytes: u64) -> String {
    #[expect(
        clippy::cast_precision_loss,
        reason = "the rounded size is only a reading aid next to the exact count"
    )]
    let mut scaled = bytes as f64;
    let mut unit = None;
    for next in BYTE_UNITS {
        if scaled < BYTE_UNIT_STEP {
            break;
        }
        scaled /= BYTE_UNIT_STEP;
        unit = Some(next);
    }
    match unit {
        Some(unit) => format!("{bytes} bytes ({scaled:.1} {unit})"),
        None => format!("{bytes} bytes"),
    }
}

/// Says where a measured size came from, when it isn't the file's own size on disk.
pub(in crate::cli) fn size_source_note(source: FileSizeSource) -> String {
    match source {
        FileSizeSource::OnDisk => String::new(),
        FileSizeSource::LfsPointer { pointer_bytes } => {
            format!(", declared by a {pointer_bytes}-byte Git LFS pointer")
        }
    }
}

/// Describes a file over its byte limit, as the human report lists it.
pub(super) fn file_bytes_entry(violation: &FileBytesViolation) -> String {
    let ceiling = violation
        .ratchet
        .map(|ceiling| format!(", ceiling {}", byte_size(ceiling.into_inner())))
        .unwrap_or_default();
    format!(
        "  {}: {}{}\n    limit {}{ceiling}\n\n",
        display_relative(violation.path.as_str()),
        byte_size(violation.actual.into_inner()),
        size_source_note(violation.measured_from),
        byte_size(violation.limit.into_inner()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    mod byte_size {
        use super::*;

        mod when_the_size_is_below_a_kilobyte {
            use super::*;

            #[test]
            fn it_shows_only_the_exact_count() {
                assert_eq!(byte_size(999), "999 bytes");
            }
        }

        mod when_the_size_reaches_a_unit {
            use super::*;

            #[test]
            fn it_adds_the_rounded_size_in_the_largest_unit() {
                assert_eq!(byte_size(1_000), "1000 bytes (1.0 kB)");
                assert_eq!(byte_size(2_560_000), "2560000 bytes (2.6 MB)");
                assert_eq!(
                    byte_size(u64::MAX),
                    "18446744073709551615 bytes (18446.7 PB)"
                );
            }
        }
    }
}
