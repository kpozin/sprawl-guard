use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A category of logical lines that can contribute to configured LOC.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IncludedLineType {
    /// Source code lines.
    Code,
    /// Comment or documentation lines.
    Comments,
    /// Blank lines.
    BlankLines,
}

impl fmt::Display for IncludedLineType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Code => formatter.write_str("code"),
            Self::Comments => formatter.write_str("comments"),
            Self::BlankLines => formatter.write_str("blank_lines"),
        }
    }
}

/// Error returned when included line types do not form a valid set.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum IncludedLineTypesError {
    /// Code was omitted even though every configured LOC measurement includes it.
    #[error("included_line_types must include code")]
    MissingCode,
    /// One line type appeared more than once.
    #[error("included_line_types contains duplicate {line_type}")]
    Duplicate {
        /// Repeated line type.
        line_type: IncludedLineType,
    },
}

/// Validated included line types in canonical serialization order.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum IncludedLineTypes {
    /// Count code only.
    #[default]
    Code,
    /// Count code and comments.
    CodeAndComments,
    /// Count code and blank lines.
    CodeAndBlankLines,
    /// Count code, comments, and blank lines.
    All,
}

impl IncludedLineTypes {
    const CODE: [IncludedLineType; 1] = [IncludedLineType::Code];
    const CODE_AND_BLANK_LINES: [IncludedLineType; 2] =
        [IncludedLineType::Code, IncludedLineType::BlankLines];
    const CODE_AND_COMMENTS: [IncludedLineType; 2] =
        [IncludedLineType::Code, IncludedLineType::Comments];
    const ALL: [IncludedLineType; 3] = [
        IncludedLineType::Code,
        IncludedLineType::Comments,
        IncludedLineType::BlankLines,
    ];

    /// Returns the members in canonical order.
    pub const fn as_slice(self) -> &'static [IncludedLineType] {
        match self {
            Self::Code => &Self::CODE,
            Self::CodeAndComments => &Self::CODE_AND_COMMENTS,
            Self::CodeAndBlankLines => &Self::CODE_AND_BLANK_LINES,
            Self::All => &Self::ALL,
        }
    }

    /// Reports whether one line type contributes to configured LOC.
    pub fn contains(self, line_type: IncludedLineType) -> bool {
        self.as_slice().contains(&line_type)
    }

    /// Returns the same set with comments included.
    pub const fn with_comments(self) -> Self {
        match self {
            Self::Code | Self::CodeAndComments => Self::CodeAndComments,
            Self::CodeAndBlankLines | Self::All => Self::All,
        }
    }
}

impl fmt::Display for IncludedLineTypes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, line_type) in self.as_slice().iter().enumerate() {
            if index > 0 {
                formatter.write_str(", ")?;
            }
            line_type.fmt(formatter)?;
        }
        Ok(())
    }
}

impl TryFrom<Vec<IncludedLineType>> for IncludedLineTypes {
    type Error = IncludedLineTypesError;

    fn try_from(line_types: Vec<IncludedLineType>) -> Result<Self, Self::Error> {
        let mut unique = BTreeSet::new();
        for line_type in line_types {
            if !unique.insert(line_type) {
                return Err(IncludedLineTypesError::Duplicate { line_type });
            }
        }
        if !unique.contains(&IncludedLineType::Code) {
            return Err(IncludedLineTypesError::MissingCode);
        }

        Ok(
            match (
                unique.contains(&IncludedLineType::Comments),
                unique.contains(&IncludedLineType::BlankLines),
            ) {
                (false, false) => Self::Code,
                (true, false) => Self::CodeAndComments,
                (false, true) => Self::CodeAndBlankLines,
                (true, true) => Self::All,
            },
        )
    }
}

impl Serialize for IncludedLineTypes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.as_slice().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for IncludedLineTypes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Vec::<IncludedLineType>::deserialize(deserializer)?
            .try_into()
            .map_err(serde::de::Error::custom)
    }
}

/// Measured number of logical lines in one category.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct LineCount(u32);

impl LineCount {
    /// Creates a measured logical-line count.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the raw measured count.
    pub const fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for LineCount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Measured aggregate after applying configured included line types.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct CountedLoc(u32);

impl CountedLoc {
    /// Creates a measured configured-LOC aggregate.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the raw measured aggregate.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Logical-line counts reported by the source parser.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct LineCounts {
    /// Source code lines.
    pub code: LineCount,
    /// Comment or documentation lines.
    pub comments: LineCount,
    /// Blank lines.
    pub blank_lines: LineCount,
}

impl LineCounts {
    /// Creates one logical-line breakdown.
    pub const fn new(code: LineCount, comments: LineCount, blank_lines: LineCount) -> Self {
        Self {
            code,
            comments,
            blank_lines,
        }
    }

    /// Sums the selected line types into configured LOC.
    pub fn included_total(
        self,
        included: IncludedLineTypes,
    ) -> Result<CountedLoc, LineCountOverflow> {
        let mut total = u64::from(self.code.get());
        if included.contains(IncludedLineType::Comments) {
            total += u64::from(self.comments.get());
        }
        if included.contains(IncludedLineType::BlankLines) {
            total += u64::from(self.blank_lines.get());
        }
        let total = u32::try_from(total).map_err(|_| LineCountOverflow::new(total))?;
        Ok(CountedLoc::new(total))
    }
}

/// Error returned when selected logical-line counts exceed the supported range.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("included logical-line count exceeds the supported maximum: {count}")]
pub struct LineCountOverflow {
    count: u64,
}

impl LineCountOverflow {
    const fn new(count: u64) -> Self {
        Self { count }
    }

    /// Returns the unrepresentable logical-line count.
    pub const fn count(self) -> u64 {
        self.count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod when_deserializing_one_included_line_type {
        use super::*;

        #[test]
        fn it_accepts_each_canonical_value() {
            let cases = [
                ("\"code\"", IncludedLineType::Code),
                ("\"comments\"", IncludedLineType::Comments),
                ("\"blank_lines\"", IncludedLineType::BlankLines),
            ];

            for (json, expected) in cases {
                assert_eq!(
                    serde_json::from_str::<IncludedLineType>(json).unwrap(),
                    expected,
                );
            }
        }

        #[test]
        fn it_rejects_an_unknown_value() {
            let error = serde_json::from_str::<IncludedLineType>("\"documentation\"").unwrap_err();

            assert!(error.to_string().contains("unknown variant"));
        }
    }

    mod when_deserializing_included_line_types {
        use super::*;

        #[test]
        fn it_requires_code() {
            let error = serde_json::from_str::<IncludedLineTypes>(r#"["comments"]"#).unwrap_err();

            assert!(error.to_string().contains("must include code"));
        }

        #[test]
        fn it_rejects_duplicate_members() {
            let error =
                serde_json::from_str::<IncludedLineTypes>(r#"["code", "comments", "code"]"#)
                    .unwrap_err();

            assert!(error.to_string().contains("contains duplicate code"));
        }

        #[test]
        fn it_serializes_in_canonical_order() {
            let included =
                serde_json::from_str::<IncludedLineTypes>(r#"["blank_lines", "code", "comments"]"#)
                    .unwrap();

            assert_eq!(
                serde_json::to_string(&included).unwrap(),
                r#"["code","comments","blank_lines"]"#,
            );
        }
    }

    mod when_displaying_included_line_types {
        use super::*;

        #[test]
        fn it_uses_canonical_order_and_names() {
            let cases = [
                (IncludedLineTypes::Code, "code"),
                (IncludedLineTypes::CodeAndComments, "code, comments"),
                (IncludedLineTypes::CodeAndBlankLines, "code, blank_lines"),
                (IncludedLineTypes::All, "code, comments, blank_lines"),
            ];

            for (included, expected) in cases {
                assert_eq!(included.to_string(), expected);
            }
        }
    }

    mod when_summing_line_counts {
        use super::*;

        #[test]
        fn it_sums_exactly_the_included_line_types() {
            let counts = LineCounts::new(LineCount::new(2), LineCount::new(3), LineCount::new(5));
            let cases = [
                (IncludedLineTypes::Code, 2),
                (IncludedLineTypes::CodeAndComments, 5),
                (IncludedLineTypes::CodeAndBlankLines, 7),
                (IncludedLineTypes::All, 10),
            ];

            for (included, expected) in cases {
                let actual: CountedLoc = counts.included_total(included).unwrap();
                assert_eq!(actual.get(), expected);
            }
        }

        #[test]
        fn it_reports_overflow() {
            let counts = LineCounts::new(
                LineCount::new(u32::MAX),
                LineCount::new(1),
                LineCount::new(0),
            );

            let error = counts
                .included_total(IncludedLineTypes::CodeAndComments)
                .unwrap_err();

            assert_eq!(error.count(), u64::from(u32::MAX) + 1);
        }
    }

    mod when_adding_comments_for_a_literate_language {
        use super::*;

        #[test]
        fn it_preserves_the_global_blank_line_selection() {
            assert_eq!(
                IncludedLineTypes::CodeAndBlankLines.with_comments(),
                IncludedLineTypes::All
            );
        }

        #[test]
        fn it_does_not_change_an_existing_comment_selection() {
            assert_eq!(
                IncludedLineTypes::CodeAndComments.with_comments(),
                IncludedLineTypes::CodeAndComments
            );
        }
    }
}
