//! Checks every byte-limited file's size against its limit and its ratchet ceiling.

use std::fs;
use std::path::Path;

use rayon::prelude::*;

use crate::classification::{
    ByteLimitedFile, UnreadablePath, UnreadablePathKind, UnreadablePathSeverity,
};
use crate::{DiagnosticDetail, FileByteLimit, FileBytes};

use super::ratchet::{MeasuredRatchets, RatchetCurrentPolicy, RatchetState};
use super::report::{
    CheckViolation, FileBytesRatchetStaleMeasuredViolation, FileBytesViolation,
    FileBytesViolationKind, RatchetStaleMeasuredViolation, RatchetViolation,
};

/// What measuring file sizes found.
#[derive(Default)]
pub(super) struct FileBytesFindings {
    pub(super) violations: Vec<CheckViolation>,
    /// Files whose size could not be read.
    pub(super) unreadable_paths: Vec<UnreadablePath>,
}

/// Measures each file's size and compares it with its limit and any ratchet ceiling.
///
/// Every file counts as measured for ratchet purposes, including one whose size could not be
/// read, so its ceiling is not reported missing while its size is unknown.
pub(super) fn check_file_bytes(
    root: &Path,
    files: Vec<ByteLimitedFile>,
    ratchet: Option<&RatchetState>,
    current_policy: RatchetCurrentPolicy,
    gap_severity: UnreadablePathSeverity,
    measured: &mut MeasuredRatchets,
) -> FileBytesFindings {
    let sizes = files
        .into_par_iter()
        .map(|file| {
            let size = fs::metadata(root.join(file.path.as_str()))
                .map(|metadata| FileBytes::new(metadata.len()));
            (file, size)
        })
        .collect::<Vec<_>>();
    let mut findings = FileBytesFindings::default();
    for (file, size) in sizes {
        measured.record_file_size(&file.path);
        match size {
            Ok(actual) => {
                let ceiling = ratchet.and_then(|ratchet| ratchet.file_size_ceiling(&file.path));
                findings.violations.extend(file_bytes_violation(
                    file,
                    actual,
                    ceiling,
                    current_policy,
                ));
            }
            Err(source) => findings.unreadable_paths.push(UnreadablePath {
                kind: UnreadablePathKind::ReadFileFailed,
                severity: gap_severity,
                path: Some(file.path),
                detail: DiagnosticDetail::new(source.to_string()),
            }),
        }
    }
    findings
}

/// Applies the byte limit and the ratchet ceiling, if any, to one measured file.
///
/// A file over its limit is a violation unless the ratchet grandfathers it at or above its
/// current size. Under `require_current`, a ceiling above the current size, or one for a file now
/// within its limit, is stale.
fn file_bytes_violation(
    file: ByteLimitedFile,
    actual: FileBytes,
    ceiling: Option<FileByteLimit>,
    current_policy: RatchetCurrentPolicy,
) -> Option<CheckViolation> {
    let exceeded = file.limit.is_exceeded_by(actual);
    let violation = |ratchet| {
        CheckViolation::FileBytes(FileBytesViolation {
            kind: FileBytesViolationKind::FileBytes,
            path: file.path.clone(),
            actual,
            limit: file.limit,
            ratchet,
        })
    };
    match ceiling {
        None => exceeded.then(|| violation(None)),
        Some(ceiling) if exceeded && ceiling.is_exceeded_by(actual) => {
            Some(violation(Some(ceiling)))
        }
        Some(ceiling)
            if current_policy.requires_current() && (!exceeded || ceiling.is_above(actual)) =>
        {
            Some(CheckViolation::Ratchet(RatchetViolation::Measured(
                RatchetStaleMeasuredViolation::FileBytes(FileBytesRatchetStaleMeasuredViolation {
                    path: file.path.clone(),
                    actual,
                    limit: file.limit,
                    ratchet: ceiling,
                }),
            )))
        }
        Some(_) => None,
    }
}
