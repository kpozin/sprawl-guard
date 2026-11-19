//! Rust `cfg(test)` preprocessing for production line counting.

use std::borrow::Cow;

use serde::Serialize;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Attribute, Item, Meta, Token};
use tokei::LanguageType;

use crate::DiagnosticDetail;
use crate::classification::{ClassifiedSource, SourceKind};
use crate::config::{Config, RustCfgTestExclusion};

/// Result of Rust `cfg(test)` preprocessing for one source string.
pub(crate) struct RustCfgTestPreprocessResult<'source> {
    /// Source passed to the line counter after any exclusions.
    pub(crate) source: Cow<'source, str>,
    /// Stable metadata for diagnostics and `explain`.
    pub(crate) info: RustCfgTestInfo,
    /// Non-fatal warning to include when preprocessing had to fall back.
    pub(crate) warning: Option<RustCfgTestWarning>,
}

/// Internal warning details from Rust `cfg(test)` preprocessing.
pub(crate) struct RustCfgTestWarning {
    /// Parser-provided detail for the warning.
    pub(crate) detail: DiagnosticDetail,
}

/// Diagnostic summary for Rust `cfg(test)` preprocessing.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RustCfgTestInfo {
    /// How far the preprocessor got for this file.
    pub status: RustCfgTestStatus,
    /// Number of top-level items excluded before line counting.
    pub excluded_items: ExcludedItemCount,
}

impl RustCfgTestInfo {
    /// Creates diagnostic info with zero excluded items.
    pub(crate) fn without_exclusions(status: RustCfgTestStatus) -> Self {
        Self {
            status,
            excluded_items: ExcludedItemCount::ZERO,
        }
    }
}

/// Preprocessing status for Rust `cfg(test)` handling.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RustCfgTestStatus {
    /// Rust-specific exclusion is disabled by config.
    Disabled,
    /// The source is a test file, so the production-only exclusion is not applied.
    PathClassifiedTest,
    /// A quick text scan found no likely `cfg(test)` attribute.
    PrefilterSkipped,
    /// The file parsed successfully and file-wide plus top-level item predicates were evaluated.
    Parsed,
    /// Parsing failed, so the original source was counted and a warning was emitted.
    ParseFailed,
}

/// Count of top-level Rust items excluded by `cfg(test)` preprocessing.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ExcludedItemCount(u32);

impl ExcludedItemCount {
    const ZERO: Self = Self(0);

    /// Creates an excluded item count.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    fn from_len(value: usize) -> Self {
        Self::new(u32::try_from(value).expect("excluded item count must fit in u32"))
    }

    /// Returns the underlying count.
    pub fn get(self) -> u32 {
        self.0
    }
}

/// Removes top-level Rust items that can compile only when `test` is enabled.
///
/// Parse failures keep the original source, so unsupported syntax never hides lines from the
/// check.
///
/// A leading byte order mark is dropped from the result. `syn` reports byte ranges relative to
/// the text after the mark, and the line counter would otherwise read the mark as code on the
/// first line.
pub(crate) fn preprocess_rust_cfg_test(source: &str) -> RustCfgTestPreprocessResult<'_> {
    let source = source.strip_prefix(BYTE_ORDER_MARK).unwrap_or(source);
    if !looks_like_cfg_test(source) {
        return RustCfgTestPreprocessResult {
            source: Cow::Borrowed(source),
            info: RustCfgTestInfo::without_exclusions(RustCfgTestStatus::PrefilterSkipped),
            warning: None,
        };
    }

    let syntax = match syn::parse_file(source) {
        Ok(syntax) => syntax,
        Err(error) => {
            return RustCfgTestPreprocessResult {
                source: Cow::Borrowed(source),
                info: RustCfgTestInfo::without_exclusions(RustCfgTestStatus::ParseFailed),
                warning: Some(RustCfgTestWarning {
                    detail: DiagnosticDetail::new(error.to_string()),
                }),
            };
        }
    };

    let excluded = excluded_ranges(&syntax, source);
    let excluded_items = excluded.excluded_items;
    let source = if excluded.ranges.is_empty() {
        Cow::Borrowed(source)
    } else {
        Cow::Owned(remove_ranges(source, &excluded.ranges))
    };

    RustCfgTestPreprocessResult {
        source,
        info: RustCfgTestInfo {
            status: RustCfgTestStatus::Parsed,
            excluded_items,
        },
        warning: None,
    }
}

/// Returns preprocessing metadata for a classified Rust source, or `None` for another language.
pub(crate) fn rust_cfg_test_policy_info(
    config: &Config,
    source: &ClassifiedSource,
) -> Option<RustCfgTestInfo> {
    if source.language().as_tokei() != LanguageType::Rust {
        return None;
    }
    if source.kind == SourceKind::Test {
        return Some(RustCfgTestInfo::without_exclusions(
            RustCfgTestStatus::PathClassifiedTest,
        ));
    }
    let rust = config
        .languages()
        .get(&source.language())
        .and_then(|language| language.rust());
    match rust.map(|config| config.cfg_test_exclusion) {
        Some(RustCfgTestExclusion::Enabled) => Some(RustCfgTestInfo::without_exclusions(
            RustCfgTestStatus::PrefilterSkipped,
        )),
        Some(RustCfgTestExclusion::Disabled) | None => Some(RustCfgTestInfo::without_exclusions(
            RustCfgTestStatus::Disabled,
        )),
    }
}

/// U+FEFF, which some editors write at the start of a UTF-8 file.
const BYTE_ORDER_MARK: char = '\u{feff}';

fn looks_like_cfg_test(source: &str) -> bool {
    source.contains("cfg") && source.contains("test")
}

fn excluded_ranges(syntax: &syn::File, source: &str) -> ExcludedRanges {
    if syntax.attrs.iter().any(attr_requires_test) {
        return ExcludedRanges {
            ranges: vec![SourceRange {
                start: 0,
                end: source.len(),
            }],
            excluded_items: ExcludedItemCount::from_len(syntax.items.len()),
        };
    }
    // `syn` takes a shebang line off before it parses, so spans start after that line.
    let parsed_from = syntax.shebang.as_ref().map_or(0, String::len);
    let ranges = excluded_item_ranges(&syntax.items, parsed_from);
    let excluded_items = ExcludedItemCount::from_len(ranges.len());
    ExcludedRanges {
        ranges,
        excluded_items,
    }
}

fn excluded_item_ranges(items: &[Item], parsed_from: usize) -> Vec<SourceRange> {
    items
        .iter()
        .filter(|item| item_requires_test(item))
        .map(|item| item_range(item, parsed_from))
        .collect()
}

fn item_requires_test(item: &Item) -> bool {
    item_attrs(item).iter().any(attr_requires_test)
}

fn item_attrs(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        Item::Verbatim(_) => &[],
        _ => &[],
    }
}

fn attr_requires_test(attr: &Attribute) -> bool {
    if !attr.path().is_ident("cfg") {
        return false;
    }
    attr.parse_args::<Meta>()
        .is_ok_and(|meta| meta_requires_test(&meta))
}

fn meta_requires_test(meta: &Meta) -> bool {
    match meta {
        Meta::Path(path) => path.is_ident("test"),
        Meta::List(list) if list.path.is_ident("all") => {
            nested_metas(list).is_some_and(|metas| metas.iter().any(meta_requires_test))
        }
        Meta::List(list) if list.path.is_ident("any") => nested_metas(list)
            .is_some_and(|metas| !metas.is_empty() && metas.iter().all(meta_requires_test)),
        Meta::List(list) if list.path.is_ident("not") => false,
        Meta::List(_) | Meta::NameValue(_) => false,
    }
}

fn nested_metas(list: &syn::MetaList) -> Option<Vec<Meta>> {
    Punctuated::<Meta, Token![,]>::parse_terminated
        .parse2(list.tokens.clone())
        .ok()
        .map(|metas| metas.into_iter().collect())
}

/// Returns the bytes an item covers, from its first outer attribute to its last token.
///
/// `parsed_from` is where the text that `syn` parsed starts in the source; spans count from there.
fn item_range(item: &Item, parsed_from: usize) -> SourceRange {
    let item_bytes = item.span().byte_range();
    let start = item_attrs(item)
        .iter()
        .map(|attr| attr.span().byte_range().start)
        .min()
        .unwrap_or(item_bytes.start);
    SourceRange {
        start: parsed_from + start,
        end: parsed_from + item_bytes.end,
    }
}

fn remove_ranges(source: &str, ranges: &[SourceRange]) -> String {
    let mut excluded = vec![false; source.len()];
    for range in ranges {
        excluded[range.start..range.end].fill(true);
    }

    let mut retained = Vec::with_capacity(source.len());
    let mut line_start = 0;
    for line in source.as_bytes().split_inclusive(|byte| *byte == b'\n') {
        let line_end = line_start + line.len();
        let line_exclusions = &excluded[line_start..line_end];
        let retained_line = line
            .iter()
            .zip(line_exclusions)
            .filter_map(|(byte, excluded)| (!excluded).then_some(*byte))
            .collect::<Vec<_>>();

        if !line_exclusions.iter().any(|excluded| *excluded)
            || !retained_line.iter().all(u8::is_ascii_whitespace)
        {
            retained.extend(retained_line);
        }
        line_start = line_end;
    }

    String::from_utf8(retained).expect("removing parsed Rust ranges must preserve UTF-8")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SourceRange {
    start: usize,
    end: usize,
}

struct ExcludedRanges {
    ranges: Vec<SourceRange>,
    excluded_items: ExcludedItemCount,
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::*;

    fn preprocess(source: &str) -> RustCfgTestPreprocessResult<'_> {
        preprocess_rust_cfg_test(source)
    }

    mod when_the_prefilter_does_not_match {
        use super::*;

        #[test]
        fn it_counts_the_original_source_without_parsing() {
            let result = preprocess("fn production() {}\n");

            assert_eq!(result.source, "fn production() {}\n");
            assert_eq!(result.info.status, RustCfgTestStatus::PrefilterSkipped);
            assert_eq!(result.info.excluded_items.get(), 0);
        }

        #[test]
        fn it_drops_a_leading_byte_order_mark() {
            let result = preprocess("\u{feff}// comment\n");

            assert_eq!(result.source, "// comment\n");
        }
    }

    mod when_a_top_level_item_requires_test {
        use super::*;

        #[test]
        fn it_removes_a_cfg_test_module() {
            let result = preprocess(indoc! {r#"
                fn production() {}

                #[cfg(test)]
                mod tests {
                    fn fixture() {}
                }
            "#});

            assert_eq!(result.info.status, RustCfgTestStatus::Parsed);
            assert_eq!(result.info.excluded_items.get(), 1);
            assert!(result.source.contains("fn production() {}"));
            assert!(!result.source.contains("fixture"));
        }

        #[test]
        fn it_removes_a_cfg_test_function() {
            let result = preprocess(indoc! {r#"
                #[cfg(test)]
                fn fixture() -> u32 {
                    1
                }
            "#});

            assert_eq!(result.info.excluded_items.get(), 1);
            assert!(!result.source.contains("fixture"));
        }

        #[test]
        fn it_removes_exactly_the_item_after_multibyte_text_on_the_same_line() {
            let result = preprocess(indoc! {r#"
                fn production() {}
                #[cfg(test)] const NAME: &str = "héllo wörld"; fn more() {}
                #[cfg(test)] fn fixture() { let value = "éééééé"; }
                // comment
            "#});

            assert_eq!(result.info.excluded_items.get(), 2);
            assert_eq!(
                result.source,
                indoc! {r#"
                    fn production() {}
                     fn more() {}
                    // comment
                "#}
            );
        }

        #[test]
        fn it_removes_exactly_the_item_after_a_shebang_line() {
            let result =
                preprocess("#!/usr/bin/env cargo\n#[cfg(test)] fn fixture() {}\nfn a() {}\n");

            assert_eq!(result.info.excluded_items.get(), 1);
            assert_eq!(result.source, "#!/usr/bin/env cargo\nfn a() {}\n");
        }

        #[test]
        fn it_removes_exactly_the_item_after_a_byte_order_mark() {
            let result =
                preprocess("\u{feff}// comment\n#[cfg(test)] fn fixture() {}\nfn a() {}\n");

            assert_eq!(result.info.excluded_items.get(), 1);
            assert_eq!(result.source, "// comment\nfn a() {}\n");
        }

        #[test]
        fn it_removes_stacked_outer_attributes_and_doc_comments() {
            let result = preprocess(indoc! {r#"
                /// fixture docs
                #[allow(dead_code)]
                #[cfg(test)]
                fn fixture() {}
                fn production() {}
            "#});

            assert_eq!(result.info.excluded_items.get(), 1);
            assert!(!result.source.contains("fixture docs"));
            assert!(!result.source.contains("allow"));
            assert!(!result.source.contains("fixture"));
            assert!(result.source.contains("fn production() {}"));
        }
    }

    mod when_the_file_requires_test {
        use super::*;

        #[test]
        fn it_removes_the_whole_file() {
            let result = preprocess(indoc! {r#"
                #![cfg(test)]

                fn fixture_one() {}
                fn fixture_two() {}
            "#});

            assert_eq!(result.info.status, RustCfgTestStatus::Parsed);
            assert_eq!(result.info.excluded_items.get(), 2);
            assert!(!result.source.contains("fixture_one"));
            assert!(!result.source.contains("fixture_two"));
        }
    }

    mod when_a_cfg_predicate_combines_test_with_other_terms {
        use super::*;

        #[test]
        fn it_excludes_all_predicates_that_require_test() {
            let result = preprocess(indoc! {r#"
                #[cfg(all(test, unix))]
                fn fixture() {}

                #[cfg(any(all(test, unix), all(test, windows)))]
                fn platform_fixture() {}
            "#});

            assert_eq!(result.info.excluded_items.get(), 2);
            assert!(!result.source.contains("platform_fixture"));
        }

        #[test]
        fn it_keeps_any_predicates_that_can_compile_without_test() {
            let result = preprocess(indoc! {r#"
                #[cfg(any(test, feature = "foo"))]
                fn helper() {}
            "#});

            assert_eq!(result.info.excluded_items.get(), 0);
            assert!(result.source.contains("helper"));
        }

        #[test]
        fn it_keeps_doctest_disjunctions_until_doctest_support_exists() {
            let result = preprocess(indoc! {r#"
                #[cfg(all(any(test, doctest), unix))]
                fn helper() {}
            "#});

            assert_eq!(result.info.excluded_items.get(), 0);
            assert!(result.source.contains("helper"));
        }
    }

    mod when_cfg_test_is_nested_inside_a_function {
        use super::*;

        #[test]
        fn it_does_not_exclude_the_nested_block() {
            let result = preprocess(indoc! {r#"
                fn production() {
                    #[cfg(test)]
                    {
                        assert!(true);
                    }
                }
            "#});

            assert_eq!(result.info.excluded_items.get(), 0);
            assert!(result.source.contains("assert"));
        }
    }

    mod when_rust_parsing_fails {
        use super::*;

        #[test]
        fn it_warns_and_counts_the_original_source() {
            let source = indoc! {r#"
                #[cfg(test)]
                fn broken( {
            "#};
            let result = preprocess(source);

            assert_eq!(result.info.status, RustCfgTestStatus::ParseFailed);
            assert_eq!(result.source, source);
            assert!(!result.warning.unwrap().detail.is_empty());
        }
    }
}
