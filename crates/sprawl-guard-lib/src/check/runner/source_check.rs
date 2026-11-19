//! The measurement and outcome of checking one counted source file against its line limit.

use crate::check::report::{CheckWarning, FileLinesViolation, FileViolationKind};
use crate::code_line_counter::LineMeasurement;
use crate::{CountedLines, LineLimit, RelativePath};

#[derive(Clone, Debug)]
pub(in crate::check) struct FileMeasurement {
    pub(in crate::check) kind: FileViolationKind,
    pub(in crate::check) path: RelativePath,
    pub(in crate::check) lines: LineMeasurement,
    pub(in crate::check) limit: LineLimit,
}

impl FileMeasurement {
    /// Returns the aggregate selected by the resolved included line types.
    pub(in crate::check) const fn current_lines(&self) -> CountedLines {
        self.lines.actual()
    }
}

pub(in crate::check) enum SourceCheck {
    WithinLimit {
        measurement: FileMeasurement,
        warnings: Vec<CheckWarning>,
    },
    Exceeded {
        measurement: FileMeasurement,
        violation: FileLinesViolation,
        warnings: Vec<CheckWarning>,
    },
}

impl SourceCheck {
    pub(in crate::check) fn measurement(&self) -> FileMeasurement {
        match self {
            Self::WithinLimit { measurement, .. } | Self::Exceeded { measurement, .. } => {
                measurement.clone()
            }
        }
    }

    pub(in crate::check) fn warnings_mut(&mut self) -> &mut Vec<CheckWarning> {
        match self {
            Self::WithinLimit { warnings, .. } | Self::Exceeded { warnings, .. } => warnings,
        }
    }

    pub(in crate::check) fn violation_mut(&mut self) -> Option<&mut FileLinesViolation> {
        match self {
            Self::WithinLimit { .. } => None,
            Self::Exceeded { violation, .. } => Some(violation),
        }
    }

    pub(in crate::check) fn has_violation(&self) -> bool {
        matches!(self, Self::Exceeded { .. })
    }

    pub(in crate::check) fn into_violation(self) -> Option<FileLinesViolation> {
        match self {
            Self::WithinLimit { .. } => None,
            Self::Exceeded { violation, .. } => Some(violation),
        }
    }
}
