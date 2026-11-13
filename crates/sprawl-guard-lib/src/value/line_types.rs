use std::collections::BTreeSet;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A category of lines that can be counted in addition to code lines.
#[derive(
    Clone, Copy, Debug, Deserialize, Eq, JsonSchema, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ExtraLineType {
    /// Comment or documentation lines.
    Comments,
    /// Blank lines.
    BlankLines,
}

impl fmt::Display for ExtraLineType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Comments => formatter.write_str("comments"),
            Self::BlankLines => formatter.write_str("blank_lines"),
        }
    }
}

/// Error returned when `count_lines` does not form a valid set.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CountLinesError {
    /// One line type appeared more than once.
    #[error("count_lines contains duplicate {line_type}")]
    Duplicate {
        /// Repeated line type.
        line_type: ExtraLineType,
    },
}

/// The line types counted toward a file's lines. Code lines always count.
///
/// Configuration and JSON list only the extra line types, in canonical order.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CountLines {
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

impl JsonSchema for CountLines {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        "CountLines".into()
    }

    fn json_schema(_generator: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "oneOf": [
                { "const": [] },
                { "const": ["comments"] },
                { "const": ["blank_lines"] },
                { "const": ["comments", "blank_lines"] }
            ],
            "title": "Line types counted in addition to code"
        })
    }
}

impl CountLines {
    const COMMENTS: [ExtraLineType; 1] = [ExtraLineType::Comments];
    const BLANK_LINES: [ExtraLineType; 1] = [ExtraLineType::BlankLines];
    const ALL_EXTRAS: [ExtraLineType; 2] = [ExtraLineType::Comments, ExtraLineType::BlankLines];

    /// Returns the line types counted in addition to code, in canonical order.
    pub const fn extras(self) -> &'static [ExtraLineType] {
        match self {
            Self::Code => &[],
            Self::CodeAndComments => &Self::COMMENTS,
            Self::CodeAndBlankLines => &Self::BLANK_LINES,
            Self::All => &Self::ALL_EXTRAS,
        }
    }

    /// Reports whether one extra line type is counted.
    pub fn contains(self, line_type: ExtraLineType) -> bool {
        self.extras().contains(&line_type)
    }

    /// Returns the same set with comments included.
    pub const fn with_comments(self) -> Self {
        match self {
            Self::Code | Self::CodeAndComments => Self::CodeAndComments,
            Self::CodeAndBlankLines | Self::All => Self::All,
        }
    }
}

/// Lists every counted line type, starting with code, for human-readable output.
impl fmt::Display for CountLines {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("code")?;
        for line_type in self.extras() {
            write!(formatter, ", {line_type}")?;
        }
        Ok(())
    }
}

impl TryFrom<Vec<ExtraLineType>> for CountLines {
    type Error = CountLinesError;

    fn try_from(line_types: Vec<ExtraLineType>) -> Result<Self, Self::Error> {
        let mut unique = BTreeSet::new();
        for line_type in line_types {
            if !unique.insert(line_type) {
                return Err(CountLinesError::Duplicate { line_type });
            }
        }

        Ok(
            match (
                unique.contains(&ExtraLineType::Comments),
                unique.contains(&ExtraLineType::BlankLines),
            ) {
                (false, false) => Self::Code,
                (true, false) => Self::CodeAndComments,
                (false, true) => Self::CodeAndBlankLines,
                (true, true) => Self::All,
            },
        )
    }
}

impl Serialize for CountLines {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.extras().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CountLines {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Vec::<ExtraLineType>::deserialize(deserializer)?
            .try_into()
            .map_err(serde::de::Error::custom)
    }
}

/// Measured number of lines in one category.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Hash, Serialize)]
#[serde(transparent)]
pub struct LineCount(u32);

impl LineCount {
    /// Creates a measured line count.
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

/// A file's lines: code lines plus any configured extra line types.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct CountedLines(u32);

impl CountedLines {
    /// Creates a measured line total.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the raw measured total.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Line counts by category, as reported by the source parser.
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
    /// Creates one line-count breakdown.
    pub const fn new(code: LineCount, comments: LineCount, blank_lines: LineCount) -> Self {
        Self {
            code,
            comments,
            blank_lines,
        }
    }

    /// Sums code lines and the selected extra line types.
    pub fn included_total(self, included: CountLines) -> Result<CountedLines, LineCountOverflow> {
        let mut total = u64::from(self.code.get());
        if included.contains(ExtraLineType::Comments) {
            total += u64::from(self.comments.get());
        }
        if included.contains(ExtraLineType::BlankLines) {
            total += u64::from(self.blank_lines.get());
        }
        let total = u32::try_from(total).map_err(|_| LineCountOverflow::new(total))?;
        Ok(CountedLines::new(total))
    }
}

/// Error returned when a line total exceeds the supported range.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("line count exceeds the supported maximum: {count}")]
pub struct LineCountOverflow {
    count: u64,
}

impl LineCountOverflow {
    const fn new(count: u64) -> Self {
        Self { count }
    }

    /// Returns the unrepresentable line count.
    pub const fn count(self) -> u64 {
        self.count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod when_deserializing_one_extra_line_type {
        use super::*;

        #[test]
        fn it_accepts_each_canonical_value() {
            let cases = [
                ("\"comments\"", ExtraLineType::Comments),
                ("\"blank_lines\"", ExtraLineType::BlankLines),
            ];

            for (json, expected) in cases {
                assert_eq!(
                    serde_json::from_str::<ExtraLineType>(json).unwrap(),
                    expected,
                );
            }
        }

        #[test]
        fn it_rejects_code_because_code_always_counts() {
            let error = serde_json::from_str::<ExtraLineType>("\"code\"").unwrap_err();

            assert!(error.to_string().contains("unknown variant"));
        }

        #[test]
        fn it_rejects_an_unknown_value() {
            let error = serde_json::from_str::<ExtraLineType>("\"documentation\"").unwrap_err();

            assert!(error.to_string().contains("unknown variant"));
        }
    }

    mod when_deserializing_count_lines {
        use super::*;

        #[test]
        fn it_counts_code_only_for_an_empty_list() {
            assert_eq!(
                serde_json::from_str::<CountLines>("[]").unwrap(),
                CountLines::Code,
            );
        }

        #[test]
        fn it_rejects_duplicate_members() {
            let error =
                serde_json::from_str::<CountLines>(r#"["comments", "comments"]"#).unwrap_err();

            assert!(error.to_string().contains("contains duplicate comments"));
        }

        #[test]
        fn it_serializes_in_canonical_order() {
            let included =
                serde_json::from_str::<CountLines>(r#"["blank_lines", "comments"]"#).unwrap();

            assert_eq!(
                serde_json::to_string(&included).unwrap(),
                r#"["comments","blank_lines"]"#,
            );
        }
    }

    mod when_displaying_count_lines {
        use super::*;

        #[test]
        fn it_lists_code_then_the_extras_in_canonical_order() {
            let cases = [
                (CountLines::Code, "code"),
                (CountLines::CodeAndComments, "code, comments"),
                (CountLines::CodeAndBlankLines, "code, blank_lines"),
                (CountLines::All, "code, comments, blank_lines"),
            ];

            for (included, expected) in cases {
                assert_eq!(included.to_string(), expected);
            }
        }
    }

    mod when_summing_line_counts {
        use super::*;

        #[test]
        fn it_sums_code_and_exactly_the_selected_extras() {
            let counts = LineCounts::new(LineCount::new(2), LineCount::new(3), LineCount::new(5));
            let cases = [
                (CountLines::Code, 2),
                (CountLines::CodeAndComments, 5),
                (CountLines::CodeAndBlankLines, 7),
                (CountLines::All, 10),
            ];

            for (included, expected) in cases {
                let actual: CountedLines = counts.included_total(included).unwrap();
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
                .included_total(CountLines::CodeAndComments)
                .unwrap_err();

            assert_eq!(error.count(), u64::from(u32::MAX) + 1);
        }
    }

    mod when_adding_comments_for_a_literate_language {
        use super::*;

        #[test]
        fn it_preserves_the_global_blank_line_selection() {
            assert_eq!(
                CountLines::CodeAndBlankLines.with_comments(),
                CountLines::All
            );
        }

        #[test]
        fn it_does_not_change_an_existing_comment_selection() {
            assert_eq!(
                CountLines::CodeAndComments.with_comments(),
                CountLines::CodeAndComments
            );
        }
    }
}
